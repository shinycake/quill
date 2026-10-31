# Changes made by the Quill project

This directory is gpui-pre-linux 0.3.10 from crates.io (Apache-2.0,
`LICENSE-APACHE`), GPUI's Linux platform (X11 and Wayland), used through
`[patch.crates-io]` in Quill's `Cargo.toml`. Commit `6be84cd7` added the
unmodified registry copy (without the registry's `.cargo-ok`,
`.cargo_vcs_info.json`, `Cargo.toml.orig` and `Cargo.lock`), so
`git diff 6be84cd7 -- third_party/gpui-pre-linux` shows every change. (The
patch was first made on 0.3.7, carried to 0.3.8 in
`docs/decisions/codex-gpui-kit-0.7.1.md` and to 0.3.9 in `docs/decisions/gpui-kit-0-7-3.md`, and to 0.3.10 in `docs/decisions/gpui-kit-0-8-1.md`.)

The Quill project changed it in 2026 so an idle X11 window stops waking the
main thread at the display's refresh rate. Wayland is unchanged (it already
draws on demand).

- `src/linux/x11/frame_idle.rs` (new, identical to
  `third_party/gpui-pre-windows/src/frame_idle.rs`): the idle decision. After
  250 ms of frames that neither drew, presented nor asked for another frame,
  with no frame demand from GPUI in between, a window's frame source parks
  and gets no frames at all, like the macOS display link; a demand unparks
  it. `next_frame_at` computes the timer's next deadline (the refresh grid,
  skipping missed frames, or none while parked). Unit tests run with
  `scripts/test-frame-idle.sh`. `QUILL_IDLE_FRAMES=0` (or `off`), read
  once, disables parking (upstream behavior).
- `src/linux/x11/window.rs`: `X11FrameIdle`, shared by the refresh timer,
  `draw` and `schedule_frame` (both mark activity) and the new `frame_waker`
  (GPUI calls it when the window becomes dirty or queues a next-frame
  callback), which pings the event loop when the timer is parked. It also
  numbers the window's refresh timers and records whether the current one
  is still registered, since a parked timer drops itself.
- `src/linux/x11/client.rs`: upstream's `start_refresh_loop` timer asks every
  visible window for a frame at the refresh rate forever. Now an idle
  window's timer drops itself after its last frame, and a per-window calloop
  ping source (`resume_refresh_loop`) starts an immediate refresh-rate timer
  when the frame waker fires. Removing the timer (hiding, resuming, closing
  the window, dropping the connection) skips one that already dropped
  itself, and a timer replaced while its callback ran stops. A pending
  forced render after GPU recovery keeps the timer running, and a window
  whose ping could not be created never parks. The ping source reaches the
  client through 0.3.8's weak `X11ClientStatePtr` (the event loop no longer
  carries the client) and is removed with the window, or with the connection
  when `X11Connection` drops. `mode_refresh_rate` no longer divides by zero
  for a dot clock below 1 mHz.
- `src/linux/platform.rs`: after each round of the event loop, before it
  sleeps, runs the X11 client's `after_runnable` hook (which processes X11
  events), so events that xcb read into its queue during a request made
  outside a runnable are handled even though no timer runs any more.
- `src/linux/x11.rs`: declares the new module.

Until `docs/decisions/sweep-idle-heartbeat.md` a parked timer still ticked
once a second as a fail-safe; that heartbeat is gone. Those changes compile
in Linux CI and the X11 path ran under Xvfb in CI's launch smoke (idle
wakeups measured there); not run on a real X11 desktop.

Each changed source file starts with a comment saying so. The changes are
licensed under Apache-2.0, like the rest of the crate. Background and what
still needs a live check: `docs/decisions/codex-idle-frames-x11-windows.md`.

The Quill project also changed it in 2026 so a bad `GPUI_X11_SCALE_FACTOR`
no longer crashes the app (step 7 of `docs/decisions/no-crash-audit.md`):

- `src/linux/x11/client.rs`: `get_scale_factor` logs a warning and ignores a
  value that is neither `randr` nor a positive normal number, falling back to
  the Xft.dpi / RandR detection, instead of panicking.

Type-checked with `cargo check --target x86_64-unknown-linux-gnu` on macOS;
not built or run on Linux. See `docs/decisions/gpui-fork-panics.md`.

The Quill project also changed it in 2026 for Agent access's screenshots
(`docs/decisions/agent-access.md`, phase 5 step 3):

- `src/linux/x11/window.rs`, `src/linux/wayland/window.rs`:
  `capture_image` renders the scene through `WgpuRenderer::capture_scene`,
  and answers with an error while the window state is borrowed. Adds the
  `image` dependency (no default features). Type-checked with
  `cargo check --target x86_64-unknown-linux-gnu` on macOS; not run on
  Linux by the author.

The Quill project also changed it in 2026 so Wayland events the protocol
allows no longer panic on the main thread (sweep finding linux-windows-5):

- `src/linux/wayland/client.rs`: a keymap xkb cannot parse is logged and the
  previous keymap kept (`compile_keymap`, was `expect`); `Modifiers` and `Key`
  events that arrive with no keymap (a compositor that sent `no_keymap`, a
  non-XkbV1 format, or one xkb refused) are dropped with a log
  (`update_modifiers`, was `unwrap`); drag enter skips the offer when no
  pipe can be made (was `Pipe::new().unwrap()`).
- `src/linux/wayland/clipboard.rs`: a clipboard read returns nothing when no
  pipe can be made, and `flush_for_reply` waits for a full socket
  (`WouldBlock`, which `Connection::flush` reports as an error) to drain
  instead of `flush().unwrap()`. Tests in both files ran with
  `cargo test -p gpui-pre-linux -p quill --lib --features quill/demo-capture`
  (the second package turns on `gpui/test-support`) on Linux.
