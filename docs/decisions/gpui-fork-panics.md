# Panics in the vendored GPUI platform crates

Branch `gpui-fork-panics`. Step 7 of the no-crash plan
(`docs/decisions/no-crash-audit.md` §4 and §5). No parity item.

## Telegram Desktop

Not a UI feature, so there is nothing to copy. For reference, lib_ui
(`ui/gl/gl_detection.cpp`) picks OpenGL or raster painting per widget
(`ChooseBackendDefault`) and keeps a crash-check marker around GPU setup
(`CrashCheckStart`, `LastCrashCheckFailed`): if the last run died while
initializing OpenGL, the next run paints with the CPU. Quill has no raster
renderer, so the closest safe behavior here is retry, then close the window
or show the startup alert.

## What changed

All fixes stay in our fork under `third_party/`; nothing goes upstream. Each
changed file has the "Modified by the Quill project" header, and each crate's
`QUILL-CHANGES.md` has an entry.

### Windows (`gpui-pre-windows`)

- **Device lost on the vsync thread** (`platform.rs`, was
  `panic!("Device lost: ..")`). `recover_from_gpu_device_lost` retries the
  whole recovery (`handle_gpu_device_lost`) after 0.5 s, 2 s and 5 s. Each
  round still makes upstream's five quick attempts at recreating the devices
  (`try_to_recover_from_device_lost`), so a driver update or reset gets about
  8 s in total. If every round fails, the thread logs the error, posts the
  new `WM_GPUI_DESTROY_AFTER_DEVICE_LOST` to every window, and exits.
- **Device lost in a window's renderer** (`events.rs`, was the same
  `panic!`). The window retries its renderer recovery after 0.2 s and 0.5 s
  (short: it runs on the UI thread while the vsync thread waits on
  `SendMessageW`). If that fails it logs and posts itself the destroy
  message. The `RefCell` borrow of the renderer ends within each statement,
  and nothing touches the window while it is held.
- The destroy message calls `DestroyWindow`, not `WM_CLOSE`. Quill can hide
  the main window to the tray on close, and a window with no GPU device must
  go away. `DestroyWindow` runs GPUI's normal `WM_DESTROY` path (close
  callback, `WM_GPUI_CLOSE_ONE_WINDOW`), so the app sees an ordinary closed
  window. The message is posted, not sent, so it runs outside any draw or
  recovery call.
- `directx_renderer.rs`: `pre_draw` and `present` return errors instead of
  `expect("resources missing")`/`unwrap` (a failed recovery leaves the
  renderer without resources or devices, so a draw queued before the destroy
  message arrives would otherwise panic). `directx_devices.rs`:
  `try_to_recover_from_device_lost` is a loop instead of
  `find_or_last(..).unwrap()`.
- **DirectX fails at launch** (`platform.rs`, was
  `panic!("failed to initialize DirectX")`). `Platform::run` (in the
  unvendored `gpui` crate's trait) returns `()`, so the error can't be
  returned from there. Instead `connect_initially` logs it and leaves the
  platform headless, and keeps the message. Quill's first `open_window` then
  gets `Err("failed to initialize DirectX: <cause chain>")` instead of the
  generic "cannot open windows while headless", and `main.rs` already sends
  that to `ui::startup_failure::window_failed` (#756), which shows a
  `MessageBoxW`, writes stderr and exits with code 1. So the propagation path
  works and no MessageBox or `exit` was needed inside the fork.
- `hide_other_apps` / `unhide_other_apps`: no-ops instead of
  `unimplemented!()`. Windows has no "hide others"; Quill doesn't call them
  today.

- The vsync thread posts `WM_GPUI_GPU_DEVICE_GONE` (`WM_USER + 12`) to the
  platform window. That drops the dead devices, so `is_windowed()` reports
  false, keeps a reason that `open_window` returns ("the GPU device was lost
  and could not be recreated"), the same way the launch failure does, and
  then tells every open window to close.
  `request_windowing(Windowed)` can attach fresh devices later.
- `close_one_window` logs a window that isn't in the list instead of
  `unwrap`ing its position, and `handle_gpui_events` returns `None` for an
  unexpected message instead of `unreachable!()`.
- `directx_renderer.rs`: upstream's device-loss recovery decided whether to
  use DirectComposition from `self.direct_composition.is_none()`, but each
  attempt clears that field first. So every retry after a failed attempt
  came back without DirectComposition, for the rest of the window's life.
  The renderer now keeps the creation-time choice in
  `disable_direct_composition`. The two `dxgi_device.unwrap()`s on that path
  became errors.

What a user sees after an unrecoverable device loss: Quill uses GPUI's
default `QuitMode`, which on Windows (and Linux) is `LastWindowClosed`. So
once the main window is destroyed and no other window is open, GPUI quits
and Quill exits. It does not stay in the tray, no message is shown, and
the reason goes only to `logs/platform.log`. If a picture-in-picture window is still open
and only the main window's renderer failed, the process keeps running with
just that window until it is closed. If the vsync thread's recovery failed,
it closes every window, PiP included, and Quill exits.

That is acceptable for this step. The alternative was a crash with the same
end state (process gone) plus a crash report. Follow-ups worth doing:

- Show the reason on the way out. Something like an `on_window_closed` check
  in Quill that notices the GPU error, or a MessageBoxW from the fork, would
  tell the user why Quill closed (as `startup_failure` does at launch).
- Better, keep Quill alive. Quill could switch to `QuitMode::Explicit` on
  Windows, ask for `request_windowing(Windowed)` after a pause, and reopen
  the main window. The platform side now supports that, because the dead
  devices are dropped and a re-attach creates new ones.
- If only the main window failed while a PiP window lives, the PiP window
  should be closed as well, or the main window reopened. Today Quill keeps
  running with only the PiP window.

### Windows, second pass

A later audit found panics and gaps the first pass left in the Windows
platform. Each is fixed in its own commit:

- `directx_devices.rs`: device creation no longer panics. A device that
  `D3D11CreateDevice` reports without a device or without a context is an
  error, so the adapter is skipped, and running out of adapters returns
  "No GPU adapter supports Direct3D 11". An unexpected feature level is
  logged instead of `unreachable!()`. Both callers already handle the
  error: `attach_gpu` at launch (the startup alert) and
  `try_to_recover_from_device_lost` after a device loss (retry, then close
  the windows).
- `platform.rs`, `begin_vsync_thread`: a failed spawn of the vsync thread
  (out of threads or memory) was `unwrap`ped. It now returns the error, and
  `attach_gpu` releases the devices it just made and returns it. At launch
  that is `connect_initially`'s DirectX error, which the first
  `open_window` returns and `main.rs` shows in the startup alert; from
  `request_windowing(Windowed)` it is that call's error.
- `platform.rs`, the vsync thread: its body runs under `catch_unwind`. A
  panic there used to end the thread silently, so no window got another
  frame and Quill looked hung. Now the thread logs it and posts
  `WM_GPUI_GPU_DEVICE_GONE`, the same message as a device that can't be
  recreated, so the platform drops its devices and closes the windows. The
  app crate's `spawn_guarded` can't be used here (the fork doesn't depend
  on Quill), so Quill's panic hook still files the report as a crash.
- `window_list.rs`: the platform's window list was a bare
  `Arc<RwLock<SmallVec<..>>>`, and the helpers that drop the guard before
  sending were opt-in, so a new caller could hold the read lock across
  `SendMessageW` and deadlock with `open_window` or `close_one_window`
  again. It is now a `WindowList` that only hands out copies (`snapshot`,
  `find`) and changes under its own lock (`push`, `remove`), so no caller
  can hold the lock at all. It uses std's `RwLock` and recovers a poisoned
  lock, so it stays std-only and `scripts/test-window-list.sh` still runs it
  on any host.
- `platform.rs`, `open_window`: a window that opened while the vsync thread
  was recovering from a device loss could stay blank. Recovery sends the
  new devices to the platform window first and then to the windows in the
  list; a window built from the old devices that joined the list after
  that read never got them. `open_window` now compares the device the
  window was made with against the platform's current one once the window
  is listed, and sends it `WM_GPUI_GPU_DEVICE_LOST` with the current devices
  when they differ. Either the window was listed in time for recovery's
  read, or the platform already holds the new devices when it checks, so
  no window misses both.

### macOS (`gpui-pre-macos`)

- `display.rs`: `MacDisplay::all` returns an empty list (and logs) when
  `CGGetActiveDisplayList` fails. The count is clamped to the buffer's
  capacity before `set_len`.
- `text_system.rs`: in `layout_line`, a glyph run without attributes or a
  font is skipped, and so is a glyph with a negative string index. The font
  is looked up with `CFDictionary::find`: upstream's `get` also panics when
  the key is missing, which the brief's list did not mention. The
  push-then-`last_mut().unwrap()` became push-then-`let Some(..) else`.
- `keyboard.rs`: input source id and name use `to_string_lossy`, through a
  small `c_string_lossy` helper that also returns an empty string for a null
  `UTF8String` (upstream would have read a null pointer).

### Linux (`gpui-pre-linux`)

- `x11/client.rs`: a bad `GPUI_X11_SCALE_FACTOR` logs a warning and is
  treated as unset (falls through to Xft.dpi, then 1.0) instead of panicking.

### First-party leftovers from #756

- `src/ui/selectable_text.rs`: the doc comment of `selected_fallbacks` was
  above the new `retained_cell`; it is back on `selected_fallbacks`.
- `src/calls/ptt_global_mac.rs`: if `start` times out after the tap thread
  stored its `CFRetain`ed run loop, nobody released it. Now the tap thread
  swaps the pointer out and releases it when `ready.send` fails, and the
  timeout path in `start` first drops the receiver and then a `Handle`
  (which does the same swap, stops the loop and releases). The order
  matters. Once the receiver is gone, every later send fails, so a retain
  stored from then on is released by the thread. A retain stored earlier,
  whose send succeeded, is still in the slot when the handle swaps. Both
  sides use `swap(0)`, so exactly one of them releases it. Dropping the
  handle first would leak a retain stored just after its swap, because the
  send would still succeed.

## How verified

- `gate.sh` (macOS: fmt, clippy, tests, size and hotspot checks): GATE OK.
- `cargo check -p gpui-pre-windows --target x86_64-pc-windows-msvc` on
  macOS: clean, no warnings. The `gpui-pre` build script needs a resource
  compiler, so `RC` pointed at a stub that writes an empty `.lib` (fine for a
  check; nothing links).
- `cargo check -p gpui-pre-linux --target x86_64-unknown-linux-gnu` on
  macOS: clean, no warnings, with `RUST_FONTCONFIG_DLOPEN=1` and a clang
  wrapper for the C build scripts.

## Not verified

- Nothing was built into a binary or run on Windows or Linux. The Windows
  changes are type-checked only; the device-loss paths were not exercised
  (that needs a real driver reset or `dxcap -forcetdr` on a Windows machine).
  Worth checking on Windows: a forced TDR with recovery succeeding (should
  look the same as before) and with recovery failing (windows close, no
  crash), and launching where DirectX can't start (the startup alert shows
  the DirectX error).
- The Linux scale factor change is type-checked only.
- The macOS CoreText and display-list paths can't be triggered on demand;
  they are covered by the gate's build and the existing text tests only.
