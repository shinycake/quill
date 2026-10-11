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

thread_local! {
    /// Glyph advances by font, size and character, as GPUI's own line
    /// wrapper caches them; cleared when it grows past
    /// [`CHAR_WIDTH_CACHE_CAP`] entries so a long session stays bounded.
    static CHAR_WIDTHS: std::cell::RefCell<HashMap<(FontId, u32, char), Pixels>> =
        std::cell::RefCell::new(HashMap::new());
}

const CHAR_WIDTH_CACHE_CAP: usize = 16 * 1024;

/// The width `c` takes in `font_id` at `font_size`.
fn char_width(text_system: &TextSystem, font_id: FontId, font_size: Pixels, c: char) -> Pixels {
    let key = (font_id, f32::from(font_size).to_bits(), c);
    CHAR_WIDTHS.with(|cache| {
        if let Some(width) = cache.borrow().get(&key) {
            return *width;
        }
        let width = text_system.layout_width(font_id, font_size, c);
        let mut cache = cache.borrow_mut();
        if cache.len() >= CHAR_WIDTH_CACHE_CAP {
            cache.clear();
        }
        cache.insert(key, width);
        width
    })
}

/// The unwrapped width of the longest line of `text` in the theme's font
/// at `font_size` (`Ui::Text::String::maxWidth`): the sum of its glyph
/// advances, which is also what GPUI's line wrapper adds up when it
/// decides where the rendered text wraps. `last_line_extra` is added to
/// the last line: the skip block the inline time takes there.
pub(in crate::ui) fn longest_line_width(
    cx: &App,
    text: &str,
    font_size: Pixels,
    weight: FontWeight,
    last_line_extra: i32,
) -> i32 {
    if text.is_empty() {
        return 0;
    }
    let mut font = font(cx.theme().font_family.clone());
    font.weight = weight;
    let text_system = cx.text_system();
    let font_id = text_system.resolve_font(&font);
    let lines: Vec<&str> = text.lines().collect();
    let last = lines.len().saturating_sub(1);
    lines
        .iter()
        .enumerate()
        .map(|(index, line)| {
            let width: f32 = line
                .chars()
                .map(|c| f32::from(char_width(text_system, font_id, font_size, c)))
                .sum();
            let width = width.ceil() as i32;
            if index == last {
                width + last_line_extra
            } else {
                width
            }
        })
        .max()
        .unwrap_or(0)
}

/// The caption of a media message, empty for other content and for media
/// without one.
pub(in crate::ui) fn caption_of(content: &MessageContent) -> &str {
    match content {
        MessageContent::Photo(photo) => &photo.caption,
        MessageContent::Video(video) => &video.caption,
        MessageContent::Animation(animation) => &animation.caption,
        MessageContent::Document(document) => &document.caption,
        MessageContent::VoiceNote(note) => &note.caption,
        MessageContent::Audio(audio) => &audio.caption,
        _ => "",
    }
}

/// `Element::textualMaxWidth`'s text part for a caption: its longest line
/// plus, on the last line, the room the inline time takes
/// (`skipBlockWidth`) when the caption ends the bubble. 0 without a
/// caption.
pub(in crate::ui) fn caption_width(
    cx: &App,
    caption: &str,
    font: Pixels,
    footer_reserve: Option<Pixels>,
) -> i32 {
    let extra = footer_reserve.map_or(0, |reserve| f32::from(reserve).ceil() as i32);
    longest_line_width(cx, caption, font, FontWeight::NORMAL, extra)
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
        WAVEFORM_MAX, WAVEFORM_MIN, content_for_outer, file_row_bounds, media_context,
        poll_min_width, voice_row_width, waveform_bars,
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
}
