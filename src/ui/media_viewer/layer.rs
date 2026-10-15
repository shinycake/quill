//! The viewer's layers in the main window: the backdrop, the viewer panel
//! as a cached slice (`SliceKind::Viewer`: top bar, the media box with its
//! thumbnail and handlers, arrows, controls, caption), and above it the
//! video layer, a small view that draws only the playing picture.
//!
//! The video element asks for its next frame on the view it is drawn in
//! (`video_player::video_frame`), so a new picture redraws the layer and
//! `QuillApp` around it while the panel and the chat replay their last
//! frame. The panel itself redraws when the player's transport changes,
//! at most every 100 ms while it plays (the time labels and seek bar).
//! Chat windows draw the viewer inline instead (`media_viewer_panel` with
//! `layered: false`).
#![deny(clippy::indexing_slicing)]

use super::*;
use crate::ui::video_player::{WeakVideoPlayer, video_frame};

/// Where the media sits in the window, and its zoomed box inside that.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct ViewerGeometry {
    /// The area the media fits into (between the bars, outside the arrow
    /// lanes).
    pub(super) fit: (f32, f32),
    /// The media's own box (fitted from its natural size).
    pub(super) media: (f32, f32),
    /// Its top-left in the window (centered in the fit area).
    pub(super) origin: (f32, f32),
    /// The media at the current zoom, and its pan inside the box.
    pub(super) zoomed: (f32, f32),
    pub(super) pan: (f32, f32),
}

/// What the video layer draws.
#[derive(Clone, Default)]
enum LayerContent {
    #[default]
    Nothing,
    /// Weak: closing the viewer drops the player even though this view
    /// is not drawn again.
    Player(WeakVideoPlayer),
    /// A `.gif` file the player can't decode, animated by `img`.
    Gif(PathBuf),
}

/// The view drawing the viewer's playing picture (see the module docs).
pub(in crate::ui) struct ViewerVideoLayer {
    content: LayerContent,
    geometry: Option<ViewerGeometry>,
    open_gen: u64,
}

impl Render for ViewerVideoLayer {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let Some(geometry) = self.geometry else {
            return Empty.into_any_element();
        };
        let (zoom_w, zoom_h) = geometry.zoomed;
        let picture = match &self.content {
            LayerContent::Nothing => return Empty.into_any_element(),
            LayerContent::Player(player) => match player.upgrade() {
                Some(player) => video_frame(&player)
                    .object_fit(ObjectFit::Contain)
                    .into_any_element(),
                None => return Empty.into_any_element(),
            },
            LayerContent::Gif(path) => img(path.clone())
                .size_full()
                .object_fit(ObjectFit::Contain)
                .into_any_element(),
        };
        div()
            .absolute()
            .left(px(geometry.origin.0))
            .top(px(geometry.origin.1))
            .w(px(geometry.media.0))
            .h(px(geometry.media.1))
            .overflow_hidden()
            .child(
                div()
                    .absolute()
                    .left(px(geometry.pan.0))
                    .top(px(geometry.pan.1))
                    .w(px(zoom_w))
                    .h(px(zoom_h))
                    .child(picture),
            )
            .with_animation(
                ("viewer-video-fade-in", self.open_gen as usize),
                Animation::new(Duration::from_millis(VIEWER_SHOW_MS)),
                |layer, t| layer.opacity(if still_frame() { 1.0 } else { t }),
            )
            .into_any_element()
    }
}

impl QuillApp {
    /// Lay out the media for `viewport`: fit it between the bars, and
    /// refit the zoom when the box changed (a resized window, another
    /// item). The panel and the video layer both place the media here.
    pub(super) fn viewer_geometry(
        &mut self,
        item: &MediaViewerItem,
        viewport: gpui_kit::Size<Pixels>,
    ) -> ViewerGeometry {
        let bottom_space = if item.caption.is_empty() { 72.0 } else { 104.0 };
        let fit = (
            (f32::from(viewport.width) - 2.0 * VIEWER_SIDE_LANE).max(240.0),
            (f32::from(viewport.height) - VIEWER_TOP_BAR - bottom_space).max(200.0),
        );
        // The media's own box inside the frame: sized from its natural
        // dimensions (axes swapped for a quarter turn) rather than trusting
        // object-fit, so it can never spill out of the frame.
        let natural = item.natural_size.map(|(w, h)| {
            if self.viewer.orientation.swaps_axes() {
                (h as f32, w as f32)
            } else {
                (w as f32, h as f32)
            }
        });
        let media = natural
            .map(|natural| quill::media_viewer::fit_within(natural, fit))
            .unwrap_or(fit);
        if media != self.viewer.frame {
            self.viewer.frame = media;
            self.viewer.zoom.reset();
        }
        let zoom = self.viewer.zoom;
        ViewerGeometry {
            fit,
            media,
            origin: (
                (f32::from(viewport.width) - media.0) / 2.0,
                VIEWER_TOP_BAR + (fit.1 - media.1) / 2.0,
            ),
            zoomed: (media.0 * zoom.zoom, media.1 * zoom.zoom),
            pan: zoom.pan,
        }
    }

    /// The viewer's picture as an element of its own (chat windows, which
    /// draw the viewer inline).
    pub(super) fn viewer_inline_video(&self) -> Option<AnyElement> {
        if let Some(player) = &self.viewer.player {
            return Some(video_frame(player).into_any_element());
        }
        self.viewer.gif.clone().map(|path| {
            img(path)
                .size_full()
                .object_fit(ObjectFit::Contain)
                .into_any_element()
        })
    }

    /// Phase 4.5 / MED1: the fullscreen media viewer. In the main window
    /// it is three layers (see the module docs); clicking the backdrop
    /// closes it. The panel draws above the backdrop and the video layer
    /// above the panel; neither layer takes clicks, so the media box's
    /// handlers in the panel get them.
    pub(in crate::ui) fn media_viewer_overlay(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(panel) = self.viewer_slot() else {
            return self.media_viewer_panel(false, window, cx);
        };
        let Some(item) = self.viewer.state.current().cloned() else {
            return Empty.into_any_element();
        };
        let geometry = self.viewer_geometry(&item, window.viewport_size());
        let layer = self.update_viewer_layer(&item, geometry, cx);
        let open_gen = self.viewer.open_gen as usize;
        div()
            .id("media-viewer-overlay")
            .occlude()
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .bottom_0()
            // Clicking anywhere outside the media and controls closes.
            .child(
                div()
                    .id("media-viewer-backdrop")
                    .occlude()
                    .absolute()
                    .inset_0()
                    .bg(gpui_kit::black().opacity(0.92))
                    .on_mouse_move(cx.listener(|this, _: &MouseMoveEvent, _, cx| {
                        this.viewer_note_activity(false, cx);
                    }))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.close_media_viewer(cx);
                    }))
                    // Fade in over 200 ms when it opens (tdesktop
                    // `mediaviewShowDuration`); closing is instant.
                    .with_animation(
                        ("media-viewer-fade-in", open_gen),
                        Animation::new(Duration::from_millis(VIEWER_SHOW_MS)),
                        |backdrop, t| backdrop.opacity(if still_frame() { 1.0 } else { t }),
                    ),
            )
            .child(panel)
            .children(layer)
            .into_any_element()
    }

    /// Point the video layer at the current clip and geometry (created on
    /// first use). `None` when nothing plays.
    fn update_viewer_layer(
        &mut self,
        item: &MediaViewerItem,
        geometry: ViewerGeometry,
        cx: &mut Context<Self>,
    ) -> Option<Entity<ViewerVideoLayer>> {
        let content = match (&self.viewer.player, &self.viewer.gif) {
            _ if !item.kind.is_playable() || self.viewer.video != Some(item.message_id) => {
                LayerContent::Nothing
            }
            (Some(player), _) => LayerContent::Player(player.downgrade()),
            (None, Some(path)) => LayerContent::Gif(path.clone()),
            (None, None) => LayerContent::Nothing,
        };
        if let Some(player) = &self.viewer.player {
            player.set_orientation(self.viewer.orientation.turns, false);
        }
        let open_gen = self.viewer.open_gen;
        let layer = self
            .viewer
            .video_layer
            .get_or_insert_with(|| {
                cx.new(|_| ViewerVideoLayer {
                    content: LayerContent::Nothing,
                    geometry: None,
                    open_gen,
                })
            })
            .clone();
        let shows = !matches!(content, LayerContent::Nothing);
        layer.update(cx, |layer, _| {
            layer.content = content;
            layer.geometry = Some(geometry);
            layer.open_gen = open_gen;
        });
        shows.then_some(layer)
    }
}
