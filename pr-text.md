Hang up when quitting mid-call, and stop call endings, the microphone and big voice chats from stalling the app

## What was wrong

Quitting or switching accounts during a call never hung up. `begin_close` sent `close` and nothing for calls, so no `discardCall` and no `leaveGroupCall` went out. The other person heard silence until their transport gave up, and a voice chat kept listing you until the server dropped you. tdesktop hangs up first (`Calls::Instance::isQuitPrevent`).

Ending a call ran the library's destroy on the UI thread with no time limit. `qc_p2p_destroy` and `qc_group_destroy` wait for callbacks in flight, so one stuck callback froze every window. At the seed commit, a blocked signaling hook against the real library held `hangup` past 15 s. A normal hang-up froze the windows for 13 to 35 ms and a voice chat leave for about 22 ms. Quit paid the same wait past `QUIT_CLOSE_DEADLINE`, so a wedged call also wedged quit. `qc_group_join_payload` waited on a future with no limit and no guard for a broken promise.

Opening the microphone ran on the UI thread. Pressing record, replying to a story by voice, resuming a paused note and Test microphone all went through cpal inline, and so did the voice chat's level tap on the driver. On this Mac the device query alone takes 68 to 98 ms as a process's first audio call, before any stream opens. tdesktop runs capture on its own thread.

A voice chat rebuilt its whole member list on every frame. The panel built one row per loaded person on each video frame and speaking tick, and copied the participant list twice per render, so the cost grew with who was loaded (100 a page) and not with what was on screen. Speaking ticks also went to the whole app, so a sound in the voice chat redrew every window.

## What changed

`begin_close` now ends the account's calls and leaves a call still ringing in alone, as tdesktop does. `close` waits behind the hang-up until the terminal `updateCall` or the leave's `ok` arrives, or for 500 ms (`HANG_UP_HOLD`), which leaves it the rest of the quit deadline. The hold waits for the state because `discardCall` answers before the server has heard it. An account switch closes a client in a call the same way. `docs/decisions/tdlib-close-off-ui-thread.md` has the details and where this differs from tdesktop.

A one-thread reaper (`src/calls/engine/tgcalls/reaper.rs`) takes each finished call or group instance. The caller retires the instance first, so nothing it still reports reaches the app, and returns at once. Quit waits for the reaper up to the close deadline and leaves with `_exit` if a teardown is stuck, since static destructors would crash under it. The retry path and the failed create path use the same hand-off. `qc_group_join_payload` gives up after 2 s and reports no payload. It still runs on the UI thread, where it takes about 2 ms.

`MicThread` in `src/cpal_stream.rs` owns a device on a thread of its own for its open, its commands and its close, and `MicStream` and `LevelTap` use it. `QuillApp::start_voice_capture` opens in the background for messages and story replies, and the recording bar shows once the device is up. `CaptureStart` replaces `recording.starting`, so a capture on its way up can be cancelled: leaving the chat, closing the window or closing the story reply drops what arrives, along with its file. Test microphone opens in the background too, and the level tap opens on a worker whose answer the pump picks up. Discarding a capture runs off the UI thread as well. `recording.open_voice` and `CallAudio::open_microphone` are the test seams.

`GroupCallMembers` (`src/ui/group_call_members_view.rs`) is a cached view holding the rows, so a video frame redraws the panel and its tiles while the rows replay. It builds only the rows in the scrolled window plus two past each end, however many people are loaded. A scroll or a resize rebuilds it, since the cache key holds the view's bounds and clip. `participants` is an `Arc<Vec<_>>`, so a snapshot of the call copies no list. A level tick goes to the panel or the rows alone through `request_window_view_tick`, and a covered or inactive main window does not hold it back. Show Chat scrolls past the list to the messages, which a long list would otherwise hide.

## Measurements

Hang-up on the UI thread, real library from CI run 41248201362, 15 runs per protocol version, debug test build, same Mac:

| | Before (seed) | After |
| --- | --- | --- |
| `hangup`, median across versions | 13 to 35 ms | 3 to 5 µs |
| `hangup`, longest | 104 ms | 437 µs |
| Group leave, median | 22 ms | 149 µs |
| `hangup` with a callback stuck | did not return in 15 s | 36 µs |

The teardown itself now runs on the reaper thread, which drained a voice chat leave in about 21 ms. Group create and payload went from a median of 1.2 ms to 2.0 ms, and to 8.4 ms in a run where the reaper was still working through 135 earlier call teardowns. The 8.4 ms is that queue: with the reaper idle before each create it stayed at 2.0 ms. So a rejoin right behind a leave can wait a little longer, by less than the 22 ms it no longer freezes for.

Voice chat with 102 people and one camera, fast release builds of 570687682 and this branch, base with only the crowd demo added, alternating runs, both build slots held, 60 Hz display. CPU is the whole process over a fixed 10 s stretch:

| | Base | Branch |
| --- | --- | --- |
| Process CPU, 3 runs (% of a core) | 24.6, 26.0, 29.3 | 5.7, 6.4, 10.2 |
| Panel renders per second | 22 to 27 | 22 to 27 |
| Member rows built per second | about 2,450 | 0 |

Both builds draw the same camera at the same rate, so the gap is the rows and the two list copies per render. The base count is computed, 102 rows times 24 renders, because base has no counter on a row. The branch count comes from the trace, which shows no row built in the stretch. Relative numbers only: `fastrel` has no LTO and 16 codegen units.

Device query as a process's first audio call, 5 fresh processes of a small cpal 0.18.2 program: 68 to 98 ms, median 72 ms, then about 0.3 ms within the process. This is a lower bound for what used to freeze the windows, since it stops before opening a stream.

## Tests

Each test asserts what the user gets, with no sleep or clock. To check they catch the bug, each fix was taken out once. Five closing tests, `pressing_record_returns_before_the_microphone_is_opened`, `the_microphone_opens_on_a_worker_and_the_pump_picks_it_up` and three member list tests failed, and `a_tap_opened_after_the_call_stopped_wanting_it_is_dropped` hung, because the pump waited on the blocked device.

- `src/connect/tests/closing.rs`: closing sends `discardCall` or `leaveGroupCall` and holds `close` behind the state. A call ringing in gets no hang-up, an outgoing call still ringing does, and a call only looked at is not left.
- `hanging_up_does_not_wait_for_the_library`: a stuck callback leaves `hangup` free, against the real library. It skips itself where the library is not built, and `reaper.rs` has its own tests for order and the deadline.
- `src/ui/recording_start_tests.rs`: pressing record returns before the microphone is asked for anything, a capture is held once it is up, and one cancelled meanwhile leaves nothing. `a_microphone_that_comes_up_after_its_window_closed_is_dropped` covers the closed window.
- `src/ui/group_call_tiles_tests.rs`: with 102 people, video frames build no member row, a change to the call builds fewer than 30, the wheel builds the rows it brings in, and Show Chat scrolls past the list.

At the final head, with the real calls library on this Mac: 2,354 core tests under `connect`, `calls`, `voice` and `state` passed, and so did 118 app tests under group calls, recording, chat windows, `cpal_stream` and the frame clock. `tools/quick-gate.sh` passed. The two call window demos, with 2 and 102 people scrolled part way down, were captured and read in light and dark: the rows sit in place with no gap at either edge of the list.

## What did not run

- No real call. A hang-up reaching the server and the other person seeing the call end come from the recording sender only.
- No microphone was opened, so the recording bar's real delay and the end-to-end press to first audio are not measured.
- The Mac run used the CI library, which predates the `group.cpp` change. The 2 s limit in `qc_group_join_payload` is built and run only by the library's CI job.
- The full suites did not run here. PR CI runs them.
- The captures show still frames. Row hover, a member's menu and the feel of a live scroll were not checked by eye.
- Windows and Linux were not run. `cpal::Stream` is `Send` here with `default-features = false`, and the `MicThread` design keeps each stream on one thread on every platform.
