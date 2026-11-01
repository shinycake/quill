On Windows, Quill no longer crashes, freezes or quits silently when GPU setup fails or the GPU is lost, and every PR now compiles the Windows build

## What was wrong

Sweep bundle B1 (items N82 to N88, audit of main at 5072e9662). Each item was checked against current main first; all seven were still open.

- N82: `directx_devices.rs` panicked while creating the GPU device: `context.unwrap()` in `get_adapter`, the `unreachable!()` after its loop, `device.unwrap()` in `get_device`, and a second `unreachable!()` on an unexpected feature level in `DirectXDevices::new`.
- N83: `begin_vsync_thread` unwrapped `Builder::spawn`, so running out of threads or memory while attaching the GPU panicked on the UI thread.
- N84: a panic on the vsync thread ended it with nobody noticing. No window got another frame and Quill looked hung.
- N85: a window that opened while the vsync thread was recovering from a device loss could stay blank. Recovery sends the new devices to the windows it lists after the platform got them; a window built from the old devices that joined the list later never got them.
- N86: the window list was a bare `Arc<RwLock<SmallVec<..>>>`. The helpers that drop the guard before `SendMessageW` were opt-in, so a new caller could bring back the deadlock with `open_window` / `close_one_window`.
- N87: PR CI compiled nothing for Windows. Windows builds ran on pushes only.
- N88: `timer_thread.rs` had no test.

The review of the first version (head b4b5a02) found four blocking problems and two more gaps:

- The new `windows-check` job was red from its first run: the `quill` bin test target compiled the Linux-only `dlopen` probe in `ui/startup_failure/vulkan.rs` into the Windows build (4 errors).
- "No GPU adapter supports Direct3D 11" could never be returned. `EnumAdapters` ends with `DXGI_ERROR_NOT_FOUND`, and the `?` in the loop returned that code first, so the startup alert showed "0x887A0002".
- A caught vsync panic was still filed as a crash: Quill's panic hook only knew the guards that `spawn_guarded` and `ffi_boundary` raise, so the next launch showed the "closed unexpectedly" notice.
- After a lost device (recovery failed, or a vsync panic), the platform destroyed every window and Quill exited with the reason only in `logs/platform.log`.
- A stopped vsync thread could still post `WM_GPUI_GPU_DEVICE_GONE` or send recovered devices, and so drop or overwrite the devices of the thread started after it (`detach_gpu` does not join).
- `open_window` on a device the GPU had removed failed outright instead of waiting for recovery, so a PiP or viewer window opened in that gap returned an error.

## What changed

One commit per item or finding, plus one fix-up commit and one docs commit.

- N82: device creation returns errors. An adapter without a device or context is skipped, and an unexpected feature level is logged. Review fix: the adapter search now ends at `DXGI_ERROR_NOT_FOUND`, skips an adapter that fails to cast or to create a device, and returns "No GPU adapter supports Direct3D 11: <last adapter's error>". The existing callers handle the error: the startup alert at launch, retry then close the windows after a device loss.
- N83: `begin_vsync_thread` returns the spawn error. `attach_gpu` detaches the devices it just made and returns it, so launch shows the startup alert (through `connect_initially`, `open_window` and `ui::startup_failure`) and `request_windowing(Windowed)` returns an error.
- N84: the vsync thread body runs under a panic guard. On a panic it logs and posts `WM_GPUI_GPU_DEVICE_GONE`, so the platform drops its devices and closes every window, the same end as a device that can't be recreated. Review fix: the per-thread guard count moved from `src/thread_guard.rs` into a new std-only workspace crate, `crates/panic-guard` (`quill-panic-guard`). The app and `gpui-pre-windows` both depend on it, so there is still one counter, and the vsync thread runs under its `guarded`, so the panic hook files that panic as caught. It is not in the vendored `gpui-pre` as the review suggested, because the app's core library (`--no-default-features`, where `thread_guard` lives) has no GPUI.
- N84 and N82 recovery path, review fix: on Windows Quill now uses `QuitMode::Explicit` and quits itself when the last window closes while the GPU works, as GPUI did before. When `graphical_environment()` is `None` instead (Quill never asks to go headless), it closes any window left (a PiP window), asks once for `request_windowing(Windowed)` and reopens the main window. If that fails, or the device goes a second time, it quits the normal way and, once the quit observers have run, shows "Quill closed because the graphics card stopped working" with the error, through the lang system. That is the alert Linux already shows; one new string covers the second loss. Opening the main window moved from `main`'s run closure into `open_main_window` so the recovery can call it again (`Hotspot-change` trailer on that commit).
- N85: `open_window` remembers the device the window was built with. Once the window is in the list it compares that with the platform's current device and, if recovery replaced it in between, sends the window `WM_GPUI_GPU_DEVICE_LOST` with the current devices. Review fix: before it builds the window, `open_window` checks whether the GPU removed the platform's device and, if so, makes new devices itself. It can't wait for the vsync thread, because recovery sends to the platform window on this same thread. The vsync thread still recovers the open windows from its own check, and the handover check covers a window it lists after replacing the devices again.
- Stale vsync threads, review fix: each vsync thread has a generation that every start and stop moves on. `WM_GPUI_GPU_DEVICE_GONE` carries it in `lparam` (`wparam` holds the validation number), and the recovered devices travel in a `RecoveredDevices` that carries it. The platform ignores both from an older generation and answers `HANDED_OVER` only when it took the devices; a thread whose devices were refused stops without sending them to the windows.
- N86: `WindowList` wraps the lock and only hands out copies (`snapshot`, `find`) or changes the list under its own lock (`push`, `remove`). Every caller in `platform.rs` and `vsync.rs` uses it and the old helpers are gone. It uses std's `RwLock` with poison recovery, so it stays std-only.
- N87: new `windows-check` job on `windows-latest`, on every PR: `cargo check --features demo-capture --all-targets --locked`. Review fix: the Vulkan loader probe and its only caller in `ui/startup_failure/vulkan.rs` are Linux-only now, so the bin test target builds on Windows. `--all-targets` stays.
- N88: the timer thread's decisions (stop, fire due timers, wait for a wake, timed wait and whether to hold the 1 ms system timer), the relative `SetWaitableTimer` due time and the fallback timeout moved to the std-only `timer_step.rs`. `scripts/test-timer-step.sh` builds it with `timer_queue.rs` on any host, and CI's Linux job runs it. This also fixed one edge: a fallback wait of exactly `u32::MAX` ms used to become `INFINITE`.
- Fix-up: running rustfmt on `gpui_windows.rs` also reformatted five modules it declares. A separate commit puts them back unchanged.

`docs/decisions/gpui-fork-panics.md` has a "Windows, second pass" section, now with the review fixes, and its follow-up list is updated. `third_party/gpui-pre-windows/QUILL-CHANGES.md` has a matching entry.

## What ran

- `cargo check --target x86_64-pc-windows-gnu --features demo-capture --all-targets --locked` on Linux with mingw-w64: clean (no errors) at the final head. Before the vulkan fix the same command failed with the 4 errors from the CI log (`dlopen`, `RTLD_LAZY`, `RTLD_LOCAL`, `dlclose` in the `quill` bin test target), so the failure reproduced here first. No new warnings in the touched files; the Windows build's existing warnings elsewhere are unchanged.
- `cargo check -p gpui-pre-windows --target x86_64-pc-windows-gnu`: clean after each fork commit.
- `cargo clippy -- -D warnings` with `--features demo-capture --all-targets` and with `--no-default-features --all-targets` on Linux: clean. The vendored Windows crate is not a workspace member, so these do not lint it.
- Tests: `cargo test -p quill-panic-guard` (1 passed: a panic inside `guarded` reads as caught in the panic hook, and the count is back to zero after), `cargo test --lib thread_guard` (7 passed), `cargo test --features demo-capture --bin quill startup_failure` (9 passed, 2 of them new for the device-loss decisions: quit on the last window while the GPU works, close the rest, recover once, then give up), `scripts/test-window-list.sh` (5 passed), `scripts/test-timer-step.sh` (8 passed), `scripts/test-timer-queue.sh` (4 passed).
- `cargo fmt --all -- --check`, `rustfmt --check` on the touched vendored files, `check-file-size.sh`, `check-hotspots.sh` (main.rs allowed by the trailer), `check-app-name.sh`: all OK.

## What did not run

- Nothing ran on Windows. The device-loss paths need a forced TDR (`dxcap -forcetdr`) or a driver reset on a real Windows machine. Worth checking there: the main window comes back after one forced device loss; a second one ends Quill with the "graphics card stopped working" alert; a forced vsync panic does the same and leaves no "closed unexpectedly" notice on the next launch; a window opened during recovery draws.
- No test drives N82, N83, N85, the vsync generation or the removed-device check in `open_window`: they are Win32 paths in a crate that only builds on Windows, with no GPU to drive here. The device-loss recovery in Quill is tested as a pure decision; the GPUI wiring around it (`on_window_closed`, `request_windowing`, reopening the window) only compiled.
- The vendored crate was checked against the `gnu` target, not `msvc`. The `windows-check` job covers `msvc`; it had not rerun on the new head when this was written.
- Nothing ran on macOS. This PR adds no macOS code; `remember_gpu_gone` is compiled out there.

## What stays open

- Reopening the main window after a lost GPU builds a new `QuillApp`, so the TDLib clients the old window closed reconnect from the stored credentials. A pending launch deep link is not replayed.
- When only the main window's renderer fails (the device itself recovered) while a PiP window lives, Quill still keeps running with only the PiP window. The recovery above only covers a device the platform gave up on.
- `snapshot` in `WindowList` allocates a `Vec` per vsync tick and gate wake where the old code iterated a `SmallVec` in place. Not measured here; likely noise at 1 to 4 windows.
