# Idle frames on X11 and Windows

Follow-up to `codex-idle-cpu.md`, which stopped the macOS display link while
a window has nothing to draw. GPUI 0.3.7's X11 and Windows backends have the
same always-on frame source:

- **X11** (`gpui-pre-linux`, `X11ClientState::start_refresh_loop`): a calloop
  timer per visible window fires at the monitor's refresh rate, calls the
  window's frame callback and processes X events, whether or not anything
  changed. It stops only while the window is unmapped or fully obscured
  (compositing window managers rarely report the latter).
- **Windows** (`gpui-pre-windows`, `begin_vsync_thread`): a thread waits for
  every vblank (`DwmFlush`) and then `RedrawWindow(RDW_INVALIDATE)`s every
  GPUI window, so each window gets a `WM_PAINT` and a frame callback per
  refresh, forever, minimized or covered or not.
- **Wayland** draws on demand already (frame callbacks plus
  `schedule_frame`); unchanged.

Neither backend implements `PlatformWindow::frame_waker`, the hook GPUI calls
when a window becomes dirty or queues a next-frame callback.

## What changed

Both crates are vendored like `gpui-pre-macos`: the commit that adds
`third_party/gpui-pre-linux` and `third_party/gpui-pre-windows` (`86dc5470`)
is the pristine registry copy, the next commit holds the changes, each changed
file starts with a notice, and each crate's `QUILL-CHANGES.md` lists them.

### The shared decision

`frame_idle.rs` (one std-only file, identical in both crates, unit tested by
`scripts/test-frame-idle.sh`, which CI's `linux-fmt-clippy-test` job runs):

- A frame counts as **active** when the platform's `draw` ran (GPUI draws or
  presents), `schedule_frame` ran (GPUI still has demand after the frame:
  the window is dirty again or next-frame callbacks are queued, which is
  also what GPUI's own inactive-window throttle calls), or the platform has
  its own reason (below).
- After **250 ms** (the macOS value) with no active frame and no frame
  demand, the window's frame source **parks**.
- A **frame demand** (the frame waker) restarts the 250 ms grace period and,
  if the source was parked, resumes it at once.
- A parked window still gets **one heartbeat frame a second**. macOS has no
  heartbeat; here it is the fail-safe for code that cannot be run on this
  machine: if some frame demand ever bypasses the waker, it is served within
  a second instead of never, and a heartbeat frame that finds work resumes
  the refresh rate. It costs one no-op frame callback a second per window
  instead of 60–144.

GPUI only skips work in a frame callback when the window is clean and has no
next-frame callbacks, and the waker fires on every clean → dirty transition
and every `on_next_frame` (which `request_animation_frame` uses), so a parked
window is woken by the next change. The one GPUI path that skips a dirty
frame is a re-entrant request while another draw is on the stack
(`draw_in_progress`, only reachable on Windows); parking would need 250 ms of
those in a row, and the heartbeat covers it anyway.

### X11

- `X11FrameIdle` (window.rs) holds the activity flag, the decision and a
  calloop `Ping`. `draw` and `schedule_frame` set the flag. `frame_waker`
  returns a closure that only touches `Cell`s and pings when the timer is
  parked, so it is safe re-entrantly, from inside GPUI updates, and after
  the window is gone.
- The refresh timer (client.rs) runs the decision after each frame. Parked,
  it re-arms at the heartbeat instead of the next refresh instant; it still
  calls `process_x11_events` on each heartbeat.
- `open_window` registers a per-window ping source; its handler
  (`resume_refresh_loop`) removes the heartbeat timer and starts an
  immediate refresh-rate timer. The ping is an eventfd, so it wakes a
  blocked `epoll` (an idle callback would not: calloop does not shorten the
  poll timeout for idles inserted from idles, and GPUI runs its foreground
  tasks as idles). If the ping cannot be created the window never parks.
  `drop_window` removes the source.
- Hidden windows are unchanged (no timer); a timer started for any reason
  starts unparked. A pending forced render after GPU recovery
  (`force_render_after_recovery`) keeps the timer running; the recovery
  itself happens inside `draw`, which counts as activity.

### Windows

- `FrameGate` (vsync.rs, a process-wide static) is the set of parked window
  handles, guarded by a mutex, plus a condition variable. The vsync thread
  waits on it while every window is parked (until an unpark or the
  heartbeat), then waits for the vblank and runs the device-lost check as
  before, then invalidates the windows that are not parked (all of them on
  the heartbeat). Lock order: the vsync thread takes the gate, then the
  window list's read lock; the UI thread never takes the gate while holding
  the window list (`close_one_window` drops its write lock first).
- `WindowsWindowInner::demand_frames` (the frame waker; it holds the window
  weakly) unparks a parked window, which wakes the vsync thread, and
  `RedrawWindow(RDW_INVALIDATE)`s it at once, so the first frame after idle
  does not wait for the vsync thread. `RDW_INVALIDATE` alone sends no
  message synchronously. If the invalidation is validated by a draw already
  in progress, the unparked window gets the next vblank's.
- `draw_window` applies the decision after GPUI's frame callback. It counts
  as active while `force_render_pending` is set (device-loss recovery) and
  while a touchpad gesture may be in progress (below). A draw deferred
  because another draw is in progress, which relies on the next vsync
  invalidation, unparks the window. When the window has no frame callback
  (minimized), nothing is decided, as upstream.
- **Direct Manipulation**: precision-touchpad scrolling and pinching run
  through a DM viewport in manual-update mode, which only advances when
  `draw_window` calls `update` once per frame. A parked window would not
  recognize a gesture. So a `DM_POINTERHITTEST` unparks the window, and
  frames keep running while the viewport is `RUNNING` or `INERTIA`, and for
  5 s after a touchpad contact is handed to it (a resting finger that only
  starts moving later still gets updates; a tap that never becomes a
  gesture costs at most 5 s of frames).
- A new window is unparked in the gate (a closed window could have had the
  same handle) and a closed one forgotten.

## Expected effect (not measured)

This Mac cannot run X11 or Windows, so there are no before/after numbers.
By construction, for a visible idle window:

| | Upstream | This branch |
| --- | --- | --- |
| X11: main-thread timer wakes | refresh rate (60–144/s) per window | 1/s per window |
| Windows: vsync thread wakes | refresh rate | 1/s (all windows parked) |
| Windows: `WM_PAINT` + frame callback | refresh rate per window | 1/s per window |

On macOS the equivalent wakeups were the whole idle floor (0.4–0.6% CPU on an
M1 Pro, down to 0.00%). Animations, input and the update stream behave as
before: anything that draws keeps the frame source at the refresh rate.

## Verification done

- Cross `cargo check` of both crates from macOS, before and after the change:
  `gpui-pre-windows` for `x86_64-pc-windows-msvc` and `gpui-pre-linux`
  (X11 + Wayland) for `x86_64-unknown-linux-gnu`, no warnings.
- `scripts/test-frame-idle.sh`: the decision's unit tests, including a
  200 000-step simulated frame source (frames at a refresh rate or the
  heartbeat, random demands and activity) checking that a demand always
  unparks, a park only follows 250 ms without activity or demand, and a
  running source never sits idle past the grace period.
- CI builds the Linux (x86_64) and Windows packages with these crates.
- Reading: every GPUI frame demand path (`notify`/`invalidate_view`,
  `refresh`/`set_dirty`, `on_next_frame`, `request_animation_frame`, the
  throttle retry, re-invalidation during a draw) reaches `frame_waker` or
  `schedule_frame`; both backends' frame paths (expose, resize,
  visibility, device loss, re-entrant draws, Direct Manipulation) are
  covered above.

## Needs a live check

On each platform, a release build signed in or on a demo fixture
(`--screenshot-demo ready-chats`, `QUILL_DEMO_UPDATE_STREAM=10`,
`QUILL_ASSUME_ACTIVE=1` as in `scripts/idle-cpu-bench.sh`):

**X11** (on a Wayland session, start Quill with `WAYLAND_DISPLAY=` to force
X11): idle CPU and wakeups before and after (`pidstat -w -t -p <pid> 1`, or
`perf trace -s`), expecting about one main-thread wake a second plus
Quill's own timers; then that nothing stalls: typing, hover, scrolling
with a mouse wheel and a touchpad, typing dots and stickers (animations keep
the timer running), opening a chat after a minute idle (first frame should
be immediate, not up to a second late), window resize, minimize and restore,
moving between monitors, and a second window (media viewer). Worth watching:
X events that xcb queued while waiting for a reply outside the event
handlers are processed by the next runnable or heartbeat rather than the
next refresh tick.

**Windows**: the same, with Process Explorer's context-switch delta for the
`VSyncProvider` and main threads (or `typeperf`). Specifically: precision
touchpad two-finger scrolling and pinch after the window has been idle
(Direct Manipulation, the riskiest path), including a finger resting before
it moves; inertia scrolling to the end; resizing by dragging the border
(modal size-move loop); minimize and restore; a second window; sleep and
resume or a driver reset (device-lost recovery).

If either platform misbehaves, the change is self-contained per crate:
removing that crate's `[patch.crates-io]` line restores upstream behavior
for it.

## Risks

- A frame demand that reaches neither `frame_waker` nor `schedule_frame`
  would be served by the heartbeat, up to a second late. None is known.
- Windows Direct Manipulation: whether `DM_POINTERHITTEST` plus the viewport
  status covers every gesture has not been observed; a gesture that starts
  more than 5 s after the contact without a status change would start up to
  a second late.
- The heartbeat keeps a floor of one no-op frame callback a second per
  window, which the macOS change does not have.
