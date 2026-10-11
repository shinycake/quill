# Changes made by the Quill project

This directory is gpui-pre-macos 0.3.8 from crates.io (Apache-2.0,
`LICENSE-APACHE`), GPUI's macOS platform, used through `[patch.crates-io]` in
Quill's `Cargo.toml`. Commit `126460ec` added the unmodified registry copy
(without the registry's `.cargo-ok`, `Cargo.toml.orig` and `Cargo.lock`), so
`git diff 126460ec -- third_party/gpui-pre-macos` shows every change. (The
patch was first made on 0.3.7 and carried to 0.3.8 in
`docs/decisions/codex-gpui-kit-0.7.1.md`; `step` keeps 0.3.8's frame
signal timestamps.)

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

The Quill project also changed it in 2026 so an OS notification can carry
the sender's picture and an inline reply field, which GPUI's
`SystemNotification` (in the unvendored `gpui` crate) has no fields for:

- `src/notification_extras.rs` (new, public, identical to
  `third_party/gpui-pre-windows/src/notification_extras.rs`; unit tests run
  with `scripts/test-notification-extras.sh`): `NotificationExtras { image,
  text_input }` registered under the notification's tag with
  `set_notification_extras` before `App::show_system_notification`, and
  `take_reply_text(tag)` for the text typed into the field, parked by the
  backend before the response is sent.
- `src/system_notifications.rs`: `show` takes the extras for the tag; the
  action named in `text_input` becomes a `UNTextInputNotificationAction`
  (categories are keyed by actions plus text input); the image is copied to
  a temporary file and attached as a `UNNotificationAttachment` (the center
  moves attached files into its own store). The delegate reads
  `UNTextInputNotificationResponse.userText` into the side channel.
- `src/gpui_macos.rs`: declares the new module.

Each changed source file starts with a comment saying so. The changes are
licensed under Apache-2.0, like the rest of the crate. Background and
measurements: `docs/decisions/codex-idle-cpu.md`; notifications:
`docs/decisions/codex-notify-native.md`.
