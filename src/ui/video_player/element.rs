//! `video_frame`: the element that draws a [`VideoPlayer`]'s picture.
//!
//! Rendering it is the pull: the element records the draw start, predicts
//! when this frame reaches the screen (on macOS from the display link's
//! next output time), and takes the picture due then. After painting it
//! asks for the frame that shows the next picture
//! ([`request_video_frame`]), redrawing only the view it is in, and wakes
//! the player's watchers (the controls) at most every 100 ms.
//!
//! A GPU surface (AVPlayer) can't be clipped by GPUI, so rounded corners
//! over a surface are cut by painting the background color over the
//! corner outsides (`inline_video::corner_mask`); images round themselves.
#![deny(clippy::indexing_slicing)]

use super::super::frame_clock::{VideoFrameAsk, request_video_frame};
use super::{Picture, VideoPlayer};
use gpui_kit::*;
use std::time::{Duration, Instant};

/// The picture of `player`, filling its parent.
pub(crate) fn video_frame(player: &VideoPlayer) -> VideoFrame {
    VideoFrame {
        player: player.clone(),
        fit: ObjectFit::Contain,
        radii: Corners::default(),
        behind: None,
        keep_alive: false,
    }
}

#[derive(IntoElement)]
pub(crate) struct VideoFrame {
    player: VideoPlayer,
    fit: ObjectFit,
    radii: Corners<Pixels>,
    /// The color behind the element, painted over a surface's corners.
    behind: Option<Hsla>,
    keep_alive: bool,
}

impl VideoFrame {
    pub(crate) fn object_fit(mut self, fit: ObjectFit) -> Self {
        self.fit = fit;
        self
    }

    /// Round the corners; `behind` is the color under the element (a GPU
    /// surface is rounded by painting it over the corner outsides).
    #[allow(dead_code)]
    pub(crate) fn corner_radii(mut self, radii: Corners<Pixels>, behind: Hsla) -> Self {
        self.radii = radii;
        self.behind = Some(behind);
        self
    }

    /// Keep playing while the window is not active (Picture-in-Picture).
    pub(crate) fn keep_alive(mut self, keep_alive: bool) -> Self {
        self.keep_alive = keep_alive;
        self
    }
}

impl RenderOnce for VideoFrame {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let now = Instant::now();
        let (hint, period) = present_hint(window, cx);
        self.player.set_refresh_period(period);
        let picture = self.player.pull(now, hint);
        let target = window.current_view();
        let surface = picture_is_surface(picture.as_ref());
        let picture = picture.map(|picture| picture_element(picture, self.fit, self.radii));
        let player = self.player;
        let keep_alive = self.keep_alive;
        let radii = self.radii;
        let mask = self.behind.filter(|_| surface && has_radius(radii));
        let pump = canvas(
            |_, _, _| {},
            move |bounds, (), window, cx| {
                if let Some(color) = mask {
                    paint_corner_mask(bounds, radii, color, window);
                }
                let now = Instant::now();
                let demand = player.frame_demand(now);
                if demand.animates {
                    request_video_frame(
                        target,
                        VideoFrameAsk {
                            next_due: demand.next_due,
                            interval: demand.interval,
                            lead: demand.lead,
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
            .child(pump)
    }
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

fn picture_is_surface(picture: Option<&Picture>) -> bool {
    match picture {
        #[cfg(target_os = "macos")]
        Some(Picture::Surface(_)) => true,
        _ => false,
    }
}

fn picture_element(picture: Picture, fit: ObjectFit, radii: Corners<Pixels>) -> AnyElement {
    match picture {
        #[cfg(target_os = "macos")]
        Picture::Surface(buffer) => gpui_kit::surface(buffer)
            .size_full()
            .object_fit(fit)
            .into_any_element(),
        Picture::Image(image) => img(ImageSource::Render(image))
            .size_full()
            .object_fit(fit)
            .rounded_tl(radii.top_left)
            .rounded_tr(radii.top_right)
            .rounded_br(radii.bottom_right)
            .rounded_bl(radii.bottom_left)
            .into_any_element(),
    }
}

/// Paint `color` over the outsides of the rounded corners.
#[cfg(target_os = "macos")]
fn paint_corner_mask(
    bounds: Bounds<Pixels>,
    radii: Corners<Pixels>,
    color: Hsla,
    window: &mut Window,
) {
    let points = |value: Pixels| f32::from(value).round().max(0.0) as u32;
    let width = points(bounds.size.width);
    let height = points(bounds.size.height);
    if width == 0 || height == 0 {
        return;
    }
    let mask = super::super::inline_video::corner_mask(
        width,
        height,
        [
            points(radii.top_left),
            points(radii.top_right),
            points(radii.bottom_right),
            points(radii.bottom_left),
        ],
        [color; 4],
    );
    // A failed paint leaves square corners; nothing else depends on it.
    let _ = window.paint_image(bounds, bounds, Corners::default(), mask, 0, false);
}

#[cfg(not(target_os = "macos"))]
fn paint_corner_mask(_: Bounds<Pixels>, _: Corners<Pixels>, _: Hsla, _: &mut Window) {}

/// The display link's next present time (with its host ticks) and refresh
/// period for this window's display.
#[cfg(target_os = "macos")]
fn present_hint(window: &Window, cx: &App) -> (Option<(Instant, u64)>, Option<Duration>) {
    let Some(display) = window.display(cx) else {
        return (None, None);
    };
    let id = u64::from(display.id());
    (
        gpui_macos::next_present_time(id),
        gpui_macos::refresh_period(id),
    )
}

#[cfg(not(target_os = "macos"))]
fn present_hint(_: &Window, _: &App) -> (Option<(Instant, u64)>, Option<Duration>) {
    (None, None)
}
