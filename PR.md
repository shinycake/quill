On Windows, Quill no longer crashes or freezes when GPU setup fails or the vsync thread dies, and every PR now compiles the Windows build

## What was wrong

Sweep bundle B1 (items N82 to N88, audit of main at 5072e9662). Each item was checked against current main first; all seven were still open.

- N82: `directx_devices.rs` panicked while creating the GPU device: `context.unwrap()` in `get_adapter`, the `unreachable!()` after its loop, `device.unwrap()` in `get_device`, and a second `unreachable!()` on an unexpected feature level in `DirectXDevices::new`.
- N83: `begin_vsync_thread` unwrapped `Builder::spawn`, so running out of threads or memory while attaching the GPU panicked on the UI thread.
- N84: a panic on the vsync thread ended it with nobody noticing. No window got another frame and Quill looked hung.
- N85: a window that opened while the vsync thread was recovering from a device loss could stay blank. Recovery sends the new devices to the windows it lists after the platform got them; a window built from the old devices that joined the list later never got them.
- N86: the window list was a bare `Arc<RwLock<SmallVec<..>>>`. The helpers that drop the guard before `SendMessageW` were opt-in, so a new caller could bring back the deadlock with `open_window` / `close_one_window`.
- N87: PR CI compiled nothing for Windows. Windows builds ran on pushes only.
- N88: `timer_thread.rs` had no test.

## What changed

One commit per item, plus one fix-up commit.

- N82: device creation returns errors. An adapter without a device or context is skipped, running out of adapters returns "No GPU adapter supports Direct3D 11", and an unexpected feature level is logged. The existing callers handle the error: the startup alert at launch, retry then close the windows after a device loss.
- N83: `begin_vsync_thread` returns the spawn error. `attach_gpu` detaches the devices it just made and returns it, so launch shows the startup alert (through `connect_initially`, `open_window` and `ui::startup_failure`) and `request_windowing(Windowed)` returns an error.
- N84: the vsync thread body runs under `catch_unwind`. On a panic it logs and posts `WM_GPUI_GPU_DEVICE_GONE`, so the platform drops its devices and closes every window, the same end as a device that can't be recreated.
- N85: `open_window` remembers the device the window was built with. Once the window is in the list it compares that with the platform's current device and, if recovery replaced it in between, sends the window `WM_GPUI_GPU_DEVICE_LOST` with the current devices. Either the window was listed before recovery read the list, or the platform already holds the new devices when it checks.
- N86: `WindowList` wraps the lock and only hands out copies (`snapshot`, `find`) or changes the list under its own lock (`push`, `remove`). Every caller in `platform.rs` and `vsync.rs` uses it and the old helpers are gone. It uses std's `RwLock` with poison recovery, so it stays std-only.
- N87: new `windows-check` job on `windows-latest`, on every PR: `cargo check --features demo-capture --all-targets --locked`.
- N88: the timer thread's decisions (stop, fire due timers, wait for a wake, timed wait and whether to hold the 1 ms system timer), the relative `SetWaitableTimer` due time and the fallback timeout moved to the std-only `timer_step.rs`. `scripts/test-timer-step.sh` builds it with `timer_queue.rs` on any host, and CI's Linux job runs it. This also fixed one edge: a fallback wait of exactly `u32::MAX` ms used to become `INFINITE`.
- Fix-up: running rustfmt on `gpui_windows.rs` also reformatted five modules it declares. A separate commit puts them back unchanged.

`docs/decisions/gpui-fork-panics.md` has a new "Windows, second pass" section, and `third_party/gpui-pre-windows/QUILL-CHANGES.md` has a new entry.

## What ran

- `cargo check -p gpui-pre-windows --target x86_64-pc-windows-gnu` on Linux (mingw-w64 for the resource compiler): clean, no warnings, after each commit.
- `scripts/test-window-list.sh` (5 passed), `scripts/test-timer-step.sh` (8 passed), `scripts/test-timer-queue.sh` (4 passed).
- `cargo fmt --all -- --check`, `rustfmt --check` on each touched vendored file, `check-file-size.sh`, `check-hotspots.sh`, `check-app-name.sh`: all OK.
- `cargo clippy -- -D warnings` with `--no-default-features --all-targets` and with `--features demo-capture --all-targets`: clean. The vendored Windows crate is not a workspace member, so these do not lint it.

## What did not run

- Nothing ran on Windows. The device-loss paths (N84, N85) need a forced TDR (`dxcap -forcetdr`) or a driver reset on a real Windows machine. Worth checking there: a window opened during recovery draws, and a forced vsync panic closes the windows.
- No test was added for N82 to N85: they are Win32 paths in a crate that only builds on Windows, with no GPU to drive here.
- The new `windows-check` job has not run yet; it runs on this PR.
- The vendored crate was checked against the `gnu` target, not `msvc`. The `windows-check` job covers `msvc`.

## What stays open

- After N84, Quill's panic hook still files a caught vsync panic as a crash, because the fork can't use the app crate's `spawn_guarded`.
- After a device that can't be recreated, or a vsync panic, Quill still exits without saying why (the follow-ups listed in `gpui-fork-panics.md` still apply).
