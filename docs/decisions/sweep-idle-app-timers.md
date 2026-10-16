# Idle app timers: event-driven wake-ups instead of fixed ticks

Branch `sweep-idle-app-timers`. Finding from the idle-heartbeat work (#882): a
signed-in Quill woke its main thread about 15 times a second with nothing
happening, on every platform. The demo fixtures do not start these timers,
which is why the CPU idle measurements missed them.

## What Telegram Desktop does

Event-driven: the Qt event loop sleeps until the network, a timer with a
real deadline (auto-lock, the typing-stop timer) or the OS has something to
deliver. There is no fixed-rate poll of the MTProto layer, the tray or the
dock menu. Quill cannot mirror that exactly because TDLib hands updates over
a blocking `td_receive` on its own thread, but the receive thread can wake
the UI instead of the UI asking.

## What was wrong (checked against origin/main)

| Timer | Where | Old period | Claim verified |
| --- | --- | --- | --- |
| TDLib poll loop | `ui/notifications/mod.rs` `spawn_poll_loop` | 10 ms busy, backs off to 120 ms idle | yes, floor of 8 wakes/s |
| Background accounts poll | `ui/background_accounts/mod.rs` | backs off to 400 ms | yes (only with extra accounts) |
| Deep-link inbox | `main.rs` `install_link_inbox` | 250 ms | yes |
| Tray + badge sync | `main.rs` | 1 s | yes |
| Passcode tick | `ui/passcode/mod.rs` | 1 s, always (even with no passcode) | yes |
| OS menus (macOS) | `ui/os_menus/mod.rs` | 1 s, reads the account registry each time | yes |
| Battery probe | `ui/battery_watch.rs` | 5 s, even with the switch off | yes |

Idle total: 8.3 + 4 + 1 + 1 + 1 + 0.2 = about 15.5 wakes/s.

## What changed

New `quill::wake` (`src/wake.rs`): a `Signal` is an epoch counter plus the
wakers of whoever waits. `poke()` works from any thread; `wait_since(epoch)`
is ready as soon as the epoch moved, including a poke that landed between two
waits, so nothing is lost. `ui::sleep_or_wake` races a timer against a signal.

- **Poll loop.** The receive side pokes `wake::UPDATES` after queueing an
  envelope (`Routes::deliver` for the shared hub, the injected loop in tests).
  The loop now sleeps up to 2 s when idle and wakes at once on arrival. It
  stays at most 120 ms apart while a batched redraw is owed
  (`PolledRedraw::pending`), so the redraw batching intervals still hold. The
  epoch is read before each poll, so an update that arrives during it ends
  the next sleep immediately. Notification clicks from worker threads (Linux
  D-Bus, call notification threads) and the notification windows poke the
  same signal. Background accounts use the same wake and a 2 s idle cap.
- **Deep-link inbox.** `push_external_link`, `request_activation` and the
  forwarded-launch path poke `wake::LINKS`. The task sleeps up to 60 s, or
  250 ms while a queued link waits behind another flow.
- **Tray, badge, OS menus.** They follow `wake::CHROME`, poked after the poll
  applied updates, on window activation, tray menu clicks (Linux: ksni
  callback; macOS and Windows: a `MenuEvent` handler replaces the receiver
  the 1 s loop drained), tray enable changes, lock changes, notification
  toggles and player start, pause, resume and stop. Fallback 10 s (a tray host
  that appears late, the menu bar theme, the account registry file); the
  macOS keep-minimized-window-hidden check keeps its 1 s cadence only while
  the window is actually minimized. The menus rebuild at most once a second.
- **Passcode.** The task parks (300 s safety net) when there is no passcode,
  the app is locked without a flood countdown, or it is a demo. With
  auto-lock on, it sleeps until the deadline computed from the idle time
  (`autolock_sleep_ms`, 1 to 5 s), so activity re-arms it on the next wake.
  `IdleTracker::plan_sleep` tells the suspend detector that a long gap was
  intended, and `reset` clears stale history when the task wakes from a park.
  Enabling, disabling, changing the timeout, locking, unlocking and a failed
  unlock poke `wake::PASSCODE`.
- **Battery probe.** 30 s while "Save Power on Low Battery" is on. With it
  off, the task only wakes (10 min safety net, or an immediate poke when the
  switch changes). The first probe still runs at startup so the switch is
  offered when the platform has a saver.

## Numbers

Timer fires per second with a signed-in account, window open, nothing
happening. "Before" counts the fixed periods above. "After" is the steady
state of the new schedules; the poll loop ramps 20 ms to 2 s after the last
update (about 21 fires in the first 30 s, asserted below).

| | Before | After (steady) |
| --- | --- | --- |
| Poll loop | 8.33/s | 0.5/s |
| Deep-link inbox | 4/s | 0.017/s |
| Tray + badge | 1/s | 0.1/s |
| Passcode (off) | 1/s | 0.003/s |
| OS menus (macOS) | 1/s | 0.09/s |
| Battery | 0.2/s | 0.033/s |
| Total | 15.5/s | about 0.75/s |

A real login is not possible here (the owner's account is off limits), and
demo fixtures do not start these tasks, so the counts come from the schedule
functions themselves, which the loops call:

- `an_idle_signed_in_poll_loop_wakes_under_one_a_second` simulates 30 s of
  silence through `next_poll_delay` and asserts at most 30 fires (21 in
  practice, 250 before).
- `an_idle_hour_with_auto_lock_on_wakes_a_minute_at_most` does the same for
  the passcode schedule.

Not measured live: end-to-end CPU wake counts in a running app (no seam to
sample the main thread's timer fires without a real session).

## Prompt delivery

- `an_update_wakes_the_ui_without_waiting_for_a_poll_tick` injects a TDLib
  JSON on the receive side and asserts the wake signal moves within 500 ms
  with the envelope already readable.
- `wake` tests: a poke before the wait is not lost; a poke from another
  thread wakes a sleeping waiter within a second (50 ms in practice).
- `a_held_back_redraw_keeps_the_loop_at_most_120_ms_apart` and
  `the_poll_loop_goes_fast_the_moment_updates_flow` pin the busy and
  held-back cadence.
- Passcode: `a_planned_long_sleep_is_not_a_suspend` (a deliberate 60 s sleep
  must not count as a laptop suspend and lock early) and
  `autolock_sleeps_until_the_deadline_and_parks_when_off`.

## Review follow-up

- Stream-mode group calls: the ntgcalls event hook pokes `wake::UPDATES`
  after queueing a segment or time request, so live streams do not wait out
  the idle sleep (`an_engine_stream_request_wakes_the_poll_loop`).
- Auto-lock after a laptop sleep: monotonic timers pause in system sleep, so
  the auto-lock sleep is capped at 5 s (0.2 wakes/s, was 60 s) and window
  activation pokes `UPDATES`, `PASSCODE` and `CHROME` (`wake::poke_on_activation`,
  from the activation observer in `app_demo.rs`). No NSWorkspace wake observer
  exists in the code base, so the cap bounds the lateness on resume without
  a window activation (`a_laptop_wake_after_a_planned_sleep_locks_on_the_next_check`).
- `IdleTracker::effective_idle` no longer counts the planned sleep twice: a
  suspend adds `gap - planned`, since the OS idle time already holds the
  planned part.
- Off-thread queues now poke: the export worker after sending a request and
  the system-proxy reader when the detected value changes. The camera
  scanner for device linking has no thread (the child process is polled), so
  its result can wait up to 2 s; not changed.
- The poll loop's idle sleep is `min(POLL_IDLE_MAX, earliest deadline)`
  (`ConnectDriver::next_deadline`: next flood retry due, next pending bot
  message expiry; `cap_at_deadline`, never below 10 ms). Tests:
  `a_due_deadline_caps_the_idle_poll_sleep`,
  `a_due_flood_retry_is_a_known_deadline_for_the_poll_sleep`.
- Presence after activation: activation pokes `UPDATES`, so the poll runs at
  once.

## Limits and risks

- Anything `poll_live` does that is driven by time alone (request sweeps,
  presence going idle, system proxy, "away" detection) now runs every 2 s at
  most when nothing arrives, not every 120 ms. Presence is 1-second granular
  in tdesktop too.
- A source that fills a queue the poll drains without poking can wait up to
  2 s. The known thread sources (D-Bus and call-notification clicks) poke.
- The cross-platform edits in `tray_sni.rs`, `linux_dbus.rs`,
  `notify_windows` and the non-Linux tray handler are one-line pokes that CI
  compiles; they were not run on Linux or Windows hardware.
