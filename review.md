CHANGES

# Review of PR 1311 (claude/sweep-b13) against main, bundle B13

Reviewed at PR head `0014294`. Merge base is `948ad90`, the current main, so
the branch is up to date. I read `sweep-briefs/rules.md` and
`sweep-briefs/bundles/B13.md` on `claude/sweep-briefs`, the whole diff, and
the PR text on `claude/prtext-b13`.

Verdict: 12 of the 13 items are fixed at their root. L10 is only half
fixed, and the rest of it does the opposite of what the PR text says. The
N50 fix brings in a stale-value race that main did not have. CI never runs
the new `recovery_timers` tests. The embedded xkeyboard-config keymap ships
with no licence notice. Those four block the merge. Items 5 to 8 below are
smaller, and each has a fix named.

## What I ran (Linux container, no GPU, no display)

- `cargo test --lib -- battery_signal passcode::tests`: 34 passed. This
  includes the hung-portal test and the signal-value test, both against a
  private dbus-daemon.
- `cargo test -p gpui-pre-linux -p quill --lib --features quill/demo-capture -- recovery_timers keymap tray_sni`:
  16 passed (4 recovery_timers, 6 wayland keymap, 2 tray_sni, plus
  related tests).
- `scripts/test-device-recovery.sh`: 17 passed.
- `scripts/test-blocking-pool.sh`: all passed, including the new
  `an_idle_thread_under_the_limit_stops_after_the_timeout`.
- `scripts/check-hotspots.sh origin/main`: OK. The commits carry the
  required trailer.
- `cargo clippy --features demo-capture --all-targets --locked -- -D warnings`:
  clean (0 warnings). I did not rerun the `--no-default-features` clippy.
- Not run: `decoder_load` in `--bin quill` (the bin test build is a
  separate 10+ minute build; I read the code instead), anything on macOS
  or Windows, and any real GPU loss.

## Blocking defects

### 1. L10 is half fixed: a GPU that keeps dying is recovered onto again, and then the app exits

`wgpu_renderer.rs:2408` turns `Losses::KeepsDying` into
`Shared::Lost { was_software: true }`, which becomes
`Attempt::Recreate { allow_software: true }`. That only *permits* a CPU
adapter. `WgpuContext::new` (`wgpu_renderer.rs:2588`) goes through
`select_adapter_and_device`, which sorts the adapters with
`sort_adapters` (`wgpu_context.rs:534`, `Cpu` is ranked last) and takes
the first that configures. So from the 3rd loss, recovery lands right back
on the same dying GPU. The only change is that it skips the 27 s
hardware-only wait. At the 6th loss within 10 minutes, `recover` calls
`device_gone`, which shows the alert and exits. A machine with a working
lavapipe/llvmpipe adapter therefore loses the app, when it should have been
moved to software. Both QUILL-CHANGES.md and the PR text say "recovery goes
straight to a CPU adapter and stops looking for the GPU". That is not what
the code does. The test `a_gpu_that_keeps_dying_is_left_and_then_given_up_on`
only checks the counter, never which adapter is picked, so it passes either
way.

Fix: make the keeps-dying case its own attempt, for example
`Attempt::Recreate { adapters: Adapters::SoftwareOnly }`, carried by
`Target::Recreate { keeps_dying: true }`. Back it with a
`WgpuContext::new_software_only` that keeps only `DeviceType::Cpu`
adapters in `select_adapter_and_device`, the mirror of `reject_software`.
Give up only when the forced CPU context is itself lost again, or when no
CPU adapter configures. Add a test at the `sort_adapters` /
filter level that a keeps-dying attempt picks the `Cpu` adapter when a
`DiscreteGpu` one is listed first, plus a policy test that `KeepsDying`
maps to the software-only attempt. Then fix the wording in QUILL-CHANGES.md
and the PR text to match.

### 2. N50 brings in a lost-update race: the wake can now return a stale value

`battery_signal::refresh` (`src/battery_signal.rs:67-77`) does this:
check `FRESH`, find it false, make the D-Bus `Get`, then
`store(Some(answer))` with no further check. Here is the race:

1. The watcher's probe calls `refresh`. `FRESH` is false, so it sends
   `Get`. The service answers with the old value `a`.
2. The service switches to `b` and emits `PropertiesChanged`. The listener
   thread runs `record(Some(Some(b)))`: it stores `b`, sets `FRESH`, and
   pokes the watcher.
3. The probe thread now runs `store(Some(a))`, which overwrites `b`.
4. The poked wake runs `refresh`. `FRESH.swap` returns true, so it returns
   `latest()`, which is `a`. The battery saver now holds the old state until
   the next signal or the ten-minute parked read.

On main, step 4 made a real D-Bus read and got `b`. So this is a
regression. The window is narrow, but with the new 5 s method limit a slow
`Get` makes it wider.

Fix: the stored read must not overwrite a newer recorded answer. Either
(a) after `read`, do `if FRESH.load(Acquire) { return Some(latest()?) }`
and skip the store; or (b) better, replace `FRESH` with a generation
counter that `record` bumps. `refresh` reads the counter before the call
and stores only if it is unchanged (`compare_exchange`). Test: serve a
`Monitor` whose getter waits on a channel. Call `refresh` on a thread,
`record(Some(Some(true)))` while it waits, release the getter with
`false`, and assert that `latest()` and the next `probe()` give
`Some(true)`.

### 3. CI never runs the new `recovery_timers` tests

`.github/workflows/ci.yml:66-67` runs the vendored Linux backend's tests
with a name filter:
`battery_signal tray_sni present_retry wayland::client::tests wayland::clipboard::tests`.
The four new `linux::recovery_timers::tests` (N42, N45) match none of
these, so CI skips them. The PR text lists them under "What ran", which is
true only for a local run. Fix: add `recovery_timers` to that filter, the
same line the PR already uses for `wayland::client::tests`.

### 4. The built-in keymap embeds xkeyboard-config data with no licence notice

`default_keymap.xkb` (1910 lines, compiled from xkeyboard-config 2.41) is
built into the binary through `include_str!`
(`wayland/client.rs:3295`). QUILL-CHANGES.md says it is MIT, but neither
`THIRD_PARTY.md` nor `licenses/` has the notice, and the file has no
header. MIT/X11 terms require the notice to ship with copies. Fix: add
`licenses/xkeyboard-config-COPYING.txt` (the 2.41 `COPYING`), a
`THIRD_PARTY.md` row naming the file and version, and a `//` header line in
`default_keymap.xkb` pointing at both. xkb text allows `//` comments, and
`built_in_keymap` still parses (the existing test covers that).

## Smaller defects (fix in this PR, as the owner wants none held back)

### 5. A look for the GPU re-arms an immediate probe timer on every draw while it runs

While a look runs on its thread, the window's own timer is armed for
`now + PROBE_POLL` (`ProbeStep::Await`). But each draw calls
`schedule_hardware_probe(renderer.hardware_probe_at())`, and
`hardware_probe_at` (`wgpu_renderer.rs:2455-2458`) returns `probe.due()`,
which is the past instant the look started at. `arm_probe` sees a
different instant and arms a timer for the past. That timer fires at once
and runs `probe_hardware`, which does a `try_recv` and re-arms the 250 ms
poll. The next draw does all of it again. For the length of a look, an
animating window runs a probe step and a calloop timer per frame where one
every 250 ms was intended. Fix: have `hardware_probe_at` return
`probe.step(Instant::now()).at()`, so it gives the `Await` instant while
looking and `due` otherwise. Extend
`a_second_recovery_does_not_start_a_second_probe_chain` so that, while a
look runs, repeated draws arm no new timer.

### 6. N49 was measured in a toy program, not in Quill

The finding gives a number ("2 wakes a second" on `blocking-1`), and the
rules ask for before and after numbers. The fix is right at its root.
`should_stop` restores the `||` that the upstream comments describe, and I
checked the vendored diff against `8dcaa9a`: it is the one hunk plus the
test. But the count was taken in a standalone program. The fix also swaps
a parked thread for a new thread spawn on each `unblock` after 500 ms idle,
and nobody has shown how often Quill calls `unblock`. Fix: run
`scripts/ci-launch-smoke.sh` (Xvfb plus lavapipe, which the container has),
with a private session bus so zbus is in use, on main and on this branch.
Report the `blocking-*` rows from `scripts/ci-idle-wakeups.py` and the
thread count over 60 s. If something in Quill calls `unblock` every couple
of seconds, report the spawn rate too.

### 7. `scripts/test-blocking-pool.sh` resolves dependencies afresh on every CI run

The copied crate has no lockfile, and the script runs `cargo test` without
`--locked`. So each CI run resolves the dev-dependencies (`futures-lite`
and the rest) from crates.io at whatever version is newest that day. That
makes the new CI step unreproducible and network-dependent. Fix: commit
`third_party/blocking/Cargo.lock` (made once with the script's
workspace trick), copy it into the temp dir, and pass `--locked`.

### 8. L09 has no test

The ordering fix is correct. `try_recover` now calls `rebuild` first and
installs into the slot only on success, and the probe-return path reuses
it. But nothing fails if someone moves the install back up. The PR text
says nothing here can make the renderer build fail on a working device.
Two options. One: split the decision so it can be tested. `try_recover`
already has the order; give `rebuild` a seam, for example
`fn rebuild(..., build: impl FnOnce(..) -> Result<Self>)`, and assert that
a failed build leaves the slot `None` and the next `recover` stays on
`Recreate`. Two: run it under lavapipe in the launch-smoke job, with a
surface config the device refuses (a size above
`max_texture_dimension_2d`).

## Items checked and fixed at their root

| Id | Verdict | Evidence |
| --- | --- | --- |
| L09 | Fixed (test gap, item 8) | `try_recover` builds the renderer on the new context before `*gpu_context.borrow_mut() = Some(context)`. A failed build leaves the slot empty, so the window does not switch to `Adopt` and its failure count is kept. |
| L10 | **Half fixed, item 1** | The loss counter and the probe suppression work. A CPU adapter is never forced. |
| L11 | Fixed | `look_for_gpu` lists adapters on a `quill-gpu-look` thread with a display-less instance. The UI thread only polls `try_recv` and runs `gpu_can_present` once a non-CPU adapter shows up. Unverified risk: a GL-only GPU listed only through the window's display. Mesa's EGL falls back to surfaceless, so I am not counting it as a defect. |
| L12 | Fixed | `connection::Builder::...().method_timeout(DESKTOP_QUERY_LIMIT)`. zbus passes the timeout into `Connection::new`, so `AddMatch`, the first `Get` and every `refresh` give up after 5 s. The test against a never-answering `HungMonitor` passes. |
| N42 | Fixed (CI gap, item 3) | `RecoveryTimers` keeps one retry, probe and close timer per window. X11 no longer keeps the refresh timer running for a pending attempt (`frame_idle_after_refresh`). A failed arm is retried from the next frame. On X11 a failure needs the client to be borrowed, and `refresh` runs with the borrow dropped (`x11/client.rs:850-865`), so this does not become a spin. |
| N43 | Fixed | Registration and `update` run under `within(DBUS_LIMIT, ..)`, and a stopped publisher triggers shutdown and re-registration. I checked whether repeated re-registration could pile up stuck services. It cannot: ksni 0.3.7 builds its connection with `internal_executor(false)` and runs every service on one driver thread, and a still-wedged bus fails the new `spawn` inside the limit (the future is dropped, nothing is spawned). Not a defect. |
| N44 | Fixed | `VideoPlayer::open` calls `warm_decoder`, so the warm task (which redraws) races the decode thread for the same `ffi::availability` once-load. The per-app `Warmed` global replaces the process static. All other players go through `open`; `from_backend` is test-only. |
| N45 | Fixed (item 5 is a cost bug here) | The probe now lives in `GpuSlot.recovery`. Every draw on a fallback context arms the probe timer, so the other windows keep looking after the recreating window closes. |
| N46 | Fixed | The length is checked against `metadata().len()` and a 16 MiB cap, then `read_exact_at`, then `new_from_string`. No mapping, so a short or truncated file gives an `UnexpectedEof` error, not SIGBUS. `size == 0` gives an empty string that xkb refuses. The unsafe block and its overclaiming SAFETY comment are gone. |
| N47 | Fixed (licence, item 4) | `keymap_to_use` installs the system default, or else the built-in US keymap, only when no keymap is installed yet. A later valid keymap replaces it. |
| N48 | Fixed | `Failed { failures, retry_ms }` with 2 s doubling to 120 s, reopened `Sources` before a retry, and a 2 s method limit on the sampler bus. I traced the fallback: `ui/passcode/mod.rs:673` uses `os_idle_ms().unwrap_or_else(presence::idle_ms)`. During a failure, auto-lock counts input to Quill's own windows, which can only lock earlier, never later. It is not a "never locks" bug. |
| N49 | Fixed at root (measurement, items 6 and 7) | The `&&` to `||` change in vendored blocking 1.7.0, wired through `[patch.crates-io]`. |
| N50 | **Regression, item 2** | The signal value is used. The read path can now overwrite it with an older value. |

## Rules check

- Branch merged with main, no force-push, one commit per finding (plus
  the vendoring and two follow-ups): OK.
- Panic-safety: no new `unwrap`, `expect` or indexing in `src`
  non-test code. The tests use `unwrap`, as the existing tests do.
- Tests and time: the tray test (`Duration::from_millis(1)`) and the
  battery test (200 ms method limit) use a timeout. Neither result depends
  on timing, because the futures they race never resolve. I am not counting
  them as defects.
- Hotspots: none touched.
- New UI text: none.
