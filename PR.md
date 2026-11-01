Fix the review findings on bundles B13 and B18: GPU recovery, battery saver, licences and suggested post times

## What was wrong

The reviews of PR 1311 (B13) and PR 1314 (B18) found ten defects in what landed on sweep-all.

B13:
1. L10 was half fixed. From the 3rd GPU loss in 10 minutes recovery only allowed a CPU adapter, and the adapter sort still put the dying GPU first, so recovery landed on it again and the 6th loss closed Quill.
2. The N50 battery saver fix had a lost-update race: a D-Bus read that went out before a PropertiesChanged signal could overwrite the signal's newer value, and the wake the signal poked then returned the stale value.
3. CI never ran the new `recovery_timers` tests.
4. The built-in keymap, compiled from xkeyboard-config 2.41, shipped without its licence notices.
5. While a look for the GPU ran, every draw armed a probe timer that fired at once, one probe step a frame instead of one each 250 ms.
6. N49 was measured in a toy program, not in Quill.
7. `scripts/test-blocking-pool.sh` resolved the vendored crate's test dependencies afresh on every run.
8. The L09 ordering (build the renderer, then install the context) had no test.

B18:
- D1. The suggested post block in chat history still read its time from the OS clock and zone, while the bar and the boxes read `WallClock`.
- D2. `time_label` sat between `price_amount` and its doc comment, so rustdoc put that comment on the wrong function.

## What changed

One commit per finding:
- Recreation attempts now carry the adapters they may use (`Adapters`: hardware only, any, software only). A GPU that keeps dying gets software only, and `select_adapter_and_device` applies it the same way it applied hardware only. Recovery gives up when no CPU adapter works, or when that forced CPU context is lost too. `new_rejecting_software` is now `WgpuContext::new_on`. QUILL-CHANGES.md now matches the code. The PR 1311 text said "recovery goes straight to a CPU adapter". Only this branch makes that true.
- The battery value, the fresh flag and a count of the listener's answers now share one atomic word. `refresh` stores its read only by compare-exchange against the word it saw before the call. A follow-up commit swaps `AtomicU64::update` for a compare-exchange loop, because the MSRV is 1.92.
- `recovery_timers` is added to the CI test filter.
- Added `licenses/xkeyboard-config-COPYING.txt` (the 2.41 COPYING, staged with the other licence texts), a `THIRD_PARTY.md` row, and a header in `default_keymap.xkb`.
- `HardwareProbe` keeps the instant of its next check while a look runs, and `hardware_probe_at` returns it (`next_at`). Draws no longer arm extra timers.
- Measured in Quill (see below), and recorded in `third_party/blocking/QUILL-CHANGES.md`.
- Committed `third_party/blocking/Cargo.lock`, and the script now runs with `--locked`.
- The L09 order now lives in `device_recovery::install_once_built`, which `try_recover` calls, and it has a test.
- The history block reads `WallClock` (`block_time`).
- The doc comment is back on `price_amount`.

One extra commit fixes a test build that was already broken on sweep-all. An earlier commit dropped the `Warmed` import from `decoder_load` tests, and a test that only builds on Linux and Windows still uses that import. The test now names the type by its path.

## What ran

- `scripts/test-device-recovery.sh`: 20 passed. New tests: `a_gpu_that_keeps_dying_is_left_for_a_cpu_adapter`, `a_keeps_dying_attempt_takes_the_cpu_adapter_listed_after_the_gpu`, `draws_during_a_look_ask_for_the_armed_check`, `a_context_the_renderer_failed_on_is_not_installed`.
- `cargo test --lib --features demo-capture -- battery_signal`: 8 passed, against a private dbus-daemon. The new test is `a_read_that_went_out_before_a_signal_keeps_the_signals_value`. It uses channels to stop the getter until the signal has been recorded, with no sleep or timeout. With the old unconditional store it fails (`Some(Some(false))` instead of `Some(Some(true))`).
- `cargo test -p gpui-pre-linux -p quill --lib --features quill/demo-capture -- recovery_timers keymap tray_sni battery_signal`: all passed. That includes the keymap parse tests with the new header, and the extended `a_second_recovery_does_not_start_a_second_probe_chain`.
- `cargo test --features demo-capture --bin quill` on the suggested posts and decoder_load tests: 13 passed. The new `the_history_block_time_reads_the_pickers_clock` fails with the old code ("today at 07:20" instead of "today at 01:20").
- `scripts/test-blocking-pool.sh` with `--locked`: all passed.
- N49 in Quill: the `ready-chats` demo idle under Xvfb, with lavapipe and a private session bus (zbus connected), over 60 s. With 1.7.0's `&&`, `blocking-1` woke 120 times (2 a second) and all threads woke 139 times. With the fix, no `blocking-*` thread was alive or started during the 60 s, and all threads woke 19 times.
- `cargo fmt --check`, plus `cargo clippy -D warnings` for `--features demo-capture --all-targets` and `--no-default-features --all-targets`: clean.
- `scripts/check-hotspots.sh origin/main`: OK. origin/main is merged.
- `cargo check -p gpui-pre-wgpu -p gpui-pre-linux`: clean.

## What did not run

- No real GPU loss and no real renderer build failure. The keeps-dying path is tested at the policy and adapter-filter level, not on hardware.
- Windows: `cargo check --target x86_64-pc-windows-gnu` could not run, because that target is not installed here. The changed renderer code is shared across platforms, and the Windows backend calls it the same way.
- macOS: nothing here is macOS-only.
- An active Telegram session for the N49 numbers. Only the idle demo was measured.

## What stays open

Nothing from either review. The suggestion in the B18 review to merge main before landing is done.
