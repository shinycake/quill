# Inline bot results: media grid, button row, paging

Checklist item `parity:composer-inline-grid`.

## What tdesktop does

`inline_bots/inline_results_inner.cpp`, `layout/layout_mosaic.cpp` and
`inline_bots/inline_bot_layout_internal.cpp`:

- Photos and GIFs are packed into justified rows by aspect ratio. A cell
  wants `width * 96 / height` (at least 48). A row closes when it holds five
  cells or wants at least the panel width; a closed row is scaled so its
  cells fill the width (narrowest first, 3px between cells), the last row
  keeps natural widths. Stickers are fixed 64px squares packed the same way.
- Articles, contacts, locations, venues, games and videos are list rows
  with a 64px picture; audio, voice notes and documents are list rows with a
  44px square. A list row wants the full panel width, so it closes its row
  and takes it alone.
- The `inlineQueryResultsButton` (start parameter or web app url) is a
  button above the results. It is shown for any non-empty text, also with no
  results. A start button remembers the current chat (`inlineReturnTo`), opens the
  bot's chat and sends the start with the token at once
  (`sendBotStartCommand`); when the bot answers with a switch-inline
  button (or the user presses one) tdesktop goes back to the remembered
  chat and types `@bot query` there; a web
  app button opens the bot's web view (`attachWebView().open`).
- Paging: `Widget::onScroll` asks for the next `next_offset` page when the
  scroll position is within one viewport of the end, which also fires when
  the first page is shorter than the panel. A page that adds nothing clears
  the offset. A new query scrolls back to the top.
- Choosing a result calls `sendInlineQueryResultMessage` through the
  result's own query id.
- There is no keyboard navigation in the panel itself in tdesktop.

## What Quill had

A text list (title and description) above the composer, a "More results"
row, no pictures, no button. Thumbnails were dropped at parse time.

## What changed

- `telegram/envelope/inline_queries.rs`: each result keeps its image
  (photo `m` size, animation, sticker, video, article, contact, location,
  venue and document thumbnails) with the size the layout needs, its own
  `query_id`, and text built from contact, venue, audio and game fields. The
  image files go through `remember_files` and the existing thumbnail
  download pass (`session_files.rs`).
- Bug fix on the way: `inline_query_id` is an int64, which TDLib writes as a
  JSON string; the parser read it with `as_i64`, so real answers produced
  query id 0 and picks could not be sent. It now uses the shared `int64`
  helper.
- `state/session_apply_queries.rs`: a next page keeps the button when it
  sends none, and a page that adds nothing ends the list (tdesktop).
- `inline_grid.rs` (new, pure): the packing port, result classification,
  button routing (`start_bot` and `web_app`), the one-viewport paging rule,
  scroll-to-reveal and selection stepping.
- `ui/inline_grid.rs` (new): draws the panel (fixed 380px, scrolls past
  360px), the button row, grid cells and list rows; loads more on scroll and
  after each page; routes the button. A start button reuses the
  same open-the-bot-by-username flow as links, but the start is sent
  right away (`sendBotStartMessage` through `press_bot_start`) and
  `ui/inline_return.rs` remembers the origin chat, then returns with
  `@bot query` typed when a bot message carries a switch-inline button or
  such a button is pressed. A web app button opens the bot's mini app for the open chat
  (`web_app_ui/inline_results_button.rs`).
- `ui/inline_mode.rs`: the dropdown builds an `InlineView`, picks and Up/Down
  walk the cells in display order, the "More results" row is gone, a new
  query resets the scroll and highlight.

## Differences from tdesktop

- Keyboard: Quill already had Up/Down/Enter in this panel; kept (it follows
  the grid's display order, wrapping at the ends).
- The mini app opens as an inline keyboard app, so the first open of a bot
  still asks for its terms.
- GIF cells autoplay muted and looped through the history's native video
  player (`inline_video` slots under a reserved chat key, frame clock at
  30 fps, only MP4 GIFs, small clips, honours autoplay and data saver).
  Quill has no GIF hover preview or send-with-options menu elsewhere (the
  GIF picker has neither), so the grid has none either.
- Rows are laid out for the fixed panel width, not a measured one.

## Verification

- Unit tests: packing (aspect widths, five-cell and wide-row closing,
  justified fill, natural last row, list rows, mixed rows like tdesktop,
  skipped results), button routing, paging rule, reveal maths, parsing of
  every result kind and the int64 query id, state tests for per-page query
  ids, button carry-over, end-of-list and the download queue.
- A temporary demo capture (not committed) showed the grid with the button,
  highlight, and list rows with pictures, letter placeholders and scrolling.
- Not verified: playback in the panel, the start and return round trip, a real bot (no network use in development), the web app
  button end to end, scroll-triggered paging against a live account.
