Notice a lost microphone at once, open call devices without freezing the app, and stop fetching sharp video nobody can see

## What was wrong

subsystems-5, remote video fetched at a quality nobody could see. Every camera was asked for at Medium and every screen at Full, whatever its tile size, and also while the call window was minimized, covered or closed. The off-screen policy in the player saved conversion and redraw, but network, decode and the per-frame plane copy went on.

subsystems-6, starting a call or a share blocked the UI thread on devices. The microphone open, the screen capture start (5 s plus 5 s limits on macOS) and the system audio start all ran inside the driver pump on the UI thread. The first device list of a process also ran there: on this Mac 226 ms for the cameras alone.

subsystems-7, a lost microphone or an ended share waited for an unrelated Telegram update. The capture events were read only inside `ingest`. In a connected call on a quiet account the other side heard nothing and the panel still showed a share that had ended, until some update arrived. The 2 s reopen had the same wait.

subsystems-8, portrait phone video turned one checked pixel at a time. Every frame also got a new buffer. Frames up to 800 px on the long side do this on the UI thread.

subsystems-11, every Telegram update repeated group call work. Any envelope, for any chat, re-ran the feed sync with a scan per track, rebuilt the video sources, built the request JSON, and copied the 0.35 MB screen preview before checking whether it was new.

A review of the first version of this PR found five more problems, all fixed here:

1. A pause reported while a screen share was still opening was lost. The pump read the capture's events before the open's answer, and a successful open then unpaused unconditionally. Sharing a minimised window on X11, or a window the macOS or Windows monitor reported hidden before the open answer was read, sent `toggleGroupCallScreenSharingIsPaused(true)` and then `(false)`. The panel showed the share as live and the feeder forwarded frames of a hidden window.
2. The call window's visibility callback did not check which window reported. When the layout switch replaced the window, the old one could report "hidden" as it was removed, and the new, visible window's pinned tile dropped to Thumbnail.
3. A device thread that panicked in its open, or failed to spawn, answered without waking the poll loop. The error waited for the next poll some other event caused.
4. `a_lost_microphone_needs_no_telegram_update` read the process-wide wake counter, so in a parallel run it passed without the poke it guards.
5. Devices were warmed as soon as a group call was tracked, which includes a scheduled video chat or one you only opened.

## What changed

Subsystems-5: `GroupVideoSource` carries `large`, and `requested_videos` follows tdesktop's table: Full for the large tile (a large screen has Full as its floor too), Thumbnail for the rest. Quill's small tiles are at most 120 px tall and tdesktop makes tiles under 240 px Thumbnail, so no tile sizes need reporting. The large tile is the pinned one while its window can be seen; the panel publishes it on render, on a visibility change and on close, and the driver applies it at once. A stream stays Full, as tdesktop's `forceFullQuality`. With one Full video at most, tdesktop's budget cannot trigger, so that code is gone. A minimized, covered or closed window dropping to Thumbnail goes past tdesktop, which has no such rule. It keeps the subscriptions, so a tile cannot get stuck and comes back on the next visibility change. Only the current call window's visibility reports count; a window that was replaced is ignored.

This changes what you see: an unpinned camera or screen in the panel now comes in at Thumbnail, as in tdesktop's narrow panel, where it was Medium or Full. Pin a tile or open the stage to get Full.

Subsystems-6: `calls::capture::device_thread::DeviceThread` opens, runs and closes one device on a thread of its own and pokes a wake signal with its answer. The answer goes through a guard that pokes after the answer or the hang-up is readable, so an open that panics wakes the poll loop too, and a failed spawn pokes after sending its error. The microphone, the screen capture and system audio all use it, so none of them blocks the pump, and each closes off the UI thread too. A device that answers after you left closes on its thread. `CallEngine::warm_devices` lists cameras and audio devices once on a worker when a 1:1 call rings or is placed, or when you are joined to a group call, so the first list's framework load no longer lands on connect. Opening a scheduled video chat, or one you are not in, loads nothing.

Subsystems-7: `request_tick` runs `pump_call_media`, which reads both captures and reports whether the call window changed. `FrameSink::push_event` pokes the signal its sink was made with (the poll loop's by default), and `next_deadline` wakes it for the 2 s reopen. When a screen capture's open finishes, the share takes the paused state its events produced while it opened, instead of always starting unpaused.

Subsystems-8: whole 8×8 blocks go through a word transpose, a half turn copies rows, and each feed keeps one buffer. Rotated frames stay on the UI thread at their small size, because a 640×360 turn now costs about 60 µs.

Subsystems-11: `ParticipantList` replaces `Arc<Vec<..>>` on `ActiveGroupCall`. `edit()` is its only mutator and moves a revision, so a missed bump cannot compile. `GroupCallSeen` remembers per call what the pump last acted on: the feed sync, the conference ssrc owners and the incoming video are skipped while their inputs stand still. The memory clears while the transport is down, so a rejoin's new instance gets its subscriptions, and a failed sync is tried again. The feed sync builds one set instead of scanning per track. `PreviewSlot::latest_after` checks the sequence number before the copy. The finding's rank map in `sort_participants` is left out: `recent_speakers` holds at most 3 entries (`schema/td_api.tl`), so `position()` costs next to nothing.

## Measurements

All on the Mac, under a build load of about 26, counting instructions and cycles. There was no A/B build of the app: the numbers below come from standalone programs and from the shipped call library.

Rotation, per frame, standalone `rustc -O3` program with exact copies of the old and new code, 500 frames. The finding guessed 3 to 7 ms; this Mac took 0.5 to 1.6 ms.

| Frame | Turn | Before | After | Cycles before | Cycles after |
| --- | --- | --- | --- | --- | --- |
| 1280×720 | R90 | 545 µs | 216 µs | 839 M per 500 | 337 M |
| 1280×720 | R270 | 775 µs | 216 µs | 1122 M | 338 M |
| 1280×720 | R180 | 860 µs | 55 µs | 1154 M | 93 M |
| 720×1280 | R90 | 1260 µs | 219 µs | 1762 M | 341 M |
| 720×1280 | R270 | 1594 µs | 229 µs | 1894 M | 356 M |
| 640×360 | R90 | 139 µs | 62 µs | 221 M | 98 M |
| 640×360 | R270 | 189 µs | 55 µs | | |
| 640×360 | R180 | 187 µs | 21 µs | | |

Buffer reuse alone saves 9 to 16 µs per frame here.

Group call pump, per envelope, standalone program with copies of the feed sync and source rebuild over a list shaped like the real participants, one in ten with a camera. The request JSON is not in these numbers and was on top of the "before" cost. The finding guessed 60 µs, 540 µs and 6 ms; this Mac is about five times quicker per scan step.

| Participants | Before, instructions | After, unchanged call | After, one participant changed |
| --- | --- | --- | --- |
| 100 | 183 K | 61 | 47 K |
| 300 | 1.41 M | 61 | 143 K |
| 1000 | 14.7 M (about 0.9 ms) | 61 | 450 K |

First device list in a process, through the shipped call library: cameras 226 ms, then 0.05 ms. The audio devices' 100 ms and 4 ms are from the sweep's earlier measurement. Both now run on a worker before connect. The review's five fixes give no number to measure.

## Tests

I removed each fix in turn and watched its tests fail, with the exceptions listed under the tests.

- `every_size_turns_pixel_for_pixel`, `turning_again_reuses_the_buffer`, `a_turned_feed_converts_through_one_buffer`
- `an_unrelated_update_leaves_feeds_and_subscriptions_alone`, `a_rejoin_subscribes_its_new_instance_with_nothing_changed`, `a_failed_subscription_is_tried_again_on_the_next_update`, and `preview_slot_publishes_at_most_four_times_a_second` for `latest_after`
- `a_lost_microphone_needs_no_telegram_update`, `a_closed_window_ends_the_share`
- `a_small_tile_asks_for_a_thumbnail_and_a_large_one_for_full`, `the_large_tile_alone_is_asked_for_more_than_a_thumbnail`, `the_large_tile_counts_only_while_its_window_can_be_seen`
- `a_slow_microphone_does_not_hold_the_pump`, `a_slow_screen_capture_does_not_hold_the_pump`, `a_microphone_that_opens_after_you_left_closes`, `a_call_you_are_in_warms_the_devices_once`, and the `device_thread` tests

For the review's fixes, each test was written first and seen failing on the unfixed code:

- `a_window_hidden_while_its_share_opens_stays_paused` (finding 1): the capture reports Paused while its open is blocked. Before the fix the share read unpaused and Telegram got a pause and then an unpause.
- `a_replaced_call_window_hiding_leaves_the_large_tile` (finding 2): another window takes the call window's place and the old one reports hidden. Before the fix the pinned tile was no longer large.
- `a_panicked_open_answers_and_wakes_the_poll_loop` (finding 3): the open panics. Before the fix the injected signal never moved.
- `a_lost_microphone_needs_no_telegram_update` (finding 4) now injects a signal only its sink pokes. With the poke in `push_event` removed, it failed in each of three parallel runs of 607 tests.
- `a_call_you_are_in_warms_the_devices_once` (finding 5): a tracked call you have not joined warms nothing. Before the fix it warmed once.

`every_size_turns_pixel_for_pixel` compares the blocks with the pixel rule, so it passes on the old code too. The conference owners check has no test that fails without it: the engine already ignores equal owners, so it saves only the scan. `a_microphone_that_opens_after_you_left_closes` and the other `device_thread` tests were not run with the fix removed, because they wait with no clock and would hang. The spawn-failure branch of `DeviceThread::open` has no test, because a thread spawn cannot be made to fail here.

New tests use no sleep, timeout or wall clock. Tests that wait on a device thread yield in a loop.

Ran on Linux x86_64 for the review's fixes: `cargo fmt --check`, `cargo clippy -- -D warnings` for `--features demo-capture --all-targets` and `--no-default-features --all-targets`, `scripts/check-hotspots.sh`, and the library tests `call_capture` (11), `call_screen` (15), `device_thread` (5), `feeder` (32), `group_calls` (113), `calls` (451), `screen` (72), `pump_gates` (4), and the binary tests `group_call` (60) and `video_player` (188). All passed.

## Left out

The call's sound output still opens on the UI thread. `open_sound` and `open_group_sound` run inside the engine's connect, and moving them needs a slot in the engine, a generation for stale opens, and volume changes during the open. The earlier measurement was 29 to 36 ms for a reopen and 88 to 119 ms for a first open; the warm-up moves the first-use part off the thread.

`refresh_call_devices` still runs on the UI thread at connect, now against warm frameworks (about 4 ms on this Mac). `qc_video_input_devices` opening a camera inside the library is not covered. ALSA's list may cost more than the Mac's, unmeasured.

A group call warms the devices once you are joined, not when you press Join, because the join request leaves no state to gate on. The warm still runs before the microphone opens, which waits for the transport.

## Not verified

There was no real call, microphone, camera or screen capture here. Everything about the driver and engine goes through `MockEngine` and fake devices.

- The bandwidth and decode saved by Thumbnail are unmeasured, and so is how Thumbnail looks in a real tile.
- `requested_videos`' JSON is tested as text; no server has answered it.
- ScreenCaptureKit, the Windows capture and WASAPI loopback have not run on the new thread. Their code reads as thread-safe: the Windows pool is free-threaded and the Mac start already waits on another queue's reply.
- The `cfg(windows)` lines have not compiled: a Windows cross-check of the library stops at `ring`'s C build on the Mac, and the Linux machine that ran the review's fixes has no Windows target. `open_loopback` and the other platform openers keep their signatures, and the new thread needs nothing from them beyond what the Linux and Mac paths compile. The review's fixes touch no platform code. The Linux build compiles, both clippy configurations included.
- A covered window counts as hidden only where the OS says so. Windows has no such report, so there a covered window keeps its Full tile. Wayland and composited X11 report less too.
- A real layout switch that replaces the call window, and real window occlusion events, were not driven. The test stands in another window as the call's current one.
