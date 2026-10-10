# Message bubble layout: Telegram Desktop's sizing rules

Quill's bubbles were sized by GPUI's flex layout with a few local numbers (a 560 px text cap, a 360x400 media box, 128 px stickers, 220 px file rows). Telegram Desktop sizes every bubble kind from a small set of rules in `history/view/history_view_message.cpp` and the media views; this slice writes those rules down, ports the pure part to `src/bubble_layout.rs` (unit tested) and makes each bubble kind follow it. Sizes below are logical pixels at 100% (tdesktop `ui/chat/chat.style`).

## The common frame

- `msgMaxWidth` 430: the widest a bubble gets, padding included. `msgMinWidth` 160: narrower bubbles still lay text out at this width. `msgPadding` 11/8/11/8. `msgMargin` 16/6/56/2 around a bubble, `msgPhotoSkip` 40 for the avatar column.
- `countGeometry`: the bubble width is `min(available, maxWidth, bubbleWidthLimit)`, where `maxWidth` is the widest of the text (`textualMaxWidth` = text + padding) and the non-text parts (sender name, via, forward line, reply, topic button, keyboard, the time pill), and `bubbleWidthLimit` is `msgMaxWidth` or the widest monospace block when that is wider. A text-only bubble then shrinks to its real text width plus padding, never under the non-text parts. (`text_bubble_width`.)
- `minWidthForMedia`: a media bubble is at least the time pill's width plus `2 * (msgDateImgDelta + msgDateImgPadding.x)` = info + 24, or the inline keyboard's natural width. (`min_width_for_media`.)
- `skipBlockWidth`: a text line keeps `msgDateSpace + infoWidth - msgDateDelta.x` free for the inline time. (`skip_block_width`.)

## Text, and text with a link preview

The bubble follows the common frame. Quill: the body's cap went from 560 to `MSG_MAX_WIDTH` (430); GPUI shrink-wraps the text as `countGeometry` does. Not done: widening for a monospace block past 430 (`monospaceMaxWidth`), and the link preview's own widths (`WebPage::countOptimalSize`, which wants the title and description's unwrapped widths that Quill does not measure before layout).

## Photos

`Photo::countOptimalSize` / `countCurrentSize` (`photo_optimal`, `photo_current`):

- the picture is fitted into `maxMediaSize` 430x430 and never upscaled (`CountDesiredMediaSize`);
- in a bubble it is at least `historyPhotoBubbleMinWidth` 200 wide (100, `minPhotoSize`, when bare) and at least 100 high;
- a tall picture is cropped to a square (`CountPhotoMediaSize`: height at most the width) unless the crop would lose more than a quarter of it, in which case its full height is shown (`adjustHeightForLessCrop`, `FrameResizeMayExpand` 3/4);
- the width also grows to the caption's width, up to `msgMaxWidth`, and never under the time pill or an inline keyboard.

Quill: `media_frame(MediaFrameKind::Photo, w, h)` now is `photo_current` at the widest; `media_content_width` keeps the 200 minimum and the pill's 24 px margins. Before, a photo was fitted into 360x400 with a 120 px short side. Not done: growing the picture to the caption's width (the caption wraps to the picture instead; Quill measures text only in layout).

## Videos, GIFs and round videos

`Gif::countThumbSize` picks the box by kind: `maxMediaSize` 430 for a video file, `maxGifSize` 320 for an animation, `maxVideoMessageSize` 240 for a round video (`clip_thumb_size`). The rest is the photo rule, plus: a clip that is not playing is at least as wide as its duration/size label with the pill margins (`clip_optimal`, `clip_current`). Quill: videos and GIFs get their own kinds (`MediaFrameKind::Video` / `Gif`); round videos went from 220 to 240.

## Albums

`GroupedMedia` lays the tiles out with `Ui::LayoutMediaGroup` at `historyGroupWidthMax` (= `maxMediaSize` 430), `historyGroupWidthMin` (= `minPhotoSize` 100) and `historyGroupSkip` 4, then scales the whole mosaic to the bubble width. Quill's port of the algorithm (`album::layout_media_group`) already existed; its constants were 320/120/2 and are now 430/100/4.

## Files, voice notes and music

`Document::countOptimalSize` (`file_max_width`):

- `msgFileMinWidth` 268 is the floor;
- the row needs `padding.left + thumb + thumbSkip` (`msgFileLayout`: 12 + 44 + 11; with a thumbnail `msgFileThumbLayout`: 6 + 72 + 14) plus the widest status line (size, duration, played) plus, for a voice note, the unread dot (5 + 7) and the time's skip block, plus the right padding;
- a file name widens the row, capped at `msgMaxWidth`;
- a voice note is at least a full waveform wide: `kWaveformSamplesCount` 100 bars of `msgWaveformBar` 2 + `msgWaveformSkip` 1 = 300, plus the row's padding, disc and skip (and the transcribe button when shown).
- Height: `padding.top + thumb + padding.bottom` (60), minus `msgFileTopMinus` 6 when something sits above; a caption or transcript adds its height and the bottom padding.

A caption or transcript wider than the waveform: `Document::countCurrentSize` caps the row at `maxWidth()` (the optimal width above), and the caption/transcript wraps to `newWidth - msgPadding`; the waveform does not stretch to the text. Quill matches that: the rows keep their floor (268, was 220) and cap (430), captions and transcripts wrap inside.

## Stickers and emoji-only messages

`Sticker::Size`: the sticker's own pixels downscaled into `maxStickerSize` 224x224, never upscaled (`sticker_size`). Quill drew every sticker at 128 and decoded animated ones at 128; both are 224 now (the decode cache keeps its 16-clip cap). Emoji-only messages use `largeEmojiSize` 36 with a 4 px skip whatever the count (`large_emoji_width`); Quill used 40/36/32 for one/two/three and now uses 36.

## Service messages

`Service::performCountOptimalSize`: the pill is the text plus `msgServicePadding` 12 left and right; in wide mode the pill may take at most the bubble column (`msgMaxWidth + 2 * msgPhotoSkip + 2 * msgMargin.left` = 542) minus the two `msgServiceMargin.left` = 522 (`service_max_width`, `service_content_width`). Quill's pill cap went from 440 to 522.

## Replies and forwards

`Reply::updateSize` / `resizeToWidth`: `historyReplyPadding` 11/2/6/2 around a name line and a text line (two name lines when the name wraps), `historyReplyTop`/`Bottom` 2 outside, a `historyReplyPreview` 32 px thumbnail with its 7/4/4/4 margin (`reply_block_height`). Quill's thumbnail went from 30 to 32. The forward line and the sender name only widen the bubble (non-text max); GPUI's flex does that already.

## Reactions under bubbles

`InlineList::countOptimalSize` / `countCurrentSize`: a chip is `reactionInlinePadding` 5/2/7/2 around an 18 px reaction and its count (with `reactionInlineSkip` 3 between them; a countless chip drops `reactionInlineEmptySkip` 2), chips sit `reactionInlineBetween` 4 apart and wrap to the bubble width; when the time does not fit after the last row, a row's height is added (`reaction_chip_width`, `reactions_rows`). Quill's chip row already wraps with a 4 px gap; the chip metrics are the kit's pill.

## Contacts, locations, polls, games, invoices

- Contact and poll: `msgFileMinWidth` 268 as the floor, widened by the question, answers and buttons (`Poll::countOptimalSize`). Quill's contact card floor went from 220 to 268.
- Location: the map tile's own size (`st::locationSize`, from lib_ui, not in the clone), at least `minPhotoSize` and the pill width, at most `maxMediaSize`. Quill keeps its 256x144 tile.
- Game and invoice: the skip block as the floor, widened by title, description and the attached media (`Game::countOptimalSize`, `Invoice::countOptimalSize`); Quill's cards follow the bubble's text rules.

## Verified

- `src/bubble_layout.rs`: Qt's integer scaling, the 430 box, the square crop and its three-quarter exception, the 200/100 minimums, the time pill and keyboard widening, clip boxes per kind, file floors and the waveform width, text bubble shrinking and the monospace limit, stickers, emoji, service widths, reaction chips and rows, reply heights, album widths.
- Captures of every kind's demo before and after the change, in a temp directory (not committed), looked at side by side.
