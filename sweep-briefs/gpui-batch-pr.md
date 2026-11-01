Survive a failed Metal setup, skip the shader compile at first launch, and draw less during drags and scrolls

This changes the macOS release workflow: `package-macos.yml` gets one new environment variable, `QUILL_REQUIRE_METALLIB=1`, on the "build release ui" step. With it set, a release build fails if the Metal toolchain can't produce the precompiled shaders. `ci.yml` also gets one new test step, and the app writes a new small file, `logs/platform.log`.

## What was wrong

### Metal setup failure aborted the app (stability-3)

`MetalRenderer::new_internal` called `expect` on the shader library and on every pipeline. When `MTLCompilerService` failed under load, Quill died with SIGABRT. Four crash reports (`quill-demo-present-2026-10-30-010851*.ips`) came from a run where several demos opened at once.

### A launchd start gets 256 open files (stability-4)

`launchctl limit maxfiles` here reports a soft limit of 256. Each account's TDLib client holds its binlog, its SQLite files and a socket per data center, so a few accounts with media open come near it. Past it, every `open` and `socket` fails.

### GPUI's warnings and errors went nowhere (review-2)

Quill installed no `log` backend, so every `log_err` in the GPUI fork wrote nothing, a Windows device give-up included.

### A window could outlive a Windows device give-up (review-3)

The vsync thread reads the window list before it posts `WM_GPUI_GPU_DEVICE_GONE`. A window opened in between missed the destroy broadcast and stayed open on dead devices.

### Each drag update redrew everything (gpui-4)

Any mouse move during a drag called `refresh()`, which turns off every view cache, though the drag ghost is painted on every draw anyway. AppKit's periodic updates while a file is held still did the same.

### A second of unchanged frames after each scroll (gpui-5)

`InputRateTracker` kept presenting the same frame at display rate for 1 s after high-rate input stopped. That also held off the macOS frame idle by a second.

### Chat windows never swept emoji glyphs (gpui-6)

The idle sweep ran on the main window only, so tiles in a chat window's atlas stayed until the window closed.

### First launch compiled the Metal shaders on the main thread (gpui-3)

gpui-kit turns on `runtime_shaders`, so every build shipped shader source. A new user paid about 0.5 s before the window appeared, as did every user after a release that edits the shaders.

## What changed

- stability-3: the renderer constructors return `Result`. `MacWindow::open` builds the renderer before any AppKit object and passes the error up through `open_window`, so a failure costs one window. One failed setup gets one retry.
- stability-4: new `src/fd_limit.rs` raises the soft limit to 1024 (capped by the hard limit) at the start of `main`. It stops at 1024 because `FD_SETSIZE` is 1024 and WebRTC's macOS socket server uses `select()`.
- review-2: new `src/advanced/platform_log.rs`, a `log` backend that writes warnings and errors to `logs/platform.log`, capped at 1 MiB like the other always-on logs. It installs first thing in `crash_log::begin_app_run`.
- review-3: `handle_device_gone` posts `WM_GPUI_DESTROY_AFTER_DEVICE_LOST` to every tracked window once more after dropping the devices. `open_window` fails from that point on, so no window can miss it.
- gpui-4: a drag move calls `refresh()` only when the hovered hitbox changed. Otherwise it marks the window dirty, and an update at the spot already painted does nothing. A file drag entering the window refreshes once.
- gpui-5: `sustain_duration` goes from 1 s to 150 ms. `InputRateTracker` moves to `input_rate.rs` with injected instants, tested by the new `scripts/test-input-rate.sh`.
- gpui-6: sweep state moves from thread-local cells to a map keyed by window. Each chat window starts sweep frames from its own `render`, and the idle trim begins a sweep for every open window.
- gpui-3: `gpui-pre-apple/build.rs` tries `xcrun metal` whatever the feature says and embeds the metallib when it works. A machine without the Metal toolchain falls back to source, as today. The renderer loads the metallib and falls back to the embedded source with a warning if it doesn't load. `QUILL_REQUIRE_METALLIB` turns the build-time fallback into a build error, and only the release workflow sets it.

## Measurements

All numbers come from the `fastrel` profile (no LTO, 16 codegen units), so they are relative. gpui-4 and gpui-5 came from one binary with local hooks that restore the old behavior, run A, B, A, B on the same demo. The hooks are not on this branch.

gpui-4: A scripted drag on `ready-chats` at 1200x800: 180 moves over empty history, about 211 frames in every run. The whole run includes launch.

| | Old | New |
| --- | --- | --- |
| Instructions retired | 6.67 G, 6.77 G | 3.43 G, 3.36 G |
| CPU (user + sys) | 1.18 s, 1.30 s | 0.93 s, 0.83 s |

That is about 16 M instructions less per drag frame. The frame count stays the same, because the ghost still moves on every frame.

gpui-5: 300 wheel events at about 60/s on `ready-albums`, display at 60 Hz.

| | 1 s sustain | 150 ms sustain |
| --- | --- | --- |
| Unchanged-frame presents after the last input | 33, 33 | 4, 4 |
| Median frame gap in the scroll | 16.7 ms, 16.7 ms | 16.8 ms, 16.7 ms |
| Gaps over 20 ms | 45 of 355, 46 of 357 | 42 of 352, 52 of 364 |
| Instructions retired | 5.36 G, 5.38 G | 5.49 G, 5.43 G |

The finding expected 60 to 120 presents before. This demo keeps drawing a fade through the tail, 27 frames of it, and 33 + 27 is one frame per vsync for the second. The scroll is no less smooth. Instructions are within 2% and I claim no CPU saving. The saving is GPU and compositor work, which I did not measure.

gpui-3: compiling the source took 499 to 514 ms on this Mac with a unique comment so nothing is cached (the finding measured 531 to 560 ms). That is the time the metallib removes. I have no number for loading the metallib, because this Mac has no Metal toolchain and hosted run 41288733677 on `cursor/gpui-batch`, which builds one and uploads it as `gpui-batch-metallib`, had not reached `macos-package` when I wrote this.

stability-4: `launchctl limit maxfiles` prints 256 soft and unlimited hard. A demo holds 41 open files with no TDLib client, so that is a floor.

## Tests

The 22 filtered tests pass on macOS, run just before the last merge of main, and `tools/quick-gate.sh` prints QUICK OK on the final head. `the_precompiled_shaders_build_every_pipeline` has not run yet: it needs a metallib, which this Mac can't build, and it waits on that hosted job.

- stability-3: `shaders_that_do_not_compile_are_an_error`, `a_library_without_our_functions_is_an_error` and `one_failed_library_build_is_retried` run the real renderer on the real device with bad shaders.
- stability-4: three tests on the limit calculation, and `the_process_limit_rises_from_launchds_default`, which sets a child process to 256 and calls the real function.
- review-2: three tests send records through the `Log` implementation into the real file. The `log_err` shape (an empty target) is one of them.
- review-3: compile only, `cargo check -p gpui-pre-windows --target x86_64-pc-windows-msvc`.
- gpui-4: four tests in `src/ui/drag_redraw_tests.rs` count renders of a cached view. They failed before the fix (the slice rendered 4 and 3 times, then 5 and 4).
- gpui-5: four tests with injected instants, run by `scripts/test-input-rate.sh`. The `the_tail_ends_150_ms_after_the_last_input` test fails with a 1 s sustain.
- gpui-6: three tests in `src/ui/glyph_sweep_tests.rs` with two windows. With the sweep limited to the first open window they fail, leaving 129 tiles behind.

## What I could not verify

- stability-3: I did not reproduce a real `MTLCompilerService` failure. The tests force the failure with bad shaders, and the evidence for the crash is the four reports.
- stability-4: no real launchd start with several accounts. `otool` on the 21 October `libntgcalls.dylib` shows `PhysicalSocketServer::WaitSelect` calling `select`. A newer build wasn't checked.
- review-2: I never saw `logs/platform.log` from a live app. Demos skip `begin_app_run`, so the backend isn't installed in them.
- review-3: never run on Windows.
- gpui-4: no real file drag from Finder, so the periodic AppKit updates come from AppKit's documented default. A known gap stays: when an overlay changes what's under a pointer that hasn't moved, a `drag_over` style stays stale until the pointer crosses a hitbox.
- gpui-5: no ProMotion panel and no power or GPU numbers.
- gpui-6: only headless atlas tests. I never watched a real chat window's atlas shrink.
- gpui-3: the release build with `QUILL_REQUIRE_METALLIB=1` has not run on `macos-15` yet. If that runner's toolchain fails, `macos-package` goes red on main. Here the guard stops the build as intended ("metal shader compilation failed"), and a plain build falls back to source. There was no launch-bench run, because that needs Quill's Metal cache deleted, and no first launch on a slower Mac. The patch has to be redone at each GPUI re-vendor.
