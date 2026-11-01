CHANGES

# Review of PR 1306 (calls-media, head 66a4401) against main (5072e96)

Scope: the PR diff and its description. Nothing on the PR branch was changed. The probe test below ran in a local worktree and was then removed.

## Findings

### 1. A pause reported while a screen share is opening is lost (medium, confirmed)

File: `src/connect/call_screen.rs`, lines 405 and 438 (`pump_shared_screen`).

`pump_shared_screen` reads the capture's events (`screen_change`, line 405) before it reads the device thread's answer (`finish_open`, line 438). Events pushed while the capture is still opening are applied first. Then `finish_open` succeeds, `set_screen_paused(false)` runs, and the pause is overwritten. The event is already used up, so nothing restores it.

Before this PR the open was synchronous, so `set_screen_paused(false)` always ran before any event could be read, and a later pump applied the pause.

Failure scenario: on X11 you share a minimised window. `screen_x11::run` checks `_NET_WM_STATE_HIDDEN` one frame interval after `start` returns and pushes `Paused`. If the UI thread is busy and reads the open answer after that event arrives, both are handled in the same pump. The same happens with the 500 ms window monitors on macOS and Windows when the answer takes longer than 500 ms to read. Result: TDLib gets `toggleGroupCallScreenSharingIsPaused(true)` and then `(false)`. The panel shows the share as live. The feeder forwards frames of a window that the OS reports as hidden. This stays wrong until the window's state changes again.

Proof: a local probe test made a fake opener that blocks, pushed `CaptureEvent::Paused` into its sink, released the open, and pumped. Output: `paused=false toggles=[true, false]`. The test asserted `paused()` and failed.

Fix: call `finish_open` before `screen_change`. Or keep the paused state that events produced, and have `finish_open` apply that state (`set_screen_paused(active.paused)`) instead of `false`.

### 2. The visibility callback does not check which window reports (low, plausible)

File: `src/ui/group_call_window.rs`, line 188 to 192.

The `observe_window_visibility` callback writes `app.group_call.window_visible` without checking that its window is still `app.group_call.window`. When the window is replaced (the layout-switch path at line ~107 removes the old window and opens a new one), the `Ok(handle)` branch sets `window_visible = true`. If the old window then reports "hidden" while it is removed, the new, visible window's pinned tile drops to Thumbnail. It stays there until the new window's visibility changes. I did not reproduce this, because there is no windowing system here. Comparing the callback's window handle with `group_call.window` removes the risk.

### 3. A failed device thread does not wake the poll loop (low)

File: `src/calls/capture/device_thread.rs`, line 26 to 44.

The thread pokes `wake::UPDATES` after a successful or failed `open`. Two paths answer without a poke:

- `open` panics. `spawn_guarded` catches the panic, `answers` is dropped, and `opened()` reports "The capture thread stopped."
- The spawn fails (line 42).

In both cases the error appears only on the next poll that something else causes, or when the poll pace ceiling runs out. This is the waiting that subsystems-7 removes for the other paths. Fix: poke in the spawn-failure branch, and send the answer plus poke from a drop guard inside the thread.

### 4. A test passes without its fix in a parallel run (low, test quality)

File: `src/connect/tests/call_capture.rs`, line 378 (`a_lost_microphone_needs_no_telegram_update`).

`wake::UPDATES` is process-wide. Any other test that pokes it between line 376 and line 378 makes the `assert_ne!` pass even when `push_event` has no poke. The PR description says so and says the removal check ran on one thread. Under the default parallel `cargo test`, this assertion does not reliably fail without the fix, which the repo's test rules require. A per-sink wake hook, or a `Signal` the test can inject, would make the test deterministic.

### 5. `warm_devices` runs for calls you are not in (nit)

File: `src/connect/call_capture.rs`, line 308 (`warm_call_devices`).

The gate is "`active_call` or `active_group_call` is tracked". A scheduled video chat, or one you have not joined, is tracked as soon as its `updateGroupCall` takes the slot (`session_calls.rs`, `accept_group_call_update`). So opening such a chat lists cameras and cpal devices on a worker. That costs about 226 ms of framework load on the Mac and is unmeasured on ALSA, and it happens with no call coming. This is harmless on macOS. Gating on `is_joined`, or on an outgoing or incoming 1:1 call, matches the doc comment ("A call is coming").

## Checked and found sound

- **Device threads outliving a call.** Dropping `ActiveCapture` or `ActiveScreen` drops the `DeviceThread` sender, and the thread closes the device after its open ends. The test `a_device_dropped_while_opening_closes_when_the_open_ends` covers this. A stale open cannot land: `finish_open` reads only the current `active.mic` or `active.capture`.
- **Mic reopen.** One `active` slot. `failed_at` gates the 2 s reopen. `until_reopen` feeds `next_deadline`. The background account loop has a 2 s ceiling. I found no path that opens twice or never reopens.
- **Share end and the UI.** The `shown` tuple in `pump_call_media` covers target, error and paused, so an Ended or failed open from `request_tick` redraws.
- **Envelope skipping.** `ParticipantList::edit` is the only mutator, `From` gets a fresh revision, and `Clone` keeps the revision with the same contents. The inputs for feeds (revision, my video, sharing, left call), video sources (revision, story sources, large tile) and ssrc owners (revision, `loaded_all`) cover everything their functions read. The memo is cleared while the transport is down and on teardown, and it is set only after `sync_group_video` succeeds. The ssrc memo is set before the engine call, but the engine creates the instance in `create_group` before connect, and the memo is re-armed on every pump that is not connected.
- **Quality.** The large tile is `pin.pinned()`. The panel lays out `layout.main` from the same pin after `retain_available`, so "pinned" and "shown large" agree, including on the full-screen stage. Stream mode forces `large: true` on the one chosen source, and RTMP uses unified. A live story is a stream, so its `large: false` is overridden.
- **Rotation.** I checked the index math of `rotate_quarter` by hand for R90 and R270. Remainder columns `across..w` above `down`, and all rows below `down`, go through the pixel path. R180 reverses rows. Odd chroma sizes keep `div_ceil`. `every_size_turns_pixel_for_pixel` passes. No new `unwrap`, `expect` or indexing outside `#[cfg(test)]`.
- **Platforms.** The Linux build compiles, including `screen_linux`, `screen_x11`, the `cfg(any(windows, test))` NV12 arm and both clippy configurations. Windows cannot be cross-checked here because there is no mingw or MSVC target. By reading: `open_loopback`, `native_backend` and `start_loopback` keep their signatures. The device is created and dropped on its own thread, so `D` needs no `Send`. The closures capture only `Arc`s, `String`s and `FrameSink`s.

## What ran

All of these ran on Linux x86_64 against the PR head in a separate worktree:

- `cargo fmt --check`: clean.
- `cargo clippy --features demo-capture --all-targets -- -D warnings`: clean.
- `cargo clippy --no-default-features --all-targets -- -D warnings`: clean.
- Library tests, filtered:
  - `call_capture`: 11 passed.
  - `call_screen`: 14 passed.
  - `device_thread`: 4 passed.
  - `group_calls`: 113 passed.
  - `convert`: 26 passed, 2 ignored.
  - `group_config`: 4 passed.
  - `live_`: 123 passed.
  - `call_audio`: 8 passed.
  - `calls`: 450 passed.
  - `pump_gates`: 4 passed.
  - `screen`: 71 passed.
  - `tile_pin`: 6 passed.
  - `large_tile`: 1 passed.
- Binary tests (`--features demo-capture`), filtered:
  - `group_call`: 59 passed.
  - `video_player`: 188 passed.
  - `live_`: 99 passed.
- Probe test for finding 1: failed as described, then removed.

## Not verified

- Windows compile.
- Real ScreenCaptureKit, WASAPI or PipeWire on the new threads.
- Real window occlusion events.
- Bandwidth or decode savings from Thumbnail.
