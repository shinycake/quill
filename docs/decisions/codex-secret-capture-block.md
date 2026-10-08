# codex/secret-capture-block — exclude the window from capture in secret chats

- While a secret chat is open, or the media viewer shows media of a secret chat, the main
  window's `NSWindow.sharingType` is set to `NSWindowSharingNone`; otherwise it is restored to
  `NSWindowSharingReadOnly` (the default). AppKit documents `.none` as "window content cannot be
  read by another process", so screenshots, screen recordings and screen sharing do not capture
  it. This is the macOS counterpart of Telegram mobile's silent screenshot block (Android
  FLAG_SECURE). Nothing is shown to the user; tdesktop has no secret chats, so there is no
  desktop hint to follow.
- The media viewer is an in-window overlay, so it is covered by the main-window setting; the
  viewer's item chat is checked too, so it stays blocked even if the open chat differs.
- `QuillApp::sync_capture_block` runs each render but only calls AppKit when the desired state
  differs from the cached `capture_blocked`. The decision is the pure `should_block` (unit-tested).
- Non-macOS builds compile to a no-op (X11/Wayland have no equivalent API).
- Not covered: view-once/self-destructing media in non-secret chats (viewer items carry no such
  flag); the PiP window (video only, opened from any chat).
- README is not edited by hand (CI forbids it); `parity-fragments/codex-secret-capture-block.txt`
  declares `parity:secret-screenshot-block` so the merge pipeline ticks it. Suggested README note:
  "(macOS: window excluded from capture via NSWindowSharingNone; Linux has no API)".
- Verification: gate.sh GATE OK; real capture behaviour not testable headlessly.
