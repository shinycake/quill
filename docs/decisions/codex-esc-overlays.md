# Escape closes every hand-drawn overlay

## What tdesktop does
Escape always dismisses exactly one thing: the topmost box, layer or panel
(`Window::Controller::hideLayer`, `LayerStackWidget`, `HistoryWidget::escape`).
It never unlocks the app: the passcode box (`PasscodeBox` / `Window::Main`
lock screen) has no Escape path.

## What changed
The Escape chain lived in `cancel_search` as ~180 lines of `if` branches, and
several overlays were never added to it. It is now one ordered table,
`ESC_LAYERS` in `src/ui/esc_stack.rs` (topmost first, each row an
`is_open` / `close` pair). `cancel_search` calls `dismiss_topmost_layer`,
which closes the first open row and nothing else. A new overlay is one row.

New rows (previously close-button / backdrop only): privacy overlay
(peels picker, exceptions, editor, then main, via `close_privacy_top`),
chat story page, voice-chat title box, photo editor, link popup, Instant
View, archive context menu, downloads panel, info panel. Existing branches
moved onto the table unchanged, in the same order.

- Lock screen: `dismiss_topmost_layer` returns early while
  `passcode_ui.locked`, so Escape neither closes layers hidden under the lock
  nor unlocks. The passcode set/change dialog is a kit dialog and still closes
  on Escape.
- Kit dialogs (proxy list/editor/link, group confirm, group-call start,
  marketplace, passcode settings, ...) already close through the kit's own
  Escape handling and `on_close`; the tests prove it for proxy list,
  marketplace and passcode settings. They are not table rows.
- Not added: the call overlay and group-call panel (Escape must not hang up).

## Verification
`cargo test --features demo-capture --bin quill esc_stack`: real Escape
keystrokes through the GPUI dispatcher into a demo `QuillApp` close each new
overlay, close only the topmost of a stack (link popup, story page, privacy),
leave everything open and locked on the lock screen, and close three kit
dialogs. Table tests (unique names, required rows) run in the normal gate.
