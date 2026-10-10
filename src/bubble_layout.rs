//! Message bubble sizing, as Telegram Desktop does it (pure, no UI).
//!
//! Ported from `history/view/history_view_message.cpp`
//! (`performCountOptimalSize`, `countGeometry`, `resizeContentGetHeight`,
//! `minWidthForMedia`), `history_view_photo.cpp`, `history_view_gif.cpp`,
//! `history_view_document.cpp`, `history_view_sticker.cpp`,
//! `history_view_service_message.cpp`, `history_view_reply.cpp`,
//! `reactions/history_view_reactions.cpp` and the constants in
//! `ui/chat/chat.style`. Sizes are logical pixels at scale 100%.
//! `docs/decisions/codex-bubble-layout.md` has the rules in words.

/// `msgMaxWidth`: the widest a text bubble gets (padding included).
pub const MSG_MAX_WIDTH: i32 = 430;
/// `msgMinWidth`: narrower bubbles still lay their text out at this width.
pub const MSG_MIN_WIDTH: i32 = 160;
/// `msgPadding`: left, top, right, bottom.
pub const MSG_PADDING: Margins = Margins::new(11, 8, 11, 8);
/// `msgMargin`: left, top, right, bottom around a bubble.
pub const MSG_MARGIN: Margins = Margins::new(16, 6, 56, 2);
/// `msgPhotoSkip`: the sender avatar column.
pub const MSG_PHOTO_SKIP: i32 = 40;
/// `maxMediaSize`: the box a photo or video is fitted into.
pub const MAX_MEDIA_SIZE: i32 = 430;
/// `minPhotoSize`: a picture is never shown smaller than this on a side.
pub const MIN_PHOTO_SIZE: i32 = 100;
/// `historyPhotoBubbleMinWidth`: a picture in a bubble is at least this wide.
pub const PHOTO_BUBBLE_MIN_WIDTH: i32 = 200;
/// `maxGifSize`.
pub const MAX_GIF_SIZE: i32 = 320;
/// `maxVideoMessageSize`: round videos.
pub const MAX_VIDEO_MESSAGE_SIZE: i32 = 240;
/// `maxStickerSize`.
pub const MAX_STICKER_SIZE: i32 = 224;
/// `largeEmojiSize` and `largeEmojiSkip`: emoji-only messages.
pub const LARGE_EMOJI_SIZE: i32 = 36;
pub const LARGE_EMOJI_SKIP: i32 = 4;
/// `msgFileMinWidth`: files, voice notes and music never get narrower.
pub const FILE_MIN_WIDTH: i32 = 268;
/// `msgFileLayout` (no thumbnail) and `msgFileThumbLayout`.
pub const FILE_LAYOUT: FileLayout = FileLayout {
    padding: Margins::new(12, 8, 10, 8),
    thumb_size: 44,
    thumb_skip: 11,
};
pub const FILE_THUMB_LAYOUT: FileLayout = FileLayout {
    padding: Margins::new(6, 6, 10, 6),
    thumb_size: 72,
    thumb_skip: 14,
};
/// `kWaveformSamplesCount`, `msgWaveformBar`, `msgWaveformSkip`: a voice
/// waveform at full width.
pub const WAVEFORM_SAMPLES: i32 = 100;
pub const WAVEFORM_BAR: i32 = 2;
pub const WAVEFORM_SKIP: i32 = 1;
pub const WAVEFORM_MIN: i32 = 3;
pub const WAVEFORM_MAX: i32 = 17;
/// `mediaUnreadSize` + `mediaUnreadSkip`: the unplayed dot after a voice note.
pub const MEDIA_UNREAD_SIZE: i32 = 7;
pub const MEDIA_UNREAD_SKIP: i32 = 5;
/// `msgDateImgDelta` and `msgDateImgPadding.x`: the time pill over media.
pub const DATE_IMG_DELTA: i32 = 4;
pub const DATE_IMG_PADDING_X: i32 = 8;
/// `msgDateSpace` and `msgDateDelta.x`: the skip block a text line leaves
/// for the inline time.
pub const DATE_SPACE: i32 = 12;
pub const DATE_DELTA_X: i32 = 2;
/// `msgServicePadding` and `msgServiceMargin`.
pub const SERVICE_PADDING: Margins = Margins::new(12, 3, 12, 4);
pub const SERVICE_MARGIN: Margins = Margins::new(10, 10, 10, 2);
/// `historyGroupWidthMax` (= `maxMediaSize`), `historyGroupWidthMin`
/// (= `minPhotoSize`), `historyGroupSkip`.
pub const GROUP_WIDTH_MAX: i32 = MAX_MEDIA_SIZE;
pub const GROUP_WIDTH_MIN: i32 = MIN_PHOTO_SIZE;
pub const GROUP_SKIP: i32 = 4;
/// `reactionInline*`: one reaction chip and the gap between chips.
pub const REACTION_PADDING: Margins = Margins::new(5, 2, 7, 2);
pub const REACTION_SIZE: i32 = 18;
pub const REACTION_SKIP: i32 = 3;
pub const REACTION_BETWEEN: i32 = 4;
pub const REACTION_EMPTY_SKIP: i32 = 2;
/// `historyReplyPadding`, `historyReplyTop`/`Bottom`, `historyReplyPreview`
/// and its margin.
pub const REPLY_PADDING: Margins = Margins::new(11, 2, 6, 2);
pub const REPLY_TOP: i32 = 2;
pub const REPLY_BOTTOM: i32 = 2;
pub const REPLY_PREVIEW: i32 = 32;
pub const REPLY_PREVIEW_MARGIN: Margins = Margins::new(7, 4, 4, 4);
/// `mediaInBubbleSkip`, `mediaCaptionSkip`.
pub const MEDIA_IN_BUBBLE_SKIP: i32 = 5;
pub const MEDIA_CAPTION_SKIP: i32 = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Margins {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Margins {
    pub const fn new(left: i32, top: i32, right: i32, bottom: i32) -> Self {
        Self {
            left,
            top,
            right,
            bottom,
        }
    }

    pub const fn horizontal(self) -> i32 {
        self.left + self.right
    }

    pub const fn vertical(self) -> i32 {
        self.top + self.bottom
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileLayout {
    pub padding: Margins,
    pub thumb_size: i32,
    pub thumb_skip: i32,
}

impl FileLayout {
    /// Where the name starts: padding, the disc or thumbnail, the skip.
    pub const fn text_left(self) -> i32 {
        self.padding.left + self.thumb_size + self.thumb_skip
    }

    /// Height of the row without a caption (`isBubbleTop` true).
    pub const fn row_height(self) -> i32 {
        self.padding.top + self.thumb_size + self.padding.bottom
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Size {
    pub w: i32,
    pub h: i32,
}

impl Size {
    pub const fn new(w: i32, h: i32) -> Self {
        Self { w, h }
    }

    pub const fn is_empty(self) -> bool {
        self.w <= 0 || self.h <= 0
    }

    /// Qt `QSize::scaled(box, KeepAspectRatio)`: the largest size of this
    /// ratio inside `bounds`, with Qt's integer arithmetic.
    pub fn scaled_inside(self, bounds: Size) -> Size {
        if self.is_empty() {
            return bounds;
        }
        let rw = i64::from(bounds.h) * i64::from(self.w) / i64::from(self.h);
        if rw <= i64::from(bounds.w) {
            Size::new(rw as i32, bounds.h)
        } else {
            let rh = i64::from(bounds.w) * i64::from(self.h) / i64::from(self.w);
            Size::new(bounds.w, rh as i32)
        }
    }

    /// Qt `QSize::scaled(box, KeepAspectRatioByExpanding)`: the smallest
    /// size of this ratio that covers `bounds`.
    pub fn scaled_covering(self, bounds: Size) -> Size {
        if self.is_empty() {
            return bounds;
        }
        let rw = i64::from(bounds.h) * i64::from(self.w) / i64::from(self.h);
        if rw >= i64::from(bounds.w) {
            Size::new(rw as i32, bounds.h)
        } else {
            let rh = i64::from(bounds.w) * i64::from(self.h) / i64::from(self.w);
            Size::new(bounds.w, rh as i32)
        }
    }

    /// `NonEmptySize`: at least 1x1.
    pub fn non_empty(self) -> Size {
        Size::new(self.w.max(1), self.h.max(1))
    }
}

/// `DownscaledSize`: never upscale; shrink into `bounds` keeping the ratio.
pub fn downscaled(size: Size, bounds: Size) -> Size {
    if size.is_empty() {
        bounds
    } else if size.w <= bounds.w && size.h <= bounds.h {
        size
    } else {
        size.scaled_inside(bounds).non_empty()
    }
}

/// `CountDesiredMediaSize`: a photo or video fitted into 430x430 (or left
/// smaller).
pub fn desired_media_size(dims: Size) -> Size {
    if dims.is_empty() {
        return Size::new(MIN_PHOTO_SIZE, MIN_PHOTO_SIZE);
    }
    downscaled(dims, Size::new(MAX_MEDIA_SIZE, MAX_MEDIA_SIZE))
}

/// `CountMediaSize`: fit the desired size into `new_width`.
pub fn count_media_size(desired: Size, new_width: i32) -> Size {
    if desired.w <= new_width {
        desired
    } else {
        desired
            .scaled_inside(Size::new(new_width, desired.h))
            .non_empty()
    }
}

/// `CountPhotoMediaSize`: fit into `min(new_width, max_width)`, then crop
/// a tall picture to a square at most (`height <= new_width`).
pub fn count_photo_media_size(desired: Size, new_width: i32, max_width: i32) -> Size {
    let media = count_media_size(desired, new_width.min(max_width));
    if media.h <= new_width {
        media
    } else {
        media
            .scaled_inside(Size::new(media.w, new_width))
            .non_empty()
    }
}

/// `DecideFrameResize(...).expanding` for the given visible fraction
/// `nominator / denominator`: whether covering `outer` with `original`
/// crops away little enough to prefer the cover over a letterbox.
fn frame_expanding(outer: Size, original: Size, nominator: i64, denominator: i64) -> bool {
    if outer.is_empty() {
        return true;
    }
    let big = original.scaled_covering(outer);
    (i64::from(big.w) <= i64::from(outer.w)
        && i64::from(big.h) * nominator <= i64::from(outer.h) * denominator)
        || (i64::from(big.h) <= i64::from(outer.h)
            && i64::from(big.w) * nominator <= i64::from(outer.w) * denominator)
}

/// `Media::Streaming::FrameResizeMayExpand` with its defaults (3/4): the
/// picture may be shown covering `outer` when at least three quarters of
/// it stays visible, allowing for one pixel of rounding on small sizes.
pub fn frame_resize_may_expand(outer: Size, original: Size) -> bool {
    let (nominator, denominator) = (3i64, 4i64);
    let min = i64::from(outer.w.min(outer.h).min(original.w).min(original.h));
    if 2 * nominator * min < 2 * denominator + denominator * min {
        return false;
    }
    frame_expanding(
        outer,
        original,
        nominator * min - denominator,
        denominator * min,
    )
}

/// `Photo::adjustHeightForLessCrop`: a picture that would be cropped by
/// more than a quarter keeps its full height instead.
pub fn adjust_height_for_less_crop(dims: Size, current: Size) -> i32 {
    if dims.is_empty() || !frame_resize_may_expand(current, dims) {
        return current.h;
    }
    let full = i64::from(current.w) * i64::from(dims.h) / i64::from(dims.w);
    current.h.max(full as i32)
}

/// `Message::minWidthForMedia`: the time (and views) pill with its
/// margins, or an inline keyboard's natural width, whichever is wider.
/// `info_width` is 0 when the bottom info is hidden.
pub fn min_width_for_media(info_width: i32, keyboard_width: i32) -> i32 {
    let info = if info_width > 0 {
        info_width + 2 * (DATE_IMG_DELTA + DATE_IMG_PADDING_X)
    } else {
        0
    };
    info.max(keyboard_width)
}

/// `Element::textualMaxWidth` for a caption or text of `text_width`
/// (0 when there is none).
pub fn textual_max_width(text_width: i32) -> i32 {
    MSG_PADDING.horizontal() + text_width.max(0)
}

/// What a photo, video or GIF bubble needs to know besides its pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MediaContext {
    /// The media sits in a bubble (a caption, a reply, a sender name,
    /// a forward line...), not bare in the history.
    pub has_bubble: bool,
    /// `Message::infoWidth`: the time pill's content width; 0 when hidden.
    pub info_width: i32,
    /// An inline keyboard's natural width under the message; 0 when none.
    pub keyboard_width: i32,
    /// The caption's unwrapped width, 0 without a caption.
    pub caption_width: i32,
}

impl MediaContext {
    fn min_width(self, bubble_min: i32, max_media_width: i32) -> i32 {
        min_width_for_media(self.info_width, self.keyboard_width).clamp(
            if self.has_bubble {
                bubble_min
            } else {
                MIN_PHOTO_SIZE
            },
            max_media_width,
        )
    }

    fn caption_max_width(self) -> i32 {
        textual_max_width(self.caption_width)
    }
}

/// `Photo::countOptimalSize`: the bubble's widest width and the height at
/// that width. `dims` are the picture's pixels.
pub fn photo_optimal(dims: Size, context: MediaContext) -> Size {
    let scaled = desired_media_size(dims);
    let min_width = context.min_width(PHOTO_BUBBLE_MIN_WIDTH, MAX_MEDIA_SIZE);
    let max_actual_width = scaled.w.max(min_width);
    let mut max_width = max_actual_width.max(scaled.h);
    let mut min_height = scaled.h.max(MIN_PHOTO_SIZE);
    if context.has_bubble {
        let max_with_caption = MSG_MAX_WIDTH.min(context.caption_max_width());
        max_width = max_width.max(max_with_caption).min(MSG_MAX_WIDTH);
        min_height = adjust_height_for_less_crop(dims, Size::new(max_width, min_height));
    }
    Size::new(max_width, min_height)
}

/// `Photo::countCurrentSize`: the picture's box when the bubble gets
/// `new_width` (the available width, already capped by the caller).
pub fn photo_current(dims: Size, context: MediaContext, new_width: i32) -> Size {
    let optimal = photo_optimal(dims, context);
    let thumb_max_width = new_width.min(MAX_MEDIA_SIZE);
    let min_width = min_width_for_media(context.info_width, context.keyboard_width).clamp(
        thumb_max_width.min(if context.has_bubble {
            PHOTO_BUBBLE_MIN_WIDTH
        } else {
            MIN_PHOTO_SIZE
        }),
        thumb_max_width.max(1),
    );
    let desired = desired_media_size(dims);
    let pix = count_photo_media_size(desired, new_width, optimal.w);
    let mut width = pix.w.max(min_width);
    let mut height = pix.h.max(MIN_PHOTO_SIZE);
    if context.has_bubble {
        let max_with_caption = MSG_MAX_WIDTH.min(context.caption_max_width());
        width = width.max(max_with_caption).min(thumb_max_width);
        height = adjust_height_for_less_crop(dims, Size::new(width, height));
    }
    if width >= optimal.w {
        height = height.min(optimal.h);
    }
    Size::new(width, height)
}

/// Which moving picture `Gif` lays out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipKind {
    /// An animation (`maxGifSize`).
    Gif,
    /// A video file (`maxMediaSize`).
    Video,
    /// A round video message (`maxVideoMessageSize`).
    VideoMessage,
}

impl ClipKind {
    const fn max_size(self) -> i32 {
        match self {
            Self::Gif => MAX_GIF_SIZE,
            Self::Video => MAX_MEDIA_SIZE,
            Self::VideoMessage => MAX_VIDEO_MESSAGE_SIZE,
        }
    }
}

/// `Gif::countThumbSize`: the clip fitted into `min(width_max, kind max)`
/// by its kind's box; returns the size and the capped width.
pub fn clip_thumb_size(dims: Size, kind: ClipKind, width_max: i32) -> (Size, i32) {
    let max_size = kind.max_size();
    let width_max = width_max.min(max_size);
    let dims = if dims.is_empty() {
        Size::new(max_size, max_size)
    } else {
        dims
    };
    (downscaled(dims, Size::new(width_max, max_size)), width_max)
}

/// `Gif::countOptimalSize`. `status_width` is the duration/size label
/// drawn over a clip that is not playing (`GifMaxStatusWidth`).
pub fn clip_optimal(dims: Size, kind: ClipKind, context: MediaContext, status_width: i32) -> Size {
    let min_width = context.min_width(PHOTO_BUBBLE_MIN_WIDTH, MAX_MEDIA_SIZE);
    let (scaled, thumb_max_width) = clip_thumb_size(dims, kind, MSG_MAX_WIDTH);
    let mut max_width = scaled.w.max(min_width).min(thumb_max_width);
    let mut min_height = scaled.h.max(MIN_PHOTO_SIZE);
    if status_width > 0 {
        max_width = max_width.max(status_width + 2 * (DATE_IMG_DELTA + DATE_IMG_PADDING_X));
    }
    if context.has_bubble {
        max_width = max_width.max(context.caption_max_width());
        min_height = adjust_height_for_less_crop(scaled, Size::new(max_width, min_height));
    }
    Size::new(max_width, min_height)
}

/// `Gif::countCurrentSize` for `new_width`.
pub fn clip_current(
    dims: Size,
    kind: ClipKind,
    context: MediaContext,
    status_width: i32,
    new_width: i32,
) -> Size {
    let (scaled, thumb_max_width) = clip_thumb_size(dims, kind, new_width);
    let min_width_by_info = if context.info_width > 0 {
        context.info_width + 2 * (DATE_IMG_DELTA + DATE_IMG_PADDING_X)
    } else {
        0
    };
    let min_photo_width = MIN_PHOTO_SIZE.min(thumb_max_width);
    let mut width = scaled
        .w
        .max(min_width_by_info)
        .clamp(min_photo_width, thumb_max_width.max(min_photo_width));
    let mut height = scaled.h.max(MIN_PHOTO_SIZE);
    if status_width > 0 {
        width = width.max(status_width + 2 * (DATE_IMG_DELTA + DATE_IMG_PADDING_X));
    }
    if context.has_bubble {
        width = width.max(min_width_for_media(
            context.info_width,
            context.keyboard_width,
        ));
        let max_with_caption = MSG_MAX_WIDTH.min(context.caption_max_width());
        width = width.max(max_with_caption).min(thumb_max_width);
        height = adjust_height_for_less_crop(scaled, Size::new(width, height));
    }
    Size::new(width, height)
}

/// What a file row needs for its width.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileContext {
    /// The row shows a thumbnail (`msgFileThumbLayout`) instead of a disc.
    pub thumbed: bool,
    /// A voice note or a transcribed round video: waveform and unread dot.
    pub voice: bool,
    /// The file name's unwrapped width; 0 for voice notes (no name).
    pub name_width: i32,
    /// The widest status line (`MaxStatusWidth`: size, duration, played).
    pub status_width: i32,
    /// `Element::skipBlockWidth`: room for the time after the status.
    pub skip_block_width: i32,
    /// The transcribe button's width plus `historyTranscribeSkip`, or 0.
    pub transcribe_width: i32,
}

/// `Document::countOptimalSize`'s width: at least `msgFileMinWidth`, wide
/// enough for the status (and the unread dot and time on a voice note),
/// for the name (capped at `msgMaxWidth`), and for a full waveform.
pub fn file_max_width(context: FileContext) -> i32 {
    let st = if context.thumbed {
        FILE_THUMB_LAYOUT
    } else {
        FILE_LAYOUT
    };
    let tleft = st.text_left();
    let tright = st.padding.right;
    let mut max_width = FILE_MIN_WIDTH;
    if context.thumbed {
        max_width = max_width.max(tleft + context.status_width + tright);
    } else {
        let unread = if context.voice {
            MEDIA_UNREAD_SKIP + MEDIA_UNREAD_SIZE
        } else {
            0
        };
        max_width = max_width.max(
            tleft + context.status_width + unread + context.skip_block_width + MSG_PADDING.right,
        );
    }
    if context.name_width > 0 {
        max_width = max_width.max(tleft + context.name_width + tright);
        max_width = max_width.min(MSG_MAX_WIDTH);
    }
    if context.voice {
        let waveform = WAVEFORM_SAMPLES * (WAVEFORM_BAR + WAVEFORM_SKIP);
        max_width = max_width.max(
            waveform
                + st.padding.horizontal()
                + st.thumb_size
                + st.thumb_skip
                + context.transcribe_width,
        );
    }
    max_width
}

/// The widest voice-note waveform (`kWaveformSamplesCount` bars).
pub const fn waveform_max_width() -> i32 {
    WAVEFORM_SAMPLES * (WAVEFORM_BAR + WAVEFORM_SKIP)
}

/// `Element::skipBlockWidth`: the trailing room a text line keeps for the
/// inline time, 0 when the bottom info is hidden.
pub fn skip_block_width(info_width: i32) -> i32 {
    if info_width > 0 {
        DATE_SPACE + info_width - DATE_DELTA_X
    } else {
        0
    }
}

/// `Message::countGeometry` for a text bubble (no media): the bubble's
/// width for `text_width` (the longest line, unpadded), `non_text_max`
/// (the widest of sender name, reply, forward line, keyboard, info; 0 when
/// none), `monospace_max` (the widest monospace block, unpadded; 0 when
/// none, lets code widen past `msgMaxWidth`) and the `available` width.
pub fn text_bubble_width(
    text_width: i32,
    non_text_max: i32,
    monospace_max: i32,
    available: i32,
) -> i32 {
    let textual = textual_max_width(text_width);
    let max_width = textual.max(non_text_max);
    let limit = MSG_MAX_WIDTH.max(textual_max_width(monospace_max));
    let mut content = available.min(max_width).min(limit);
    // `shrunk`: the real text width (which wrapped at the limit) plus
    // padding, never under the non-text parts.
    let shrunk = textual.min(limit).max(non_text_max);
    content = content.min(shrunk);
    content.max(0)
}

/// `Sticker::Size(document)`: a sticker's box, downscaled into 224x224
/// (and 224x224 when the dimensions are unknown).
pub fn sticker_size(dims: Size) -> Size {
    let bounds = Size::new(MAX_STICKER_SIZE, MAX_STICKER_SIZE);
    if dims.is_empty() {
        bounds
    } else {
        downscaled(dims, bounds)
    }
}

/// `LargeEmoji::countOptimalSize` width for `count` emoji in a row.
pub fn large_emoji_width(count: i32) -> i32 {
    if count <= 0 {
        return 0;
    }
    count * LARGE_EMOJI_SIZE + (count - 1) * LARGE_EMOJI_SKIP
}

/// `Service::performCountOptimalSize`: the pill's width for its text.
pub fn service_max_width(text_width: i32) -> i32 {
    text_width.max(0) + SERVICE_PADDING.horizontal()
}

/// `Service::performCountCurrentSize`: the width a service pill may take
/// in a history `available` wide (wide mode caps it like a bubble column).
pub fn service_content_width(available: i32) -> i32 {
    let capped = available.min(MSG_MAX_WIDTH + 2 * MSG_PHOTO_SKIP + 2 * MSG_MARGIN.left);
    (capped - 2 * SERVICE_MARGIN.left).max(SERVICE_PADDING.horizontal() + 1)
}

/// `InlineList::countOptimalSize` for one chip: `count_width` is the
/// count label's width (0 when the chip shows no count).
pub fn reaction_chip_width(count_width: i32) -> i32 {
    if count_width > 0 {
        REACTION_PADDING.left + REACTION_SIZE + REACTION_SKIP + REACTION_PADDING.right + count_width
    } else {
        REACTION_PADDING.horizontal() + REACTION_SIZE - REACTION_EMPTY_SKIP
    }
}

/// Height of one reaction chip.
pub const fn reaction_chip_height() -> i32 {
    REACTION_PADDING.vertical() + REACTION_SIZE
}

/// `InlineList::countCurrentSize`: the rows the chips wrap into at
/// `available` width; returns `(row count, height)`. `skip_block_width`
/// adds a row when the time would not fit after the last chip.
pub fn reactions_rows(chip_widths: &[i32], available: i32, skip_block_width: i32) -> (i32, i32) {
    if chip_widths.is_empty() {
        return (0, 0);
    }
    let height = reaction_chip_height();
    let mut rows = 1;
    let mut x = 0;
    for &width in chip_widths {
        if x > 0 && x + width > available {
            rows += 1;
            x = 0;
        }
        x += width + REACTION_BETWEEN;
    }
    let right = x - REACTION_BETWEEN + skip_block_width;
    let extra = if right > available { 1 } else { 0 };
    (
        rows,
        rows * height + (rows - 1) * REACTION_BETWEEN + extra * height,
    )
}

/// `Reply::resizeToWidth`/`height`: the reply block's height for a name of
/// `name_lines` (1 or 2) and `text_lines` of text, with the fonts' line
/// heights, plus the block's outer top and bottom.
pub fn reply_block_height(
    name_lines: i32,
    name_line_height: i32,
    text_lines: i32,
    text_line_height: i32,
) -> i32 {
    REPLY_PADDING.top
        + name_lines.max(1) * name_line_height
        + (text_lines.max(1) * text_line_height).max(text_line_height)
        + REPLY_PADDING.bottom
        + REPLY_TOP
        + REPLY_BOTTOM
}

/// The width an album mosaic is laid out at (`historyGroupWidthMax`,
/// capped by what the history can show).
pub fn album_max_width(available: i32) -> i32 {
    GROUP_WIDTH_MAX.min(available).max(GROUP_WIDTH_MIN)
}

#[cfg(test)]
mod tests {
    use super::*;

    const NO_CAPTION: MediaContext = MediaContext {
        has_bubble: false,
        info_width: 30,
        keyboard_width: 0,
        caption_width: 0,
    };

    #[test]
    fn qt_scaling_matches_integer_arithmetic() {
        assert_eq!(
            Size::new(1000, 500).scaled_inside(Size::new(430, 430)),
            Size::new(430, 215)
        );
        assert_eq!(
            Size::new(500, 1000).scaled_inside(Size::new(430, 430)),
            Size::new(215, 430)
        );
        assert_eq!(
            Size::new(3, 2).scaled_inside(Size::new(430, 430)),
            Size::new(430, 286)
        );
        assert_eq!(
            Size::new(1000, 500).scaled_covering(Size::new(430, 430)),
            Size::new(860, 430)
        );
        assert_eq!(
            downscaled(Size::new(200, 100), Size::new(430, 430)),
            Size::new(200, 100)
        );
        assert_eq!(
            downscaled(Size::new(0, 0), Size::new(10, 10)),
            Size::new(10, 10)
        );
    }

    #[test]
    fn desired_media_fits_430_and_never_grows() {
        assert_eq!(
            desired_media_size(Size::new(4000, 3000)),
            Size::new(430, 322)
        );
        assert_eq!(desired_media_size(Size::new(300, 200)), Size::new(300, 200));
        assert_eq!(desired_media_size(Size::new(0, 0)), Size::new(100, 100));
    }

    #[test]
    fn tall_photos_are_cropped_to_a_square_unless_little_is_lost() {
        // A 9:16 photo in a bubble: width 430 from the box's height, the
        // square crop would lose more than a quarter, so it stays square.
        let dims = Size::new(1080, 1920);
        let optimal = photo_optimal(
            dims,
            MediaContext {
                has_bubble: true,
                ..NO_CAPTION
            },
        );
        assert_eq!(optimal, Size::new(430, 430));
        // Nearly square: the small crop is not worth it, full height shown.
        let dims = Size::new(1000, 1150);
        let optimal = photo_optimal(
            dims,
            MediaContext {
                has_bubble: true,
                ..NO_CAPTION
            },
        );
        assert_eq!(optimal.w, 430);
        assert_eq!(optimal.h, 430 * 1150 / 1000);
    }

    #[test]
    fn photo_current_shrinks_to_the_available_width() {
        let dims = Size::new(1600, 900);
        let context = MediaContext {
            has_bubble: true,
            ..NO_CAPTION
        };
        assert_eq!(photo_current(dims, context, 430), Size::new(430, 241));
        assert_eq!(photo_current(dims, context, 300), Size::new(300, 168));
        // Never narrower than the bubble minimum for a tiny picture.
        assert_eq!(
            photo_current(Size::new(40, 40), context, 430),
            Size::new(200, 100)
        );
        assert_eq!(
            photo_current(Size::new(40, 40), NO_CAPTION, 430),
            Size::new(100, 100)
        );
        // A caption widens the picture up to msgMaxWidth.
        let captioned = MediaContext {
            caption_width: 500,
            ..context
        };
        assert_eq!(photo_current(Size::new(300, 300), captioned, 600).w, 430);
        let captioned = MediaContext {
            caption_width: 100,
            ..context
        };
        assert_eq!(photo_current(Size::new(300, 300), captioned, 600).w, 300);
    }

    #[test]
    fn the_time_pill_and_keyboard_widen_small_media() {
        assert_eq!(min_width_for_media(0, 0), 0);
        assert_eq!(min_width_for_media(30, 0), 54);
        assert_eq!(min_width_for_media(30, 200), 200);
        let context = MediaContext {
            info_width: 150,
            ..NO_CAPTION
        };
        assert_eq!(photo_current(Size::new(40, 40), context, 430).w, 174);
    }

    #[test]
    fn clips_follow_their_kind_box() {
        let dims = Size::new(1920, 1080);
        let (gif, _) = clip_thumb_size(dims, ClipKind::Gif, 430);
        assert_eq!(gif, Size::new(320, 180));
        let (video, _) = clip_thumb_size(dims, ClipKind::Video, 430);
        assert_eq!(video, Size::new(430, 241));
        let (round, _) = clip_thumb_size(Size::new(640, 640), ClipKind::VideoMessage, 430);
        assert_eq!(round, Size::new(240, 240));
        let bare = clip_current(dims, ClipKind::Video, NO_CAPTION, 60, 430);
        assert_eq!(bare, Size::new(430, 241));
        let gif_in_bubble = clip_current(
            dims,
            ClipKind::Gif,
            MediaContext {
                has_bubble: true,
                ..NO_CAPTION
            },
            60,
            430,
        );
        assert_eq!(gif_in_bubble, Size::new(320, 180));
        // The status label can widen a narrow clip.
        let narrow = clip_current(Size::new(100, 400), ClipKind::Gif, NO_CAPTION, 120, 430);
        assert_eq!(narrow.w, 144);
    }

    #[test]
    fn file_rows_start_at_268_and_grow_for_names_and_waveforms() {
        let file = FileContext {
            thumbed: false,
            voice: false,
            name_width: 80,
            status_width: 60,
            skip_block_width: 40,
            transcribe_width: 0,
        };
        assert_eq!(file_max_width(file), FILE_MIN_WIDTH);
        let long = FileContext {
            name_width: 900,
            ..file
        };
        assert_eq!(file_max_width(long), MSG_MAX_WIDTH);
        let voice = FileContext {
            voice: true,
            name_width: 0,
            ..file
        };
        // 300 px waveform + 22 padding + 44 disc + 11 skip.
        assert_eq!(file_max_width(voice), 377);
        // A wide status widens a thumbnail row; a name caps it at msgMaxWidth.
        let thumbed = FileContext {
            thumbed: true,
            status_width: 400,
            name_width: 0,
            ..file
        };
        assert_eq!(file_max_width(thumbed), 6 + 72 + 14 + 400 + 10);
        let thumbed = FileContext {
            name_width: 80,
            ..thumbed
        };
        assert_eq!(file_max_width(thumbed), MSG_MAX_WIDTH);
        assert_eq!(waveform_max_width(), 300);
        assert_eq!(FILE_LAYOUT.row_height(), 60);
        assert_eq!(FILE_LAYOUT.text_left(), 67);
    }

    #[test]
    fn text_bubbles_shrink_to_their_text_and_cap_at_430() {
        assert_eq!(text_bubble_width(100, 0, 0, 800), 122);
        assert_eq!(text_bubble_width(1000, 0, 0, 800), 430);
        assert_eq!(text_bubble_width(1000, 0, 0, 300), 300);
        // A long sender name or reply widens a short text.
        assert_eq!(text_bubble_width(20, 180, 0, 800), 180);
        // Monospace blocks may pass the cap, up to their own width.
        assert_eq!(text_bubble_width(1000, 0, 520, 800), 542);
        assert_eq!(skip_block_width(30), 40);
        assert_eq!(skip_block_width(0), 0);
    }

    #[test]
    fn stickers_emoji_and_service_pills() {
        assert_eq!(sticker_size(Size::new(512, 512)), Size::new(224, 224));
        assert_eq!(sticker_size(Size::new(512, 256)), Size::new(224, 112));
        assert_eq!(sticker_size(Size::new(100, 100)), Size::new(100, 100));
        assert_eq!(sticker_size(Size::new(0, 0)), Size::new(224, 224));
        assert_eq!(large_emoji_width(3), 36 * 3 + 8);
        assert_eq!(service_max_width(100), 124);
        assert_eq!(service_content_width(1000), 430 + 80 + 32 - 20);
        assert_eq!(service_content_width(200), 180);
    }

    #[test]
    fn reaction_chips_wrap_and_keep_room_for_the_time() {
        assert_eq!(reaction_chip_width(10), 5 + 18 + 3 + 7 + 10);
        assert_eq!(reaction_chip_width(0), 5 + 7 + 18 - 2);
        assert_eq!(reaction_chip_height(), 22);
        let chips = [43, 43, 43];
        assert_eq!(reactions_rows(&chips, 200, 0), (1, 22));
        assert_eq!(reactions_rows(&chips, 90, 0), (2, 48));
        // The time does not fit after the row: one more row of height.
        assert_eq!(reactions_rows(&chips, 141, 40), (1, 44));
        assert_eq!(reactions_rows(&[], 100, 40), (0, 0));
    }

    #[test]
    fn reply_block_and_album_widths() {
        assert_eq!(reply_block_height(1, 18, 1, 18), 2 + 18 + 18 + 2 + 4);
        assert_eq!(reply_block_height(2, 18, 1, 18), 2 + 36 + 18 + 2 + 4);
        assert_eq!(album_max_width(1000), 430);
        assert_eq!(album_max_width(300), 300);
        assert_eq!(album_max_width(50), 100);
    }
}
