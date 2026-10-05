# codex/edit-last — Up edits your last message

- Pressing Up in a focused, empty composer starts editing your newest editable message, as in
  Telegram Desktop. It does nothing while an edit, attachments or the `/` command menu are
  active, so Up keeps its usual meaning there.
- The textarea binds Up to a cursor move, and GPUI matches bindings before `on_key_down`, so
  the keystroke is caught with `cx.intercept_keystrokes`. This follows the pattern used for
  shortcut capture. The interceptor only consumes the key when an edit actually starts.
- `Session::last_editable_message` picks the newest outgoing, sent (not pending or failed)
  message whose content `ComposerEdit::from_own_content` accepts. It returns `None` when the
  loaded window stops short of the latest messages, because its newest own message might not
  be the real last one. Covered by a driver test.
