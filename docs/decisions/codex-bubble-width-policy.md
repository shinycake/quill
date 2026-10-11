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
only), link preview, game and invoice cards (widen the text bubble).

## Not done

- Monospace blocks widening a text bubble past 430 (`monospaceMaxWidth`).
- The link preview's large photo at the bubble's inner width
  (`Photo` attached to a `WebPage`), and its small thumbnail's
  `ArticleThumbWidth`.
- Calls as a bubble card (`historyCallWidth`); Quill shows a service-like
  row.
- Documents and music in the plain look keep the padded bounds (the
  renderers don't receive the look; 26 px).
- Inline video decode size (`InlineTile::media`) uses the uncaptioned
  frame; a caption-widened clip is drawn slightly upscaled from its
  decoded frames.
- Caption measuring ignores entity styling (bold runs are measured at the
  normal weight) and custom emoji (measured as their text).

## Verified

- Unit tests: `bubble_width::tests` (row widths less the chrome, the 377
  voice row, waveform bar count, pitch, heights, merging, short and empty
  waveforms), `layout::tests` (a caption widening a picture to 402 and
  cropping, to 362 and keeping the height, the 430 cap, the GIF 320 cap).
- Captures of `ready-voice`, `ready-audio`, `ready-media`, `ready-albums`,
  `ready-link-preview`, `ready-reactions`, `ready-downloads`, `ready-video`
  and `ready-showcase` before and after, in a scratch directory (not
  committed), compared side by side.
