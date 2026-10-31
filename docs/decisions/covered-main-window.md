# A main window covered by its own chat window stops animating

Measured on a 120 Hz Mac (M1 Pro, offline demo, release build of main 58ff96b7e): a chat opened in its own
window, moved to cover the main window whole and left active. The main window showed a chat with two Lottie
stickers and kept drawing them: 89 to 104 frames a second, each one the window frame and its two animation
layers (no chat list or conversation slice), about 20% of a core. It also held about 14 MB of drawables that a
stopped window gives back (`idle-drawables.md`). On a 60 Hz display: 29 frames a second, 6 to 7% CPU.

## What was wrong

- `QuillApp::render` (`src/ui/app_render/mod.rs`, origin/main line 106) counts the main window as active while
  any chat window is (`windows.any_active()`), so its animation layers kept asking the frame clock for ticks
  (`tick_animation_layer`, `src/ui/app_slice.rs`).
- macOS tells a window it is occluded only when another app's window or a borderless one covers it. Quill's
  own titled chat window never made the main window "not visible", so the display link's occlusion stop never
  ran.
- A chat window's own animated content asks for whole-app ticks (it has no cached slices), which notify
  `QuillApp` and so redraw the main window too: a covered main window drew the chat window's frame rate as
  well.

Turning the main window's gate off as a whole would not do: `frame.window_active` is also the gate of the one
frame clock that the chat windows' content ticks through, and presence and notifications read it.

## What changed

- `src/ui/chat_window/cover.rs`: `WindowPlace` is a window's bounds and display. The main window notes its own
  at each render. A chat window notes its own at each render and in its activation, bounds and display
  observers, which run even when the window does not draw (a minimized one may not). The main window counts as
  covered when it is not active itself and the active chat window's bounds hold its bounds on the same display.
  `frame.main_covered` keeps the answer of the main window's last render.
- `src/ui/frame_clock.rs` `request_tick`: while the main window is covered, its slices and layers, and the
  app outside a chat window's scope, get no tick. A chat window's content gets its tick on that window's
  view, not the whole app, so the main window under it does not redraw.
- When a chat window is deactivated, moved, resized, goes to another display or fullscreen, and the main
  window is no longer covered, the app redraws (`chat_window_placed`). The main window's next render then
  lets its layers ask again. A closed chat window already redraws the app; the main window's own moves and
  activation redraw it.
- Bounds are compared on one display only (macOS measures a window from its screen, Windows in its monitor's
  scale).

A partly covered main window, or one covered by two chat windows together, animates as before.

## Where the main window never counts as covered

All of this rests on the active window being in front of the app's other windows. A visible main window must
never hold still, so where that is not sure, the main window animates as before:

- Fullscreen windows: on macOS a fullscreen window has a Space of its own, so the desktop beside it may show.
- Linux: Wayland gives no window positions, and an X11 window manager may focus a window without raising it.
- Windows, while "Activate a window by hovering over it with the mouse" is on (`SPI_GETACTIVEWINDOWTRACKING`):
  the window under the pointer becomes active, and whether it is raised depends on another setting
  (`SPI_GETACTIVEWNDTRKZORDER`) that this does not trust. The setting is read each time the check runs,
  only once a chat window's bounds already hold the main window's (one `SystemParametersInfoW` call). It
  runs at the main window's render and at each activation, move or resize of a chat window, which are the
  events that could put an active chat window behind the main window, so a changed setting counts from the
  next one. A failed read counts as on. It needs no new crate feature (`windows-sys`
  `Win32_UI_WindowsAndMessaging` was already on).

macOS has no such setting. The ways it orders a window without making it key (a Cmd-click or Cmd-drag on a
background window) need a visible part of the main window to click. Quill brings the main window forward
only with `makeKeyAndOrderFront` (GPUI's `activate_window`, `window_control::set_visible`), and never lifts
the main window or a chat window above the normal level (`set_always_on_top` is for the call and
Picture-in-Picture windows); a change there must revisit this. Switching Spaces makes a window on the new
Space key. A chat window opened as a native tab of the main window counts as covering it while its tab is the
selected one, which is right: the main window's tab is hidden then.

## Tests

`chat_window::cover::tests` run the `ready-sticker-playback` demo (two 30 fps stickers in the open chat) with
the main window at 1100 × 700 and count what it draws over 2 s of the test clock (`frames_over`):

- `a_covered_main_window_draws_nothing_until_the_chat_window_moves`: covered by the active chat window, no
  layer draws and no frames, five times over; moved 40 px right, the same 120 layer draws and 60 frames as
  alone in front.
- `a_covered_main_window_animates_once_resized_past_or_closed`: resized to show the main window's right edge,
  then covering again, then closed with the main window activated as the platform does.
- `a_chat_window_that_leaves_the_main_window_in_view_keeps_it_animating`: a chat window beside it, at covering
  bounds on another display, and fullscreen: as alone in front.
- `a_covering_chat_window_keeps_its_own_stickers_animating`: chat 11 in its own window draws the same frames
  covering the main window as beside it (64 in 2 s), and the main window under it draws nothing.

On the old code every test fails: the covered main window drew 120 layer draws and 60 frames in 2 s (30 a
second, both layers each frame), also when only the chat window animated. Without the redraw in
`chat_window_placed`, the main window stayed still after the chat window moved, shrank or went fullscreen.
The test platform gained `simulate_window_move` and `simulate_window_move_to_display`
(`third_party/gpui-pre/QUILL-CHANGES.md`).

## To measure on a Mac

Release build, `ready-chats` demo with `QUILL_DEMO_HISTORY_ANIM=stickers` and `QUILL_TRACE_TICKS=1`. Open
another chat in a new window, drag it over the whole main window, leave it active. Expected: the main
window's `(window frame)` count goes to 0 a second (from 89 to 104), CPU to about what the chat window alone
costs, and 3 s later its spare drawables are released (one left). Dragging the chat window a few points off
an edge starts the stickers again at once.

## Not done

- GPUI's own `with_animation` (the kit's `Skeleton` and `Spinner`) and the playback progress tick still
  redraw a covered main window, as they do behind another app.
- The Windows check above was not compiled here (a Linux machine without the MSVC toolchain); it makes the same
  call as `src/screen_reader.rs` with another action.
