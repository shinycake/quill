# Composer bars, placeholder and "Cancel editing?"

Three gaps from the composer audit (group B): C1, C2 and C5.

## What tdesktop does

`history/view/controls/history_view_compose_controls.cpp`:

- C1, `maybeCancelEditMessage` (5879): Escape in an edit asks "Cancel
  editing?" (Yes / No) when the text changed, otherwise cancels at once.
  The change test is `EditTextChanged` (`chat_helpers/message_field.cpp:404`),
  which compares prepared (trimmed) text with entities. Only Escape goes
  through it (`handleCancelRequest`, ~5992). The header's X button calls
  `cancelEditMessage` directly, with no question. The audit said the X asks
  too; the source says it does not, so Quill matches the source.
- C2, `updateFieldPlaceholder` (3239), first match wins: inline bot's
  placeholder, "New message..." while editing, "Message for N Stars", forum
  topic ("Reply in / Message in {topic}"), "Silent broadcast..." /
  "Broadcast a message..." in channels, "Send anonymously..." for anonymous
  admins, else "Write a message...".
- C5, `FieldHeader` clicks (~545-587): a left click on an edit bar jumps to
  the edited message; on a reply bar it opens the reply options box;
  Ctrl+click or right click on a reply bar jumps to the replied message.
  Right click on an edit bar opens the media edit menu.

## What Quill does now

- `quill::composer::edit_text_changed` (trimmed markup compare) decides.
  `QuillApp::request_cancel_edit` is what Escape (`esc_stack.rs`) runs; it
  opens `DialogKind::CancelEditConfirm` (kit dialog, flag
  `composer_ui.cancel_edit_confirm`) or cancels. The X still cancels.
- `quill::composer::ComposerPlaceholder::for_facts` is a pure function of
  `PlaceholderFacts` (editing, Stars per message, broadcast, silent).
  `sync_composer_placeholder` (frame upkeep) sets the field's placeholder when
  the text it should show changes. `Session::paid_send_price` gives the Stars
  price of one chat. Silent follows the composer's silent toggle, which is
  what tdesktop's bell writes (`silentPosts`).
- `quill::composer::bar_click_action` maps a click to jump, quote picker or
  nothing. The icon and text of both bars are the click target; the options,
  replace and X buttons are not. Left click on an edit bar jumps to the
  message; left click on a reply bar opens Quill's quote picker (its
  equivalent of the options box, offered when the message has text);
  Ctrl/Cmd+click or right click on a reply jumps when the reply is in the
  open chat.

## Not done, and why

- Inline bot placeholder: tdesktop draws it after the typed `@bot ` through a
  placeholder shift; the kit field hides its placeholder once it holds text,
  and the bot type does not keep `inline_query_placeholder`.
- Anonymous admin: Quill does not keep the own `is_anonymous` admin right.
- Forum topic: tdesktop shows it only when replying to a topic message from
  outside the topic; Quill's composer exists only inside a topic, where
  tdesktop shows the plain placeholder too.
- Right click on the edit bar: tdesktop's media edit menu. Quill keeps those
  tools as buttons on the bar.
- Chat windows keep their own composer and the default placeholder.

## How it was verified

Unit tests for the three pure functions; GPUI tests over the
`ready-composer-bars` demo (`bars_tests.rs`) for placeholder changes per
chat, the confirm flow, the X, and the bar handlers. Captures in light and
dark of reply, edit, confirm, Stars, broadcast and silent broadcast, plus a
scripted real click on the reply bar opening the quote picker.
