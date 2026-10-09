# Changes made by the Quill project

This directory is gpui-pre-macos 0.3.7 from crates.io (Apache-2.0,
`LICENSE-APACHE`), GPUI's macOS platform, used through `[patch.crates-io]` in
Quill's `Cargo.toml`. Commit `e49b3724` added the unmodified registry copy
(without the registry's `.cargo-ok`, `Cargo.toml.orig` and `Cargo.lock`), so
`git diff e49b3724 -- third_party/gpui-pre-macos` shows every change.

The Quill project changed it in 2026 so an idle window stops costing CPU:

- `src/window.rs`: upstream keeps a window's `CVDisplayLink` subscription
  running whenever the window is visible, so the main thread and a CoreVideo
  thread wake 60–120 times a second even when nothing changes. Now a step
  that neither draws, presents nor asks for another frame counts as idle;
  after 250 ms of those the window unsubscribes from the display link.
  `MacWindow` implements `frame_waker` (GPUI calls it when a window becomes
  dirty or queues a next-frame callback) to request one step on the window's
  dispatch source while the link is idle; that step draws and resubscribes.
  `schedule_frame` and `draw` mark activity. Stops for other reasons
  (occlusion, direct draws) behave as before. The waker holds the window's
  dispatch source; window teardown still cancels it, so a waker that
  outlives the window never runs a step.
- `src/display_link.rs`: `WindowFrameSource::is_running` and
  `WindowFrameSource::requests`.
- `Cargo.toml`: allows the `deprecated` lint, which the registry build caps
  anyway (the crate still uses the `cocoa` crate).

Each changed source file starts with a comment saying so. The changes are
licensed under Apache-2.0, like the rest of the crate. Background and
measurements: `docs/decisions/codex-idle-cpu.md`.
