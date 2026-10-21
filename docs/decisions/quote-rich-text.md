# quote-rich-text: quote formatted text from rich messages (`parity:msg-quote-rich-text`)

## tdesktop
`api.tl` gives `messageReplyHeader` and `inputReplyToMessage` a `quote_rich:Vector<RichText>` field.

- Receive (`history_item_components.cpp`): a header with `quote_rich` becomes `richQuote`
  (`Iv::ParseRichQuote`); the flat `quote_text` fields stay empty, because "the server rejects a
  flat copy of a rich quote". The quote shown in the reply header is the quoted texts joined by
  line feeds, filtered to bold, italic, underline, strikethrough, spoiler, custom emoji and dates.
- Send (`history_item_helpers.cpp`, `NewMessageReplyHeader`): a rich quote goes out only as
  `quote_rich`, with the offset, and no quote text or entities.
- Cutting the quote from a rich page is the work of `iv/iv_rich_page_quote*.cpp`, which maps a
  selection in the rendered article back to the page's texts.

## TDLib 1.8.69
`textQuote` and `inputTextQuote` carry `rich_texts`. The quotable texts of a message are defined by
`WebPageBlock::for_each_quote_text` (title, paragraphs, headings, captions with credits, table
cells, details headers and so on, in order; buttons, anchors and dividers are skipped). The
position is the UTF-16 offset in those texts joined by line feeds. An input quote needs one
element per quoted text, the first and last possibly partial, formatting kept, math whole, at most
32 texts. `RichText::get_quote_fragment` shows which formatting a quote keeps: plain, bold, italic,
underline, strikethrough, spoiler, sub- and superscript, date and time, custom emoji and math.
Everything else (links, code, mentions...) reduces to its text.

## What changed in Quill
- `src/rich/quote.rs` (new, pure): collects the quotable `RichText` trees of a `richMessage` from the
  raw `pageBlock*` JSON, builds the joined quote text, cuts `rich_texts` for a span (the port of
  `get_quote_text` and `get_quote_fragment`, with a depth limit), and turns received `rich_texts`
  into text plus entities. The flattened `RichBlock`s lose this structure, so
  `RichMessageContent::quote_texts` keeps the trees next to them.
- Send: `QuoteSelection::rich_texts` is filled when the quote is picked from a rich message
  (`QuoteSelection::in_content`); `SendReply::quote_rich` carries it and `send_reply_value` writes it
  into `inputTextQuote.rich_texts`. The picker, "Quote & Reply" from a selection and draft restore
  all quote from the joined quotable texts for a rich message, so the position matches TDLib's.
  Drafts store the quote text and position as before and re-derive `rich_texts` on restore.
- Receive: `textQuote.rich_texts` is parsed in `parse_reply_quote` into `quote_text` and
  `quote_entities`; the reducer's `ReplyHeader` carries the entities and the strip draws them
  (`src/ui/reply_quote_text.rs`, a `StyledText` with highlights). Unknown rich kinds fall back to
  their plain text; a quote without `rich_texts` is unchanged.

## Where Quill differs
- tdesktop picks the span from a selection in the rendered article. Quill's rich messages render
  through its own block renderer, so a selection is mapped by searching the selected text in the
  joined quotable texts. A selection that does not match verbatim (for example text Quill adds,
  such as a list marker) falls back to replying to the whole message, as for plain messages.
- A span that cannot be cut into at most 32 whole elements is sent as the plain quote text, which
  TDLib then validates against the message as it does for any text.
- The strip shows a spoiler as a solid block that never reveals; it is one line, as for plain
  quotes. Custom emoji and dates in a quote show their fallback text.

## How verified
Unit tests for the quote module (collection order, partial edges, UTF-16 positions, unsupported
kinds, math, limits, depth), reducer tests (`src/state/tests/quote_rich.rs`) for parse, fallback,
send JSON and plain-message quotes, a UI test over the `ready-quote-rich` demo
(`src/ui/quote_rich_tests.rs`), and light and dark captures of that demo.
