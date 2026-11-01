# Changes made by the Quill project

This directory is gpui-pre-macos 0.3.10 from crates.io (Apache-2.0,
`LICENSE-APACHE`), GPUI's macOS platform, used through `[patch.crates-io]` in
Quill's `Cargo.toml`. Commit `6be84cd7` added the unmodified registry copy
(without the registry's `.cargo-ok`, `.cargo_vcs_info.json`,
`Cargo.toml.orig` and `Cargo.lock`), so
`git diff 6be84cd7 -- third_party/gpui-pre-macos` shows every change. (The
patch was first made on 0.3.7, carried to 0.3.8 in
`docs/decisions/codex-gpui-kit-0.7.1.md`, where `step` kept 0.3.8's frame
signal timestamps, and to 0.3.9 in `docs/decisions/gpui-kit-0-7-3.md`, and to 0.3.10 in `docs/decisions/gpui-kit-0-8-1.md`.)

The Quill project changed it in 2026 so an idle window stops costing CPU:

- `src/window.rs`: upstream keeps a window's `CVDisplayLink` subscription
  running whenever the window is visible, so the main thread and a CoreVideo
  thread wake 60–120 times a second even when nothing changes. Now a step
  that neither draws, presents nor asks for another frame counts as idle;
  after 250 ms of those the window unsubscribes from the display link.
  `MacWindow` implements `frame_waker` (GPUI calls it when a window becomes
  dirty or queues a next-frame callback) to request one step on the window's
  dispatch source while the window is visible and the link is not running.
  That step draws, and the link stays stopped after a lone draw: it comes
  back when GPUI asks for another frame after the draw, or for a second
  woken draw within 100 ms, so a caret blink wakes the app once per blink
  and scrolling is still paced by the display. `schedule_frame` before a
  draw does not count, because GPUI's effect flushes call it for any dirty
  window. The decision is in `src/link_pace.rs` (new, standard library only;
  `scripts/test-frame-idle.sh` runs its tests on any host). Spare drawables
  go 3 s after the last activity, one timer at a time.
  The waker keys on whether the link runs, not on why it stopped:
  a restart that finds no screen yet, or misses the occlusion change that
  would retry it, no longer leaves a visible window that never repaints
  (`docs/decisions/notification-click-repaint.md`). The waker holds the window's
  dispatch source; window teardown still cancels it, so a waker that
  outlives the window never runs a step.
- `src/display_link.rs`: `WindowFrameSource::is_running` and
  `WindowFrameSource::requests`.
- `Cargo.toml`: allows the `deprecated` lint, which the registry build caps
  anyway (the crate still uses the `cocoa` crate).

The Quill project also changed it in 2026 so an OS notification can carry
the sender's picture and an inline reply field, which GPUI's
`SystemNotification` (in the unvendored `gpui` crate) has no fields for:

- `src/notification_extras.rs` (new, public, identical to
  `third_party/gpui-pre-windows/src/notification_extras.rs`; unit tests run
  with `scripts/test-notification-extras.sh`): `NotificationExtras { image,
  photo, text_input }` registered under the notification's tag with
  `set_notification_extras` before `App::show_system_notification`, and
  `take_reply_text(tag)` for the text typed into the field, parked by the
  backend before the response is sent.
- `src/system_notifications.rs`: `show` takes the extras for the tag; the
  action named in `text_input` becomes a `UNTextInputNotificationAction`
  (categories are keyed by actions plus text input); the photo, or else the
  image, is copied to a temporary file (keeping its extension) and attached as a `UNNotificationAttachment` (the center
  moves attached files into its own store). The delegate reads
  `UNTextInputNotificationResponse.userText` into the side channel.
- `src/system_notifications.rs`: the notification center lives on one
  worker thread (`gpui-notifications`). Creating it (about 130 ms on the
  first post), copying and validating the picture, registering categories
  and every post or dismiss are round trips to `usernoted` that used to run
  on the main thread; the main thread now only queues the job. A single
  thread keeps posts and dismissals in order.
- `src/gpui_macos.rs`: declares the new module.

Each changed source file starts with a comment saying so. The changes are
licensed under Apache-2.0, like the rest of the crate. Background and
measurements: `docs/decisions/codex-idle-cpu.md`; notifications:
`docs/decisions/codex-notify-native.md`.

The Quill project also changed it in 2026 to remove panics a user can hit
(step 7 of `docs/decisions/no-crash-audit.md`):

- `src/keyboard.rs`: the input source id and name are read with
  `to_string_lossy` (and as empty for a null pointer) instead of
  `to_str().unwrap()`.

See `docs/decisions/gpui-fork-panics.md`. Two more fixes from that step
are no longer here: 0.3.9 fixed `MacDisplay::all` upstream (it logs and
returns no displays when `CGGetActiveDisplayList` fails), and it moved
`text_system.rs` to gpui-pre-apple, which now carries the CoreText fix.

The Quill project also changed it in 2026 so video can be paced on the
display's real present time (`docs/decisions/video-player-core.md`):

- `src/display_link.rs`: the output callback converts its `output_time`
  (the vsync the frame being requested is shown at) to an `Instant`, sampled
  against the current host time like `frame_request_instant` (both through
  `objc2_core_video::CVGetCurrentHostTime` since 0.3.9), and keeps it
  with its host ticks on the display's registry entry, together with the
  output time's refresh period (`videoRefreshPeriod / videoTimeScale`).
  `present_timing(display_id)` reads both under one registry lock.
- `src/gpui_macos.rs`: `pub fn present_timing(display_id: u64)`, taking a
  `DisplayId` as `u64`. Quill's video player passes the host ticks to
  `AVPlayerItemVideoOutput::itemTimeForMachAbsoluteTime` and paces from the
  refresh period: a player that sleeps between pictures draws at its own
  rate, so draw spacing alone would mistake that rate for the display's.
  The time is per display (the link is shared by the windows on it), not
  per window.

Picture-in-Picture (`docs/decisions/video-pip.md`):

- `src/window.rs`: a `WindowKind::Floating` window gets the same
  always-active `NSTrackingArea` (`add_mouse_tracking_area`) and
  `acceptsMouseMovedEvents = NO` as a normal window. Upstream gave floating
  windows only `acceptsMouseMovedEvents = YES`, which delivers mouse moves
  to the key window alone, so a panel opened without focus never saw
  hover, cursor or mouse-exit events.

The Quill project also changed it in 2026 to give back idle window memory
(`docs/decisions/idle-drawables.md`):

- `src/window.rs`: when `step` stops the display link for idling, it
  schedules `MetalRenderer::release_idle_drawables` 3 s later on the main
  queue (`DRAWABLE_RELEASE_AFTER`); `FrameIdle::idle_stops` skips the
  release if the window drew in between. An occluded window releases at
  once.

The Quill project also changed it in 2026 for Agent access's screenshots
(`docs/decisions/agent-access.md`, phase 5 step 3):

- `src/window.rs`: `MacWindow::capture_image` renders the scene through
  `MetalRenderer::capture_scene`. No screen-recording permission is
  involved: the window renders its own frame.

The Quill project also changed it in 2026 so a failed Metal setup costs one
window, not the app:

- `src/window.rs`: `MacWindow::open` returns `Result` and builds the renderer
  before any AppKit object, so a renderer error leaves no half-made window.
  gpui-pre-apple's `new_renderer` no longer takes the window and view.
  `src/platform.rs` passes the error on from `open_window`.

The Quill project also changed it in 2026 so a main window hidden by its
close button comes back the way Telegram Desktop's does:

- `src/platform.rs`: `should_handle_reopen` runs the reopen callback with
  windows showing too (upstream skips it then). A call or
  Picture-in-Picture window counts as showing, and the Dock click must still
  bring back a hidden main window.
- `src/platform.rs`: `applicationDidBecomeActive:` reaches
  `Platform::on_app_lifecycle` as `AppLifecyclePhase::Active`, deferred to
  the main queue. The other phases stay unused on macOS.
- `src/window.rs`: public `is_exiting_fullscreen`, so Quill can wait out the
  animation before it hides a full screen window.

The Quill project also changed it in 2026 so a Dock Quit or a log out can be
refused, as Telegram Desktop refuses them through Qt while an export runs:

- `src/platform.rs`: `applicationShouldTerminate:` asks the new
  `Platform::on_quit_request` callback and answers `NSTerminateCancel` when it
  refuses. Upstream has no such method, so AppKit terminated at once. When the
  app cannot be asked (it is borrowed, or the callback is running further up
  the stack), it answers `NSTerminateLater` and asks again from the main
  queue, then calls `replyToApplicationShouldTerminate:` with the answer.
  `Platform::quit` sets `quit_decided`, so the app's own quit, `restart`
  included, reaches `terminate:` without asking.
