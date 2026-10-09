# Changes made by the Quill project

This directory is gpui-pre-linux 0.3.7 from crates.io (Apache-2.0,
`LICENSE-APACHE`), GPUI's Linux platform (X11 and Wayland), used through
`[patch.crates-io]` in Quill's `Cargo.toml`. Commit `86dc5470` added the
unmodified registry copy (without the registry's `.cargo-ok`,
`Cargo.toml.orig` and `Cargo.lock`), so
`git diff 86dc5470 -- third_party/gpui-pre-linux` shows every change.

The Quill project changed it in 2026 so an idle X11 window stops waking the
main thread at the display's refresh rate. Wayland is unchanged (it already
draws on demand).

- `src/linux/x11/frame_idle.rs` (new, identical to
  `third_party/gpui-pre-windows/src/frame_idle.rs`): the idle decision. After
  250 ms of frames that neither drew, presented nor asked for another frame,
  with no frame demand from GPUI in between, a window's frame source parks
  and gets one heartbeat frame a second; a demand unparks it. Unit tests run
  with `scripts/test-frame-idle.sh`. `QUILL_IDLE_FRAMES=0` (or `off`), read
  once, disables parking (upstream behavior).
- `src/linux/x11/window.rs`: `X11FrameIdle`, shared by the refresh timer,
  `draw` and `schedule_frame` (both mark activity) and the new `frame_waker`
  (GPUI calls it when the window becomes dirty or queues a next-frame
  callback), which pings the event loop when the timer is parked.
- `src/linux/x11/client.rs`: upstream's `start_refresh_loop` timer asks every
  visible window for a frame at the refresh rate forever. Now, after each
  frame, an idle window's timer re-arms at the heartbeat (1 s) instead, and a
  per-window calloop ping source (`resume_refresh_loop`) replaces it with an
  immediate refresh-rate timer when the frame waker fires. A pending forced
  render after GPU recovery keeps the timer running, and a window whose ping
  could not be created never parks. The ping source is removed with the
  window.
- `src/linux/x11.rs`: declares the new module.

Each changed source file starts with a comment saying so. The changes are
licensed under Apache-2.0, like the rest of the crate. Background and what
still needs a live check: `docs/decisions/codex-idle-frames-x11-windows.md`.
