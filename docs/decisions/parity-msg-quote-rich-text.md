# Quote formatted text, and text from rich messages

README item `parity:msg-quote-rich-text`.

## What TDLib 1.8.69 changed

`textQuote` and `inputTextQuote` gained `rich_texts:vector<RichText>`
(`schema/td_api.tl:3477`, `:3488`). A quote chosen from a rich message sends
one `RichText` per quoted text, in order of appearance. Only the first and
last may be partial, and math is quoted whole. Its `position` counts in the
plain texts of all quotable texts joined by line feeds. TDLib then ignores
`text`, and it sends such a quote back with `text` empty
(`MessageQuote.cpp:76-96`, `:215-231`). A quote from an ordinary message sends
the empty array.

What a quote keeps:

- `text` (ordinary messages): Bold, Italic, Underline, Strikethrough,
  Spoiler, CustomEmoji and DateTime entities (`td_api.tl:3481`, TDLib
  `is_allowed_quote_entity_type`). TDLib strips the rest.
- `rich_texts`: plain text, bold, italic, underline, strikethrough, spoiler,
  subscript, superscript, date-time, custom emoji, math and `richTexts`
  (`td_api.tl:3474`). Links, code, mentions, marked text and the other
  wrappers keep their text and lose the wrapper
  (`RichText::get_quote_fragment`, which unwraps `Marked` with `Fixed` and
  `Url`). Quill does the same, so marked text is quoted plain on purpose.

The quotable texts of a message are TDLib's `for_each_quote_text`
(`WebPageBlock.cpp`): a paragraph or heading's text; a list item's blocks
without its label; a block quote's blocks, then its credit; a pull quote's
text and credit; captions with their credit; table caption and cells; a
details block's header and blocks; a math block as one expression. Buttons,
dividers, anchors and chat links have none.

## Before

Quill sent every quote with no entities and `rich_texts: []`. The first
version of this PR sent entities and a single `RichText` built from the
display blocks, which was wrong in several ways: it read only `text` of an
incoming rich quote (always empty), counted list labels and the `❝` marks
of block quotes, dropped credits, kept code and links in quotes, flattened
subscript, date-time, custom emoji and math, and fell back to an ordinary
quote of a rich message it could not model.

## What tdesktop does

`Data::FullReplyTo` carries the quote as `TextWithEntities` plus an optional
`richQuote` (`data/data_msg_id.h`). For a rich quote it sends no quote text
(`data/data_histories.cpp:84-120`). The composer's reply bar shows the quote
as plain message text with its formatting and no quote marks
(`history_view_compose_controls.cpp:579-586`). tdesktop builds a full model of
the rich page (`RichQuoteModel`, about 2,500 lines) so a selection can start
and end anywhere.

## What changed

- `src/rich/quote.rs` reads a received `richMessage` a second time, apart
  from the display blocks: `rich_message_quote_texts` walks the blocks in
  TDLib's `for_each_quote_text` order and keeps each quotable text as a
  `QuoteRichText`, a `RichText` reduced to what a quote may hold.
  `QuoteRichText::fragment` cuts it the way `get_quote_fragment` does
  (custom emoji and math only when wholly inside). An unknown block or text
  type makes the whole message unquotable, since every position after it
  would be off. Recursion stops at depth 64.
- `RichMessageContent.quote_texts` holds those texts. Instant View pages
  keep none.
- `QuoteSelection` (`src/composer/reply.rs`) carries the kept `entities`
  and `rich_texts` (one per quoted text; empty for an ordinary message).
  `SendReply.quote` and `ChatDraft.quote` use it, so a draft keeps both.
- `src/quote_source.rs` turns a message into quotable texts. The picker
  offers one part per sentence of each text. Quote & Reply finds the
  selection in the texts joined by line feeds; a selection over several
  texts becomes one `RichText` each, up to 32 texts and 1,024 characters.
  A part or selection that would cut custom emoji or math is not offered.
  A rich message Quill cannot read offers no parts at all, so the reply goes
  to the whole message; other messages without known formatting still fall
  back to their copy text.
- `src/text/quote.rs` slices entities and keeps only the allowed types;
  `src/telegram/requests/messages/quote.rs` sends only those.
- Incoming quotes and drafts read `rich_texts` when `text` is empty and
  join the texts with line feeds. A draft's rich quote goes back out
  unchanged; one with a text type Quill does not know is dropped rather than
  sent as an ordinary quote. Before, the echo of a rich draft parsed to an
  empty quote that never matched the composer's, so every keystroke
  re-saved the draft; the draft check now also compares quotes by text and
  position only (`src/connect/composer.rs`), so a different formatting
  shape in TDLib's echo cannot start that loop again.
- Date entities keep their `DateTimeFormattingType`
  (`src/text/date_time_format.rs`, `TextEntityKind::DateTime.format`) and a
  quote sends it back; null only when the date had no format, which is how
  TDLib itself reports it (`FormattedDate::get_date_time_formatting_type_object`).
- Photo, video, animation, document, audio and voice-note captions quote
  with their formatting, like message text.
- The composer's reply bar shows every quote with its formatting and custom
  emoji and without `❝❞`, as tdesktop does. The picker marks the chosen part
  by text and position.

## What works

Quoting from rich messages made of paragraphs, headings, lists, block and
pull quotes, captions, tables, details blocks and math, with positions and
`rich_texts` as TDLib counts them, including selections across texts.
Formatting of ordinary quotes is kept within the allowed types.

## Gaps

- Quote & Reply gets the selected string only, so when the same text
  appears twice the first occurrence wins.
- A selection that spans display-only text (list labels, `❝` marks drawn on
  block quotes) does not match the quotable texts and replies to the whole
  message.
- Subscript and superscript are kept in `rich_texts` but drawn plain in the
  reply strip and bar; math shows its source.
- Memory: each rich message keeps a second copy of its text as
  `QuoteRichText`s. Not measured; rich messages are rare next to text ones.

## How verified

- Tests that failed on the first version and pass now: incoming rich quote
  with empty `text` (`src/state/tests/bubble_headers.rs`), draft round trip
  with empty `text` (`src/telegram/envelope/tests_chats.rs`), labelled list
  items and block-quote credit, entity filtering, no copy-text fallback for
  rich messages (`src/quote_source.rs`), entity filtering in the request
  (`src/telegram/requests/tests_groups.rs`).
- Draft echo with a rich quote no longer re-saves
  (`src/connect/tests/drafts_polls.rs`); date format and caption tests.
- Fragment, round trip and block-order tests in `src/rich/quote.rs`.
- `ready-reply` screenshot demo in light and dark: bold, italic and
  strikethrough in the bubble's reply strip and the composer's reply bar.
- Not checked: a real account, since TDLib never ran.
