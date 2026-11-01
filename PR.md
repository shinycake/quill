Stop Linux hangs, stuck threads, idle wakeups and endless GPU recoveries around D-Bus, the tray, idle time, keymaps and lost GPUs

Sweep bundle B13 (linux-edges): all 13 items are fixed, one commit each. Three more commits: the unmodified vendored `blocking` crate before its N49 fix (so the diff shows the change), a clippy fix to the N43 tray code (`drop` of the shutdown awaiter), and a follow-up to N42 (a recovery timer the event loop could not arm is asked for again from the next frame, instead of leaving the window waiting on a timer that never fires).

## What was wrong

- L12: the power-saver listener read the portal or power-profiles-daemon property over a zbus connection with no method timeout. A service that took the call and never answered held a background executor thread for the rest of the session.
- N50: a PropertiesChanged signal carries the new value, but the watcher's wake read the same property over D-Bus again.
- N48: one idle sample that no backend answered (gnome-shell or KDE restarting) set the idle cache to Failed for good. I traced the fallback: auto-lock keeps working, using input to Quill's own windows (`passcode_tick`, `os_idle_ms().unwrap_or_else(presence::idle_ms)`). So it is not a "never locks" bug. It is a "locks while you work in another app" bug, and the setting reads "if inactive" instead of "if away" for the rest of the session.
- N43: the tray registered and published through ksni's blocking API with no limit. On a wedged session bus the registration worker waited for good: the tray stayed "registering" and never retried. The publisher could also wait inside `Handle::update` forever, which froze the icon count, and each later registration added another stuck thread.
- N44: a player can open before any history renders (a chat list's userpic video, the first-run intro). Its decode thread then did the first FFmpeg load, and nothing asked for a redraw once the load finished.
- N46: `compile_keymap` mapped the size the compositor sent. The SAFETY comment said a short file is reported as an error. It is not: reading past the end of the file raises SIGBUS. The test reproduces this: with the old code it dies with signal 7.
- N47: with no usable first keymap (`no_keymap`, a non-XkbV1 format, or a keymap xkb refuses), every key was dropped and nothing could be typed.
- N49: traced `blocking-1`. It is the `blocking` crate's pool thread, started by the first `unblock` (zbus, smol). In 1.7.0 an idle thread stops only when it timed out AND the pool is over its 500-thread limit, so it never stops and wakes every 500 ms for the life of the process.
- N42: on X11, a pending recovery attempt kept the refresh timer running at the refresh rate until the attempt was due. On both backends, a second recovery started a second hardware-probe timer chain, and every draw before a `CloseWindow` armed another close.
- L09: a recreation put its new context in the shared slot before building its renderer. When the build failed, the context stayed: the window's next attempt switched to adopting it with its failures reset, and the other windows adopted a context nobody could draw with. On main this path had never been run.
- N45: only the window that recreated the context looked for the GPU after a software fallback. Closing that window left every other window on the CPU renderer for the session.
- L11: each look for the GPU (every 10 s to 120 s) made a Vulkan instance and a surface on the UI thread.
- L10: every successful recovery reset the state, so a GPU that kept dying was recovered onto forever, and the probe moved the windows back onto it.

## What changed

- `src/battery_signal.rs`: the listener's connection has a 5 s method timeout (`DESKTOP_QUERY_LIMIT`, the gdbus fallback's limit). An answer the listener stored is marked fresh, and the probe that its wake runs takes it with no call. The parked ten-minute check still reads the service.
- `src/passcode/idle.rs`: `IdleCache::Failed` records failures in a row and the next try: 2 s, doubling to 2 minutes. The sampler reopens its bus and X connections before a retry and keeps the backoff across restarts. Its D-Bus calls have a 2 s limit, so a hung shell cannot leave an old sample counting up towards a lock.
- `src/tray_sni.rs`: registration and publishing run ksni's async API under a 5 s limit. A registration that runs out is a failed attempt and is retried after 10 s. A publisher that runs out stops its thread, and the tray shuts that service down and registers again. ksni's unused `blocking` feature is dropped.
- `src/ui/video_player`: `VideoPlayer::open` warms the decoder, so whichever thread loads FFmpeg, the warm task redraws once it ends. The started flag is a per-app global.
- `third_party/gpui-pre-linux`: the keymap is read with `read_exact_at` after its length is checked (16 MiB cap). With no keymap installed yet, the default one is used: libxkbcommon's from the system rules, or a built-in US keymap (`default_keymap.xkb`, compiled from xkeyboard-config 2.41, MIT). The new `RecoveryTimers` keeps one retry, probe and close timer per window. X11 waits for an attempt with one timer and lets the refresh timer park. Each window arms the probe timer from its draws while the fallback lasts.
- `third_party/blocking` (new vendored copy of 1.7.0): the two stop reasons are joined with `||`, as their comments say. `scripts/test-blocking-pool.sh` runs its tests and is now a CI step.
- `third_party/gpui-pre-wgpu`: `GpuContext` is `Rc<GpuSlot>`: the context plus `SharedRecovery`, which holds the hardware probe and the loss count. A recreated context is installed only after the window's renderer is built on it. The adapter listing for a look runs on its own thread, and windows check for the answer every 250 ms. Only a seen GPU adapter leads to the surface check on the UI thread. From the 3rd loss within 10 minutes, recovery goes straight to a CPU adapter and stops looking for the GPU. At the 6th it gives up, which shows the existing "graphics card stopped working" alert and exits.

## What ran

- `cargo fmt --check`. `cargo clippy -- -D warnings` with `--features demo-capture --all-targets` and with `--no-default-features --all-targets`: both clean on the final branch.
- `cargo test --lib battery_signal` (7, including two against a private dbus-daemon), `passcode::tests` (27), `tray_sni` (2).
- `cargo test --features demo-capture --bin quill decoder_load` (3).
- `cargo test -p gpui-pre-linux -p quill --lib --features quill/demo-capture` for `keymap` (6) and `recovery_timers` (4).
- `scripts/test-device-recovery.sh` (17) and `scripts/test-blocking-pool.sh` (23 tests, one new).
- Each new test was checked against the bug: the hung-portal test hangs without the timeout, the player-open test fails without the call, the short-keymap test dies with SIGBUS on the old mapping, and the others assert values the old code cannot produce.
- Measured N49 with a small program that calls `unblock` once and reads `/proc/self/task/*/status`. Before: `blocking-1` made 19 voluntary context switches in 10 s and stayed alive. After: the thread is gone 1 s after its task.
- `scripts/check-hotspots.sh`: OK.

## What did not run

- `cargo check --target x86_64-pc-windows-gnu` stops in gpui-pre's build script before any Quill code compiles, so it gives no result here. The Windows-reachable change is `decoder_load.rs`, and it compiles on Linux.

- This container has no GPU loss and no display, so the GPU recovery wiring (L09, L10, L11, N45, and the N42 platform code) is type-checked only. The device-recovery policy tests cover its decisions. L09 has no test: nothing here can make the renderer build fail on a working device.
- L11's UI-thread cost per look was not timed, for the same reason.
- N49 was not measured in the app itself. The CI idle-wakeups script should now show no `blocking-1` thread on Linux.
- No real compositor or tray host was used for N43, N46 and N47. Those tests drive the code with stand-ins.
- No macOS code changed. On macOS, `VideoPlayer::open` calls `warm_decoder`, which returns at once there.

## What stays open

- On a desktop with no idle backend (wlroots), the idle sampler now keeps trying every 2 minutes while auto-lock is armed, where before it stopped after the first failure.
- The flap limits (3 and 6 losses in 10 minutes) are judgment calls. Telegram Desktop has no direct equivalent: it turns OpenGL off for the next launch after a crash.
