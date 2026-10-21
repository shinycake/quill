# Quote formatted text, and text from rich messages

README item `parity:msg-quote-rich-text`.

## What TDLib 1.8.69 changed

`textQuote` and `inputTextQuote` gained `rich_texts:vector<RichText>`
(`schema/td_api.tl:3477`, `:3488`). A quote chosen from a rich message sends
one `RichText` per quoted text, in order of appearance, and its `position`
counts in the plain texts of all quotable texts joined by line feeds. A quote
from an ordinary message sends the empty array. `text` is a `formattedText`
in both cases.

## Before

Quill sent every quote as `formattedText` with an empty `entities` array and
`rich_texts: []`. Quoting bold or code text from a message lost the bold and
the code, and a quote from a rich message went out as if it were an ordinary
one. An incoming quote kept its text only, and drafts dropped the formatting
on a round trip.

## What tdesktop does

`Data::FullReplyTo` carries the quote as `TextWithEntities` plus an optional
`richQuote` (`data/data_msg_id.h`). The quote text keeps its entities in the
reply strip and in the composer's reply bar, and a quote taken from a rich
message sends `quote_rich` next to the offset (`iv/iv_rich_page_quote.*`).
tdesktop builds a full model of the rich page (`RichQuoteModel`, about 2,500
lines) so a selection can start and end anywhere.

## What changed

- `QuoteSelection` (`src/composer/reply.rs`) carries `entities` and
  `from_rich`. `SendReply.quote` and `ChatDraft.quote` use it instead of a
  `(text, position)` pair, so a draft keeps the formatting too.
- `src/telegram/requests/messages/quote.rs` builds `inputTextQuote`: the quoted
  text with its entities at UTF-16 offsets, and for a rich quote one
  `RichText` built from the same entities (plain, bold, italic, underline,
  strikethrough, spoiler: the only styles the schema allows in a quote).
- `src/quote_source.rs` turns a message into quotable texts. An ordinary text
  message is one text. A rich message is its paragraphs, headings and list
  items, with the position counted in them joined by line feeds. The picker
  and Quote and Reply both cut parts from it, so the part keeps the entities
  inside it (`src/text/quote.rs` slices and rebases them).
- An incoming `textQuote` keeps its entities (`MessageReplyTo.quote_entities`,
  `ReplyHeader.entities`). The reply strip in the bubble and the composer's
  reply bar draw them with the chat list's one-line styled preview.

## Limits

- A rich message with any block Quill does not turn into text nodes (tables,
  collapsibles, document or media captions, unsupported blocks) cannot be
  quoted as rich: its position would be off. It falls back to the plain copy
  text as before.
- A quote covers one text of a rich message. tdesktop can start in one block
  and end in another.
- Custom emoji, date-time and math in a rich quote are not sent as
  `richTextCustomEmoji`, `richTextDateTime` or math; they stay plain text.
- Custom emoji inside a quote show as their text in the strip, not as stickers.

## How verified

- Unit tests: request JSON with entities and with `rich_texts`
  (`src/telegram/requests/tests_groups.rs`), parsed quote and draft keep their
  entities (`src/state/tests/bubble_headers.rs`,
  `src/telegram/envelope/tests_chats.rs`), part and position math
  (`src/quote_source.rs`, `src/text/quote.rs`).
- `ready-reply` screenshot demo in light and dark: the quote in the bubble's
  reply strip and in the composer's reply bar shows bold, italic and code.
- Not checked: a real account, since TDLib never ran.
