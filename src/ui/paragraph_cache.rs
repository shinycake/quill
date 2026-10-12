//! Shared cache of laid-out message paragraphs.
//!
//! Message text is rebuilt every frame, and the layout engine measures each
//! paragraph several times per frame (unconstrained, then at its width).
//! Shaping is the expensive part (CoreText on macOS), so a scroll that
//! re-shapes every visible message misses its frame budget. Paragraphs are
//! kept here by everything that changes their layout — the text, its styled
//! runs, font size, line height and wrap width — so a repeat measure or a
//! scroll frame gets the same paragraph back instead of shaping again.

use gpui_kit::base::input::bidi_paragraph::BidiParagraph;
use gpui_kit::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::rc::Rc;

/// Entries kept before old ones are dropped.
const CAPACITY: usize = 2048;

#[derive(Clone, PartialEq, Eq, Hash)]
struct Key {
    text: SharedString,
    runs: u64,
    font_size: u32,
    line_height: u32,
    wrap: Option<u32>,
    one_line: bool,
}

#[derive(Default)]
struct Cache {
    entries: HashMap<Key, (Rc<BidiParagraph>, u64)>,
    clock: u64,
}

thread_local! {
    static CACHE: RefCell<Cache> = RefCell::new(Cache::default());
}

fn hash_color(color: &Hsla, h: &mut impl Hasher) {
    color.h.to_bits().hash(h);
    color.s.to_bits().hash(h);
    color.l.to_bits().hash(h);
    color.a.to_bits().hash(h);
}

/// Fingerprint of the styled runs: anything in them that changes shaping
/// or what the paragraph paints.
fn runs_fingerprint(runs: &[TextRun]) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for run in runs {
        run.len.hash(&mut h);
        run.font.hash(&mut h);
        hash_color(&run.color, &mut h);
        if let Some(bg) = &run.background_color {
            1u8.hash(&mut h);
            hash_color(bg, &mut h);
        }
        if let Some(u) = &run.underline {
            2u8.hash(&mut h);
            f32::from(u.thickness).to_bits().hash(&mut h);
            u.wavy.hash(&mut h);
            if let Some(c) = &u.color {
                hash_color(c, &mut h);
            }
        }
        if let Some(s) = &run.strikethrough {
            3u8.hash(&mut h);
            f32::from(s.thickness).to_bits().hash(&mut h);
            if let Some(c) = &s.color {
                hash_color(c, &mut h);
            }
        }
    }
    h.finish()
}

/// The paragraph for this text and style at `wrap`, laid out once and then
/// reused until it falls out of the cache.
pub(super) fn layout(
    text: &SharedString,
    runs: &[TextRun],
    font_size: Pixels,
    line_height: Pixels,
    wrap: Option<Pixels>,
    window: &mut Window,
) -> Option<Rc<BidiParagraph>> {
    cached(
        text,
        runs,
        font_size,
        line_height,
        wrap,
        false,
        |window| BidiParagraph::layout(text, runs, font_size, line_height, wrap, window),
        window,
    )
}

/// The same for a one-line (elided) layout at `width`.
pub(super) fn layout_one_line(
    text: &SharedString,
    runs: &[TextRun],
    font_size: Pixels,
    line_height: Pixels,
    width: Pixels,
    window: &mut Window,
) -> Option<Rc<BidiParagraph>> {
    cached(
        text,
        runs,
        font_size,
        line_height,
        Some(width),
        true,
        |window| BidiParagraph::layout_one_line(text, runs, font_size, line_height, width, window),
        window,
    )
}

#[allow(clippy::too_many_arguments)]
fn cached(
    text: &SharedString,
    runs: &[TextRun],
    font_size: Pixels,
    line_height: Pixels,
    wrap: Option<Pixels>,
    one_line: bool,
    lay_out: impl FnOnce(&mut Window) -> Option<BidiParagraph>,
    window: &mut Window,
) -> Option<Rc<BidiParagraph>> {
    let key = Key {
        text: text.clone(),
        runs: runs_fingerprint(runs),
        font_size: f32::from(font_size).to_bits(),
        line_height: f32::from(line_height).to_bits(),
        wrap: wrap.map(|w| f32::from(w).to_bits()),
        one_line,
    };
    let hit = CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        cache.clock += 1;
        let now = cache.clock;
        cache.entries.get_mut(&key).map(|(paragraph, used)| {
            *used = now;
            paragraph.clone()
        })
    });
    if hit.is_some() {
        return hit;
    }
    let paragraph = Rc::new(lay_out(window)?);
    CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if cache.entries.len() >= CAPACITY {
            // Keep the most recently used half.
            let mut ages: Vec<u64> = cache.entries.values().map(|(_, used)| *used).collect();
            ages.sort_unstable();
            let cutoff = ages[ages.len() / 2];
            cache.entries.retain(|_, (_, used)| *used > cutoff);
        }
        let now = cache.clock;
        cache.entries.insert(key, (paragraph.clone(), now));
    });
    Some(paragraph)
}

#[cfg(test)]
mod tests {
    use super::runs_fingerprint;
    use gpui_kit::{Hsla, TextRun, font};

    fn run(len: usize, color: Hsla) -> TextRun {
        TextRun {
            len,
            font: font("Helvetica"),
            color,
            background_color: None,
            underline: None,
            strikethrough: None,
        }
    }

    #[test]
    fn fingerprint_changes_with_style() {
        let a = runs_fingerprint(&[run(5, gpui_kit::black())]);
        let b = runs_fingerprint(&[run(5, gpui_kit::white())]);
        let c = runs_fingerprint(&[run(4, gpui_kit::black())]);
        assert_ne!(a, b);
        assert_ne!(a, c);
        assert_eq!(a, runs_fingerprint(&[run(5, gpui_kit::black())]));
    }
}
