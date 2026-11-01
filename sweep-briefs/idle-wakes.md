# Sweep bundle idle-wakes

## idle-1: Notifications of the account on screen go out only when the main window draws
- Severity: P1 user-visible failure
- Kind: robustness
- Platforms: macOS (hidden, minimized, fully covered by another app, on another Space, main window closed with a chat window left open). Windows (hidden to the tray, likely minimized). X11 (unmapped or reported fully obscured). Wayland (minimized or hidden, by compositor).
- Where: `src/ui/app_render/frame_upkeep.rs:17,19` is the only writer of `session.app_active` and the only caller of `flush_notifications`. `src/ui/notifications/mod.rs:788` drains `pending_notifications` (line 841), sounds, call toasts (`flush_call_notifications`), clears, force reply. The chain: `poll_live` queues, line 402 sets `deliver`, `redraw_polled` calls `cx.notify()`, GPUI's frame waker does nothing for a hidden window (`third_party/gpui-pre-macos/src/window.rs:2174`, `visible` false).
- Trigger: a message for the on-screen account arrives while the main window does not draw. The tray setup makes this the normal state.
- Effect and size: no banner, no sound, no Dock bounce, no incoming-call toast until something makes the window draw. Then all of it fires at once (coalesced per chat, so memory is bounded). A second effect: `app_active` keeps its last rendered value. After Cmd-H it can stay true, because the deactivation redraw may never run, and `decide_notify` (`src/notify/mod.rs:285`) then treats a message in the open chat as seen and drops it for good. Label: inferred from code, not run.
- Why it is real: comments already say the render does not run. `poll_live` line ~459 says "the root render does not run while the main window is minimized or hidden", and `inline_reply.rs:45,118` and `background_accounts/clicks.rs:127` say the window "may never render again on its own". `docs/decisions/notification-click-repaint.md` says "Hidden windows still never draw". The comment on `deliver` (line 399) assumes `cx.notify()` draws "whether or not the window is in front", which holds only for a window still partly visible. Background accounts are fine: `src/notify_accounts.rs:195` drains them with no render, so the on-screen account is the one left out. Tests cannot see this, because GPUI test mode draws every dirty window itself (`third_party/gpui-pre/src/app.rs:2241`). One note looked like counter-evidence, a notification posted during a frozen window, but that window was visible and took input, and `dispatch_key_event` draws a dirty window (`third_party/gpui-pre/src/window.rs:6425`).
- What tdesktop does: the notification manager runs on data events and never depends on a paint.
- Fix sketch: split `flush_notifications`. `deliver_notifications(cx)` takes no `Window` and runs at the end of `poll_live`: queue, sounds, clears, call toast, tone upload. `request_attention` goes through the stored main window handle (`AnyWindowHandle::update` works for a hidden window). `notify.windows.platform` is noted on display change, not read here. Clicks and force reply stay in the render path, or go through the handle, since they open things in the window. Set `app_active` at the top of `poll_live` from `main_handle.is_active(cx)`. Then `deliver` no longer forces a full redraw for each notification, a small saving as well.
- Proof plan: deterministic, no clock. Inside one `view.update`, feed a message through the poll, then assert `pending_notifications` is empty and the sink holds the notification before the closure ends. No draw can have run, so today's code fails. Second test: deactivate the window without drawing, feed a message in the open chat, assert it notifies. Mac check: demo with a temporary `cx.hide()` hook and a trace in `spawn_os_notification`.
- Scope: `src/ui/notifications/mod.rs`, `frame_upkeep.rs`, `src/ui/notify_windows`. Medium risk: ordering of clears against new notifications must stay the same. Overlaps calls (toast) and failure paths lanes. Do this before idle-2.

## idle-2: Closing the window on a Mac leaves a process that cannot be reopened, and its Quit menu is dead
- Severity: P1 user-visible failure
- Kind: robustness
- Platforms: macOS only. On other platforms the last closed window quits (`QuitMode::Default`).
- Where: `src/tray/mod.rs:393` `close_outcome` returns `Quit` when Run in the background is off, the default (`src/settings/appearance.rs:255`). `src/main.rs:388` then returns true, so the window closes with no `cx.quit()`. `third_party/gpui-pre/src/app.rs:3389` keeps a windowless macOS app alive. No `on_reopen` is registered anywhere in `src` (only the forwarder at `src/ui/interface_zoom/mod.rs:395`), so `third_party/gpui-pre-macos/src/platform.rs:1362` calls nothing.
- Trigger: click the red close button on a default install.
- Effect and size: confirms the backlog lead, and it is wider than the Dock. The window is gone and a Dock click does nothing. `QuitApp` has only a window-scoped handler (`src/ui/app_render/mod.rs:175`), so `is_action_available` is false (`app.rs:2981`), and the Quit menu item and Cmd-Q are disabled. The only exits are Quit in the Dock's context menu or Force Quit. `QuillApp` is released, so the connection drops and no message or notification comes in, while the Dock still shows Quill running. Every recovery path is gone: the link inbox loop ended, so a forwarded launch is ignored, and tray and menu callbacks fail on a dead handle. Left over: `td_receive` thread and `scutil` every 15 s, doing nothing for hours. If a chat window stays open, `QuillApp` lives but no main window can come back, and idle-1 stops notifications for that whole session. Label: inferred from code; `docs/decisions/retired-images-any-window.md:173` records the missing `on_reopen`.
- Why it is real: the comment at `src/ui/app_render/mod.rs:204` shows the windowless state is deliberate, for the menu bar, but that menu bar has nothing live in it.
- What tdesktop does: `Window::MainWindow::hideNoQuit` (`window/main_window.cpp:513`) forces `RunInBackground` on macOS whatever the setting says, so the window hides. `specific_mac_p.mm:128` shows it on reopen. It quits only with no account signed in.
- Fix sketch: `close_outcome` on macOS with a signed-in account returns a hide outcome regardless of the tray, via `closeWithoutDestroy` behavior: leave fullscreen first, then hide the window. With no account, `cx.quit()`. Register `cx.on_reopen` in `src/main.rs` with the stored window handle, calling `raise_main_window`. Note that `should_handle_reopen` calls back only when `hasVisibleWindows` is false, which is right for a hidden window with no chat window open. Register `QuitApp` with `cx.on_action` so it works with no key window. Also apply the same `CloseWindow` action path at `app_render/mod.rs:202`.
- Proof plan: table test of `close_outcome`. Add `simulate_reopen` to the test platform (`third_party/gpui-pre/src/platform/test/platform.rs:689` is a no-op today). Test: close with default settings, assert `QuillApp` and the connection stay alive; reopen, assert visible and active. Test: `is_action_available(QuitApp)` is true with no window.
- Scope: `src/tray/mod.rs`, `src/main.rs`, `src/ui/app_render/mod.rs`, `src/ui/link_inbox.rs`, one test hook in `third_party/gpui-pre`. Behavior change on Mac, low technical risk. Needs idle-1 first, or a hidden window stops notifying.

## idle-3: Each lone frame keeps the display link running 250 ms, so the caret costs 30 to 60 wakes a second
- Severity: P2 measurable waste
- Kind: power
- Platforms: macOS (`CVDisplayLink`), X11 and Windows (same grace in the shared `frame_idle.rs`). Not Wayland, whose frames come from compositor callbacks.
- Where: `third_party/gpui-pre-macos/src/window.rs:3473` restarts the link after a woken step that drew, and `:647` keeps it 250 ms. `third_party/gpui-pre-windows/src/frame_idle.rs:52,102` and its X11 copy do the same. The most common caller is `third_party/gpui-base/src/input/base/blink_cursor.rs:4` at 500 ms.
- Trigger: Quill frontmost with a chat open, composer focused, the usual state. Also any isolated redraw: a chat-list batch, one reaction.
- Effect and size: 2 draws a second, each followed by 250 ms of steps. At 120 Hz that is 30 steps per blink: 60 main-thread wakes a second and 60 on the CoreVideo thread, 7,200 a minute in all. At 60 Hz, half that. 59 of 60 steps do nothing. This is about 10 times everything in the rest set. CPU is small: `codex-idle-cpu.md` measured the always-running link at 0.3 to 0.5% of a core, so a 50% duty cycle is about 0.15 to 0.25% (inferred). The wakes matter more than that CPU share because they stop the package from entering deep idle (guess, not measured). The link is also started and stopped twice a second.
- Why it is real: the grace exists so animations keep a paced link, and 250 ms was compared only against 1 s (`codex-idle-cpu.md:110`). `idle-inactive-redraw.md:104` left the blink itself to a product decision, counting draws (2 a second) and not the steps behind them. A wake already draws within 1.3 to 2.4 ms (measured, `codex-idle-cpu.md`), so a lone frame gains nothing from the link.
- Fix sketch: separate what `activity` merges. `draw` sets `drew`, `schedule_frame` sets `demand`. In `step`, a woken step that drew with no demand stays stopped. A second woken draw within about 100 ms of the last one restarts the link, so scrolling and typing stay paced by the display. Timed animations already ask for the next frame, which is demand. Move the arming of `DRAWABLE_RELEASE_AFTER` so a last one-shot draw still releases. Add the same rule to `FrameIdle::frame` in `frame_idle.rs`, which already takes `now` as an argument. Do not simply shrink the 250 ms: 12 fps dots would then flap the link on every frame.
- Proof plan: pure tests on the state machine with an injected `now`: lone draws never subscribe, two within the window do, demand always does. Extract the macOS decision so it shares those tests (`scripts/test-frame-idle.sh`). Mac measurement with a temporary trace in `step`, the method in `codex-idle-cpu.md`, on a 120 Hz panel with `ready-chats` and the composer focused: about 60 steps a second before, 2 after; scroll frame gaps unchanged.
- Scope: `third_party/gpui-pre-macos/src/window.rs`, both `frame_idle.rs`, `QUILL-CHANGES.md`. Medium risk: pacing regressions in scroll. Overlaps the vendored GPUI lane.

## idle-5: A chat window hidden behind another Quill window keeps drawing
- Severity: P2 measurable waste
- Kind: cpu
- Platforms: macOS confirmed; Windows only with the same guard as the main window; not Linux (no trusted stacking).
- Where: `src/ui/chat_window/cover.rs:87` `main_covered` and `:192` `follow_windows` hold only the main window. `src/ui/frame_clock.rs:722` `AppGate.held` is one `Option<WindowId>`. `tick_wanted` (`:741`) passes while any Quill window is active. macOS reports no occlusion between titled windows of one app.
- Trigger: a chat window with stickers, GIFs or a clip sits whole behind the active main window or another active chat window.
- Effect and size: confirms the backlog lead; `docs/decisions/inline-players-per-window.md:154` lists it as a known follow-up. The window draws at the inactive cap, 30 ms (`src/ui/window_frame.rs:134`), so about 29 frames a second, plus GPU work and held drawables, for nobody. Reference: the same case for the main window measured 29 f/s and 6 to 7% of a core at 60 Hz (`covered-main-window.md`, two Lottie stickers). Inferred for chat windows. It also redraws on whole-app ticks asked by other windows.
- Why it is real: #1264 built the check but aimed it only at the main window. Nothing else stops a hidden chat window: its `request_video_frame` passes `AppGate::animates`, and `AppTick` notifies every `follow_app` view.
- Fix sketch: `AppGate.held` becomes a small set of window ids, worked out in `follow_windows`: every window whose `WindowPlace` the active window's place covers on the same display. Chat windows' `follow_app` subscription skips `cx.notify()` while its own window is held. `chat_window_placed` redraws a window that stops being held, and has to run when any window moves, not only the active one. Keep every exclusion in `cover.rs`: Linux, fullscreen, Windows hover activation. Once held and not drawing, its content stops asking for ticks, so the clock goes quiet by itself.
- Proof plan: extend `chat_window::cover::tests` with the `frames_over` helper: two chat windows, one covered by the active one, assert 0 frames for the covered one and equal frames for the active one, then move it out and assert it resumes at once. Check `a_call_window_keeps_animating_under_a_covering_chat_window` still holds.
- Scope: `src/ui/chat_window/cover.rs`, `src/ui/frame_clock.rs`, `src/ui/chat_window/view.rs`. Medium risk: a visible window that holds still is the failure to avoid, as in #1264's first version. Overlaps the render lane.

## idle-7: The TDLib receive thread wakes five times a second only to notice shutdown
- Severity: P2 measurable waste
- Kind: power
- Platforms: all
- Where: `src/telegram/hub.rs:31` `RECEIVE_TIMEOUT_SECS = 0.2`, `:388` `source(RECEIVE_TIMEOUT_SECS)`, `:35` `EXIT_STOP_WAIT`, `:324` `stop_shared_at_exit`. One thread for all accounts.
- Trigger: always, in every state, also with the window gone.
- Effect and size: 300 wakes a minute, about 70% of the rest set by count (inferred). Cost per wake is tiny. It matters because it sets the floor: nothing else at rest wakes more than twice a second.
- Why it is real: the timeout has one use, noticed in the comment at line 31: shutdown. An update ends `td_receive` at once regardless of the timeout.
- Fix sketch: use a long timeout, such as 60 s, and end the wait on stop by making TDLib answer. `td_send` to a client id that is not open should come straight back as an error `Invalid TDLib instance specified`, which wakes `td_receive`. That is from memory, and no TDLib source is on this Mac, so check it first against the real library. The atexit path must keep its guarantee: `stop_within(EXIT_STOP_WAIT)` must end the thread before TDLib tears down, or `td_receive` reads freed memory (`tdlib-exit-teardown` memory, #1022). If the error path does not wake it, leave this alone.
- Proof plan: a test against the real `libtdjson`: a thread in `td_receive(60.0)`, send to an unused id, assert it returns and the hub stops, driven by a channel and no sleep. Rerun the child-process quit stress from `tdlib-exit-teardown`.
- Scope: `src/telegram/hub.rs`, `src/telegram/ffi.rs`. Risk is medium for a small gain, a crash at exit being the failure. Do it only with that stress run.

## idle-8: On macOS a window minimized with the tray on polls every second and hides the whole app
- Severity: P2 measurable waste, and a user-visible bug
- Kind: robustness
- Platforms: macOS only
- Where: `src/main.rs:450-472`: the tray loop checks `isMiniaturized`, sets `wait = 1 s`, and calls `cx.hide()`.
- Trigger: Run in the background on, and the yellow button minimizes the main window. Cmd-M takes a different path (`app_render/mod.rs:216`) and hides at once.
- Effect and size: within 10 s the whole app hides, then 60 more `cx.hide()` calls a minute for as long as the window stays minimized (120 thread wakes a minute). With a chat window open it gets worse: AppKit does not restore the minimized window when another window is visible, so Dock or Cmd-Tab brings the chat window back and it vanishes within a second. A chat window cannot be used while the main window is minimized. Label: inferred from code, not run.
- Why it is real: the comment says "a minimized window has no event to announce its state", yet the occlusion change does fire for a minimize, and GPUI forwards it (`third_party/gpui-pre-macos/src/window.rs:3080`, `Window::observe_window_visibility`).
- Fix sketch: replace the polling with `observe_window_visibility` on the main window. On a change to not visible, if `isMiniaturized`, hide once, and only if no chat window is open. Drop the 1 s wait.
- Proof plan: pure decision function `minimize_hides_app(miniaturized, tray_on, chat_windows_open)` with a table test. Mac: minimize with a chat window open, and the chat window stays.
- Scope: `src/main.rs`, a small helper in `src/tray`. Low risk.

## idle-9: Every account's updates wake every account's poll loop
- Severity: P3 minor
- Kind: power
- Platforms: all
- Where: `src/telegram/hub.rs:94` pokes `wake::UPDATES` for any route. `src/ui/notifications/mod.rs:216` and `src/ui/background_accounts/mod.rs:396` both wait on it.
- Trigger: a background account is signed in.
- Effect and size: each envelope from any account runs and ramps both loops, doubling idle-4's polls with one background account. `poll_background` walks all clients each time. Label: inferred. After idle-4 this falls to one extra poll per update.
- Why it is real: the hub knows the route but pokes one global signal.
- Fix sketch: routes carry their signal: `UPDATES` for the on-screen client, a new `BACKGROUND_UPDATES` for the rest. An account switch swaps them under the same lock as the route.
- Proof plan: hub test with fake clients: delivery for a background route moves only its signal's epoch, and the same after a switch.
- Scope: `src/telegram/hub.rs`, `src/wake.rs`, `src/ui/background_accounts/mod.rs`. Low risk. Do after idle-4.

## idle-10: A paused track ticks four times a second, forever
- Severity: P3 minor
- Kind: power
- Platforms: all
- Where: `src/ui/audio_playback.rs:305`
- Trigger: a voice note or song left paused in the player bar.
- Effect and size: 240 main-thread wakes a minute and 240 timer wakes, more than the whole rest set, for as long as it stays paused. In any state, hidden included. Label: inferred. #1257 removed the redraw from this tick but kept the tick.
- Why it is real: `idle-inactive-redraw.md:22` keeps it for media keys, and the code also uses it to see Previous and Next change. Both have events: the OS media callback, and `CHROME` after applied updates.
- Fix sketch: wait with `sleep_or_wake` on `CHROME`, plus a poke from `media_session`'s command callback, with a 250 ms timeout while playing and none while paused. Publish Now Playing on change.
- Proof plan: test with the test clock: paused for a simulated minute runs 0 ticks, a media key and a chat-list change each run one, and playing runs 4 a second.
- Scope: `src/ui/audio_playback.rs`, `src/media_session/mod.rs` and its three backends. Low risk.

## render-4: File progress in the open chat redraws the chat list and composer too
- Severity: P2 measurable waste
- Kind: cpu, power
- Platforms: macOS, Windows, Linux
- Where: `src/state/redraw.rs:220-238` `file_need` grades any `updateFile` in `open_chat_media_downloads` or a user download as `Now` → `src/ui/notifications/mod.rs:272` `PolledAction::Full => cx.notify()` → `observe_self` → `notify_slices` (`src/ui/app_slice.rs:192-195`) renders every slice. `auto_download_media_file_ids` (`src/state/session_files.rs:570`) covers every loaded message, not only those on screen.
- Trigger: any chat with auto-downloaded media open, so scrolling a media channel or watching a clip that is still downloading. Also other open-chat updates that fall to the `_ => Now` default, such as `updateMessageInteractionInfo` on a busy channel.
- Effect and size: measured split of a full redraw from `p2-idle-sample.txt` (tree and MSL on, so shares only): sidebar prepaint 26% of draw, history list 30%. The update rate is a guess: `docs/decisions/codex-perf-pass.md:194` shows `ingested: 8.0/s UpdateFile` only as a sample log line, not as a measurement of this case. Expect about that for one file and more with several; the poll's 10 ms cadence while busy is the upper limit. A worker should read the real rate first with that same trace. While a clip streams, each update replaces a layer-only frame with a full render of every slice. Rows off screen show no progress at all, so those redraws are entirely waste.
- Why it is real: `revision_without_file_progress` already keeps the rows cache from reacting, but the redraw grade has only `ChatList`, `Later` and `Now`, so a change only the conversation shows has no narrower option. `Later` applies only to files not in the open chat.
- Fix sketch: add `RedrawNeed::Conversation` and `PolledAction::Conversation`, which call `notify_conversations`. Grade in-flight progress of `open_chat_media_downloads` as `Conversation`; keep `completes` and `is_user` at `Now`, since a finished picture can show elsewhere and user downloads show in the Downloads panel. Then consider grading interaction info for the open chat the same way. Second step: grade `Later` when the file's row was not painted last frame (`history.painted_ids` plus a file id per row), so off-screen downloads cost at most one redraw a second.
- Proof plan: table test in `src/state/redraw.rs` for each grade. A `PolledRedraw` test that `Conversation` does not hold back a pending `Now`. Then a demo that injects `updateFile` progress for a visible and an off-screen file while a clip plays and reads slice renders per second with the existing slice trace: sidebar 0/s.
- Scope: `src/state/redraw.rs`, `src/ui/notifications/mod.rs`. Low risk, narrow. Touches `poll_live` next to the engine and ui-thread lanes, but only the grade.
