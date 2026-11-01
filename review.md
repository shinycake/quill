CHANGES

# Review of PR 1307 (claude/sweep-b1, bundle B1, items N82 to N88)

Reviewed head b4b5a02 against main 948ad90. I read the whole diff, and read
`platform.rs`, `events.rs`, `directx_devices.rs` and `vsync.rs` in full
around the changed paths. I ran `scripts/test-timer-step.sh` (8 passed) and
`scripts/test-window-list.sh` (5 passed) on Linux. I did not cross-compile
the fork locally because this container has no Windows target. The PR's own
`windows-check` job on `windows-latest` (msvc) is the evidence for the
Windows build.

The fork changes are mostly sound. Four things block the merge: the new CI
job is red, the "no adapter" error is never reached, and the two items under
"What stays open" are open defects.

## Blocking

### 1. The new `windows-check` job fails on this PR (N87)

Job 124899968191 (and the earlier run 124897159739) fail with 4 errors, all
in the `quill` bin test target:

```
error[E0425]: cannot find function `dlopen` in crate `libc`
   --> src\ui\startup_failure\vulkan.rs:142:28
error[E0425]: cannot find value `RTLD_LAZY` / `RTLD_LOCAL` in crate `libc`
error[E0425]: cannot find function `dlclose` in crate `libc`
error: could not compile `quill` (bin "quill" test) due to 4 previous errors
```

`src/ui/startup_failure.rs:11` declares `#[cfg(any(test, target_os = "linux"))] mod vulkan;`,
so `--all-targets` compiles the Linux-only `dlopen` probe into the Windows
test binary. The code is on main and the PR did not touch it, but N87 is
"PR CI compiles Windows". A job that is red from its first run does not do
that: it either gets ignored or blocks every PR. The PR text says the job
"has not run yet". It has now run twice and failed both times.

Fix: in `startup_failure/vulkan.rs`, gate `loader_installed` (and its
`probe` caller) on `target_os = "linux"`, and keep the pure manifest parsing
under `test`. Another option is `#[cfg(any(all(test, unix), target_os = "linux"))]`
on the module. Then rerun the job until it is green. Dropping
`--all-targets` would hide Windows-only test code again, so don't.
The fork itself (`gpui-pre-windows`) compiled under msvc in that job with no
errors.

### 2. "No GPU adapter supports Direct3D 11" can never be returned (N82)

`directx_devices.rs`, `get_adapter`: the loop is
`for adapter_index in 0.. { let adapter = dxgi_factory.EnumAdapters(adapter_index)?...`.
When adapters run out, `EnumAdapters` returns `DXGI_ERROR_NOT_FOUND` and
the `?` returns that error before the loop can end. The new
`Err(anyhow!("No GPU adapter supports Direct3D 11"))` after the loop is as
unreachable as the `unreachable!()` it replaced. Users get
"Creating DirectX devices: ... (0x887A0002)" in the startup alert, and the
reason each adapter failed only reaches the log. The PR text and
`gpui-fork-panics.md` both claim the friendly message.

Fix: match the `EnumAdapters` error. On `DXGI_ERROR_NOT_FOUND`, `break`.
Pass any other error up. Keep the last `get_device` error and return it
with context ("No GPU adapter supports Direct3D 11: <last error>").
`cast()?` failures should skip the adapter, not abort the search.

### 3. Open item: a caught vsync panic is still filed as a crash (N84)

`src/crash_log.rs` asks `thread_guard::panic_will_be_caught()`, which reads
a thread-local count that only `spawn_guarded` and `ffi_boundary` raise. The
fork's `catch_unwind` in `begin_vsync_thread` does not raise it. A vsync panic
is therefore reported as an uncaught crash, and the next launch shows the
"closed unexpectedly" notice for a panic that was handled.

Fix: move the guard count below both crates. Add a small std-only
`panic_guard` module to the vendored `gpui` core
(`third_party/gpui-pre`, which `gpui-pre-windows` already depends on), with
`pub fn guarded<R>(f: impl FnOnce() -> R) -> std::thread::Result<R>` and
`pub fn panic_will_be_caught() -> bool` over one `thread_local!` counter.
Make `src/thread_guard.rs`'s `guarded` and `panic_will_be_caught` delegate to
it, so there is still one counter. Then replace the bare `catch_unwind` in
`begin_vsync_thread` with `gpui::panic_guard::guarded`. Test it on any host:
a panic inside `guarded` makes `panic_will_be_caught()` true inside the hook,
and false after.

### 4. Open item: Quill exits after a lost device without saying why (N84, N82 recovery path)

After `WM_GPUI_GPU_DEVICE_GONE` (recovery failed, or now a vsync panic),
`handle_device_gone` stores `gpu_error`, destroys every window, and GPUI's
`LastWindowClosed` quit ends the process. The reason goes only to
`logs/platform.log`. The PR turns one more failure (the vsync panic) into
this silent exit.

Fix, in the app crate:

1. On Windows, when the main window closes, ask `cx.graphical_environment()`.
   `None` while Quill never asked to go headless means the device is gone.
2. Try to recover once: `request_windowing(Windowed(GraphicalEnvironment::detect()))`
   (the platform supports this, since the dead devices were dropped), then
   reopen the main window. This needs `QuitMode::Explicit` on Windows, so
   the last window closing does not end the app first.
3. If that fails, call `ui::startup_failure::tell` with the error from
   `request_windowing` or `open_window`, which carries `gpu_error`. Title and
   body go through the lang system ("Quill closed because the graphics
   device stopped working"), and then Quill quits.

Step 3 alone is the minimum. Steps 1 to 3 together match the follow-up that
`gpui-fork-panics.md` already names. Close a PiP window left alone in the same
path.

## Not blocking, worth fixing in the same PR

- **A stale vsync thread can kill a newer one's devices.** `detach_gpu` only
  sets `stop` and does not join. If an old thread then panics, or fails
  recovery, it posts `WM_GPUI_GPU_DEVICE_GONE` without checking `stop`, and
  `handle_device_gone` drops the devices of the thread `attach_gpu` started
  since then. The same goes for its `WM_GPUI_GPU_DEVICE_LOST`, which overwrites
  `directx_devices` with the stale thread's devices. The recovery path had
  this before. The new panic path adds a second way in. Quill never calls
  `request_windowing` at runtime today, so it is latent. Fix: give each vsync
  thread a generation number, send it in `wparam` of both messages, and have
  the platform ignore a message from an older generation. At the least, skip
  the post when `stop` is set.
- **N85 handover reasoning holds.** Recovery sends to the platform window
  synchronously before it copies the list. The UI thread reads the devices,
  pushes, then compares. So a window is either in the copy, or sees the
  new device at its check. The device comparison holds a reference to the
  old device, so a reused address cannot fool it. One gap stays:
  `WindowsWindow::new` on a removed device can fail outright, and
  `open_window` then returns an error instead of waiting for recovery. That
  matters for windows opened mid-session (PiP, viewer), not at launch.
- **N83** is correct. `attach_gpu` detaches on spawn failure and keeps
  `gpu_error` unset only on success. `connect_initially` and
  `request_windowing` are the only callers, and both surface the error.
- **N86** is correct. No lock outlives `snapshot`, `find`, `push` or `remove`,
  no closure that runs under the lock takes another lock, and the gate-then-list
  order in `wait_for_frames` is unchanged. Every former caller of the old
  helpers moved over. Only `QUILL-CHANGES.md` history still names them.
  Small cost: `snapshot` now allocates a `Vec` and bumps two `Arc`s per
  window on every vsync tick and every gate wake, where the old code
  iterated an inline `SmallVec`. Not measured here. It is likely noise at
  1 to 4 windows.
- **N88** is a faithful move. The `MutexGuard` is a temporary in
  `let step = shared.lock().next_step(..)`, so due runnables still run
  unlocked. The `u32::MAX` to `INFINITE` fix is real.

## What I could not check

Nothing ran on Windows: no forced TDR, no vsync panic, no startup without
DirectX. The msvc compile of the fork comes from the PR's CI log only.
