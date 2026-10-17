//! The round arrow a horizontal trackpad swipe pulls in from the edge of
//! the viewer (tdesktop `Ui::Controls::SetupSwipeBack`). It fades in and
//! slides from outside the edge to half in as the swipe nears the
//! threshold, and bounces once when the swipe is armed.
#![deny(clippy::indexing_slicing)]

use super::geometry::FitArea;
use super::*;
use quill::media_viewer::{SwipeDir, SwipeIndicator};

/// tdesktop's 150 px `swipeBackSize` box less a quarter on each side.
/// Its fill is a dark grey rather than tdesktop's translucent black,
/// which would vanish on Quill's black backdrop.
const CIRCLE: f32 = 75.0;
/// The share of the circle past the edge: just outside at the start
/// (tdesktop `kMaxOuterOffset`), half in once armed (`kMaxInnerOffset`).
const HIDDEN_START: f32 = 1.1;
const HIDDEN_ARMED: f32 = 0.5;
/// The armed bounce: up to a quarter larger and back (tdesktop
/// `kBouncePart` of its 500 ms reach animation).
const BOUNCE_MS: u64 = 250;
const BOUNCE_SCALE: f32 = 0.25;

/// The swipe arrow for `indicator`, vertically centred on the picture's
/// fit area like the prev/next arrows. It takes no pointer events.
pub(super) fn swipe_indicator(
    indicator: SwipeIndicator,
    fit: FitArea,
    viewport_w: f32,
) -> AnyElement {
    let SwipeIndicator {
        dir,
        ratio,
        reached,
    } = indicator;
    let hidden = CIRCLE * (HIDDEN_START + (HIDDEN_ARMED - HIDDEN_START) * ratio);
    let (id, icon, left) = match dir {
        SwipeDir::Previous => (
            "media-viewer-swipe-prev",
            gpui_kit::assets::IconName::ChevronLeft,
            -hidden,
        ),
        SwipeDir::Next => (
            "media-viewer-swipe-next",
            gpui_kit::assets::IconName::ChevronRight,
            viewport_w - CIRCLE + hidden,
        ),
    };
    let top = fit.origin.1 + fit.size.1 / 2.0 - CIRCLE / 2.0;
    // The arrow sits in the half of the circle that is in view.
    let circle = div()
        .debug_selector(move || id.into())
        .absolute()
        .left(px(left))
        .top(px(top))
        .size(px(CIRCLE))
        .rounded_full()
        .bg(hsla(0., 0., 0.2, 0.85))
        .opacity(ratio)
        .flex()
        .items_center()
        .justify_center()
        .map(|circle| match dir {
            SwipeDir::Previous => circle.pl(px(CIRCLE / 2.0)),
            SwipeDir::Next => circle.pr(px(CIRCLE / 2.0)),
        })
        .child(Icon::new(icon).size(px(24.)).text_color(gpui_kit::white()));
    if !reached {
        return circle.into_any_element();
    }
    circle
        .with_animation(
            ("media-viewer-swipe-reach", dir as usize),
            Animation::new(Duration::from_millis(BOUNCE_MS)),
            move |circle, t| {
                let t = if still_frame() { 1.0 } else { t };
                let grow = CIRCLE * BOUNCE_SCALE * (1.0 - (2.0 * t - 1.0).abs());
                circle
                    .size(px(CIRCLE + grow))
                    .left(px(left - grow / 2.0))
                    .top(px(top - grow / 2.0))
            },
        )
        .into_any_element()
}
