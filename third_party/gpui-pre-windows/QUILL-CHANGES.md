# Changes made by the Quill project

This directory is gpui-pre-windows 0.3.7 from crates.io (Apache-2.0,
`LICENSE-APACHE`), GPUI's Windows platform, used through `[patch.crates-io]`
in Quill's `Cargo.toml`. Commit `86dc5470` added the unmodified registry copy
(without the registry's `.cargo-ok`, `Cargo.toml.orig` and `Cargo.lock`), so
`git diff 86dc5470 -- third_party/gpui-pre-windows` shows every change.

The Quill project changed it in 2026 so an idle window stops receiving a
`WM_PAINT` and a GPUI frame request on every vblank:

- `src/frame_idle.rs` (new, identical to
  `third_party/gpui-pre-linux/src/linux/x11/frame_idle.rs`): the idle
  decision. After 250 ms of frames that neither drew, presented nor asked for
  another frame, with no frame demand from GPUI in between, a window parks
  and gets one heartbeat invalidation a second; a demand unparks it. Unit
  tests run with `scripts/test-frame-idle.sh`. `QUILL_IDLE_FRAMES=0` (or
  `off`), read once, disables parking (upstream behavior).
- `src/vsync.rs`: `FrameGate`, the set of parked windows shared with the
  vsync thread, which sleeps on a condition variable while every window is
  parked (waking for the heartbeat or an unpark).
- `src/platform.rs`: upstream's `begin_vsync_thread` invalidates every window
  on every vblank. It now waits on `FrameGate` first and skips parked windows
  except on the heartbeat; the device-lost check still runs on every
  iteration. A new window is unparked (in case a closed one had the same
  handle); a closed one is forgotten.
- `src/window.rs`: `draw` and `schedule_frame` mark activity; the new
  `frame_waker` unparks a parked window and invalidates it at once
  (`demand_frames`); `frames_after_draw` applies the idle decision.
- `src/events.rs`: `draw_window` applies the idle decision after each frame
  request, kept running while a forced render is pending (device-loss
  recovery) or a touchpad gesture may be in progress; a draw deferred because
  another draw was in progress, and a Direct Manipulation hit test, unpark
  the window.
- `src/direct_manipulation.rs`: the viewport runs in manual-update mode and
  only advances when `draw_window` calls `update`, so it reports whether a
  gesture may be in progress: the viewport is running or in inertia, or a
  touchpad contact was handed to it in the last 5 s.
- `src/gpui_windows.rs`: declares the new module.

Each changed source file starts with a comment saying so. The changes are
licensed under Apache-2.0, like the rest of the crate. Background and what
still needs a live check: `docs/decisions/codex-idle-frames-x11-windows.md`.
