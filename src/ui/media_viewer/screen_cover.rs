//! The part of the viewer's window that the Dock, the taskbar or a panel
//! hides (`quill::media_viewer::screen_cover`), read per platform:
//!
//! - macOS: the window's screen's `visibleFrame` (Dock and menu bar),
//!   through GPUI's `visible_bounds`, against the window's frame. Both are
//!   in points from the top-left of the window's own screen.
//! - Windows: the monitor's work area (`MONITORINFO::rcWork`, the
//!   taskbar and app bars left out), against the window's frame, both in
//!   the virtual desktop's logical pixels.
//! - X11: `_NET_WORKAREA` of the current desktop against the window's
//!   place on the root window, read from the server
//!   (`window_control::x11_frame_and_work_area`): GPUI's X11 bounds are
//!   relative to the window manager's frame and its displays report no
//!   work area.
//! - Wayland: nothing. A client can't learn where its window is; the
//!   compositor keeps panels clear of maximized windows itself.
//!
//! In OS full screen nothing covers the window: the Dock and taskbar only
//! slide in over it while the pointer asks for them, and the chrome's own
//! bottom margin keeps the controller off the screen's edge.
#![deny(clippy::indexing_slicing)]

use crate::ui::window_control::{self, Backend};
use gpui_kit::*;
use quill::media_viewer::{ScreenCover, ViewRect, screen_cover};
use std::cell::Cell;

fn view_rect(bounds: Bounds<Pixels>) -> ViewRect {
    ViewRect::new(
        bounds.origin.x.as_f32(),
        bounds.origin.y.as_f32(),
        bounds.size.width.as_f32(),
        bounds.size.height.as_f32(),
    )
}

/// The cover of `window` by the screen's furniture, in window points.
pub(in crate::ui) fn window_cover(window: &Window, cx: &App) -> ScreenCover {
    if window.is_fullscreen() {
        return ScreenCover::default();
    }
    #[cfg(feature = "demo-capture")]
    if let Some(cover) = demo_cover() {
        return cover;
    }
    match window_control::backend(window) {
        Backend::MacOS | Backend::Windows => window
            .display(cx)
            .map(|display| {
                screen_cover(
                    view_rect(window.bounds()),
                    view_rect(display.visible_bounds()),
                )
            })
            .unwrap_or_default(),
        Backend::X11 => x11_cover(window),
        Backend::Wayland | Backend::Unknown => ScreenCover::default(),
    }
}

thread_local! {
    /// The last X11 answer, keyed by the window and its GPUI bounds: the
    /// viewer lays out several times a frame and each read is four round
    /// trips. A move or resize changes the bounds and asks again.
    static X11_LAST: Cell<Option<(u64, [i32; 4], ScreenCover)>> = const { Cell::new(None) };
}

fn x11_cover(window: &Window) -> ScreenCover {
    let id = window.window_handle().window_id().as_u64();
    let bounds = window.bounds();
    let key = [
        bounds.origin.x.as_f32().round() as i32,
        bounds.origin.y.as_f32().round() as i32,
        bounds.size.width.as_f32().round() as i32,
        bounds.size.height.as_f32().round() as i32,
    ];
    if let Some((last_id, last_key, cover)) = X11_LAST.get()
        && last_id == id
        && last_key == key
    {
        return cover;
    }
    let scale = window.scale_factor();
    let cover = window_control::x11_frame_and_work_area(window)
        .filter(|_| scale.is_finite() && scale > 0.0)
        .map(|(frame, work)| {
            let rect = |[x, y, w, h]: [i32; 4]| {
                ViewRect::new(
                    x as f32 / scale,
                    y as f32 / scale,
                    w as f32 / scale,
                    h as f32 / scale,
                )
            };
            screen_cover(rect(frame), rect(work))
        })
        .unwrap_or_default();
    X11_LAST.set(Some((id, key, cover)));
    cover
}

/// Screenshot demos: `QUILL_DEMO_SCREEN_COVER=top,right,bottom,left` stands
/// in for a Dock or taskbar over the demo window.
#[cfg(feature = "demo-capture")]
fn demo_cover() -> Option<ScreenCover> {
    let value = std::env::var("QUILL_DEMO_SCREEN_COVER").ok()?;
    let edges: Vec<f32> = value
        .split(',')
        .map(|edge| edge.trim().parse().ok())
        .collect::<Option<_>>()?;
    match edges.as_slice() {
        [top, right, bottom, left] => Some(ScreenCover {
            top: *top,
            right: *right,
            bottom: *bottom,
            left: *left,
        }),
        _ => None,
    }
}
