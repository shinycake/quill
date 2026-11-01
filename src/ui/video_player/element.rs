//! `video_frame`: the element that draws a [`VideoPlayer`]'s picture.
//!
//! Rendering it is the pull: the element records the draw start, predicts
//! when this frame reaches the screen (on macOS from the display link's
//! next output time; the display's refresh period where GPUI reports one),
//! and takes the picture due then. After painting it
//! asks for the frame that shows the next picture
//! ([`request_video_frame`]), redrawing only the view it is in, and wakes
//! the player's watchers (the controls) at most every 100 ms.
//!
//! A surface (AVPlayer on macOS, NV12 on Windows) can't be rounded by GPUI, so corners
//! over a surface are cut by painting the background color over each
//! corner's outside (`inline_video::corner_mask`); images round themselves.
//!
//! A live player (`backend_live`) is redrawn by its feed when a frame
//! lands; the element only records the view it is drawn in and asks the
//! frame clock to notice a stall, and not at all while painted off
//! screen. `.paused_overlay(true)` draws the feed's blurred 90 px picture
//! while the sender paused; `.on_width(report)` reports the element's
//! width whenever it changes (the stream's quality bucket).
#![deny(clippy::indexing_slicing)]

use super::super::frame_clock::{VideoFrameAsk, request_video_frame};
use super::super::frame_grid::{self, ContentGrid, Source};
#[cfg(target_os = "macos")]
use super::super::frame_grid::{Vsync, display_grid};
use super::{Picture, PlayState, VideoPlayer};
use gpui_kit::*;
use std::rc::Rc;
use std::time::{Duration, Instant};

/// Called with the element's width in logical pixels when it changes.
pub(crate) type WidthReport = Rc<dyn Fn(f32, &mut App)>;

/// The picture of `player`, filling its parent.
pub(crate) fn video_frame(player: &VideoPlayer) -> VideoFrame {
    VideoFrame {
        player: player.clone(),
        fit: ObjectFit::Contain,
        radii: Corners::default(),
        behind: None,
        keep_alive: false,
        paused_overlay: false,
        on_width: None,
    }
}

#[derive(IntoElement)]
pub(crate) struct VideoFrame {
    player: VideoPlayer,
    fit: ObjectFit,
    radii: Corners<Pixels>,
    /// The colors behind each corner (top-left, top-right, bottom-right,
    /// bottom-left), painted over a surface's corner outsides.
    behind: Option<[Hsla; 4]>,
    keep_alive: bool,
    paused_overlay: bool,
    on_width: Option<WidthReport>,
}

impl VideoFrame {
    pub(crate) fn object_fit(mut self, fit: ObjectFit) -> Self {
        self.fit = fit;
        self
    }

    /// Round the corners; `behind` is the color under the element (a GPU
    /// surface is rounded by painting it over the corner outsides).
    pub(crate) fn corner_radii(self, radii: Corners<Pixels>, behind: Hsla) -> Self {
        self.corner_radii_over(radii, [behind; 4])
    }

    /// [`Self::corner_radii`] with a color of its own behind each corner
    /// (top-left, top-right, bottom-right, bottom-left): a tile in a
    /// bubble has the bubble behind some corners and the chat behind
    /// others.
    pub(crate) fn corner_radii_over(mut self, radii: Corners<Pixels>, behind: [Hsla; 4]) -> Self {
        self.radii = radii;
        self.behind = Some(behind);
        self
    }

    /// Keep playing while the window is not active (Picture-in-Picture).
    pub(crate) fn keep_alive(mut self, keep_alive: bool) -> Self {
        self.keep_alive = keep_alive;
        self
    }

    /// While a live sender paused, draw its last frame small and blurred
    /// over the picture, cropped to fill (tdesktop's paused tile; the
    /// caller adds the shade and the pause icon).
    pub(crate) fn paused_overlay(mut self, paused_overlay: bool) -> Self {
        self.paused_overlay = paused_overlay;
        self
    }

    /// Report the element's width on every paint where it changed.
    pub(crate) fn on_width(mut self, report: WidthReport) -> Self {
        self.on_width = Some(report);
        self
    }
}

impl RenderOnce for VideoFrame {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let now = Instant::now();
        let (hint, period, grid) = present_hint(window, cx, now);
        self.player.set_refresh_period(period);
        // On the display's grid only while something else animates there.
        let source = Source::Clip(self.player.id());
        let display = grid.map(|(display, _)| display);
        let shared = display.is_some_and(|display| frame_grid::shared(display, source, now));
        self.player
            .set_content_grid(grid.map(|(_, grid)| grid), shared);
        let picture = self.player.pull(now, hint);
        let target = window.current_view();
        self.player.note_live_view(target);
        let surface = picture.as_ref().is_some_and(Picture::is_surface);
        let picture = picture.map(|picture| picture_element(picture, self.fit, self.radii));
        let paused = (self.paused_overlay && self.player.state() == PlayState::Paused)
            .then(|| self.player.paused_picture())
            .flatten()
            .map(|image| {
                img(ImageSource::Render(image))
                    .debug_selector(|| "live-paused-picture".into())
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .object_fit(ObjectFit::Cover)
                    .rounded_tl(self.radii.top_left)
                    .rounded_tr(self.radii.top_right)
                    .rounded_br(self.radii.bottom_right)
                    .rounded_bl(self.radii.bottom_left)
            });
        let player = self.player;
        let keep_alive = self.keep_alive;
        let radii = self.radii;
        let mask = self.behind.filter(|_| surface && has_radius(radii));
        let on_width = self.on_width;
        let pump = canvas(
            |_, _, _| {},
            move |bounds, (), window, cx| {
                if let Some(colors) = mask {
                    paint_corner_mask(bounds, radii, colors, window);
                }
                if let Some(report) = on_width {
                    let width = f32::from(bounds.size.width);
                    let last = player.reported_width();
                    if (width - last.get()).abs() >= 0.5 {
                        last.set(width);
                        // Not while painting: the owner updates its state.
                        window.on_next_frame(move |_, cx| report(width, cx));
                    }
                }
                let visible = is_visible(bounds, window.content_mask().bounds);
                if visible {
                    player.note_shown();
                }
                player.note_live_paint(visible);
                // An inline tile's row, painted under the copy the
                // conversation's animation layer draws: the layer asks for
                // the frames, so the conversation is not redrawn for them.
                if super::super::anim_layer::painting_under_layer() {
                    return;
                }
                let now = Instant::now();
                let demand = player.frame_demand(now);
                // A live picture off screen asks nothing: its feed stops
                // redrawing it too.
                if demand.animates && (visible || !player.is_live()) {
                    if let Some(display) = display {
                        let next = demand.next_due.unwrap_or(now);
                        frame_grid::note_playing(display, source, now, next);
                    }
                    request_video_frame(
                        target,
                        VideoFrameAsk {
                            next_due: demand.next_due,
                            interval: demand.interval,
                            lead: demand.lead,
                            even_spacing: demand.even_spacing,
                            plays_sound: demand.plays_sound,
                            keep_alive,
                            requested: demand.requested,
                        },
                        window,
                        cx,
                    );
                }
                let watchers = player.due_watchers(now);
                if !watchers.is_empty() {
                    // Notifying while painting is ignored; the next frame
                    // redraws them.
                    window.on_next_frame(move |_, cx| {
                        for id in watchers {
                            cx.notify(id);
                        }
                    });
                }
            },
        )
        .absolute()
        .top_0()
        .left_0()
        .size_full();
        div()
            .relative()
            .size_full()
            .overflow_hidden()
            .children(picture)
            .children(paused)
            .child(pump)
    }
}

/// Whether any of `bounds` shows inside the visible area `mask`.
pub(super) fn is_visible(bounds: Bounds<Pixels>, mask: Bounds<Pixels>) -> bool {
    let visible = bounds.intersect(&mask);
    visible.size.width > Pixels::ZERO && visible.size.height > Pixels::ZERO
}

fn has_radius(radii: Corners<Pixels>) -> bool {
    [
        radii.top_left,
        radii.top_right,
        radii.bottom_right,
        radii.bottom_left,
    ]
    .iter()
    .any(|radius| *radius > px(0.0))
}

fn picture_element(picture: Picture, fit: ObjectFit, radii: Corners<Pixels>) -> AnyElement {
    match picture {
        Picture::Surface(buffer) => gpui_kit::surface(buffer)
            .size_full()
            .object_fit(fit)
            .into_any_element(),
        Picture::Image(image) => image_element(image, fit, radii),
    }
}

fn image_element(
    image: std::sync::Arc<RenderImage>,
    fit: ObjectFit,
    radii: Corners<Pixels>,
) -> AnyElement {
    img(ImageSource::Render(image))
        .size_full()
        .object_fit(fit)
        .rounded_tl(radii.top_left)
        .rounded_tr(radii.top_right)
        .rounded_br(radii.bottom_right)
        .rounded_bl(radii.bottom_left)
        .into_any_element()
}

/// Paint `colors` over the outsides of the rounded corners.
fn paint_corner_mask(
    bounds: Bounds<Pixels>,
    radii: Corners<Pixels>,
    colors: [Hsla; 4],
    window: &mut Window,
) {
    let points = |value: Pixels| f32::from(value).round().max(0.0) as u32;
    let tile = (points(bounds.size.width), points(bounds.size.height));
    if tile.0 == 0 || tile.1 == 0 {
        return;
    }
    let radii = [
        radii.top_left,
        radii.top_right,
        radii.bottom_right,
        radii.bottom_left,
    ];
    for (corner, (radius, color)) in radii.into_iter().zip(colors).enumerate() {
        let radius = points(radius);
        if radius == 0 {
            continue;
        }
        let (w, h) = super::super::inline_video::corner_mask_size(radius, tile);
        let extent = size(px(w as f32 / 2.0), px(h as f32 / 2.0));
        let x = match corner {
            0 | 3 => bounds.left(),
            _ => bounds.right() - extent.width,
        };
        let y = match corner {
            0 | 1 => bounds.top(),
            _ => bounds.bottom() - extent.height,
        };
        let at = Bounds::new(point(x, y), extent);
        let mask = super::super::inline_video::corner_mask(corner, radius, tile, color);
        // A failed paint leaves a square corner; nothing else depends on it.
        let _ = window.paint_image(at, at, Corners::default(), mask, 0, false);
    }
}

/// What [`present_hint`] reports: the next present time (with its host
/// ticks), the refresh period, and the display with its content grid.
type Hint = (
    Option<(Instant, u64)>,
    Option<Duration>,
    Option<(u64, ContentGrid)>,
);

/// The display link's next present time (with its host ticks) and refresh
/// period for this window's display, and the display's content grid
/// (`frame_grid`); without the link's period, the one GPUI reports for the
/// window's display.
#[cfg(target_os = "macos")]
fn present_hint(window: &Window, cx: &App, now: Instant) -> Hint {
    let Some(display) = window.display(cx) else {
        return (None, window.refresh_interval(cx), None);
    };
    let display = u64::from(display.id());
    let (hint, period) = gpui_macos::present_timing(display);
    let grid = hint.zip(period).and_then(|((at, _), period)| {
        Some((display, display_grid(display, Vsync { at, period }, now)?))
    });
    (hint, period.or_else(|| window.refresh_interval(cx)), grid)
}

/// No present time; the refresh period GPUI reports for the window's
/// display (Windows and Wayland; `None` on X11, where the interval is
/// learned from draw spacing). GPUI caches it per display, so this costs
/// nothing per draw. Without vsync times there is no content grid.
#[cfg(not(target_os = "macos"))]
fn present_hint(window: &Window, cx: &App, _: Instant) -> Hint {
    (None, window.refresh_interval(cx), None)
}
