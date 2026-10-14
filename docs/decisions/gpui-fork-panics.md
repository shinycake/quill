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

What a user sees after an unrecoverable device loss: every Quill window
closes. If Quill keeps running (tray), reopening a window fails with an
error from the renderer rather than a crash, because the vsync thread has
stopped and the platform still holds the dead device. Restarting Quill
recovers. A full "re-attach the GPU later" path would need a GPUI windowing
change and is out of scope.

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
  timeout path in `start` drops a `Handle` (which does the same swap, stops
  the loop and releases). Both sides use `swap(0)`, so exactly one of them
  releases the retain, whichever order the race goes.

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
