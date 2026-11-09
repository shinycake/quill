# The passcode lock is a render state

While Quill is locked, every window draws the lock screen or nothing. No kit dialog, sheet, popover, tooltip
or deferred overlay is drawn or takes input, whoever opened it and whenever. Every path that starts an effect
or takes input without being drawn asks one gate. This replaces a lock that closed or hid the surfaces it knew
about. Each review round found more surfaces it did not know about.

## What was wrong

Round 12 of the state-ownership review (area C, findings 1 to 7) found these:

1. **Kit alerts drew over the lock screen.** The lock screen was a plain child of the main window's view. The
   kit mounts its dialog layer above the view (`WindowStateLayers` in `gpui-component/src/root.rs`), so a box
   opened with `window.open_alert_dialog` (Clear History, Delete, Unpin, the agent's Undo, 23 sites) stayed on
   top of the lock with its OK button live. The shell hid only the dialogs it tracked.
2. **A notification click switched accounts behind the lock.** `switch_account` had no lock check, and the
   Dock reached it too.
3. **The profile photo camera stayed on.** Nothing in the lock reached `sources.camera`. `lock_surfaces.py`
   scanned only `src/ui` for `Command::new(`, and this start goes through `media_tools::command(` in `src/`.
4. **An armed key capture took the passcode's first key.** The Shortcuts and push-to-talk captures are
   app-wide `intercept_keystrokes` handlers with no lock check.
5. **Deferred overlays painted over the lock.** The emoji suggestion bar, the AI hint and open popups use
   `deferred(..)`, which paints after the lock overlay.
6. **A second link waited for a third.** Not confirmed by reading: the link inbox retries every 250 ms while a
   link is pending (`link_inbox.rs`). The unlock now pokes `wake::LINKS` anyway.
7. **The agent socket read and sent while locked.**

The first and fifth come from one cause. The lock was one more child among the window's children, so
anything drawn later or in a higher layer covered it.

## tdesktop

- `Application::lockByPasscode` sets up the lock on every window (`Window::Controller::setupPasscodeLock`),
  closes the media viewer and hides the call panels' layers.
- `MainWindow::setupPasscodeLock` creates the lock widget over the body, calls
  `ui_hideSettingsAndLayer(anim::type::instant)` (which destroys the layer and its boxes), hides `_main`, and
  closes the session's attached web views.
- `passcodeLockChanges` runs `closeAdditionalWindows`: checkouts, web views, the wallet, Instant View and the
  photo editors.
- `Window::Controller::locked()` is the per-window gate input checks. The tray's accounts menu is empty while
  locked (`tray_accounts_menu.cpp`), and so is the Dock's (`menu_dock.cpp`). The tray hides the notifications
  toggle (`tray.cpp`). A notification click only shows the window (`openNotificationMessage`).
- The call panel stays up and usable.

## Designs compared

| Design | What a new surface must do to be covered | Bypassed by |
| --- | --- | --- |
| A. The root of every window gated: a kit root plugin, registered after the kit's, drops the built surface and draws the lock screen alone | nothing: the plugin runs in every window `Root` builds | a window not built on the kit `Root` (none today), or a window type that should stay usable being named wrongly |
| B. The dialog layer cleared on lock (`close_all_dialogs` in `close_for_lock`) | nothing for boxes open at the lock | a box opened after the lock (a late answer, a deferred open); a deferred overlay, which is not in the layer; a popover or sheet |
| C. One `LockState` every starter must consult, by type (a token only the unlocked state hands out) or by a crate-wide guard | call the gate, or be listed | a new starter that skips the token: a type cannot force a call site to ask, only make asking easy |

**A was chosen for what is drawn, and C (as a guard) for what is not.** B was rejected. It fixes only the
boxes that exist when the lock comes, and it loses their content, which tdesktop also loses. A covers
the boxes and also every layer, overlay or window type that comes later. C is still needed, because
drawing is not the only way in: key interceptors, OS menus, notification clicks, the agent socket, helper
processes and devices all run without being drawn.

## The design

### What is drawn (`src/ui/passcode/lock_layer.rs`)

- `LockLayer` is a `gpui_base::RootPlugin`. `PasscodeUi::new` registers it, before the main window's `Root`
  exists, so every window gets it, the main window included. Plugins run in registration order, so its
  `decorate` gets the whole surface: the application view plus the kit's dialog, sheet, notification,
  tooltip, menu and touch-selection layers.
- While locked it drops that surface unbuilt. An `AnyView` renders lazily, so the dropped layers do not
  render at all.
- It returns a themed root that holds only the window's own view, or nothing. The role comes from the view's
  type (`role_of`), here and nowhere else:
  - `LockScreen`: the main window (`QuillShell`) and the chat windows (`ChatWindowView`). Each view builds only
    its lock screen while locked: `QuillApp::locked_root` for the main window and `ChatWindowView::render_locked`
    for a chat window. Neither builds the chat list, the chat, menus or any deferred overlay, and each keeps
    only the window's own commands (Quit, Close, Minimize, Zoom, Full Screen). Each window has its own field
    (docs/decisions/lock-field-per-window.md).
  - `Unlocked`: the call windows and the screen chooser opened from them, as tdesktop leaves its call panel up,
    and notification windows. A notification shown while locked already hides its sender and text
    (`hide_sender`), as tdesktop's do.
  - `Blank` for every other type. A new kind of window is covered without anyone listing it.
- The kit's Tab handling went with its root, so the lock root moves focus with Tab and Shift-Tab itself. The
  lock field takes focus when nothing inside the lock screen has it.
- `LockGate` (a GPUI global) is the copy of the flag the plugin reads. `set_locked` publishes it and refreshes
  every window.

### Boxes are hidden, not closed

A kit dialog open at the lock is hidden for the lock and back after the unlock. This is a render state, and it
departs from tdesktop on purpose: tdesktop destroys its layer on lock, so a half-written box is lost, and a box
whose flag is set must come back the same way it would after any frame. The shell does not sync its dialogs
while locked (`QuillShell::sync_host_dialogs` returns at once). So a box neither opens, closes nor moves
between windows under the lock. A box asked for while locked opens after the unlock, in the window that asked.

### What is not drawn: one gate, one writer

- `QuillApp::set_locked` is the only writer of the flag. It pokes the wakes, publishes `LockGate`, pauses or
  resumes the agent socket, and pokes `wake::LINKS` on unlock.
- `QuillApp::refused_while_locked()` is the one gate:
  - Every app key interceptor goes through `passcode::intercept_keys_unlocked`, so no shortcut, viewer key,
    composer key or capture acts on a key typed on the lock screen. The secret-code observer skips locked keys.
  - `switch_account` refuses while locked. A notification click from another account only shows the window,
    and the Dock's account items are hidden and refused when stale. A ringing call's Answer still switches
    (`switch_account_even_locked`), because calls stay usable.
  - Every OS open goes through `ui/os_open.rs` and refuses while locked.
  - Links wait for the unlock, and recordings, the camera, mini apps and the OS window menu are refused.
- `close_for_lock` stops what keeps running without being drawn: recordings, viewers and their
  Picture-in-Picture, the mini app, notification windows, the device-linking scanner, the profile photo camera
  (`close_cameras_for_lock`, whose `CameraDraft` drop kills ffmpeg), and both key captures.

### The agent socket

The agent server runs on its own threads, so its gate is a process-wide flag: `agent::access::paused_by_lock`.
`set_locked` sets the flag, and the demo apps never set it, so UI tests share the process safely. While it is
set, every tool returns `LOCKED_MESSAGE` as an error. That includes `quill_status` (it reads `getMe`) and
`td_call`. The unread resource is refused too. tdesktop has no agent socket. The lock is the user's "nothing
of mine is readable now", and an agent at a locked machine is no different from a person there. The error
tells the agent to ask the user to unlock.

### The guard (`scripts/lock_surfaces.py`)

The module doc states exactly what it checks:

- In the production code of all of `src/`, every window, helper process, capture, app-wide key hook, app-wide
  action handler and lock-skipping account switch sits in a function that tests a gate in a condition, or is
  listed as `path::fn` with a closer, a `via:` function that tests a gate, or a reason.
- `set_locked` is the only writer of the flag.
- `LockLayer` is registered.

Before, the guard scanned only `src/ui` and three patterns, and it accepted a gate named anywhere in a body,
even in a comment.

## Tests

- `passcode::lock_layer_tests`:
  - a box open at the lock and a box opened while locked are not drawn until the unlock;
  - the main window draws only the lock screen;
  - an armed push-to-talk or shortcut capture is disarmed, and a key typed while locked binds nothing.
- `agent::access` `a_locked_quill_answers_no_tool_and_no_resource`.
- `chat_window::dialog_host_tests::a_chat_windows_box_stays_there_through_a_lock`.
- The existing `chat_window::lock_tests` still pass, `locking_with_a_chat_window_open_lets_both_windows_go_idle`
  among them.

## What is left

- The profile camera's close on lock has no test: starting the camera needs ffmpeg and a device.
- The notification-click and Dock refusals are tested only by reading. The demo has no second signed-in
  account, so a switch could not happen in a test either way.
- A window built without the kit `Root` would bypass the layer. The main, chat, notification, What's New and
  screen chooser windows are built on it. The call windows draw everything while locked anyway, and the lock
  closes Picture-in-Picture and the mini app.
