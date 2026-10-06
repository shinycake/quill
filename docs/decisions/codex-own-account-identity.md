# Own account identity, a spellcheck crash, drafts with formatting

## Problems (seen live, compared with Telegram Desktop)
- **Quill never knew who the user was.** `my_user_id` was set only from a
  `getMe` answer, and the live app never sends `getMe`. A Premium account
  was treated as non-Premium: custom emoji were dimmed and refused with
  "Custom emoji need Telegram Premium". Every other "is this me" check
  (own chat membership) was inert too.
- **Typing could crash the app.** The spellchecker skips URLs byte by
  byte until `(byte as char).is_whitespace()`. The tail byte `0xA0` of
  emoji such as 🔠 reads as U+00A0 (no-break space), so the skip stopped
  mid-character and the next slice panicked inside a GPUI input handler,
  which aborts the process. Inserting a custom emoji (`tg://emoji?id=…`
  counts as a URL) after another one triggered it every time.
- **Drafts synced as raw markup.** `setChatDraftMessage` sent the composer
  text with no entities, so other clients showed
  `![🔠](tg://emoji?id=…)`, `**bold**`, … literally. Incoming drafts
  dropped their entities, losing formatting on the way back.
- Messages of only custom emoji never got the big-emoji size.

## Decision
- The own id comes from TDLib's `my_id` option. Premium state comes from
  the `is_premium` option, falling back to the own user record.
- The URL skip stops only at ASCII whitespace, the only single-byte
  whitespace in UTF-8.
- Outgoing drafts parse markup into entities, as sending does. Incoming
  drafts turn entities back into markup (`composer::entities_to_markup`).
  The markup cannot nest, so overlapping entities keep custom emoji first,
  then links, then the longer span.
- `big_emoji_count_with_entities` counts each custom-emoji span as one
  emoji; any other entity still rules big emoji out.
