# Share rich quote texts

Follow-up to #1071 (rich quotes).

tdesktop keeps the quotable text of a message once, on the history item.
Quill copied the parsed `quote_texts` Vec along with every clone of the
message content.

Changes:

- `RichMessageContent.quote_texts` is an `Arc<[QuoteRichText]>`; cloning
  a message no longer copies the texts.
- A streaming bot draft clears them (`stream_rich` in
  `src/ui/bot_stream.rs`): draft ids are negative, so a draft can never be
  quoted.
- `quote_segments` moved into the `reply_options` test module, its only user.
- The reply quote panel still rebuilds its fragments per frame; caching them
  would add a second copy of the quotable text, so it stays.

Verified by unit tests (`streaming_draft_carries_no_quote_texts` and the
existing quote tests), clippy with `demo-capture`, the gate and both check
scripts. No visible UI change.
