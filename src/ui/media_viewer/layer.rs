//! The viewer's layers in the main window, bottom to top: the opaque
//! backdrop, the media box as a cached slice (`SliceKind::Viewer`: the
//! thumbnail or photo with its zoom, pan and menu handlers), the video
//! layer (a small view that draws only the playing picture), and the
//! chrome as a cached slice (`SliceKind::ViewerChrome`: top bar, arrows,
//! caption, controls), so nothing the chrome draws is ever under the
//! picture.
//!
//! The video element asks for its next frame on the view it is drawn in
//! (`video_player::video_frame`), so a new picture redraws the layer and
//! `QuillApp` around it while both slices and the chat replay their last
//! frame. The chrome redraws when the player's transport changes, at most
//! every 100 ms while it plays (the time labels and seek bar). Chat
//! windows draw the viewer inline instead (`ViewerPart::Inline`).
#![deny(clippy::indexing_slicing)]

use super::caption::CaptionLayout;
use super::*;
use crate::ui::video_player::controls::{PictureOverlay, PictureState, WaitingCheck, WaitingGate};
use crate::ui::video_player::{PlayState, WeakVideoPlayer, video_frame};

/// Where the media sits in the window, and its zoomed box inside that.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct ViewerGeometry {
    /// The area the media fits into: between the bars and outside the
    /// arrow lanes, or the whole window in video full screen.
    pub(super) fit: geometry::FitArea,
    /// The caption as shaped for its plate (`None`: no caption, or video
    /// full screen, which hides it).
    pub(super) caption: Option<CaptionLayout>,
    /// The media's own box (fitted from its natural size).
    pub(super) media: (f32, f32),
    /// What the bottom bar (caption and controls) takes from the window's
    /// bottom edge.
    pub(super) bottom_chrome: f32,
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
    /// The waiting spinner's delay (`kWaitingShowDelay`).
    waiting: WaitingGate,
}

impl ViewerVideoLayer {
    /// Whether the clip has waited long enough for the spinner; arms a
    /// redraw for when it will have (an opening clip asks for no frames).
    fn waiting_shown(&mut self, waiting: bool, cx: &mut Context<Self>) -> bool {
        let now = cx.background_executor().now();
        match self.waiting.observe(waiting, now) {
            WaitingCheck::Settled { shown } => shown,
            WaitingCheck::Pending(wait) => {
                let timer = cx.spawn(async move |this, cx| {
                    cx.background_executor().timer(wait).await;
                    let _ = this.update(cx, |_, cx| cx.notify());
                });
                self.waiting.arm(timer);
                false
            }
        }
    }
}

impl Render for ViewerVideoLayer {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(geometry) = self.geometry else {
            return Empty.into_any_element();
        };
        let (zoom_w, zoom_h) = geometry.zoomed;
        let mut waiting = false;
        let picture = match &self.content {
            LayerContent::Nothing => return Empty.into_any_element(),
            LayerContent::Player(player) => match player.upgrade() {
                // A failed clip leaves the thumbnail and the panel's
                // "can't be played" plate in view.
                Some(player) if player.state() == PlayState::Failed => {
                    return Empty.into_any_element();
                }
                Some(player) => {
                    waiting = matches!(player.state(), PlayState::Opening | PlayState::Buffering);
                    video_frame(&player)
                        .object_fit(ObjectFit::Contain)
                        .into_any_element()
                }
                None => return Empty.into_any_element(),
            },
            LayerContent::Gif(path) => img(path.clone())
                .size_full()
                .object_fit(ObjectFit::Contain)
                .into_any_element(),
        };
        let spinner = self
            .waiting_shown(waiting, cx)
            .then(|| PictureOverlay::new(self.open_gen, PictureState::Waiting));
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
            .children(spinner)
            .with_animation(
                ("viewer-video-fade-in", self.open_gen as usize),
                Animation::new(Duration::from_millis(VIEWER_SHOW_MS)),
                |layer, t| layer.opacity(if still_frame() { 1.0 } else { t }),
            )
            .into_any_element()
    }
}

impl QuillApp {
    /// How far up from the window's bottom edge the open viewer's caption
    /// and controls reach, as the frame lays them out (0 with no viewer
    /// item). Overlays that float over the viewer (the status toast) clear it.
    pub(in crate::ui) fn viewer_bottom_chrome(&mut self, window: &mut Window, cx: &App) -> Pixels {
        let Some(item) = self.viewer.state.current().cloned() else {
            return px(0.);
        };
        px(self.viewer_geometry(&item, window, cx).bottom_chrome)
    }

    /// Tests: the media's box in the window, and whether the caption takes
    /// room under it, as the next frame lays them out.
    #[cfg(test)]
    #[cfg(feature = "demo-capture")]
    pub(in crate::ui) fn viewer_media_bounds(
        &mut self,
        window: &mut Window,
        cx: &App,
    ) -> Option<(Bounds<Pixels>, bool)> {
        let item = self.viewer.state.current().cloned()?;
        let geometry = self.viewer_geometry(&item, window, cx);
        let bounds = Bounds::new(
            point(px(geometry.origin.0), px(geometry.origin.1)),
            size(px(geometry.media.0), px(geometry.media.1)),
        );
        Some((bounds, geometry.caption.is_some()))
    }

    /// Lay out the media in the window: fit it between the bars and the
    /// shaped caption, and refit the zoom when the box changed (a resized
    /// window, another item). The media box, the chrome and the video
    /// layer all place the media here.
    pub(super) fn viewer_geometry(
        &mut self,
        item: &MediaViewerItem,
        window: &mut Window,
        cx: &App,
    ) -> ViewerGeometry {
        let viewport = window.viewport_size();
        let viewport = (f32::from(viewport.width), f32::from(viewport.height));
        let fullscreen = self.viewer.extra.video_fullscreen.is_on();
        let caption = if fullscreen {
            None
        } else {
            let width = geometry::lane_width(viewport.0);
            self.viewer_caption_layout(item, width, window, cx)
        };
        let frame = geometry::ViewerFrame {
            viewport,
            video: item.kind == MediaViewerKind::Video,
            caption: caption.map_or(0.0, |caption| caption.plate_h),
            fullscreen,
            strip: self.viewer_group_strip(viewport.0).is_some(),
        };
        let area = geometry::fit_area(frame);
        let fit = area.size;
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
        let levels = natural.and_then(|natural| ZoomLevels::new(natural, fit));
        if media != self.viewer.frame || levels != self.viewer.zoom.levels {
            self.viewer.frame = media;
            self.viewer.zoom = ViewerZoom::fitted(levels);
        }
        let zoom = self.viewer.zoom;
        ViewerGeometry {
            fit: area,
            caption,
            media,
            bottom_chrome: geometry::bottom_chrome(frame),
            origin: (
                area.origin.0 + (fit.0 - media.0) / 2.0,
                area.origin.1 + (fit.1 - media.1) / 2.0,
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
    /// it is four layers (see the module docs); clicking the backdrop
    /// closes it. The video layer takes no clicks, so the media box's
    /// handlers under it get them, and the chrome above takes clicks only
    /// on its controls.
    pub(in crate::ui) fn media_viewer_overlay(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some((media, chrome)) = self.viewer_slots() else {
            return self.media_viewer_panel(ViewerPart::Inline, window, cx);
        };
        let Some(item) = self.viewer.state.current().cloned() else {
            return Empty.into_any_element();
        };
        let geometry = self.viewer_geometry(&item, window, cx);
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
                    .bg(gpui_kit::black())
                    .on_mouse_move(cx.listener(|this, _: &MouseMoveEvent, _, cx| {
                        this.viewer_note_pointer(false, cx);
                    }))
                    .on_scroll(cx.listener(move |this, event: &ScrollEvent, _, cx| {
                        this.viewer_scroll_wheel(event, geometry.origin, cx);
                    }))
                    .on_pinch(cx.listener(move |this, event: &PinchEvent, _, cx| {
                        this.viewer_pinch(event, geometry.origin, cx);
                    }))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.viewer_backdrop_click(cx);
                    }))
                    // Fade in over 200 ms when it opens (tdesktop
                    // `mediaviewShowDuration`); closing is instant.
                    .with_animation(
                        ("media-viewer-fade-in", open_gen),
                        Animation::new(Duration::from_millis(VIEWER_SHOW_MS)),
                        |backdrop, t| backdrop.opacity(if still_frame() { 1.0 } else { t }),
                    ),
            )
            .child(media)
            .children(layer)
            .child(chrome)
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
            _ if !item.plays_clip() || self.viewer.video != Some(item.message_id) => {
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
                    waiting: WaitingGate::default(),
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
