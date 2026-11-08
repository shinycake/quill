# Each window has its own passcode field on the lock screen

Locking Quill with a chat window open made the main window and the chat window redraw each other without
end. In the UI test harness `lock_by_passcode` never returned: GPUI's test mode draws dirty windows inside
`flush_effects` until none is dirty, and here one always was.

## What was wrong

Every window shows the lock screen while locked (`chat_window_body` draws `lock_overlay` too), and both drew
the same `InputState`, `PasscodeUi::lock_input`. A text field keeps the geometry of its last paint
(`last_bounds`, `input_bounds`) and notifies its observers when that geometry changes
(`gpui-base/src/input/base/element.rs`, the paint's `geometry_changed` check). The two windows have different
sizes, so the centered field sits at a different place in each:

1. The main window paints the field and stores its bounds there. They differ from the last paint (the chat
   window's), so the field notifies, which dirties every window that drew it: the chat window.
2. The chat window paints it, stores its own bounds, notifies, and dirties the main window.
3. Back to 1.

## Measured

The test `chat_window::lock_tests::locking_with_a_chat_window_open_lets_both_windows_go_idle`, with temporary
tracing of `Window::draw`, `Context::notify` and `Window::refresh` in the vendored GPUI (taken out again), run
on 2026-11-08 on the M-series Mac in the debug test profile:

| | Draws after the lock | Ends |
|---|---|---|
| Before (one shared field) | 149 of each window in the first 600 traced events, every one after the first triggered by `InputBaseState<InputMode>` notifying from `element.rs:4368` | never; `sample` showed the test thread in `App::flush_effects` → `Window::draw` at 100% CPU |
| After (a field per window) | 3 (main 1, chat window 2) | parks |

In the app the loop goes through the display link instead of `flush_effects`: each notify dirties the other
window, which draws at its next frame. So it is not a freeze. Both windows stay responsive, but they redraw at
the display's rate for as long as the lock lasts. This was not measured in a live build. The mechanism is
GPUI's own, though, and no platform code is involved, so it holds on every backend. It needs the windows to
place the field differently, which they always do unless both windows are the same size.

## The fix

Telegram Desktop builds a `PasscodeLockWidget` per window (`Window::Controller::setupPasscodeLock`), each with
its own field. Quill now does the same. `LockField` (`src/ui/passcode/lock_field.rs`) holds one window's
field and what that window does with it at its next lock-screen frame (focus it, or empty and focus it). The
main window's sits in `PasscodeUi::lock`, and a chat window's in its `WindowSlot`, swapped in with the rest of
the window's state. Enter in a field submits from that field's window (`in_window_of`). A lock resets every
window's field (`reset_lock_fields`), and a wrong passcode refocuses the field of the window it came from.

## Rejected

- Skipping the notify for a field painted in another window: that would be a kit patch for a field that is
  wrong to share in the first place. The field's mouse handling reads the same stored bounds
  (`input_bounds.contains(&event.position)` in `gpui-base/src/input/base/state.rs`), so a shared field also
  tested clicks in one window against the other window's layout.
- Leaving the lock field out of chat windows: tdesktop locks every window, and a chat window must not show
  its chat while locked (`the_passcode_box_covers_a_chat_window_too`).
