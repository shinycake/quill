# Bubble width: one policy per kind, after Telegram Desktop

Slice `codex/bubble-layout` (second pass; the first,
`codex-bubble-layout.md`, ported the numbers into `src/bubble_layout.rs`).
The owner's report: "the bubble width/layout is still wrong sometimes, e.g.
I would expect the audio wave to fill the width of the bubble", with the
clarification "see what Telegram does". So each kind was checked against
Telegram Desktop's source and made to follow it.

## How Telegram Desktop decides a bubble's width

`history/view/history_view_message.cpp`:

- `performCountOptimalSize`: `maxWidth` starts at the text's longest line
  plus `msgPadding` (`textualMaxWidth`), then takes the widest of the
  sender name, "via", forward line, reply, the reactions row (capped at
  `msgMaxWidth`), the inline keyboard, and `minWidthForMedia` (the time
  pill plus `2 * (msgDateImgDelta + msgDateImgPadding.x)`). With media,
  `media->maxWidth()` joins the maximum, or replaces it when the media
  "enforces" the bubble width (photos, GIFs/videos, album grids).
- `countGeometry` / `resizeContentGetHeight`: the bubble is
  `min(available, maxWidth, bubbleWidthLimit)`, where the limit is
  `msgMaxWidth` or a wider monospace block. The media is then laid out at
  that width; if it comes out narrower than the bubble and narrower than
  the text, the bubble shrinks to the text, otherwise to the media. A
  text-only bubble shrinks to its real text width plus padding, never
  under the non-text parts.

Per media kind (`history/view/media/*.cpp`, `ui/chat/chat.style`,
`lib_ui/ui/basic.style`), at 100 % scale:

| Kind | min width | max width | text vs media | footer (time) | waveform / progress |
| --- | --- | --- | --- | --- | --- |
| Text | `msgMinWidth` 160 lays text out; bubble shrinks to text | `msgMaxWidth` 430 (monospace may pass) | name/reply/forward/keyboard only widen | last line keeps `skipBlockWidth` = 12 + info − 2 | none |
| Photo | 200 (`historyPhotoBubbleMinWidth`), 100 bare | 430 (`maxMediaSize` box) | `enforceBubbleWidth`: picture decides; widened to `min(430, caption + padding)`, cropped/kept per `adjustHeightForLessCrop` | pill over the picture; `minWidthForMedia` = info + 24 floors the width | none |
| Video / GIF | 200 / 100 | 430 video, 320 GIF (`maxGifSize`) | as photo; status label (duration/size) + 24 also floors | pill | none |
| Round video | 240 (`maxVideoMessageSize`) | 240 | bare, no bubble | on the background | none |
| Voice note | `msgFileMinWidth` 268 | not capped by name (none) | caption/transcript wraps inside; the row stays at its own width (`enforceBubbleWidth` false) | `tleft + status + unread 12 + skipBlock + 11` floors | **row = 100 bars × (2 + 1) + padding 22 + disc 44 + skip 11 = 377**; `PaintWaveform` fills `width − 67 − 10` |
| Music / file | 268 | 430 (name capped) | name widens between the two | status line + skip block floors | none |
| Sticker | `maxStickerSize` 224 box, never upscaled | 224 | bare | background | none |
| Album | `historyGroupWidthMin` 100 | `historyGroupWidthMax` 430 | grid enforces the bubble | pill | none |
| Poll | 268 | bubble's 430 | question / answers / buttons widen | inline | none |
| Contact | `skipBlockWidth` only | 430 | name, phone, buttons widen | inline | none |
| Location | `minPhotoSize` 100 and info + 24 | tile `locationSize` 320×240, ≤ 430 | title/description wrap under the tile | pill | none |
| Link preview | `skipBlockWidth` | 430 | site/title/description/attach widen the text bubble | inline | none |
| Game / invoice | `skipBlockWidth` | 430 | title/description/attach widen | inline | none |
| Call | `historyCallWidth` | same | fixed card | inline | none |

## What Quill does now

`src/ui/history/bubble_width.rs` is the one place the renderers ask:

- `content_for_outer(outer, plain)`: Telegram Desktop's widths are bubble
  widths; Quill's padded bubble adds 12 + 12 + 2 (`PADDED_CHROME`), so a
  row inside takes that much less. The plain look takes none off.
- `voice_row_width`: `file_max_width` for a voice note (377 at the
  default status/time), so the row is fixed, and the new `waveform_canvas`
  (`src/ui/message_media/audio.rs`) lays `waveform_bars` out at paint time
  over whatever width the row gives: 2 px bars 1 px apart, 3 to 17 px
  tall, samples merged on the `samples × bars` grid taking each cell's
  peak, the same walk `PaintWaveform` makes. One deviation: fewer samples than bars
  are repeated so a short waveform still fills the row (Telegram Desktop
  would draw them at the left; real notes always carry 100 samples, the
  demo fixtures carry 12). An unplayed incoming note shows the whole
  waveform in the active color, as Telegram Desktop does.
- `file_row_bounds`: music and document rows keep the 268..430 bubble
  range (242..404 inside the padding; they were 268..430 inside it, so
  26 px too wide at the floor).
- `poll_min_width`: polls get the 268 floor they lacked.
- Contact: the 268 floor is gone; Telegram Desktop has none.
- `caption_width` / `longest_line_width`: the caption's longest line,
  measured as the sum of glyph advances (`TextSystem::layout_width`, the
  same per-character widths GPUI's line wrapper adds up, cached per
  font/size/character and bounded), plus the footer's skip block on the
  last line. `media_frame_for` feeds it to `photo_current` / `clip_current`
  as `caption_width`, so a picture widens to a wide caption up to
  `msgMaxWidth` and keeps or crops its height per
  `adjustHeightForLessCrop`, as Telegram Desktop. Before, the caption
  wrapped to the picture.
- Location tiles are 320×240 (`locationSize`), and the map request asks
  for that box (`MAP_THUMB_HEIGHT` 240, was 180 drawn at 256×144).

Unchanged because they already matched: text bubbles (flex shrink-wrap,
430 cap), albums (`media_width` from the grid), stickers, round videos,
reactions (wrap at the bubble, widen it up to 430), replies/forwards (widen
only), game and invoice cards (widen the text bubble).

## Follow-ups (`codex/bubble-layout-followups`)

The six items the first pass left open, each checked against Telegram
Desktop again:

- Monospace. `Message::_bubbleWidthLimit` is the widest of `msgMaxWidth`
  and `monospaceMaxWidth()` (the text's `countMaxMonospaceWidth`: the
  widest `pre` block, its quote padding included, plus `msgPadding`).
  `text_measure::widest_pre_block` measures each `pre` run's longest line
  in the monospace face, with the language label at the size
  `pre_block` draws it and the block's own chrome (`px_2` twice and the
  copy button's `pr_6`, 40 px, which happens to equal lib_ui's
  `quoteMinWidth` of 40 for `historyTextStyle.pre`).
  `bubble_width::bubble_width_limit` turns that into the bubble's outer
  limit (`MessageChrome::max_width`, read by `synthetic.rs` in place of
  the fixed 430). Inline `code` doesn't count, as in lib_ui. The kit
  bubble's share of the pane still caps it, so on a narrow pane the code
  wraps as before. Deviation: Quill's padded bubble is 26 px of chrome
  where Telegram Desktop's `msgPadding` is 22, so the limit is 4 px wider
  than Telegram Desktop's for the same code; it is what keeps the line
  unwrapped in Quill's box.
- Link preview pictures (`WebPage::countOptimalSize` /
  `countCurrentSize`). The large photo is a `Photo` attach laid out at the
  card's inner width: `preview_photo_frame` takes the photo's optimal
  width (`photo_optimal`, at least 200, 430 for a landscape picture),
  capped at the card's inside at the widest bubble (430 − 26 − 18 = 386),
  and its frame there; the card draws it at least that wide and fills a
  wider card at the same proportions (`ThumbFrame::Fill`, a box with the
  ratio and the picture covering it). It was a fixed 240×140. The small
  article thumbnail follows `ArticleThumbWidth` (proportional in its box,
  never wider than tall) in a box that starts five lines tall and drops a
  line at a time while the copy beside it has fewer lines
  (`article_thumb`); the copy's lines are estimated by wrapping the
  measured site name, title (two lines at most) and description at what
  the thumbnail leaves of the widest card. Telegram Desktop's lines are
  all `UnitedLineHeight`; Quill's site and description lines are smaller
  than the title's, so the box is the real height of that many lines. It
  was a fixed 72×72: the demo's three-line copy now gets a 53×53 thumb and
  its description no longer wraps.
- Calls (`HistoryView::Call`). `messageCall` now renders as an ordinary
  bubble holding a card `historyCallWidth` (240) wide: the title in
  semibold (`Data::MediaCall::Text`: Incoming / Outgoing / Missed /
  Cancelled / Declined, "video" variants; a declined outgoing call reads
  "Outgoing call", as there), an arrow and the status line (the time,
  plus ", 6 min 12 s" / ", 45 seconds" when the call connected,
  `lng_call_duration_info` with `FormatDurationWords`), and the phone or
  camera icon in the corner, which calls back in a 1:1 chat. The arrow
  is the bundled up / down arrow turned 45° (the Lucide diagonal arrows
  aren't in the app's icon bundle, and `main.rs` is a no-change hotspot),
  red for missed and declined calls, green otherwise (on an outgoing
  bubble, the bubble's text color). The time footer sits inline at the
  bottom right like a text bubble's. The bubble brings the usual reply,
  react and menu actions with it. The service-like `call_message_row` is
  gone (`message_media/call.rs` has the card).
- The plain look. `document_chip`, `audio_row` and `contact_row` take the
  look, so `file_row_bounds(plain)` drops the 26 px padding allowance a
  bubble-less row doesn't have: 268..430 in the plain look, 242..404 in a
  padded bubble. Before, plain rows got the padded bounds, 26 px short of
  Telegram Desktop's widths.
- Inline video decoding. `InlineTile::media` takes the caption width and
  sizes the tile with `media_frame_for`, the frame the bubble draws, so a
  caption-widened clip is decoded for its wider frame (a 320×180 clip
  under a 380 px caption: 402 px at 1x instead of 320). `inline_frame`
  measures the caption as if it ended the bubble (skip block included),
  an upper bound of the drawn frame, so pictures are never decoded
  smaller than drawn. The 800 px `INLINE_MAX_EDGE` cap still applies at
  2x.
- Caption measuring (`text_measure`). Captions are measured run by run
  from `styled_runs`, each in the face it renders in: bold and italic in
  those faces, `code` and `pre` in the monospace font, a custom emoji as
  the em-space placeholder it occupies (`EMOJI_PLACEHOLDER`) rather than
  its fallback text, blocks with their chrome and on lines of their own.
  Captions now also resolve custom emoji stickers (they rendered the
  fallback text before), so the measured placeholder is what is drawn.
  The skip block joins the last line only when the caption ends the
  bubble (`caption_tail` / `caption_ends_bubble`): not when the caption
  is above the media, when reactions, an inline keyboard, a self-destruct
  or auto-delete badge or the replies bar follow, when views or a
  signature put the time on its own line, or when the last line is
  right-to-left. Before, it was added to every caption below the media.
  Measuring is cheap: glyph advances are cached per font, size and
  character (bounded at 16k entries), captions are measured only when
  present, and the monospace pass returns before parsing runs unless the
  text has a `pre` entity.

## Verified

- Unit tests: `bubble_width::tests` (row widths less the chrome, the 377
  voice row, waveform bar count, pitch, heights, merging, short and empty
  waveforms), `layout::tests` (a caption widening a picture to 402 and
  cropping, to 362 and keeping the height, the 430 cap, the GIF 320 cap).
- Captures of `ready-voice`, `ready-audio`, `ready-media`, `ready-albums`,
  `ready-link-preview`, `ready-reactions`, `ready-downloads`, `ready-video`
  and `ready-showcase` before and after, in a scratch directory (not
  committed), compared side by side.
- Follow-ups: `text_measure::tests` (bold and italic faces, a custom emoji
  at its placeholder, newlines and the skip block, block chrome and the
  time after a closing block, the widest `pre` block and its label),
  `bubble_width::tests` (plain file bounds, the monospace limit, when the
  skip block joins a caption, `ArticleThumbWidth`, the article box at
  three and five lines and for a portrait thumbnail, the large photo at
  386×259 and the 200 floor), `call::tests` (titles, duration words,
  status line), `inline_video::tests` (a caption-widened clip decodes at
  402). Captures before and after of `ready-text-entities` (with a
  temporary, uncommitted fixture adding a long `preCode` line and three
  call messages; at 1700 px wide the code line fits unwrapped),
  `ready-link-preview`, `ready-showcase` (the large preview photo at
  386×259), and `ready-video`, `ready-gifs`, `ready-audio`,
  `ready-downloads` (pixel-identical before and after), plus
  `ready-audio` and `ready-downloads` with the plain look forced locally.
