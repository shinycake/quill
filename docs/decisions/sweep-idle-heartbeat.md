# No idle heartbeat on X11 and Windows

Quality sweep item 19 (`docs/decisions/quality-sweep.md`): X11 and Windows kept
waking a parked window once a second, where the macOS display link stops.

## Was the finding right

Yes. `codex-idle-frames-x11-windows.md` parked idle frame sources on both
platforms but kept a one-second heartbeat as a fail-safe:

- X11 (`third_party/gpui-pre-linux/src/linux/x11/client.rs`,
  `start_refresh_loop`): a parked window's refresh timer re-armed with
  `TimeoutAction::ToDuration(PARKED_HEARTBEAT)`, so the main thread woke, ran
  a no-op frame callback and polled X events every second per window.
- Windows (`third_party/gpui-pre-windows/src/platform.rs`,
  `begin_vsync_thread`, and `FrameGate::wait_for_frames` in `vsync.rs`): with
  every window parked the vsync thread slept on a condition variable with a
  one-second timeout, then waited for a vblank, checked its device and
  invalidated every window, so each one got a `WM_PAINT` and a frame callback
  on the main thread.
- Wayland already draws on demand (frame callbacks, plus a one-shot retry
  timer when a frame is throttled). Unchanged.

Measured on CI's Linux launch smoke (Xvfb + lavapipe, `ready-chats`, 10 s of
an idle window, main thread voluntary context switches from `/proc`): the main
thread woke exactly 10 times, all of them the heartbeat. Nothing else in the
demo fixture wakes it.

Quill-side timers were checked too. The demo fixture starts none that wake
the main thread (the count above). A signed-in session does, on every platform
including macOS: the TDLib poll loop (backs off to 120 ms,
`src/ui/notifications/mod.rs`), the deep-link inbox (250 ms, `src/main.rs`),
the tray and badge sync (1 s, `src/main.rs`), the passcode tick (1 s,
`src/ui/passcode/mod.rs`), the OS menu refresh (1 s, `src/ui/os_menus/mod.rs`)
and the battery probe (5 s, `src/ui/battery_watch.rs`). About 15 wakeups a
second together, which makes them the idle floor of a real session, well
above the heartbeat. They are outside this change (other files, other owners) and listed
as a follow-up below.

## What changed

The vendored crates record it in their `QUILL-CHANGES.md`.

Shared (`frame_idle.rs`, identical in both crates, std-only, tested by
`scripts/test-frame-idle.sh` in CI):

- `PARKED_HEARTBEAT` is gone. A parked source delivers no frames until a frame
  demand.
- `next_frame_at(parked, due, refresh, now)`: the X11 timer's next deadline.
  The refresh grid after `now` (missed frames skipped, never `now` itself,
  which fired the timer twice when a frame ended exactly on a grid instant), or
  `None` while parked. A zero refresh interval falls back to 60 Hz instead of
  spinning (upstream's `while instant < now { instant += refresh }` never
  ended for a zero interval).
- `FrameIdle::park`: the platform parks a source for its own reason (hidden,
  minimized), so the next demand reports a resume.
- `vsync_has_work`: when the Windows vsync thread may sleep.
- `contact_frames`: what a Windows touchpad contact needs (below).

These decisions sit in the shared file so they can be unit tested on any
host; the platform crates only build on their own OS.

X11:

- A parked refresh timer drops itself. The frame waker's ping starts a new
  one, as before. `X11FrameIdle` numbers the window's timers and records
  whether the current one is still registered, so the paths that remove the
  timer (hide, resume, window close, connection drop) skip one that already
  dropped itself, and a timer replaced while its own callback ran stops.
- With no timer left to process X events, events that xcb reads into its
  queue during a request (rather than from the socket becoming readable) could
  wait for the next unrelated wakeup. 0.3.8 already drains the queue after
  every runnable; `LinuxPlatform::run` now also runs that drain once per event
  loop round, before the loop sleeps, which covers requests made from other
  event sources' callbacks.
- `mode_refresh_rate` no longer divides by zero for a dot clock below 1 mHz.

Windows:

- `FrameGate::wait_for_frames` waits with no timeout until a window unparks
  or `wake_requested` (stop flag, or `invalidate_devices`) is true.
  `FrameGate::wake` takes the lock before notifying, so no wakeup is lost.
  Stopping vsync (headless mode, platform drop) and a renderer resize failure
  call it.
- Hidden and minimized windows park through `park_frames`, which parks the
  idle state with the gate. Before, `WM_SHOWWINDOW` parked only the gate, so a
  window hidden right after a click (hide to tray) came back with a running
  idle state that no demand would unpark; the heartbeat hid this (1 fps until
  the idle state caught up), without it the window would have frozen.
  Minimizing did not park at all: a minimized window has no frame callback, so
  the idle decision never ran and the vsync thread invalidated it every vblank
  for as long as it stayed minimized. Now it parks, and every non-minimized
  `WM_SIZE` demands frames.
- `demand_frames` ignores a hidden or minimized window (it gets no `WM_PAINT`,
  so an unpark would never be undone). `WM_SHOWWINDOW` arrives before the
  window is visible, so it calls `resume_frames`, the same without the check.
  A window created hidden starts parked.
- Direct Manipulation (precision touchpads) advances only when the window
  calls `update`. Frames keep running for 5 s after a contact, as before;
  after that the heartbeat used to give a resting finger that starts moving
  later one `update` a second. Now a contact arms a 250 ms window timer that
  updates the viewport between 5 s and 65 s after the contact, resumes frames
  as soon as a gesture starts or input is waiting, and kills itself after
  that. An idle window has no timer; a touchpad user gets four cheap polls a
  second for a minute after their last touch.

## Measurement

CI, push runs, `ready-chats` demo, idle window, 10 s window starting 3 s after
the ready marker. Linux: Xvfb with Mesa lavapipe, `/proc/<pid>/task/*/status`
voluntary context switches (`scripts/ci-idle-wakeups.py`). Windows:
`windows-latest` with WARP, raw cumulative context switches per thread
(`Win32_PerfRawData_PerfProc_Thread`). "Parking off" is the same build with
`QUILL_IDLE_FRAMES=0` (upstream behavior).

Before: run 38890647211 (`cursor/sweep-idle-heartbeat-baseline`, the
measurement commits on top of main without the fix). After: run 38890638118
(`cursor/sweep-idle-heartbeat`). Wakeups in the 10 s window:

| | Main thread | All threads | CPU |
| --- | --- | --- | --- |
| Linux X11, before | 10 (1.0/s) | 30 | 0.00% |
| Linux X11, after | 0 | 20 | 0.00% |
| Linux X11, parking off | 626 (62.6/s) | 646 | 0.30% |
| Windows, before | 10 (1.0/s) | 44 | 0.16% |
| Windows, after | 0 | 1 | 0.00% |
| Windows, parking off | 631 (63/s) | 1232 | 0.47% |

On Windows the "before" thread list has three threads waking with the
heartbeat: the main thread (10, one `WM_PAINT` and frame callback a second),
`quill/6` (23; the thread at about 60 a second with parking off, so the vsync
thread) and one more thread at 10. After the change one switch was left, on a
fourth thread. The
Linux `blocking-1` thread wakes twice a second before and after; it is not
a frame source and not the main thread (left alone here). The first,
report-only baseline run (38888934618) measured the same Linux numbers (10
main-thread wakeups, 625 with parking off).

The baseline run's Linux job is red on the new idle check ("main thread woke
1.0/s, limit 0.5/s"); the fix run's is green.

## CI check

`scripts/ci-launch-smoke.sh` now measures the first fixture's idle wakeups and,
with `IDLE_MAX_MAIN_PER_S=0.5` set in `ci.yml`, fails above half a wakeup a
second on the main thread: 0 when parked, 1 with the old heartbeat, 60 when
parking is broken. It also prints the parking-off contrast. The Windows smoke
prints the same numbers but does not fail on them (two runs of data so far).

Unit tests (`scripts/test-frame-idle.sh`, 17): new ones cover a parked source
scheduling no frame, the refresh grid (skipping missed frames, never firing
at `now`, a clock that went backwards, a zero interval, a multi-day stall),
an idle window waking 16 times in 10 s after its last active frame (the
250 ms grace period at 16 ms, then nothing; with the heartbeat it was 10
more), a platform-parked source resuming on the first demand (the Windows
hide-to-tray case), the vsync sleep decision and the touchpad contact phases.
The 200 000-step simulation now runs with no frames while parked and checks
that every demand restarts the source.

## Not verified

- No real X11 desktop or Windows machine ran this. X11 ran under Xvfb in CI;
  Windows ran on WARP in CI. The changed Windows crate is type-checked and
  linted for `x86_64-pc-windows-msvc` from macOS; the Linux crate compiles in
  CI (the macOS cross-check needs a Linux C sysroot this machine lacks).
- Windows paths CI cannot exercise: hide to tray and restore, minimize and
  restore, a touchpad gesture that starts after the finger rested more than
  5 s (the new poll timer), device loss while every window is parked.
- Device loss while every window is parked is now noticed at the next frame
  demand (any input or update), not within a second; the window may show
  stale or black content until then. Registering
  `ID3D11Device4::RegisterDeviceRemovedEvent` and waking the vsync thread on
  it would close this; not done.
- A frame demand that bypasses both `frame_waker` and `schedule_frame` would
  now wait for the next real event instead of up to a second. None is known
  (`codex-idle-frames-x11-windows.md` lists the paths that were read).

## Follow-up

- The Quill-side periodic timers listed above (poll loop, deep-link inbox,
  tray sync, passcode tick, OS menus, battery probe) wake every platform's
  main thread about 15 times a second in a signed-in session. Event-driven
  replacements (a channel wake from the TDLib receive thread, the OS deep-link
  callback, tray updates on unread changes) would bring the idle floor to the
  level measured here.
