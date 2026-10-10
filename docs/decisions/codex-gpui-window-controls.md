# Window control on Linux and Windows: hide, always on top, taskbar

Three things GPUI could not do after a window exists: hide it, keep it above the others, and keep it off the taskbar. Telegram Desktop uses all three: "Run in the background" hides the main window to the tray, "Show taskbar icon" off keeps it out of the taskbar, picture-in-picture carries `Qt::WindowStaysOnTopHint`, and the call windows have a pin-on-top button (`Calls::Window::setPinnedOnTop`, `Calls::Panel::createPinOnTop`). Earlier notes said why Quill fell short (`codex-platform-shortcuts-data.md`: "Can GPUI hide a window on Linux and Windows?", `codex-quality-state-hygiene.md`: the Windows PiP gap, `codex-calls-live.md`: "Window options ... not claimed").

## Where the methods live

The `PlatformWindow` trait is in `gpui-pre` from the registry, which Quill does not vendor (only the three platform crates are), so there are no new trait methods and `interface_zoom.rs` forwards exactly what the trait has; its `forwards_every_trait_method` test is unchanged. The operations are free functions in `src/ui/window_control.rs` that take a `&Window` and work on the native handle every GPUI window exposes through `raw-window-handle`:

- `set_visible(window, bool)`, `set_always_on_top(window, bool)`, `set_skip_taskbar(window, bool)`, each returning whether the backend could;
- `hide_supported(window)` and `taskbar_toggle_supported(window)` for the callers that must know;
- `backend(window)`: Windows, X11, Wayland, macOS, from the handle kind.

Per platform:

- Windows: `ShowWindow(SW_HIDE)` / `SW_SHOW` (or `SW_RESTORE` when iconic) plus `SetForegroundWindow`; `SetWindowPos` with `HWND_TOPMOST` / `HWND_NOTOPMOST` and `SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE`; `ITaskbarList::DeleteTab` / `AddTab` (the `windows` crate, `Win32_UI_Shell`, already linked for the badge overlay). `WS_EX_TOOLWINDOW` was the alternative; it needs the window hidden and shown again to take effect and changes its frame, so the taskbar list it is.
- X11: a short-lived `x11rb` connection of Quill's own (GPUI's lives inside its client; X lets any client act on a window and the window manager answers either). Hide is `UnmapWindow` plus the synthetic `UnmapNotify` ICCCM asks for, so the window is withdrawn rather than iconified and leaves the taskbar and the switcher; show is `MapWindow` plus `_NET_ACTIVE_WINDOW`. Always on top and skip taskbar are `_NET_WM_STATE` client messages with `_NET_WM_STATE_ABOVE` / `_NET_WM_STATE_SKIP_TASKBAR` on a mapped window, and a direct edit of the `_NET_WM_STATE` property on an unmapped one (the window manager reads it at map time; a client message would be ignored). The message data and the property edit are pure and tested.
- Wayland: nothing. xdg-shell gives a toplevel no hide, no stacking order and no taskbar flag; `set_minimized` is the closest thing and `activate` is a request the compositor may refuse. `hide_supported` is false there, so "Run in the background" minimizes (the previous behavior) and the tray's "Open Quill" asks for activation. Always on top and the taskbar toggle report failure and the UI leaves the state unchanged.
- macOS: `orderOut` / `makeKeyAndOrderFront` (after `deminiaturize`), `setLevel` with the floating level, and `NSWindowCollectionBehavior::IgnoresCycle` for "skip taskbar" (there is no per-window Dock entry). The same API, so callers do not branch.

## Idle frames

- Windows: a hidden window gets no `WM_PAINT`, so the frame idle decision (which runs after a frame) could never park it and the vsync thread would keep invalidating it. `third_party/gpui-pre-windows/src/events.rs` now parks the window in `FrameGate` on `WM_SHOWWINDOW` hide and calls `demand_frames` (unpark and invalidate) on show. Recorded in `QUILL-CHANGES.md`.
- X11: an unmapped window draws nothing, so the refresh timer sees idle frames and parks within 250 ms as before; `MapNotify` reports visibility, GPUI asks for a frame, and the frame waker resumes the timer. No vendored change.
- macOS: the display link already stops when nothing draws.

## What Quill does with it

- Close to tray: `tray::close_outcome` takes `hide_supported` and returns the new `CloseOutcome::Hide` on Windows and X11 (the title-bar close box in `main.rs`, Ctrl/Cmd+W and the File menu in `app_render.rs`). Wayland keeps `Minimize`. The close-behavior hint no longer promises a minimized window.
- Restore from the tray: `raise_main_window` shows the window again before activating it; the tray's "Open Quill" and the second-instance raise share it. The macOS deminiaturize moved into `set_visible`.
- "Show taskbar icon" (tdesktop's `WorkMode` taskbar bit): a new `show_taskbar_icon` preference, on by default, with a switch under the close-behavior section on Windows and Linux, shown only while the tray icon is on (tdesktop keeps it behind the tray for the same reason: with neither, nothing could bring the window back). Applied at startup and on change through `apply_taskbar_icon`.
- Picture-in-picture: `set_always_on_top(true)` on every platform after the window opens; macOS keeps `WindowKind::Floating` too.
- Calls: a pin-on-top button in the corner of the call window and the video chat window (tdesktop `createPinOnTop`), hidden on the full-screen stage, toggling `set_always_on_top`; the icon shows the state.

## Not done

- Wayland can do none of the three; the limits above are what the protocol allows an ordinary client.
- A monochrome tray icon and device settings inside the call window are still missing, so `data-tray-toggles` and `calls-window-options` stay open.
- Never sent upstream: the vendored change is Quill's.

## Verified

- Unit tests: the close outcome table (hide on Windows and X11, minimize on Wayland, hide-app on macOS, quit otherwise), the `_NET_WM_STATE` message data, the property edit (no duplicates, other atoms kept), which backends hide, the settings round trip with the new field.
- macOS: built and gated here; the X11 and Windows paths are compiled by their `cfg` blocks and `cargo check --target x86_64-pc-windows-msvc` of the vendored Windows crate passes. No Linux or Windows machine ran them.
