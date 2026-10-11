//! One width policy for every bubble kind, following Telegram Desktop's
//! `Message::countGeometry` and each media view's `countOptimalSize` /
//! `countCurrentSize`. The pure numbers live in `quill::bubble_layout`;
//! this module turns them into the widths the renderers set, measures the
//! text they depend on, and lays a voice waveform out across its row
//! (`PaintWaveform` in `history_view_document.cpp`).
//!
//! The rules per kind (`docs/decisions/codex-bubble-width-policy.md` has
//! the long form):
//!
//! - text: shrink-wrap to the text, never past `msgMaxWidth`; the sender
//!   name, reply and forward lines only widen it (flex does both);
//! - photo / video / GIF: the picture decides the bubble, widened to the
//!   caption's longest line up to `msgMaxWidth` (`Photo::countCurrentSize`);
//! - voice note: a fixed row as wide as a full waveform, the waveform
//!   filling it; a caption or transcript wraps inside;
//! - music and files: `msgFileMinWidth` to `msgMaxWidth`, the name
//!   widening the row in between;
//! - poll: `msgFileMinWidth` floor, the question and answers widen it;
//! - contact: as wide as its lines and buttons, no floor.

use super::*;
use quill::bubble_layout::{
    FILE_MIN_WIDTH, FileContext, MSG_MAX_WIDTH, MediaContext, WAVEFORM_BAR, WAVEFORM_MAX,
    WAVEFORM_MIN, WAVEFORM_SAMPLES, WAVEFORM_SKIP, file_max_width, skip_block_width,
};

/// Horizontal chrome of a padded bubble around its content: the kit
/// bubble's `px_3` on each side and its 1 px border
/// (`message_media::bubble_outer_width`). Telegram Desktop's widths are
/// bubble widths, so a row inside a padded bubble takes this much less.
pub(in crate::ui) const PADDED_CHROME: i32 = 26;

/// Content width of a bubble Telegram Desktop makes `outer` wide.
pub(in crate::ui) fn content_for_outer(outer: i32, plain: bool) -> Pixels {
    let inner = if plain { outer } else { outer - PADDED_CHROME };
    px(inner.max(0) as f32)
}

/// A voice note row (`Document::countOptimalSize` with a
/// `HistoryDocumentVoice`): the widest of `msgFileMinWidth`, the status
/// line with the unread dot and the time's skip block, and a full
/// 100-bar waveform with the row's disc and padding. The waveform then
/// fills whatever is left of it. `status_width` is the duration label's
/// width, `info_width` the time footer's (0 when hidden).
pub(in crate::ui) fn voice_row_width(status_width: i32, info_width: i32, plain: bool) -> Pixels {
    let outer = file_max_width(FileContext {
        thumbed: false,
        voice: true,
        name_width: 0,
        status_width,
        skip_block_width: skip_block_width(info_width),
        transcribe_width: 0,
    });
    content_for_outer(outer, plain)
}

/// A music or document row: `msgFileMinWidth` at the narrowest and
/// `msgMaxWidth` at the widest (`Document::countOptimalSize` caps the name
/// there); between the two the name widens the row, which flex does.
pub(in crate::ui) fn file_row_bounds(plain: bool) -> (Pixels, Pixels) {
    (
        content_for_outer(FILE_MIN_WIDTH, plain),
        content_for_outer(MSG_MAX_WIDTH, plain),
    )
}

/// A poll never gets narrower than `msgFileMinWidth`
/// (`Poll::countOptimalSize`); its question and answers widen it.
pub(in crate::ui) fn poll_min_width(plain: bool) -> Pixels {
    content_for_outer(FILE_MIN_WIDTH, plain)
}

pub(in crate::ui) use super::text_measure::longest_line_width;

/// A media message's caption and its entities, empty for other content
/// and for media without one.
pub(in crate::ui) fn caption_parts(content: &MessageContent) -> (&str, &[TextEntity]) {
    match content {
        MessageContent::Photo(photo) => (&photo.caption, &photo.caption_entities),
        MessageContent::Video(video) => (&video.caption, &video.caption_entities),
        MessageContent::Animation(animation) => (&animation.caption, &animation.caption_entities),
        MessageContent::Document(document) => (&document.caption, &document.caption_entities),
        MessageContent::VoiceNote(note) => (&note.caption, &note.caption_entities),
        MessageContent::Audio(audio) => (&audio.caption, &audio.caption_entities),
        _ => ("", &[]),
    }
}

/// `Element::textualMaxWidth`'s text part for a caption: its longest
/// line, bold and italic runs in their own faces and custom emoji at
/// their placeholder's width, plus, on the last line, the room the inline
/// time takes (`skipBlockWidth`) when `footer_reserve` is given, which the
/// caller does only when the caption ends the bubble. 0 without a caption.
pub(in crate::ui) fn caption_width(
    cx: &App,
    content: &MessageContent,
    font: Pixels,
    footer_reserve: Option<Pixels>,
) -> i32 {
    let (caption, entities) = caption_parts(content);
    let extra = footer_reserve.map_or(0, |reserve| f32::from(reserve).ceil() as i32);
    super::text_measure::styled_longest_line(cx, caption, entities, font, extra)
}

/// What follows a caption in its bubble, for [`caption_ends_bubble`].
#[derive(Clone, Copy, Debug, Default)]
pub(in crate::ui) struct CaptionTail {
    /// The caption sits above the media (`show_caption_above_media`).
    pub(in crate::ui) above_media: bool,
    /// An inline keyboard, reactions, a self-destruct or auto-delete
    /// badge, or the comments / replies bar renders after it.
    pub(in crate::ui) rows_after: bool,
    /// The footer carries views or a signature and takes its own line.
    pub(in crate::ui) footer_own_line: bool,
    /// The caption's last line is right-to-left: the time goes under it.
    pub(in crate::ui) ends_rtl: bool,
    /// The message has a date to stamp.
    pub(in crate::ui) dated: bool,
}

/// Whether the inline time shares the caption's last line, so the
/// caption's width takes the skip block (Telegram Desktop keeps the skip
/// block only on the text that ends the bubble).
pub(in crate::ui) fn caption_ends_bubble(tail: CaptionTail) -> bool {
    !tail.above_media && !tail.rows_after && !tail.footer_own_line && !tail.ends_rtl && tail.dated
}

/// [`CaptionTail`] for `message` from what it carries itself;
/// `bottom_bar` says whether a comments / replies bar renders under it.
pub(in crate::ui) fn caption_tail(
    message: &HistoryMessage,
    bottom_bar: bool,
    now_ms: u64,
) -> CaptionTail {
    use quill::telegram::envelope::ReplyMarkup;
    let keyboard = message
        .ephemeral
        .as_ref()
        .and_then(|ephemeral| ephemeral.reply_markup.as_ref())
        .or(message.reply_markup.as_ref())
        .is_some_and(|markup| {
            matches!(markup, ReplyMarkup::InlineKeyboard(keyboard)
                if keyboard.rows.iter().any(|row| !row.is_empty()))
        });
    let views = message
        .interaction_info
        .as_ref()
        .is_some_and(|info| info.view_count > 0);
    let signature = message.author_signature.is_some() && message.forward_info.is_none();
    let (caption, _) = caption_parts(&message.content);
    CaptionTail {
        above_media: crate::ui::message_text::caption_above_media(&message.content),
        rows_after: keyboard
            || bottom_bar
            || !message.reaction_chips().is_empty()
            || message.self_destruct_badge(now_ms).is_some()
            || message.auto_delete_chip(now_ms).is_some(),
        footer_own_line: views || signature,
        ends_rtl: quill::text::last_line_is_rtl(caption),
        dated: message.date > 0,
    }
}

/// The widest a text bubble gets (`Message::_bubbleWidthLimit`):
/// `msgMaxWidth`, or wider so a `pre` block of `monospace` width (its
/// chrome included, `text_measure::widest_pre_block`) fits unwrapped
/// (`monospaceMaxWidth`). The kit bubble's own share of the pane still
/// caps it. `None` keeps the default limit.
pub(in crate::ui) fn bubble_width_limit(monospace: i32, plain: bool) -> Option<Pixels> {
    let outer = if plain {
        monospace
    } else {
        monospace + PADDED_CHROME
    };
    (outer > MSG_MAX_WIDTH).then(|| px(outer as f32))
}

/// [`bubble_width_limit`] for a text message or a media caption.
pub(in crate::ui) fn message_width_limit(
    cx: &App,
    content: &MessageContent,
    font: Pixels,
    plain: bool,
) -> Option<Pixels> {
    let (text, entities) = match content {
        MessageContent::Text(text) => (text.text.as_str(), text.entities.as_slice()),
        other => caption_parts(other),
    };
    let monospace = super::text_measure::monospace_width(cx, text, entities, font);
    bubble_width_limit(monospace, plain)
}

/// Horizontal room the link preview card takes around its copy and media
/// (`link_preview_card`: `px_2` on both sides and the 2 px accent bar).
pub(in crate::ui) const PREVIEW_CARD_CHROME: i32 = 8 + 8 + 2;

/// `webPagePhotoDelta`: the gap between an article's copy and its
/// thumbnail (`gap_2`).
pub(in crate::ui) const PREVIEW_PHOTO_DELTA: i32 = 8;

/// The widest a link preview card's inside gets: the padded bubble at
/// `msgMaxWidth`, less the card's own chrome.
pub(in crate::ui) fn preview_inner_max() -> i32 {
    MSG_MAX_WIDTH - PADDED_CHROME - PREVIEW_CARD_CHROME
}

/// `ArticleThumbWidth`: a thumbnail of `dims` in a box `height` tall
/// keeps its proportions but is never wider than tall, nor under 1 px.
pub(in crate::ui) fn article_thumb_width(dims: (i32, i32), height: i32) -> i32 {
    let (w, h) = dims;
    if h > 0 {
        (height * w.max(0) / h).min(height).max(1)
    } else {
        1
    }
}

/// An article preview's copy, for its thumbnail's box: the unwrapped
/// widths of the site name (0 without one), the title (0 without one)
/// and each description paragraph, and the line heights the card draws
/// them at (the title's is the taller, Telegram Desktop's
/// `UnitedLineHeight`).
#[derive(Clone, Debug, Default)]
pub(in crate::ui) struct ArticleCopy {
    pub(in crate::ui) site: i32,
    pub(in crate::ui) title: i32,
    pub(in crate::ui) description: Vec<i32>,
    pub(in crate::ui) title_line: f32,
    pub(in crate::ui) small_line: f32,
}

impl ArticleCopy {
    /// The heights of the copy's lines wrapped at `width`: the site name
    /// on one line, the title on at most two, the description on what is
    /// left of `linesMax` (5).
    fn lines(&self, width: i32) -> Vec<f32> {
        let wrapped = |text: i32| -> usize {
            if text <= 0 {
                0
            } else {
                ((text + width.max(1) - 1) / width.max(1)) as usize
            }
        };
        let site = usize::from(self.site > 0);
        let title = wrapped(self.title).min(2);
        let description: usize = self.description.iter().map(|&w| wrapped(w).max(1)).sum();
        let description = description.min(5usize.saturating_sub(site + title));
        std::iter::repeat_n(self.small_line, site)
            .chain(std::iter::repeat_n(self.title_line, title))
            .chain(std::iter::repeat_n(self.small_line, description))
            .collect()
    }
}

/// `WebPage::countCurrentSize` for an article: the small thumbnail's
/// box (width, height) beside `copy` in a card `inner` wide. It starts
/// five lines tall and loses a line at a time while the copy beside it
/// has fewer lines, down to one; its width is [`article_thumb_width`] for
/// its height, and the copy wraps in what the thumbnail leaves. Telegram
/// Desktop's lines are all `UnitedLineHeight` tall; Quill's site name
/// and description lines are smaller than the title's, so the box takes
/// the copy's real height over that many lines.
pub(in crate::ui) fn article_thumb(dims: (i32, i32), copy: &ArticleCopy, inner: i32) -> (i32, i32) {
    let line = copy.title_line.max(1.);
    let floor = line.round() as i32;
    for count in (1..=5usize).rev() {
        let trial = (count as f32 * line).round() as i32;
        let left = inner - PREVIEW_PHOTO_DELTA - article_thumb_width(dims, trial).max(floor);
        let lines = copy.lines(left);
        if lines.len() >= count || count == 1 {
            let height = lines.iter().take(count).sum::<f32>().max(line).round() as i32;
            return (article_thumb_width(dims, height).max(floor), height);
        }
    }
    (floor, floor)
}

/// A link preview's large photo (`WebPage` with a `Photo` attach): the
/// photo's own optimal width, capped at the card's inside at
/// `msgMaxWidth`, and its frame there (`Photo::countCurrentSize`). The
/// card draws it at least that wide and fills a wider card's inside at
/// the same proportions.
pub(in crate::ui) fn preview_photo_frame(dims: (i32, i32)) -> (i32, i32) {
    use quill::bubble_layout::{Size, photo_current, photo_optimal};
    let dims = Size::new(dims.0, dims.1);
    let context = media_context(0, 0);
    let width = photo_optimal(dims, context).w.min(preview_inner_max());
    let frame = photo_current(dims, context, width);
    (frame.w.min(width), frame.h)
}

/// What a picture in a bubble needs besides its pixels: the caption's
/// width and the time pill's (`Message::infoWidth`, 0 when hidden).
pub(in crate::ui) fn media_context(caption_width: i32, info_width: i32) -> MediaContext {
    MediaContext {
        has_bubble: true,
        info_width,
        keyboard_width: 0,
        caption_width,
    }
}

/// One bar of a laid-out waveform: its left edge, top and height in the
/// row's pixels (`msgWaveformBar` wide).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::ui) struct WaveBar {
    pub(in crate::ui) left: i32,
    pub(in crate::ui) top: f32,
    pub(in crate::ui) height: i32,
}

/// Height the waveform takes: `msgWaveformMax` under a `lineWidth` of top
/// room.
pub(in crate::ui) const WAVEFORM_HEIGHT: i32 = WAVEFORM_MAX + 1;

/// `PaintWaveform`: lays `values` (5-bit peaks, 0..=31) out across
/// `available` pixels as `msgWaveformBar` bars `msgWaveformSkip` apart,
/// between `msgWaveformMin` and `msgWaveformMax` tall. More samples than
/// bars are merged on a `samples x bars` grid, taking each cell's peak, as
/// Telegram Desktop does; fewer samples are repeated so a short waveform
/// still fills the row (Telegram Desktop would draw them at the left).
/// No samples draw flat bars.
pub(in crate::ui) fn waveform_bars(values: &[u8], available: i32) -> Vec<WaveBar> {
    let step = WAVEFORM_BAR + WAVEFORM_SKIP;
    let fit = (available.max(0) / step).max(0) as usize;
    if fit == 0 {
        return Vec::new();
    }
    let stretched: Vec<u8>;
    let samples: &[u8] = if values.is_empty() {
        stretched = vec![0; WAVEFORM_SAMPLES as usize];
        &stretched
    } else if values.len() < fit {
        stretched = (0..fit).map(|i| values[i * values.len() / fit]).collect();
        &stretched
    } else {
        values
    };
    let count = samples.len();
    let bar_count = fit.min(count);
    let wavemax = samples.iter().copied().max().unwrap_or(0);
    let norm = i32::from(wavemax) + 1;
    let max_delta = WAVEFORM_MAX - WAVEFORM_MIN;
    let mut bars = Vec::with_capacity(bar_count);
    let mut left = 0;
    let mut sum = 0usize;
    let mut max_value = 0i32;
    for &sample in samples {
        let value = i32::from(sample);
        if sum + bar_count < count {
            max_value = max_value.max(value);
            sum += bar_count;
            continue;
        }
        sum = sum + bar_count - count;
        if sum < bar_count.div_ceil(2) {
            max_value = max_value.max(value);
        }
        let bar_value = (max_value * max_delta + norm / 2) / norm;
        bars.push(WaveBar {
            left,
            top: 1. + (WAVEFORM_MAX - bar_value) as f32 / 2.,
            height: WAVEFORM_MIN + bar_value,
        });
        left += step;
        max_value = if sum < bar_count.div_ceil(2) {
            0
        } else {
            value
        };
    }
    bars
}

#[cfg(test)]
mod tests {
    use super::{
        ArticleCopy, CaptionTail, WAVEFORM_MAX, WAVEFORM_MIN, article_thumb, article_thumb_width,
        bubble_width_limit, caption_ends_bubble, content_for_outer, file_row_bounds, media_context,
        poll_min_width, preview_inner_max, preview_photo_frame, voice_row_width, waveform_bars,
    };
    use gpui_kit::px;

    #[test]
    fn rows_take_telegram_desktop_widths_less_the_bubble_chrome() {
        // msgFileMinWidth 268 is the bubble's width; the padded kit bubble
        // adds 12 + 12 + 2 around its content.
        assert_eq!(content_for_outer(268, false), px(242.));
        assert_eq!(content_for_outer(268, true), px(268.));
        assert_eq!(file_row_bounds(false), (px(242.), px(404.)));
        assert_eq!(poll_min_width(true), px(268.));
    }

    #[test]
    fn a_voice_row_is_as_wide_as_a_full_waveform() {
        // 100 bars of 3 px, the row's 12 + 10 padding, the 44 px disc and
        // its 11 px skip: 377, as Telegram Desktop; 351 inside the bubble.
        assert_eq!(voice_row_width(40, 30, true), px(377.));
        assert_eq!(voice_row_width(40, 30, false), px(351.));
        // A very wide status line (a long "played / total") still wins.
        // 67 text left + 300 status + 12 unread dot + 40 skip block + 11.
        assert_eq!(voice_row_width(300, 30, true), px(430.));
    }

    #[test]
    fn waveform_fills_the_row_with_three_pixel_bars() {
        let samples: Vec<u8> = (0..100).map(|i| (i % 32) as u8).collect();
        let bars = waveform_bars(&samples, 300);
        assert_eq!(bars.len(), 100);
        assert_eq!(bars[0].left, 0);
        assert_eq!(bars[99].left, 297);
        // Peak 31 of 32: (31 * 14 + 16) / 32 = 14, so 17 px tall at the top.
        let tallest = bars.iter().map(|bar| bar.height).max().unwrap();
        assert_eq!(tallest, WAVEFORM_MAX);
        assert_eq!(bars[0].height, WAVEFORM_MIN);
        // A flat bar sits centred in the 17 px band under 1 px of room.
        assert_eq!(bars[0].top, 1. + 8.5);
        // Fewer pixels: the samples merge, taking each cell's peak.
        let merged = waveform_bars(&samples, 150);
        assert_eq!(merged.len(), 50);
        assert_eq!(merged[0].height, bars[1].height);
    }

    #[test]
    fn short_and_missing_waveforms_still_fill_the_row() {
        assert_eq!(waveform_bars(&[4, 16, 28], 300).len(), 100);
        let flat = waveform_bars(&[], 300);
        assert_eq!(flat.len(), 100);
        assert!(flat.iter().all(|bar| bar.height == WAVEFORM_MIN));
        assert!(waveform_bars(&[1, 2], 2).is_empty());
    }

    #[test]
    fn media_context_carries_caption_and_time() {
        let context = media_context(300, 46);
        assert!(context.has_bubble);
        assert_eq!(context.caption_width, 300);
        assert_eq!(context.info_width, 46);
    }

    #[test]
    fn file_rows_in_the_plain_look_take_the_whole_bubble_widths() {
        // No bubble padding to leave out: 268..430, not 242..404.
        assert_eq!(file_row_bounds(true), (px(268.), px(430.)));
        assert_eq!(file_row_bounds(false), (px(242.), px(404.)));
    }

    #[test]
    fn a_wide_monospace_block_lifts_the_bubble_limit() {
        // No block, or one that fits: msgMaxWidth stays.
        assert_eq!(bubble_width_limit(0, false), None);
        assert_eq!(bubble_width_limit(404, false), None);
        // 500 px of code (its chrome included) and the 26 px bubble chrome.
        assert_eq!(bubble_width_limit(500, false), Some(px(526.)));
        assert_eq!(bubble_width_limit(500, true), Some(px(500.)));
        assert_eq!(bubble_width_limit(430, true), None);
    }

    #[test]
    fn the_skip_block_joins_a_caption_only_when_it_ends_the_bubble() {
        let ends = CaptionTail {
            dated: true,
            ..CaptionTail::default()
        };
        assert!(caption_ends_bubble(ends));
        for tail in [
            CaptionTail {
                above_media: true,
                ..ends
            },
            // Reactions, a keyboard, badges or the replies bar follow.
            CaptionTail {
                rows_after: true,
                ..ends
            },
            CaptionTail {
                footer_own_line: true,
                ..ends
            },
            CaptionTail {
                ends_rtl: true,
                ..ends
            },
            CaptionTail {
                dated: false,
                ..ends
            },
        ] {
            assert!(!caption_ends_bubble(tail), "{tail:?}");
        }
    }

    #[test]
    fn article_thumbnails_are_at_most_square() {
        assert_eq!(article_thumb_width((90, 90), 50), 50);
        assert_eq!(article_thumb_width((200, 100), 50), 50);
        assert_eq!(article_thumb_width((100, 200), 50), 25);
        assert_eq!(article_thumb_width((10, 1000), 50), 1);
        assert_eq!(article_thumb_width((0, 0), 50), 1);
    }

    #[test]
    fn an_article_thumbnail_is_as_tall_as_the_copy_beside_it() {
        let copy = ArticleCopy {
            site: 50,
            title: 80,
            description: vec![200],
            title_line: 19.6,
            small_line: 16.8,
        };
        let inner = preview_inner_max();
        assert_eq!(inner, 386);
        // Site, title and one description line: 16.8 + 19.6 + 16.8.
        assert_eq!(article_thumb((90, 90), &copy, inner), (53, 53));
        // A portrait thumbnail keeps its proportions in that height.
        assert_eq!(article_thumb((45, 90), &copy, inner), (26, 53));
        // A long description fills the five lines (`linesMax`).
        let long = ArticleCopy {
            description: vec![2000],
            ..copy.clone()
        };
        assert_eq!(article_thumb((90, 90), &long, inner), (87, 87));
        // No copy: one line, never less.
        let empty = ArticleCopy {
            title_line: 19.6,
            small_line: 16.8,
            ..ArticleCopy::default()
        };
        assert_eq!(article_thumb((90, 90), &empty, inner), (20, 20));
    }

    #[test]
    fn a_large_preview_photo_fills_the_cards_inside() {
        // A landscape photo's optimal width is msgMaxWidth: the whole
        // inside of the card at the widest bubble.
        let (w, h) = preview_photo_frame((1100, 740));
        assert_eq!(w, preview_inner_max());
        assert_eq!(h, 259);
        // A small one keeps `historyPhotoBubbleMinWidth`.
        assert_eq!(preview_photo_frame((90, 90)).0, 200);
    }
}
