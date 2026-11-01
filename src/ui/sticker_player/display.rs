//! The display each window shows on, as its sticker views need it: noted as
//! the window starts a frame, before anything in it renders.
#![deny(clippy::indexing_slicing)]

use gpui_kit::*;
use quill::ids::ChatId;
use smallvec::SmallVec;
use std::cell::RefCell;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::ui) struct WindowDisplay {
    pub(in crate::ui) scale: f32,
    pub(in crate::ui) hz: f64,
    pub(in crate::ui) id: Option<u64>,
}

impl WindowDisplay {
    const UNKNOWN: Self = Self {
        scale: 2.0,
        hz: 60.0,
        id: None,
    };
}

thread_local! {
    static DISPLAYS: RefCell<SmallVec<[(Option<ChatId>, WindowDisplay); 4]>> =
        const { RefCell::new(SmallVec::new_const()) };
}

/// The display `window` (`None`: the main window) shows on.
pub(in crate::ui) fn display_of(window: Option<ChatId>) -> WindowDisplay {
    DISPLAYS.with(|displays| {
        displays
            .borrow()
            .iter()
            .find(|(of, _)| *of == window)
            .map_or(WindowDisplay::UNKNOWN, |(_, display)| *display)
    })
}

pub(in crate::ui) fn set_display(window: Option<ChatId>, display: WindowDisplay) {
    DISPLAYS.with(|displays| {
        let mut displays = displays.borrow_mut();
        match displays.iter_mut().find(|(of, _)| *of == window) {
            Some((_, known)) => *known = display,
            None => displays.push((window, display)),
        }
    });
}

/// `surface`, the window of `window`, starts a frame on the display `id`
/// names, if known.
pub(in crate::ui) fn note_display(
    window: Option<ChatId>,
    id: Option<u64>,
    surface: &Window,
    cx: &App,
) {
    let hz = surface
        .refresh_interval(cx)
        .map(|interval| 1.0 / interval.as_secs_f64())
        .filter(|hz| hz.is_finite() && *hz > 0.0)
        .unwrap_or(60.0);
    let scale = surface.scale_factor();
    set_display(window, WindowDisplay { scale, hz, id });
}

pub(super) fn forget_display(window: Option<ChatId>) {
    DISPLAYS.with(|displays| displays.borrow_mut().retain(|(of, _)| *of != window));
}
