//! Window control GPUI lacks on Linux and Windows: hide a window after it
//! was created, keep it above the others, keep it off the taskbar
//! (docs/decisions/codex-gpui-window-controls.md).
//!
//! GPUI's `PlatformWindow` trait lives in the `gpui-pre` crate from the
//! registry, which Quill does not vendor, so these are not trait methods.
//! They work on the native handle every GPUI window hands out through
//! `raw-window-handle`:
//!
//! - Windows: `ShowWindow`, `SetWindowPos(HWND_TOPMOST)`, `ITaskbarList`.
//! - X11: unmap/map plus the ICCCM withdraw notice, `_NET_WM_STATE` with
//!   `_NET_WM_STATE_ABOVE` and `_NET_WM_STATE_SKIP_TASKBAR`, through a
//!   short-lived `x11rb` connection of its own (requests on another
//!   client's window are ordinary X; the window manager acts on them).
//! - Wayland: nothing. xdg-shell has no hide, no stacking order and no
//!   taskbar flag for a toplevel; only the compositor decides those.
//!   [`hide_supported`] says so and the close-to-tray setting minimizes
//!   instead.
//! - macOS: `orderOut` / `makeKeyAndOrderFront`, `setLevel`, and the
//!   collection behavior for the Dock cycle, for symmetry.
//!
//! The vendored Windows platform parks a hidden window's frames
//! (`third_party/gpui-pre-windows/src/events.rs`); on X11 an unmapped
//! window stops drawing and the idle timer parks it on its own.

use gpui_kit::*;
#[allow(unused_imports)]
use raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle};

/// The windowing system behind a GPUI window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Backend {
    Windows,
    X11,
    Wayland,
    MacOS,
    Unknown,
}

pub(crate) fn backend(window: &Window) -> Backend {
    match HasWindowHandle::window_handle(window).map(|handle| handle.as_raw()) {
        Ok(RawWindowHandle::Win32(_)) => Backend::Windows,
        Ok(RawWindowHandle::Xcb(_) | RawWindowHandle::Xlib(_)) => Backend::X11,
        Ok(RawWindowHandle::Wayland(_)) => Backend::Wayland,
        Ok(RawWindowHandle::AppKit(_)) => Backend::MacOS,
        _ => Backend::Unknown,
    }
}

/// Whether [`set_visible`] can hide this window and bring it back.
pub(crate) fn hide_supported(window: &Window) -> bool {
    backend_hides(backend(window))
}

/// Pure half of [`hide_supported`].
pub(crate) fn backend_hides(backend: Backend) -> bool {
    matches!(backend, Backend::Windows | Backend::X11 | Backend::MacOS)
}

/// Whether [`set_skip_taskbar`] means anything here (there is no per-window
/// taskbar entry on macOS, and Wayland offers no flag).
pub(crate) fn taskbar_toggle_supported(window: &Window) -> bool {
    matches!(backend(window), Backend::Windows | Backend::X11)
}

/// Hide the window (it leaves the taskbar and the task switcher) or show it
/// again, restored and in front. Returns whether the backend could.
pub(crate) fn set_visible(window: &Window, visible: bool) -> bool {
    platform::set_visible(window, visible)
}

/// Keep the window above every other window, or stop.
pub(crate) fn set_always_on_top(window: &Window, on: bool) -> bool {
    platform::set_always_on_top(window, on)
}

/// Leave the window out of the taskbar (tdesktop's "Show taskbar icon" off),
/// or put it back.
pub(crate) fn set_skip_taskbar(window: &Window, skip: bool) -> bool {
    platform::set_skip_taskbar(window, skip)
}

/// X11 `_NET_WM_STATE` client message data: `_NET_WM_STATE_REMOVE` (0) or
/// `_NET_WM_STATE_ADD` (1), the state atom, no second atom, source 1
/// (a normal application).
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) fn net_wm_state_data(on: bool, state_atom: u32) -> [u32; 5] {
    [u32::from(on), state_atom, 0, 1, 0]
}

/// The `_NET_WM_STATE` property of an unmapped window with `state_atom`
/// added or removed (a client message only reaches mapped windows).
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) fn net_wm_state_property(current: &[u32], on: bool, state_atom: u32) -> Vec<u32> {
    let mut atoms: Vec<u32> = current
        .iter()
        .copied()
        .filter(|atom| *atom != state_atom)
        .collect();
    if on {
        atoms.push(state_atom);
    }
    atoms
}

#[cfg(target_os = "windows")]
mod platform {
    use super::*;
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        HWND_NOTOPMOST, HWND_TOPMOST, IsIconic, IsWindowVisible, SW_HIDE, SW_RESTORE, SW_SHOW,
        SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SetForegroundWindow, SetWindowPos, ShowWindow,
    };

    fn hwnd(window: &Window) -> Option<HWND> {
        match HasWindowHandle::window_handle(window).ok()?.as_raw() {
            RawWindowHandle::Win32(handle) => Some(handle.hwnd.get() as HWND),
            _ => None,
        }
    }

    pub(super) fn set_visible(window: &Window, visible: bool) -> bool {
        let Some(hwnd) = hwnd(window) else {
            return false;
        };
        // SAFETY: the handle belongs to a live GPUI window for the duration
        // of this call; these are plain user32 calls on it.
        unsafe {
            if visible {
                if IsIconic(hwnd) != 0 {
                    ShowWindow(hwnd, SW_RESTORE);
                } else {
                    ShowWindow(hwnd, SW_SHOW);
                }
                SetForegroundWindow(hwnd);
            } else if IsWindowVisible(hwnd) != 0 {
                ShowWindow(hwnd, SW_HIDE);
            }
        }
        true
    }

    pub(super) fn set_always_on_top(window: &Window, on: bool) -> bool {
        let Some(hwnd) = hwnd(window) else {
            return false;
        };
        let after = if on { HWND_TOPMOST } else { HWND_NOTOPMOST };
        // SAFETY: as above; only the z-order changes.
        unsafe {
            SetWindowPos(
                hwnd,
                after,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            ) != 0
        }
    }

    pub(super) fn set_skip_taskbar(window: &Window, skip: bool) -> bool {
        use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance};
        use windows::Win32::UI::Shell::{ITaskbarList, TaskbarList};
        let Some(hwnd) = hwnd(window) else {
            return false;
        };
        let hwnd = windows::Win32::Foundation::HWND(hwnd as *mut _);
        // SAFETY: COM is initialized on the UI thread by GPUI; the taskbar
        // list is a plain in-process COM object.
        unsafe {
            let Ok(list) =
                CoCreateInstance::<_, ITaskbarList>(&TaskbarList, None, CLSCTX_INPROC_SERVER)
            else {
                return false;
            };
            if list.HrInit().is_err() {
                return false;
            }
            let done = if skip {
                list.DeleteTab(hwnd)
            } else {
                list.AddTab(hwnd)
            };
            done.is_ok()
        }
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use super::*;
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::{
        AtomEnum, ClientMessageEvent, ConnectionExt as _, EventMask, MapState, PropMode,
        UnmapNotifyEvent,
    };
    use x11rb::rust_connection::RustConnection;
    use x11rb::wrapper::ConnectionExt as _;

    fn x_window(window: &Window) -> Option<u32> {
        match HasWindowHandle::window_handle(window).ok()?.as_raw() {
            RawWindowHandle::Xcb(handle) => Some(handle.window.get()),
            RawWindowHandle::Xlib(handle) => u32::try_from(handle.window).ok(),
            _ => None,
        }
    }

    /// A connection of our own: GPUI's lives inside its client and the
    /// window manager answers requests from any client alike.
    fn connect() -> Option<(RustConnection, u32)> {
        let (conn, screen) = x11rb::connect(None).ok()?;
        let root = conn.setup().roots.get(screen)?.root;
        Some((conn, root))
    }

    fn atom(conn: &RustConnection, name: &str) -> Option<u32> {
        Some(
            conn.intern_atom(false, name.as_bytes())
                .ok()?
                .reply()
                .ok()?
                .atom,
        )
    }

    fn is_mapped(conn: &RustConnection, x_window: u32) -> bool {
        conn.get_window_attributes(x_window)
            .ok()
            .and_then(|cookie| cookie.reply().ok())
            .is_some_and(|attributes| attributes.map_state == MapState::VIEWABLE)
    }

    pub(super) fn set_visible(window: &Window, visible: bool) -> bool {
        let (Some(x_window), Some((conn, root))) = (x_window(window), connect()) else {
            return false;
        };
        let ok = if visible {
            let mapped = conn.map_window(x_window).is_ok();
            // `_NET_ACTIVE_WINDOW` asks the window manager to raise and
            // focus it (source 1: a normal application).
            if let Some(active) = atom(&conn, "_NET_ACTIVE_WINDOW") {
                let message = ClientMessageEvent::new(32, x_window, active, [1, 0, 0, 0, 0]);
                let _ = conn.send_event(
                    false,
                    root,
                    EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
                    message,
                );
            }
            mapped
        } else {
            // ICCCM 4.1.4: unmap, then tell the window manager with a
            // synthetic UnmapNotify so the window is withdrawn (out of the
            // taskbar and the switcher), not merely iconified.
            let unmapped = conn.unmap_window(x_window).is_ok();
            let notice = UnmapNotifyEvent {
                response_type: x11rb::protocol::xproto::UNMAP_NOTIFY_EVENT,
                sequence: 0,
                event: root,
                window: x_window,
                from_configure: false,
            };
            let _ = conn.send_event(
                false,
                root,
                EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
                notice,
            );
            unmapped
        };
        let _ = conn.flush();
        ok
    }

    fn set_net_wm_state(window: &Window, on: bool, name: &str) -> bool {
        let (Some(x_window), Some((conn, root))) = (x_window(window), connect()) else {
            return false;
        };
        let (Some(state), Some(value)) = (atom(&conn, "_NET_WM_STATE"), atom(&conn, name)) else {
            return false;
        };
        let ok = if is_mapped(&conn, x_window) {
            let message =
                ClientMessageEvent::new(32, x_window, state, net_wm_state_data(on, value));
            conn.send_event(
                false,
                root,
                EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
                message,
            )
            .is_ok()
        } else {
            // The window manager reads the property when the window maps.
            let current: Vec<u32> = conn
                .get_property(false, x_window, state, AtomEnum::ATOM, 0, 64)
                .ok()
                .and_then(|cookie| cookie.reply().ok())
                .and_then(|reply| reply.value32().map(|atoms| atoms.collect()))
                .unwrap_or_default();
            let atoms = net_wm_state_property(&current, on, value);
            conn.change_property32(PropMode::REPLACE, x_window, state, AtomEnum::ATOM, &atoms)
                .is_ok()
        };
        let _ = conn.flush();
        ok
    }

    pub(super) fn set_always_on_top(window: &Window, on: bool) -> bool {
        match backend(window) {
            Backend::X11 => set_net_wm_state(window, on, "_NET_WM_STATE_ABOVE"),
            _ => false,
        }
    }

    pub(super) fn set_skip_taskbar(window: &Window, skip: bool) -> bool {
        match backend(window) {
            Backend::X11 => set_net_wm_state(window, skip, "_NET_WM_STATE_SKIP_TASKBAR"),
            _ => false,
        }
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use super::*;
    use objc2_app_kit::{NSView, NSWindow, NSWindowCollectionBehavior};
    use objc2_foundation::NSInteger;

    const NS_NORMAL_WINDOW_LEVEL: NSInteger = 0;
    const NS_FLOATING_WINDOW_LEVEL: NSInteger = 3;

    fn with_window(window: &Window, f: impl FnOnce(&NSWindow)) -> bool {
        let Ok(handle) = HasWindowHandle::window_handle(window) else {
            return false;
        };
        let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
            return false;
        };
        // SAFETY: GPUI owns this NSView for the lifetime of the window and
        // the handle is read inside a window callback.
        let view = unsafe { handle.ns_view.cast::<NSView>().as_ref() };
        match view.window() {
            Some(native) => {
                f(&native);
                true
            }
            None => false,
        }
    }

    pub(super) fn set_visible(window: &Window, visible: bool) -> bool {
        with_window(window, |native| {
            if visible {
                if native.isMiniaturized() {
                    native.deminiaturize(None);
                }
                native.makeKeyAndOrderFront(None);
            } else {
                native.orderOut(None);
            }
        })
    }

    pub(super) fn set_always_on_top(window: &Window, on: bool) -> bool {
        with_window(window, |native| {
            native.setLevel(if on {
                NS_FLOATING_WINDOW_LEVEL
            } else {
                NS_NORMAL_WINDOW_LEVEL
            });
        })
    }

    /// No per-window Dock entry exists; the window leaves the Cmd+` cycle
    /// and Mission Control instead.
    pub(super) fn set_skip_taskbar(window: &Window, skip: bool) -> bool {
        with_window(window, |native| {
            let mut behavior = native.collectionBehavior();
            let skipped = NSWindowCollectionBehavior::IgnoresCycle;
            if skip {
                behavior |= skipped;
            } else {
                behavior &= !skipped;
            }
            native.setCollectionBehavior(behavior);
        })
    }
}

#[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
mod platform {
    use super::*;

    pub(super) fn set_visible(_window: &Window, _visible: bool) -> bool {
        false
    }
    pub(super) fn set_always_on_top(_window: &Window, _on: bool) -> bool {
        false
    }
    pub(super) fn set_skip_taskbar(_window: &Window, _skip: bool) -> bool {
        false
    }
}

/// tdesktop's "pin on top" control of the call windows
/// (`Calls::Panel::createPinOnTop`): a small icon in the window's corner
/// that toggles [`set_always_on_top`].
pub(crate) fn pin_on_top_button<V: 'static>(
    id: &'static str,
    pinned: bool,
    cx: &mut Context<V>,
    on_toggle: impl Fn(&mut V, bool, &mut Window, &mut Context<V>) + 'static,
) -> gpui_kit::Stateful<Div> {
    use gpui_kit::component::{Icon, Sizable};
    div()
        .id(id)
        .absolute()
        .top(px(8.))
        .right(px(8.))
        .w(px(32.))
        .h(px(32.))
        .rounded(px(8.))
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .hover(|style| style.bg(hsla(0., 0., 1., 0.12)))
        .on_click(cx.listener(move |this, _, window, cx| {
            let next = !pinned;
            if set_always_on_top(window, next) {
                on_toggle(this, next, window, cx);
            }
        }))
        .child(
            Icon::new(if pinned {
                gpui_kit::assets::IconName::PinOff
            } else {
                gpui_kit::assets::IconName::Pin
            })
            .with_size(px(16.)),
        )
}

#[cfg(test)]
mod tests {
    use super::{Backend, backend_hides, net_wm_state_data, net_wm_state_property};

    #[test]
    fn wayland_is_the_backend_that_cannot_hide() {
        assert!(backend_hides(Backend::Windows));
        assert!(backend_hides(Backend::X11));
        assert!(backend_hides(Backend::MacOS));
        assert!(!backend_hides(Backend::Wayland));
        assert!(!backend_hides(Backend::Unknown));
    }

    #[test]
    fn net_wm_state_message_adds_or_removes_one_atom() {
        assert_eq!(net_wm_state_data(true, 42), [1, 42, 0, 1, 0]);
        assert_eq!(net_wm_state_data(false, 42), [0, 42, 0, 1, 0]);
    }

    #[test]
    fn net_wm_state_property_keeps_other_atoms_and_never_duplicates() {
        assert_eq!(net_wm_state_property(&[7, 42], true, 42), vec![7, 42]);
        assert_eq!(net_wm_state_property(&[7], true, 42), vec![7, 42]);
        assert_eq!(net_wm_state_property(&[7, 42, 9], false, 42), vec![7, 9]);
        assert_eq!(net_wm_state_property(&[], false, 42), Vec::<u32>::new());
    }
}
