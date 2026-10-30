//! Shared cache of laid-out message paragraphs.
//!
//! Message text is rebuilt every frame, and the layout engine measures each
//! paragraph several times per frame.
//! Shaping is the expensive part (CoreText on macOS), so a scroll that
//! re-shapes every visible message misses its frame budget. Paragraphs are
//! kept here by everything that changes their layout — the text, its styled
//! runs, font size, line height and wrap width — so a repeat measure or a
//! scroll frame gets the same paragraph back instead of shaping again.
//!
//! Layout asks for a message paragraph at about four widths (unconstrained,
//! its widest line, the room offered and the final width) and it is drawn at
//! one of them. Only that one stays laid out; the others keep their size,
//! which is all a measure needs (`docs/decisions/paragraph-cache-widths.md`).
#![deny(clippy::indexing_slicing)]

use gpui_kit::base::input::bidi_paragraph::BidiParagraph;
use gpui_kit::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::rc::Rc;

/// Paragraphs kept before old ones are dropped. The cache used to hold
/// 2,048 laid-out widths, about four per message paragraph: about 512
/// paragraphs.
const CAPACITY: usize = 512;

/// Widths a paragraph remembers its size at.
const SIZES: usize = 8;

/// Widths a paragraph stays laid out at once drawn: two, so a panel opened
/// and closed again finds both.
const DRAWN: usize = 2;

/// Everything but the width.
#[derive(Clone, PartialEq, Eq, Hash)]
struct Key {
    text: SharedString,
    runs: u64,
    font_size: u32,
    line_height: u32,
    one_line: bool,
}

/// The paragraph at one wrap width.
struct Width {
    wrap: Option<u32>,
    size: Size<Pixels>,
    /// Laid out: drawn at this width, or measured and not drawn yet.
    paragraph: Option<Rc<BidiParagraph>>,
    drawn: bool,
    used: u64,
}

#[derive(Default)]
struct Entry {
    widths: Vec<Width>,
    used: u64,
}

impl Entry {
    fn width(&mut self, wrap: Option<u32>, now: u64) -> Option<&mut Width> {
        self.used = now;
        let width = self.widths.iter_mut().find(|w| w.wrap == wrap)?;
        width.used = now;
        Some(width)
    }

    /// Adds `width`, forgetting the least recently used one past [`SIZES`].
    fn add(&mut self, width: Width) {
        self.used = width.used;
        self.widths.push(width);
        if self.widths.len() > SIZES
            && let Some(oldest) = self
                .widths
                .iter()
                .enumerate()
                .min_by_key(|(_, w)| w.used)
                .map(|(ix, _)| ix)
        {
            self.widths.remove(oldest);
        }
    }

    /// The paragraph was drawn at `wrap`: the widths it was only measured at
    /// let their layout go, and at most [`DRAWN`] drawn ones stay laid out.
    fn drawn_at(&mut self, wrap: Option<u32>) {
        for width in &mut self.widths {
            if width.wrap == wrap {
                width.drawn = true;
            } else if !width.drawn {
                width.paragraph = None;
            }
        }
        while self.widths.iter().filter(|w| w.drawn).count() > DRAWN {
            let Some(oldest) = self
                .widths
                .iter_mut()
                .filter(|w| w.drawn && w.wrap != wrap)
                .min_by_key(|w| w.used)
            else {
                break;
            };
            oldest.drawn = false;
            oldest.paragraph = None;
        }
    }
}

#[derive(Default)]
struct Cache {
    entries: HashMap<Key, Entry>,
    clock: u64,
    /// Paragraphs laid out, for tests.
    #[cfg(test)]
    layouts: u64,
}

impl Cache {
    fn tick(&mut self) -> u64 {
        self.clock += 1;
        self.clock
    }

    fn entry(&mut self, key: Key) -> &mut Entry {
        if !self.entries.contains_key(&key) && self.entries.len() >= CAPACITY {
            // Keep the most recently used half.
            let mut ages: Vec<u64> = self.entries.values().map(|e| e.used).collect();
            ages.sort_unstable();
            match ages.get(ages.len() / 2) {
                Some(&cutoff) => self.entries.retain(|_, e| e.used > cutoff),
                None => self.entries.clear(),
            }
        }
        self.entries.entry(key).or_default()
    }
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

fn key(
    text: &SharedString,
    runs: &[TextRun],
    font_size: Pixels,
    line_height: Pixels,
    one_line: bool,
) -> Key {
    Key {
        text: text.clone(),
        runs: runs_fingerprint(runs),
        font_size: f32::from(font_size).to_bits(),
        line_height: f32::from(line_height).to_bits(),
        one_line,
    }
}

fn wrap_bits(wrap: Option<Pixels>) -> Option<u32> {
    wrap.map(|w| f32::from(w).to_bits())
}

/// The size of the paragraph for this text and style at `wrap`, for a
/// layout measure.
pub(super) fn measure(
    text: &SharedString,
    runs: &[TextRun],
    font_size: Pixels,
    line_height: Pixels,
    wrap: Option<Pixels>,
    window: &mut Window,
) -> Option<Size<Pixels>> {
    measured(
        key(text, runs, font_size, line_height, false),
        wrap_bits(wrap),
        |window| BidiParagraph::layout(text, runs, font_size, line_height, wrap, window),
        window,
    )
}

/// The same for a one-line (elided) layout at `width`.
pub(super) fn measure_one_line(
    text: &SharedString,
    runs: &[TextRun],
    font_size: Pixels,
    line_height: Pixels,
    width: Pixels,
    window: &mut Window,
) -> Option<Size<Pixels>> {
    measured(
        key(text, runs, font_size, line_height, true),
        wrap_bits(Some(width)),
        |window| BidiParagraph::layout_one_line(text, runs, font_size, line_height, width, window),
        window,
    )
}

/// The paragraph for this text and style wrapped at `width`, to draw: laid
/// out once (or taken from the measure that laid it out) and then reused
/// until it falls out of the cache.
pub(super) fn layout(
    text: &SharedString,
    runs: &[TextRun],
    font_size: Pixels,
    line_height: Pixels,
    width: Pixels,
    window: &mut Window,
) -> Option<Rc<BidiParagraph>> {
    drawn(
        key(text, runs, font_size, line_height, false),
        wrap_bits(Some(width)),
        |window| BidiParagraph::layout(text, runs, font_size, line_height, Some(width), window),
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
    drawn(
        key(text, runs, font_size, line_height, true),
        wrap_bits(Some(width)),
        |window| BidiParagraph::layout_one_line(text, runs, font_size, line_height, width, window),
        window,
    )
}

fn measured(
    key: Key,
    wrap: Option<u32>,
    lay_out: impl FnOnce(&mut Window) -> Option<BidiParagraph>,
    window: &mut Window,
) -> Option<Size<Pixels>> {
    let hit = CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        let now = cache.tick();
        let entry = cache.entries.get_mut(&key)?;
        entry.width(wrap, now).map(|width| width.size)
    });
    if hit.is_some() {
        return hit;
    }
    let paragraph = lay_out(window)?;
    let size = paragraph.size();
    CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        #[cfg(test)]
        {
            cache.layouts += 1;
        }
        let now = cache.tick();
        cache.entry(key).add(Width {
            wrap,
            size,
            // Kept for the draw that usually follows in the same frame; a
            // paragraph is never drawn unwrapped.
            paragraph: wrap.is_some().then(|| Rc::new(paragraph)),
            drawn: false,
            used: now,
        });
    });
    Some(size)
}

fn drawn(
    key: Key,
    wrap: Option<u32>,
    lay_out: impl FnOnce(&mut Window) -> Option<BidiParagraph>,
    window: &mut Window,
) -> Option<Rc<BidiParagraph>> {
    let hit = CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        let now = cache.tick();
        let entry = cache.entries.get_mut(&key)?;
        let paragraph = entry.width(wrap, now)?.paragraph.clone()?;
        entry.drawn_at(wrap);
        Some(paragraph)
    });
    if hit.is_some() {
        return hit;
    }
    let paragraph = Rc::new(lay_out(window)?);
    CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        #[cfg(test)]
        {
            cache.layouts += 1;
        }
        let now = cache.tick();
        let entry = cache.entry(key);
        match entry.width(wrap, now) {
            Some(width) => width.paragraph = Some(paragraph.clone()),
            None => entry.add(Width {
                wrap,
                size: paragraph.size(),
                paragraph: Some(paragraph.clone()),
                drawn: false,
                used: now,
            }),
        }
        entry.drawn_at(wrap);
    });
    Some(paragraph)
}

/// What the cache holds, for tests.
#[cfg(test)]
#[derive(Debug)]
pub(super) struct Held {
    /// Paragraphs drawn at some width.
    pub(super) drawn: usize,
    /// Widths those paragraphs are laid out at.
    pub(super) laid_out: usize,
    /// Widths of every paragraph whose size is known.
    pub(super) widths: u64,
}

#[cfg(test)]
pub(super) fn held_in_tests() -> Held {
    CACHE.with(|cache| {
        let cache = cache.borrow();
        let drawn: Vec<&Entry> = cache
            .entries
            .values()
            .filter(|e| e.widths.iter().any(|w| w.drawn))
            .collect();
        Held {
            drawn: drawn.len(),
            laid_out: drawn
                .iter()
                .flat_map(|e| &e.widths)
                .filter(|w| w.paragraph.is_some())
                .count(),
            widths: cache.entries.values().map(|e| e.widths.len() as u64).sum(),
        }
    })
}

/// Paragraphs laid out on this thread since the last [`reset_in_tests`].
#[cfg(test)]
pub(super) fn layouts_in_tests() -> u64 {
    CACHE.with(|cache| cache.borrow().layouts)
}

/// Empties the cache, for a test that counts what it holds.
#[cfg(test)]
pub(super) fn reset_in_tests() {
    CACHE.with(|cache| *cache.borrow_mut() = Cache::default());
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
