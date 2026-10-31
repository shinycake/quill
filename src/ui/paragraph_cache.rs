//! Shared cache of laid-out message paragraphs.
//!
//! Message text is rebuilt every frame, and the layout engine measures each
//! paragraph several times per frame. Shaping is the expensive part
//! (CoreText on macOS), so a scroll that re-shapes every visible message
//! misses its frame budget. Paragraphs are kept here by everything that
//! changes their layout — the text, its styled runs, font size, line
//! height and wrap width — so a repeat measure or a scroll frame gets the
//! same paragraph back instead of shaping again.
//!
//! Layout asks for a message paragraph at about four widths (unconstrained,
//! its widest line, the room offered and the final width) and it is drawn at
//! one of them. Whatever a frame lays out stays until that frame ends; then
//! the widths only measured let their layouts go and keep their size, which
//! is all a measure needs. Widths drawn stay laid out, the least recently
//! used going first past the old cache's budget
//! (`docs/decisions/paragraph-cache-widths.md`).
#![deny(clippy::indexing_slicing)]

use gpui_kit::base::input::bidi_paragraph::BidiParagraph;
use gpui_kit::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::rc::Rc;

/// Wrapped paragraphs kept before old ones are dropped. The cache used to
/// hold 2,048 laid-out widths, about four per message paragraph: about 512.
const PARAGRAPHS: usize = 512;

/// One-line texts (chat list rows, quotes, names) kept, counted apart so a
/// long history doesn't push them out. The old widths held about 1,000 of
/// them (two each).
const LINES: usize = 1024;

/// Widths a paragraph remembers, laid out or not: a window or splitter
/// dragged across 30 or so widths and back finds every one.
const SIZES: usize = 64;

/// Laid-out widths each kind keeps once the frames that used them are over,
/// least recently used out first: the 2,048 the cache used to hold. A width
/// a wrapped paragraph was only measured at doesn't stay laid out past its
/// frame (see [`Entry::settle`]), so the budget goes to widths drawn.
const LAID_OUT: usize = 2048;

/// One frame of one window.
#[derive(Clone, Copy, PartialEq, Eq)]
struct Pass {
    window: WindowId,
    frame: u64,
}

impl Pass {
    fn of(window: &Window) -> Self {
        Self {
            window: window.window_handle().window_id(),
            frame: window.frame_number(),
        }
    }
}

/// Everything but the width.
#[derive(Clone, PartialEq, Eq, Hash)]
struct Key {
    text: SharedString,
    runs: u64,
    font_size: u32,
    line_height: u32,
}

/// The paragraph at one wrap width.
struct Width {
    wrap: Option<u32>,
    size: Size<Pixels>,
    paragraph: Option<Rc<BidiParagraph>>,
    /// The clock at its last draw, 0 if never drawn.
    drawn: u64,
    used: u64,
}

#[derive(Default)]
struct Entry {
    widths: Vec<Width>,
    /// The last width layout measured it at: the one layout settled on.
    measured: Option<u32>,
    used: u64,
}

impl Entry {
    fn width(&mut self, wrap: Option<u32>, now: u64) -> Option<&mut Width> {
        self.used = now;
        let width = self.widths.iter_mut().find(|w| w.wrap == wrap)?;
        width.used = now;
        Some(width)
    }

    /// Adds `width`, then forgets the least recently used widths past
    /// [`SIZES`], never one used in the frame begun at `start`.
    fn add(&mut self, width: Width, start: u64) {
        self.used = width.used;
        self.widths.push(width);
        while self.widths.len() > SIZES {
            let Some(oldest) = self
                .widths
                .iter()
                .enumerate()
                .filter(|(_, w)| w.used <= start)
                .min_by_key(|(_, w)| w.used)
                .map(|(ix, _)| ix)
            else {
                break;
            };
            self.widths.remove(oldest);
        }
    }

    /// The frame begun at `start`, which used this wrapped paragraph, is
    /// over. Widths it was only measured at let their layout go, unless the
    /// paragraph was not drawn in that frame: then the width layout settled
    /// on keeps it for the draw to come (a list measures rows just outside
    /// its viewport before they scroll in). Widths ever drawn keep theirs.
    fn settle(&mut self, start: u64) {
        let drawn_now = self.widths.iter().any(|w| w.drawn > start);
        for width in &mut self.widths {
            let waiting = !drawn_now && width.wrap.is_some() && width.wrap == self.measured;
            if width.drawn == 0 && !waiting {
                width.paragraph = None;
            }
        }
    }
}

/// Paragraphs of one kind: wrapped, or one-line.
struct Pool {
    entries: HashMap<Key, Entry>,
    /// Paragraphs kept.
    capacity: usize,
    /// Whether widths only measured let their layouts go when their frame
    /// ends. One-line texts keep them: most are short, a row measures one
    /// at the room it offers, draws it narrower and may draw it at that
    /// room later, and a quote drawn from a cached view's replay never
    /// reaches the cache.
    settles: bool,
    /// Laid-out widths at the last count plus those laid out since: never
    /// fewer than are held.
    laid_out: usize,
}

impl Pool {
    fn new(capacity: usize, settles: bool) -> Self {
        Self {
            entries: HashMap::new(),
            capacity,
            settles,
            laid_out: 0,
        }
    }

    fn entry(&mut self, key: Key, start: u64) -> &mut Entry {
        if !self.entries.contains_key(&key) && self.entries.len() >= self.capacity {
            // Keep the most recently used half, and whatever this frame used.
            let mut ages: Vec<u64> = self.entries.values().map(|e| e.used).collect();
            ages.sort_unstable();
            let cutoff = ages.get(ages.len() / 2).copied().unwrap_or(0).min(start);
            self.entries.retain(|_, e| e.used > cutoff);
        }
        self.entries.entry(key).or_default()
    }

    /// The frame begun at `start` is over: settle what it used, and keep
    /// the layouts within `budget`.
    fn end_frame(&mut self, start: u64, budget: usize) {
        if self.settles {
            for entry in self.entries.values_mut().filter(|e| e.used > start) {
                entry.settle(start);
            }
        }
        if self.laid_out > budget {
            self.trim(start, budget);
        }
    }

    /// Past `budget` laid-out widths, lets the least recently used ones go
    /// down to three quarters of it, never one used in the frame begun at
    /// `start`. Their sizes stay.
    fn trim(&mut self, start: u64, budget: usize) {
        let widths = || self.entries.values().flat_map(|e| &e.widths);
        let held = widths().filter(|w| w.paragraph.is_some()).count();
        self.laid_out = held;
        if held <= budget {
            return;
        }
        let excess = held - budget * 3 / 4;
        let mut ages: Vec<u64> = widths()
            .filter(|w| w.paragraph.is_some() && w.used <= start)
            .map(|w| w.used)
            .collect();
        let cutoff = match excess.checked_sub(1) {
            Some(ix) if ix < ages.len() => *ages.select_nth_unstable(ix).1,
            _ => ages.iter().copied().max().unwrap_or(0),
        };
        for width in self.entries.values_mut().flat_map(|e| &mut e.widths) {
            if width.used <= cutoff && width.paragraph.take().is_some() {
                self.laid_out -= 1;
            }
        }
    }
}

struct Cache {
    paragraphs: Pool,
    lines: Pool,
    clock: u64,
    /// The frame being drawn, and the clock when it began.
    pass: Option<(Pass, u64)>,
    /// Layouts made at each width, for tests.
    #[cfg(test)]
    laid: HashMap<(Key, bool, Option<u32>), u64>,
}

impl Default for Cache {
    fn default() -> Self {
        Self {
            paragraphs: Pool::new(PARAGRAPHS, true),
            lines: Pool::new(LINES, false),
            clock: 0,
            pass: None,
            #[cfg(test)]
            laid: HashMap::new(),
        }
    }
}

impl Cache {
    /// The clock for a request from `pass`. The first request of a frame
    /// ends the one before it.
    fn tick(&mut self, pass: Pass) -> u64 {
        if self.pass.map(|(p, _)| p) != Some(pass) {
            if let Some((_, start)) = self.pass {
                self.paragraphs.end_frame(start, LAID_OUT);
                self.lines.end_frame(start, LAID_OUT);
            }
            self.pass = Some((pass, self.clock));
        }
        self.clock += 1;
        self.clock
    }

    /// When the frame being drawn began.
    fn start(&self) -> u64 {
        self.pass.map_or(0, |(_, start)| start)
    }

    fn pool(&mut self, one_line: bool) -> &mut Pool {
        if one_line {
            &mut self.lines
        } else {
            &mut self.paragraphs
        }
    }

    /// The entry for `key`, after a layout was made for it.
    fn laid_out(&mut self, key: Key, one_line: bool) -> &mut Entry {
        let start = self.start();
        let pool = self.pool(one_line);
        pool.laid_out += 1;
        pool.entry(key, start)
    }

    #[cfg(test)]
    fn count_layout(&mut self, key: &Key, one_line: bool, wrap: Option<u32>) {
        *self.laid.entry((key.clone(), one_line, wrap)).or_default() += 1;
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

fn key(text: &SharedString, runs: &[TextRun], font_size: Pixels, line_height: Pixels) -> Key {
    Key {
        text: text.clone(),
        runs: runs_fingerprint(runs),
        font_size: f32::from(font_size).to_bits(),
        line_height: f32::from(line_height).to_bits(),
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
        key(text, runs, font_size, line_height),
        false,
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
        key(text, runs, font_size, line_height),
        true,
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
        key(text, runs, font_size, line_height),
        false,
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
        key(text, runs, font_size, line_height),
        true,
        wrap_bits(Some(width)),
        |window| BidiParagraph::layout_one_line(text, runs, font_size, line_height, width, window),
        window,
    )
}

fn measured(
    key: Key,
    one_line: bool,
    wrap: Option<u32>,
    lay_out: impl FnOnce(&mut Window) -> Option<BidiParagraph>,
    window: &mut Window,
) -> Option<Size<Pixels>> {
    let pass = Pass::of(window);
    let hit = CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        let now = cache.tick(pass);
        let entry = cache.pool(one_line).entries.get_mut(&key)?;
        if wrap.is_some() {
            entry.measured = wrap;
        }
        entry.width(wrap, now).map(|width| width.size)
    });
    if hit.is_some() {
        return hit;
    }
    let paragraph = lay_out(window)?;
    let size = paragraph.size();
    CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        let now = cache.tick(pass);
        #[cfg(test)]
        cache.count_layout(&key, one_line, wrap);
        let start = cache.start();
        let entry = cache.laid_out(key, one_line);
        if wrap.is_some() {
            entry.measured = wrap;
        }
        entry.add(
            Width {
                wrap,
                size,
                // Kept for the draw that usually follows; a paragraph is
                // never drawn unwrapped.
                paragraph: wrap.is_some().then(|| Rc::new(paragraph)),
                drawn: 0,
                used: now,
            },
            start,
        );
    });
    Some(size)
}

fn drawn(
    key: Key,
    one_line: bool,
    wrap: Option<u32>,
    lay_out: impl FnOnce(&mut Window) -> Option<BidiParagraph>,
    window: &mut Window,
) -> Option<Rc<BidiParagraph>> {
    let pass = Pass::of(window);
    let hit = CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        let now = cache.tick(pass);
        let width = cache
            .pool(one_line)
            .entries
            .get_mut(&key)?
            .width(wrap, now)?;
        let paragraph = width.paragraph.clone()?;
        width.drawn = now;
        Some(paragraph)
    });
    if hit.is_some() {
        return hit;
    }
    let paragraph = Rc::new(lay_out(window)?);
    CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        let now = cache.tick(pass);
        #[cfg(test)]
        cache.count_layout(&key, one_line, wrap);
        let start = cache.start();
        let entry = cache.laid_out(key, one_line);
        match entry.width(wrap, now) {
            // Laid out again (its layout went, or the fonts it uses loaded
            // since): the size may have changed too.
            Some(width) => {
                width.size = paragraph.size();
                width.paragraph = Some(paragraph.clone());
                width.drawn = now;
            }
            None => entry.add(
                Width {
                    wrap,
                    size: paragraph.size(),
                    paragraph: Some(paragraph.clone()),
                    drawn: now,
                    used: now,
                },
                start,
            ),
        }
    });
    Some(paragraph)
}

/// What the cache holds of wrapped paragraphs or of one-line texts, for
/// tests.
#[cfg(test)]
#[derive(Debug, Default)]
pub(super) struct Held {
    /// Paragraphs drawn at some width.
    pub(super) drawn: usize,
    /// Widths those paragraphs are laid out at.
    pub(super) laid_out: usize,
    /// The most widths only measured, never drawn, one of those
    /// paragraphs is laid out at.
    pub(super) most_measured_only: usize,
    /// Paragraphs measured and never drawn.
    pub(super) waiting: usize,
    /// Widths those are laid out at.
    pub(super) waiting_laid_out: usize,
}

#[cfg(test)]
pub(super) fn held_in_tests(one_line: bool) -> Held {
    CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        let mut held = Held::default();
        for entry in cache.pool(one_line).entries.values() {
            let laid_out = entry
                .widths
                .iter()
                .filter(|w| w.paragraph.is_some())
                .count();
            if entry.widths.iter().any(|w| w.drawn > 0) {
                held.drawn += 1;
                held.laid_out += laid_out;
                let measured_only = entry
                    .widths
                    .iter()
                    .filter(|w| w.paragraph.is_some() && w.drawn == 0)
                    .count();
                held.most_measured_only = held.most_measured_only.max(measured_only);
            } else {
                held.waiting += 1;
                held.waiting_laid_out += laid_out;
            }
        }
        held
    })
}

/// Paragraphs laid out on this thread since the last [`reset_in_tests`].
#[cfg(test)]
pub(super) fn layouts_in_tests() -> u64 {
    CACHE.with(|cache| cache.borrow().laid.values().sum())
}

/// Layouts of a paragraph at a width it was laid out at before.
#[cfg(test)]
pub(super) fn relayouts_in_tests() -> u64 {
    CACHE.with(|cache| cache.borrow().laid.values().map(|n| n - 1).sum())
}

/// Empties the cache, for a test that counts what it holds.
#[cfg(test)]
pub(super) fn reset_in_tests() {
    CACHE.with(|cache| *cache.borrow_mut() = Cache::default());
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "paragraph_cache_tests.rs"]
mod widths_tests;

#[cfg(test)]
mod tests {
    use super::{Entry, SIZES, Width, runs_fingerprint};
    use gpui_kit::{Hsla, Size, TextRun, font};

    /// A frame may use more widths than a paragraph remembers; the next
    /// width added after it trims back to [`SIZES`], oldest first.
    #[test]
    fn a_paragraph_trims_to_its_widths_once_their_frame_is_over() {
        let width = |wrap: u64| Width {
            wrap: Some(wrap as u32),
            size: Size::default(),
            paragraph: None,
            drawn: 0,
            used: wrap,
        };
        let mut entry = Entry::default();
        let many = SIZES as u64 + 5;
        // One frame, begun at clock 0, uses them all: all stay.
        for wrap in 1..=many {
            entry.add(width(wrap), 0);
        }
        assert_eq!(entry.widths.len(), SIZES + 5);
        // The next frame begins.
        entry.add(width(many + 1), many);
        assert_eq!(entry.widths.len(), SIZES);
        let oldest = entry.widths.iter().map(|w| w.used).min();
        assert_eq!(oldest, Some(7), "the six oldest went");
    }

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
