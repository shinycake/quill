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
//! its widest line, the room offered and a narrower one) and it is drawn at
//! the room, or at its widest line when that fits. Unconstrained, it keeps
//! only its size, which is all a measure needs; so does its widest line
//! once a frame that never drew it ends, if that is wider than the window,
//! where nothing is drawn. Every other width stays laid out, the least
//! recently used going first past the old cache's budget
//! (`docs/decisions/paragraph-cache-widths.md`).
#![deny(clippy::indexing_slicing)]

use gpui_kit::base::input::bidi_paragraph::BidiParagraph;
use gpui_kit::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::rc::Rc;

/// Texts each kind keeps (wrapped paragraphs, and one-line texts such as
/// chat list rows, quotes and names, counted apart so a long history
/// doesn't push the chat list out): as many as the 2,048 widths the cache
/// used to hold could be, one each. The layout and size budgets bound what
/// they hold.
const TEXTS: usize = 2048;

/// Laid-out widths both kinds keep together once the frames that used them
/// are over, least recently used out first: the 2,048 the cache used to
/// hold. Layouts at a wrapped paragraph's widest line and wider than the
/// window, never drawn, don't stay past their frame (see
/// [`Entry::settle`]), so none of it goes to them.
const LAID_OUT: usize = 2048;

/// Widths each kind remembers, laid out or keeping only a size (about 40
/// bytes), least recently used sizes out first: four times the entries the
/// cache used to hold, so it still knows every width the old cache did.
const WIDTHS: usize = 8192;

/// One frame of one window.
#[derive(Clone, Copy)]
struct Pass {
    window: WindowId,
    frame: u64,
    /// How wide the window is: none of its frames draws text wider.
    width: f32,
}

impl Pass {
    fn of(window: &Window) -> Self {
        Self {
            window: window.window_handle().window_id(),
            frame: window.frame_number(),
            width: f32::from(window.viewport_size().width),
        }
    }

    fn same_frame(self, other: Self) -> bool {
        self.window == other.window && self.frame == other.frame
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
    used: u64,
}

impl Entry {
    fn width(&mut self, wrap: Option<u32>, now: u64) -> Option<&mut Width> {
        self.used = now;
        let width = self.widths.iter_mut().find(|w| w.wrap == wrap)?;
        width.used = now;
        Some(width)
    }

    fn add(&mut self, width: Width) {
        self.used = width.used;
        self.widths.push(width);
    }

    /// The width of its widest line unwrapped, once measured so.
    fn natural(&self) -> Option<f32> {
        let whole = self.widths.iter().find(|w| w.wrap.is_none())?;
        Some(f32::from(whole.size.width))
    }

    /// A frame of a window `window` wide that used this wrapped paragraph
    /// is over: layouts at its widest line or wider, never drawn, let go
    /// when that is wider than the window. Every other layout stays.
    fn settle(&mut self, window: f32) {
        let natural = self.natural();
        for width in &mut self.widths {
            if width.drawn == 0 && unwrapped(width.wrap, natural) && wider(width.wrap, window) {
                width.paragraph = None;
            }
        }
    }
}

/// Whether a paragraph `natural` wide unwrapped is one line a row at
/// `wrap`, as unconstrained: `wrap` is its widest line (which taffy
/// measures at, give or take a pixel) or wider.
fn unwrapped(wrap: Option<u32>, natural: Option<f32>) -> bool {
    match (wrap, natural) {
        (None, _) => true,
        (Some(wrap), Some(natural)) => f32::from_bits(wrap) + 1. >= natural,
        (Some(_), None) => false,
    }
}

/// Whether `wrap` is wider than a window `window` wide, where no frame of
/// it draws: a long paragraph's widest line, while a shorter one's may yet
/// be drawn, when its box grows to fit it.
fn wider(wrap: Option<u32>, window: f32) -> bool {
    wrap.is_none_or(|wrap| f32::from_bits(wrap) > window)
}

/// Paragraphs of one kind: wrapped, or one-line.
struct Pool {
    entries: HashMap<Key, Entry>,
    /// Paragraphs kept: up to an eighth more before the least recently used
    /// go, so a new one doesn't search them all.
    capacity: usize,
    /// Whether layouts at a paragraph's widest line or wider, never drawn,
    /// let go when their frame ends ([`Entry::settle`]). One-line texts
    /// keep them: a line drawn whole when it fits is laid out at its
    /// widest, and a quote drawn from a cached view's replay never reaches
    /// the cache.
    settles: bool,
    /// Laid-out widths at the last count plus those laid out since: never
    /// fewer than are held.
    laid_out: usize,
    /// Widths, laid out or not, counted the same way.
    widths: usize,
}

impl Pool {
    fn new(capacity: usize, settles: bool) -> Self {
        Self {
            entries: HashMap::new(),
            capacity,
            settles,
            laid_out: 0,
            widths: 0,
        }
    }

    /// The entry for `key`. A new paragraph past [`Self::capacity`] and
    /// its slack lets the least recently used go, down to the capacity with
    /// it, never one used in the frame begun at `start`.
    fn entry(&mut self, key: Key, start: u64) -> &mut Entry {
        let full = self.entries.len() >= self.capacity + self.capacity / 8;
        if full && !self.entries.contains_key(&key) {
            let excess = self.entries.len() + 1 - self.capacity;
            let ages = self
                .entries
                .values()
                .map(|e| e.used)
                .filter(|used| *used <= start)
                .collect();
            if let Some(cutoff) = cutoff(excess, ages) {
                self.entries.retain(|_, e| e.used > cutoff);
            }
        }
        self.entries.entry(key).or_default()
    }

    /// The frame begun at `start`, of a window `window` wide, is over:
    /// settle what it used.
    fn settle(&mut self, start: u64, window: f32) {
        if self.settles {
            for entry in self.entries.values_mut().filter(|e| e.used > start) {
                entry.settle(window);
            }
        }
    }

    /// Past `budget` widths, forgets the least recently used of those that
    /// keep only a size, down to three quarters of it, never one used in
    /// the frame begun at `start`.
    fn trim_widths(&mut self, start: u64, budget: usize) {
        let held = self.count(|_| true);
        self.widths = held;
        let size_only = |w: &Width| w.paragraph.is_none();
        let ages = self.ages(start, size_only);
        if let Some(cutoff) = excess(held, budget).and_then(|excess| cutoff(excess, ages)) {
            for entry in self.entries.values_mut() {
                entry
                    .widths
                    .retain(|w| w.paragraph.is_some() || w.used > cutoff);
            }
            self.entries.retain(|_, e| !e.widths.is_empty());
            self.widths = self.count(|_| true);
        }
    }

    fn count(&self, which: impl Fn(&Width) -> bool) -> usize {
        self.entries
            .values()
            .flat_map(|e| &e.widths)
            .filter(|w| which(w))
            .count()
    }

    /// When each width `which` picks was last used, but those used in the
    /// frame begun at `start`.
    fn ages(&self, start: u64, which: impl Fn(&Width) -> bool) -> Vec<u64> {
        self.entries
            .values()
            .flat_map(|e| &e.widths)
            .filter(|w| which(w) && w.used <= start)
            .map(|w| w.used)
            .collect()
    }
}

/// Past `budget` laid-out widths in `pools` together, lets the least
/// recently used go down to three quarters of it, never one used in the
/// frame begun at `start`, as the old cache's one budget did. Their sizes
/// stay.
fn trim_layouts(pools: &mut [&mut Pool], start: u64, budget: usize) {
    let laid_out = |w: &Width| w.paragraph.is_some();
    let held = pools.iter().map(|pool| pool.count(laid_out)).sum();
    let ages = pools
        .iter()
        .flat_map(|pool| pool.ages(start, laid_out))
        .collect();
    let cutoff = excess(held, budget).and_then(|excess| cutoff(excess, ages));
    for pool in pools.iter_mut() {
        if let Some(cutoff) = cutoff {
            for width in pool.entries.values_mut().flat_map(|e| &mut e.widths) {
                if width.used <= cutoff {
                    width.paragraph = None;
                }
            }
        }
        pool.laid_out = pool.count(laid_out);
    }
}

/// How many of `held` go to bring it down to three quarters of `budget`;
/// none while within it.
fn excess(held: usize, budget: usize) -> Option<usize> {
    (held > budget).then(|| held - budget * 3 / 4)
}

/// The last use at or before which `excess` of the uses in `ages` go, all
/// of them if fewer; none if `excess` is 0.
fn cutoff(excess: usize, mut ages: Vec<u64>) -> Option<u64> {
    let ix = excess.checked_sub(1)?;
    if ix < ages.len() {
        Some(*ages.select_nth_unstable(ix).1)
    } else {
        ages.into_iter().max()
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
            paragraphs: Pool::new(TEXTS, true),
            lines: Pool::new(TEXTS, false),
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
        if !self.pass.is_some_and(|(p, _)| p.same_frame(pass)) {
            if let Some((ended, start)) = self.pass {
                self.end_frame(start, ended.width);
            }
            self.pass = Some((pass, self.clock));
        }
        self.clock += 1;
        self.clock
    }

    /// The frame begun at `start`, of a window `window` wide, is over:
    /// settle what it used, and keep to the budgets.
    fn end_frame(&mut self, start: u64, window: f32) {
        self.paragraphs.settle(start, window);
        self.lines.settle(start, window);
        if self.paragraphs.laid_out + self.lines.laid_out > LAID_OUT {
            trim_layouts(
                &mut [&mut self.paragraphs, &mut self.lines],
                start,
                LAID_OUT,
            );
        }
        for pool in [&mut self.paragraphs, &mut self.lines] {
            if pool.widths > WIDTHS {
                pool.trim_widths(start, WIDTHS);
            }
        }
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
        pool.widths += 1;
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
        cache.laid_out(key, one_line).add(Width {
            wrap,
            size,
            // Kept for the draw that usually follows; a paragraph is never
            // drawn unconstrained.
            paragraph: wrap.is_some().then(|| Rc::new(paragraph)),
            drawn: 0,
            used: now,
        });
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
        let entry = cache.laid_out(key, one_line);
        match entry.width(wrap, now) {
            // Laid out again (its layout went, or the fonts it uses loaded
            // since): the size may have changed too.
            Some(width) => {
                width.size = paragraph.size();
                width.paragraph = Some(paragraph.clone());
                width.drawn = now;
            }
            None => entry.add(Width {
                wrap,
                size: paragraph.size(),
                paragraph: Some(paragraph.clone()),
                drawn: now,
                used: now,
            }),
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
    /// Paragraphs measured and never drawn.
    pub(super) waiting: usize,
    /// Widths those are laid out at.
    pub(super) waiting_laid_out: usize,
    /// Layouts, in any paragraph, at its widest line or wider and never
    /// drawn.
    pub(super) unwrapped_undrawn: usize,
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
            } else {
                held.waiting += 1;
                held.waiting_laid_out += laid_out;
            }
            let natural = entry.natural();
            held.unwrapped_undrawn += entry
                .widths
                .iter()
                .filter(|w| w.paragraph.is_some() && w.drawn == 0 && unwrapped(w.wrap, natural))
                .count();
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
