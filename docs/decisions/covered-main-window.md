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

Nor does it work to send a whole-app tick to one window. Every window's content asks through the same call
(`request_animation_tick` with no slice in scope): a chat window's stickers, a dialog, sheet or toast in a chat
window (the kit builds those after the window's view has put back the state it found, the main window's in
live mode, so `style_preview.rs` and `peer_qr/surface.rs` ask with no chat in scope), and the group call
window's microphone halo and speaking blobs (`group_call_panel.rs`, `group_call_speaker.rs`, which never put
a chat window's state in place; `poll_live` puts the main window's back at every poll). The state in place at
the request says nothing about which window asked. The first version of this change sent a tick asked in a
chat window's scope to that window alone and dropped the rest, which froze the call window and those dialogs
while the main window was covered, or redrew another chat window instead.

## What changed

- `src/ui/chat_window/cover.rs`: `WindowPlace` is a window's bounds and display. The main window notes its own
  at each render. A chat window notes its own at each render and in its activation, bounds and display
  observers, which run even when the window does not draw (a minimized one may not). The main window counts as
  covered when it is not active itself and the active chat window's bounds hold its bounds on the same display.
  `frame.main_covered` keeps the answer, worked out again at each render of the main window and each change
  of a chat window's place.
- `src/ui/frame_clock.rs`: while the main window is covered, its slices and animation layers (the only ticks
  aimed at a view: `tick_animation_layer`, the main window's own) get no tick, neither asked then nor asked
  before it was covered. A tick for the whole app still runs, whoever asked. In place of the `QuillApp` notify
  that redraws every window, `QuillApp::tick_app` then emits `AppTick`, which only the views that follow the
  app from other windows hear (`frame_clock::follow_app`: the chat, call, group call, stream, screen chooser
  and What's New views, which observed the app before). They redraw as on the notify. The main window keeps
  its frame, and so do a window that draws by its own clock (Picture-in-Picture, a notification popup) and a
  cached view that does not follow the app (the call pictures do follow it, so they rebuild as before), which the notify does not
  redraw either. A first take refreshed every window but the main one, which redrew those two as well.
- When a chat window is activated or deactivated, moved, resized, goes to another display or fullscreen,
  `chat_window_placed` updates `frame.main_covered` at once: a main window it now hides holds still from that
  moment (unless it is the active window, asked of the platform, since the main window's own activation
  callback may come later), and one it no longer hides gets an app redraw, whose
  render lets its layers ask again. A closed chat window already redraws the app; the main window's own moves
  and resizes redraw it (GPUI's `bounds_changed`) and its activation redraws the app, each recomputing the
  answer.
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

## What still shows a covered main window still

While the chat window covers it, the main window keeps the last frame it drew. Wherever the system shows the
main window by itself, that frame is what shows, until the user brings the main window forward: Mission
Control and App Exposé thumbnails on macOS, taskbar and Alt-Tab previews on Windows, and a screen share of
the main window alone. Nothing tells the app such a view is open, so this is inherent. Sharing the whole
screen shows the chat window on top, which is right.

## Tests

`chat_window::cover::tests` run the `ready-sticker-playback` demo (two 30 fps stickers in the open chat) with
the main window at 1100 × 700 and count what it draws over 2 s of the test clock (`frames_over`), counted from
the change itself with no time to settle, so animation that resumes late counts short. "Alone" is the main
window counted the same way from coming back from behind another app (120 layer draws, 60 frames).

- `a_covered_main_window_draws_nothing_until_the_chat_window_moves`: covered by the active chat window, no
  layer draws and no frames, five times over; moved 40 px right, the same as alone.
- `a_covered_main_window_animates_once_resized_past_or_closed`: resized to show the main window's right edge,
  then covering again, then closed with the main window activated as the platform does.
- `a_covered_main_window_animates_once_moved_or_resized_out`: the main window moved or narrowed within the
  chat window stays still; moved or resized so an edge shows, it animates as alone.
- `a_chat_window_that_leaves_the_main_window_in_view_keeps_it_animating`: a chat window beside it, at covering
  bounds on another display, and fullscreen: as alone.
- `a_covering_chat_window_keeps_its_own_stickers_animating`: chat 11 in its own window draws the same frames
  covering the main window as beside it, and the main window under it draws nothing.
- `a_call_window_keeps_animating_under_a_covering_chat_window`: a window that follows the app and asks for
  whole-app ticks while it draws, like the group call window, draws as many frames while a chat window covers
  the main window as with none, with the main window's state in place (live mode) or the chat window's; the
  main window draws nothing.
- `a_dialog_in_a_covering_chat_window_keeps_animating`: a kit dialog in the chat window that asks for ticks
  while it is built, with the main window's state in place, draws as many frames covering the main window as
  beside it.
- `a_covered_main_window_leaves_windows_and_views_that_do_not_follow_the_app_alone`: while the call window
  animates, a window that does not follow the app draws no frame and a cached view in the call window that
  does not follow it is not built again, with the main window in view and covered alike. With the refresh of
  every window but the main one (aeea77d00), the window drew 60 frames and the view was built 60 times in 2 s
  while the main window was covered.

Every test checks that a covered main window draws nothing, which the code before this change (04644b4a3)
never does: it drew 120 layer draws and 60 frames in 2 s, also when only the chat window animated. On the
first version of this change (1b0ce0b15) the call window and the chat window with the dialog drew 1 frame in
2 s instead of 60 while the main window was covered, and the main window drew one more frame after a chat
window came over it (it learned so at its next render; `chat_window_placed` now says so at once). Counted from
the change, a resume that starts late counts short and fails too. The test platform gained
`simulate_window_move` and `simulate_window_move_to_display` (`third_party/gpui-pre/QUILL-CHANGES.md`).

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
