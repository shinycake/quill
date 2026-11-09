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
  - Every OS open goes through `ui/os_open.rs`. While locked it waits for the unlock as a queued intent
    (`wait_for_unlock`), see "What runs while locked" below.
  - Links wait for the unlock, and recordings, the camera, mini apps and the OS window menu are refused.
- `close_for_lock` stops what keeps running without being drawn: recordings, viewers and their
  Picture-in-Picture, the mini app, notification windows, the device-linking scanner, the profile photo camera
  (`close_cameras_for_lock`, whose `CameraDraft` drop kills ffmpeg), and both key captures.

### What runs while locked

Round 13 of the review (area C) found that drawing was only half of it. The main window's frame upkeep and
the poll pass ran every frame while locked, so late answers opened viewers, submitted a login code, dropped
links and closed a checkout with no bank page behind the lock. Background work also kept treating a locked,
focused window as "the user is here". The rule that replaced those patches:

1. **Ingest, timers, delivery and teardown run as always.** Updates are applied, notifications go out (with
   the sender hidden), sounds play, exports keep running, and things whose subject went away close.
2. **Work that comes from the user or from a late answer and would show or start something waits for the
   unlock, or is refused.** Nothing is dropped without a word.
   - A pump that drains stored state waits by not running. Its state stays where it landed and is drained
     at the first frame or pass after the unlock. `QuillApp::lock_waits()` is that gate. The main frame's
     upkeep returns on it right after notification delivery (`app_render/frame_upkeep.rs`), which also
     skips the main window's `per_window_upkeep`, as a chat window's locked frame always did. The poll's
     drains moved into `notifications/poll_drains.rs`, which `poll_live` runs only while unlocked. A parked
     voice note or song does not wait (round 15, as tdesktop's player plays on under the lock): `poll_live`
     resumes it locked (`runs-locked`). The lock drops parked clips, the viewer's and the inline ones
     (`drop_play_intents_for_lock`), as tdesktop's lock closes the media viewer.
   - Something parked for a late answer records the lock count (`PasscodeUi::locks`, round 15) and is
     dropped when `locked_since(count)`: a lock happened in between, even if Quill is unlocked again when
     the answer lands. The pending profile gallery uses it.
   - The lock closes the passkey ceremony box, as tdesktop's lock closes its boxes
     (`close_passkey_ceremony_for_lock`, round 15): the key is told to stop, and a PIN sheet on its way
     cannot take focus from the passcode field.
   - A starter reached outside those pumps queues itself (`passcode::UnlockIntents`, `wait_for_unlock`) and
     runs again after the unlock, asking every gate a second time. Every open in `os_open.rs` does this,
     so a 3-D Secure page, an Earn or Grams withdrawal, a gift's Fragment page or a bot's URL opens after
     the unlock. The lock screen's Log out drops the queue, since it was for the wiped accounts. Each
     intent keeps who asked (round 14): the account on screen and the window (`asking_window`). It runs
     as that window (`in_window_scope` and `as_asked_by`; the main window when that one closed), not the
     one the passcode was typed in, and is dropped when another account came on screen meanwhile (the
     lock card's Answer switches accounts under the lock).
   - A box or secret over a call window, Picture-in-Picture and a kit dialog close are refused (below).
3. **Calls follow tdesktop's per-layer rule** (`Application::lockByPasscode` calls
   `Calls::Instance::hidePanelLayers`; `Panel::addPeople` and the share link actions ask for the passcode
   first). The call windows stay drawn and answerable. At the lock their boxes close (Add People, rename,
   Settings, the screen privacy box, the stream window's boxes) and the stream key is hidden again
   (`calls::lock_layers`). While locked their openers refuse, the stream URL and key are not drawn, and
   Add People is not offered. A call ringing on another account shows on the main window's lock screen
   with Answer and Decline (`background_ring_lock_card`), and its OS notification keeps its buttons while
   locked, with the caller hidden. tdesktop's call panel shows for any account under the lock. Round 14:
   the stream window's Leave/End box is not refused (tdesktop's `Panel::endCall` shows `LeaveBox` locked
   or not, and a manager has no other way out). A stream PiP the lock closes is closed by the lock, not
   the user: `window_closed_by_user` is cleared, so the stream window comes back and draws under the lock.
   The background ring sounds over an older call's end screen, and an answer from the lock card dismisses
   its OS notification (`end_background_ring`). The lock also stops every window's inline clips
   (`leave_window`): a locked chat window runs no frame upkeep, so its round video kept its sound.
   Round 15: a second call ringing in during a call rings, and the lock screen shows it with Decline and
   End & Answer (`swap_call_lock_card`), since its swap box is a box of the locked main window. Both lock
   cards come from one builder (`lock_call_card`), stacked by `locked_root`. A conference invitation with
   no call keeps the call tick, so its ringtone keeps looping (`call_tick_wanted`).
4. **A locked Quill is not looked at.** `session.app_active` is false while locked, whichever window has
   the focus, and the lock clears `active_window_chats`. A chat window's locked frame does not mark its
   chat active again. So the open chat's messages notify and sound, as in tdesktop, whose `markingAsRead`
   needs the main widget shown. The story viewer and the info cover's video avatar do not play, and the
   background accounts' sessions get the same `app_active`.
5. **The OS media controls go off.** `publish_now_playing` clears Now Playing while locked and the media
   keys do nothing (`drain_media_commands`). The unlock publishes again. tdesktop disables its system
   media controls on lock (`system_media_controls_manager.cpp`) and keeps the audio playing.
6. **Nothing connects on the cleared key.** The lock clears the master key, so a preparation fails with
   `LockedStore`. A connection asked while locked (`start_connection`, for example after a remote log
   out) waits for the unlock (`connect_waits_for_unlock`, `connect_at_unlock`), and one prepared before
   the lock that lands refused by it waits too. The held connect names its account (round 15): the unlock
   connects it only while that account is on screen with no client, so a switch under the lock
   (`adopt_live`, from the lock card's Answer) never has its account, or the call just answered, closed. Background accounts start at the unlock, not as failed
   entries that nothing retries.
7. **Quit from the lock screen quits safely.** The "Stop and quit?" box would be a box of a locked window,
   never drawn, holding every later Quit and the OS shutdown. Under the lock, Quit stops the exports, as
   that box's Stop does, and the quit's teardown waits for them to end (`close_all_for_quit`).
8. **Passcode jobs and kit closes.** A lock asked while a passcode job runs lands with the job's answer
   (`PasscodeJob::RunningThenLock`). So a Turn off cannot finish under a lock that then has no passcode to
   lift it. The job is its own value (`PasscodeUi::job`, round 14), not the box's `busy` flag: closing the
   box (Esc, "Lock now") leaves it running, so a lock asked then still waits for the answer.
   `close_kit_dialog_if_done` closes nothing while locked, because every flag reads closed then and the
   window's top box would close under a flag still set. The shell's sync after the unlock closes what is
   done.
9. **The lock screen's Log out lifts the lock only when the wipe started.** With no data folder, or when
   the deletion record cannot be written, the data and the passcode stay. Quill stays locked and the lock
   screen says why (`confirm_lock_logout_in`).
10. **Agents.** Area A of the same round handles the agent socket across the lock: `agent::access::LOCKS`
    counts locks, `wait_for` wakes on a lock and returns nothing, and `call_on` withholds a result when
    Quill locked during the call.

**The guard.** `scripts/lock_surfaces.py` (check 4) reads the bodies of the main frame upkeep and of
`poll_live`. Each must test `lock_waits()` in a condition, and every `self.method(` call outside the gated
part must be listed in `scripts/lock-surfaces.txt` as `path::fn -> method | runs-locked | reason`. A new
step added there that would show or start something fails the check until it goes behind the gate or is
listed with why it may run, and a listed step that is gone fails too. The locked frames (`locked_root`,
`render_locked`) must not run any upkeep. `refuses` entries pin the new gates (the call window openers, the
stream PiP, the kit close, the export quit, the media session and the connection deferral).
`scripts/test_lock_surfaces.py` tests the check itself. The check reads one level only: what a
`runs-locked` step calls in turn is read in review.

### The agent socket

The agent server runs on its own threads, so its gate is a process-wide flag: `agent::access::paused_by_lock`.
`set_locked` sets the flag, and the demo apps never set it, so UI tests share the process safely. While it is
set, every tool returns `LOCKED_MESSAGE` as an error. That includes `quill_status` (it reads `getMe`) and
`td_call`. Every resource but `quill://guide` is refused too (`quill://grants` names the granted chats).
tdesktop has no agent socket. The lock is the user's "nothing of mine is readable now", and an agent at a
locked machine is no different from a person there. The error tells the agent to ask the user to unlock.

Round 14 closed what still ran past the gate, because hiding a call's answer does not undo its effects:

- A write still asleep in the paced queue (up to 30 s) asks the lock at its turn. `still_allowed` refuses
  once Quill locked since the call began (`tools::LOCKS_AT_START`), with
  `LOCKED_BEFORE_SENDING_MESSAGE` ("nothing was sent"), which the call returns as is.
- `download_file` with `to` checks `locked_since_call_began()` after the download and before the copy, so
  a chat's file never lands at the agent's path while locked.
- The `quill://unread` watcher's look fails at once while locked, so the watcher sleeps between looks.
  Before, the round-13 lock check in `wait_for` returned at once and the watcher spun a core for the whole
  lock.
- Check 5 of `scripts/lock_surfaces.py` holds these in place: every queue turn is followed by
  `still_allowed()`, every `copy_new(` is gated on `locked_since_call_began()`, and `resources/read` tests
  `paused_by_lock()` before `read_resource(`.

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
- Round 14: `agent::tools` `a_lock_while_a_write_waits_stops_it` and
  `a_locked_watch_look_fails_without_waiting`, and `agent::server` `a_locked_quill_reads_out_only_the_guide`.
- `chat_window::dialog_host_tests::a_chat_windows_box_stays_there_through_a_lock`.
- Round 13, what runs while locked: `passcode::lock_waits_tests` (a late link and a Grams withdrawal open
  after the unlock, Log out drops what waited, a locked window is not looked at, a lock during a passcode
  job, the call windows' boxes, the deferred connection), `deleting_tests`
  `a_lock_screen_log_out_that_cannot_start_the_wipe_stays_locked`, `background_accounts::tests`
  `a_call_on_another_account_can_be_answered_from_the_lock_screen`, `export_quit`
  `a_quit_from_the_lock_screen_stops_the_export_and_quits`, and `scripts/test_lock_surfaces.py`.
- Round 15: `a_second_call_under_the_lock_rings_and_can_be_declined`, `an_invitation_alone_keeps_the_call_tick`,
  `the_lock_drops_a_parked_clip_and_keeps_a_parked_voice_note`, `the_lock_closes_the_ceremony_and_its_pin_field`,
  `the_unlock_connects_only_the_account_that_waited`, and `the_group_avatar_opens_its_photos_newest_first`
  (a lock and an unlock between the ask and the photos).
- The existing `chat_window::lock_tests` still pass, `locking_with_a_chat_window_open_lets_both_windows_go_idle`
  among them.

## What is left

- Round 13: the poll's drains, the app-active change in `poll_live` and the background start deferral have
  no test that drives them, because the demo has no TDLib client. The guard covers their placement, and the
  tests drive the same gates through the frame upkeep, the OS opens and `start_connection`.
- Round 13: the chat window's full-screen hand-back under the lock (`viewer_follow_window_fullscreen` in
  `render_locked`) and the stream PiP refusal have no tests: the test platform has no full screen and no
  stream player.
- The 1:1 call window's device menu stays usable under the lock. It lists audio devices, not the account.

- The profile camera's close on lock has no test: starting the camera needs ffmpeg and a device.
- The notification-click and Dock refusals are tested only by reading. The demo has no second signed-in
  account, so a switch could not happen in a test either way.
- A window built without the kit `Root` would bypass the layer. The main, chat, notification, What's New and
  screen chooser windows are built on it. The call windows draw everything while locked anyway, and the lock
  closes Picture-in-Picture and the mini app.
