// Added by the Quill project (2026) to gpui-base (Apache-2.0). See
// third_party/gpui-base/QUILL-CHANGES.md.
//! Soft wrap of one logical line, one row at a time.
//!
//! A row is a pure function of where it starts and of the text after that. So
//! an edit wraps again from just before itself and stops at the first row start
//! it shares with the old wrap: everything past that point is the old wrap,
//! moved by the length of the edit.
//!
//! Each row looks at a window of text around its expected end, seeded by the
//! summed widths of its characters, and shapes only to settle the last few
//! characters. A line of [`LONG_LINE`] bytes or more, or any line of a text of
//! [`LONG_TEXT`] bytes or more, trusts the sum and shapes nothing, so even a tail
//! that never settles costs a table lookup per character.

use std::ops::Range;
use std::rc::Rc;
use std::sync::Arc;

use gpui::{
    Boundary, Font, LineWrapper, LineWrapperHandle, Pixels, TextSystem, WindowTextSystem, px,
};
use unicode_segmentation::UnicodeSegmentation as _;

use super::text_wrapper::WrappingIndent;
use crate::input::{
    tab_stops::expanded_width,
    text_spans::{FontChange, SpanFonts},
};

/// The line length, in bytes, from which wrapping stops shaping text.
pub(crate) const LONG_LINE: usize = 16 * 1024;

/// The text length, in bytes, from which wrapping stops shaping any line. Many
/// short lines cost a shaped measure each, however few of them are on screen.
pub(crate) const LONG_TEXT: usize = 256 * 1024;

/// Whether a text of `len` bytes wraps without shaping.
pub(crate) fn is_long_text(len: usize) -> bool {
    len >= LONG_TEXT
}

/// Text this close before an edit can still decide a row: a grapheme or a line
/// break opportunity looks a few characters ahead.
const EDIT_CONTEXT: usize = 32;

/// Bytes past the expected end of a row that the row's window holds, so a row
/// that fits more than its seed says rarely needs a second window.
const WINDOW_SLACK: usize = 64;

/// Bytes of a line [`TextMetrics`] takes the font runs of at once.
const RUN_CHUNK: usize = 4096;

/// The widths wrapping needs, over byte ranges of one line.
pub(crate) trait LineWidths {
    /// The width of a range as it is painted.
    fn measure(&mut self, range: Range<usize>) -> Pixels;

    /// The offset of the first character past `start` that no longer fits in
    /// `available` by summed character widths, or the end of the line.
    fn overflow(&mut self, start: usize, available: Pixels) -> usize;
}

/// The old wrap of a line, and the one edit inside it.
pub(crate) struct Resume<'a> {
    rows: &'a [Range<usize>],
    edit_start: usize,
    edit_end: usize,
    delta: isize,
}

impl<'a> Resume<'a> {
    /// `None` when the edit can move rows the old wrap says nothing about: one
    /// that reaches the indent of the line or takes it across [`LONG_LINE`].
    pub(crate) fn of(
        rows: &'a [Range<usize>],
        indent: usize,
        edit: Range<usize>,
        inserted: usize,
        new_len: usize,
    ) -> Option<Self> {
        let old_len = rows.last()?.end;
        let delta = inserted as isize - edit.len() as isize;
        let usable = edit.start > indent
            && (old_len >= LONG_LINE) == (new_len >= LONG_LINE)
            && old_len.checked_add_signed(delta) == Some(new_len);
        usable.then_some(Self {
            rows,
            edit_start: edit.start,
            edit_end: edit.start + inserted,
            delta,
        })
    }

    /// How many old rows stand. A row looks past its own end as far as the word
    /// that did not fit, and that word never reaches past the next row.
    fn kept_rows(&self) -> usize {
        self.rows
            .partition_point(|row| row.end + EDIT_CONTEXT < self.edit_start)
            .saturating_sub(1)
    }

    /// The old row ends past a new row start, moved onto the new text, once that
    /// start is past the edit and the old wrap also had a row start there.
    fn settled(&self, start: usize) -> Option<impl Iterator<Item = usize> + 'a> {
        if start < self.edit_end {
            return None;
        }
        let old_start = start.checked_add_signed(-self.delta)?;
        let row = self
            .rows
            .binary_search_by_key(&old_start, |row| row.start)
            .ok()?;
        let delta = self.delta;
        let closed = self.rows.get(row..self.rows.len().saturating_sub(1))?;
        Some(closed.iter().map(move |row| row.end.saturating_add_signed(delta)))
    }
}

/// The wrap boundaries of `text`. A line whose rows all come from `resume`
/// measures nothing past the edit.
pub(crate) fn wrap_rows(
    text: &str,
    width: Pixels,
    wrapping_indent: WrappingIndent,
    atoms: &[Range<usize>],
    resume: Option<&Resume>,
    widths: &mut impl LineWidths,
    long_text: bool,
) -> Vec<Boundary> {
    let indent = match wrapping_indent {
        WrappingIndent::Same => text
            .chars()
            .take_while(|&c| c == ' ')
            .count()
            .min(LineWrapper::MAX_INDENT as usize),
        WrappingIndent::None => 0,
    };
    let indent_width = if indent == 0 {
        px(0.)
    } else {
        widths.measure(0..indent)
    };
    let next_indent = indent as u32;
    let boundary = |ix| Boundary { ix, next_indent };

    let kept = resume.map_or(&[][..], |resume| {
        resume.rows.get(..resume.kept_rows()).unwrap_or_default()
    });
    let mut boundaries: Vec<Boundary> = kept.iter().map(|row| boundary(row.end)).collect();
    let mut start = kept.last().map_or(0, |row| row.end);
    let mut rows = RowEnds {
        text,
        width,
        indent,
        indent_width,
        atoms,
        long: long_text || text.len() >= LONG_LINE,
        widths,
        ends: Vec::new(),
    };
    while let Some(end) = rows.row_end(start) {
        boundaries.push(boundary(end));
        start = end;
        if let Some(rest) = resume.and_then(|resume| resume.settled(start)) {
            boundaries.extend(rest.map(boundary));
            break;
        }
    }
    boundaries
}

/// Finds where the row starting at an offset ends.
struct RowEnds<'a, W> {
    text: &'a str,
    width: Pixels,
    indent: usize,
    indent_width: Pixels,
    atoms: &'a [Range<usize>],
    long: bool,
    widths: &'a mut W,
    /// The grapheme ends of the current window that a row may end at.
    ends: Vec<usize>,
}

impl<W: LineWidths> RowEnds<'_, W> {
    /// The end of the row at `start`, or `None` when the rest of the line fits.
    fn row_end(&mut self, start: usize) -> Option<usize> {
        let available = if start == 0 {
            self.width
        } else {
            self.width - self.indent_width
        };
        let guess = self.widths.overflow(start, available);
        let len = self.text.len();
        if self.long && guess >= len {
            return None;
        }
        let span = guess.saturating_sub(start);
        let mut reach = span + span / 4 + WINDOW_SLACK;
        loop {
            let window_end = ceil_char_boundary(self.text, start.saturating_add(reach));
            let window = self.text.get(start..window_end)?;
            let whole = window_end >= len;
            self.collect_ends(start, window, whole);
            let fit = self.fitting_graphemes(start, available, guess);
            if fit >= self.ends.len() && !whole {
                reach = reach.saturating_mul(2);
                continue;
            }
            // An indivisible grapheme wider than the viewport still takes a row.
            let fitting_end = *self.ends.get(fit.max(1) - 1)?;
            if fitting_end >= len {
                return None;
            }
            let opportunity = unicode_linebreak::linebreaks(window)
                .map(|(ix, _)| start + ix)
                .take_while(|&ix| ix <= fitting_end)
                .filter(|&ix| ix > start && (start != 0 || ix > self.indent))
                .filter(|ix| self.ends.binary_search(ix).is_ok())
                .last();
            return Some(opportunity.unwrap_or(fitting_end));
        }
    }

    fn collect_ends(&mut self, start: usize, window: &str, whole: bool) {
        let first = self.atoms.partition_point(|atom| atom.end <= start);
        let atoms = self.atoms.get(first..).unwrap_or_default();
        self.ends.clear();
        self.ends.extend(
            window
                .grapheme_indices(true)
                .map(|(ix, grapheme)| start + ix + grapheme.len())
                .filter(|&end| {
                    !atoms
                        .iter()
                        .take_while(|atom| atom.start < end)
                        .any(|atom| end < atom.end)
                }),
        );
        // A cut window can end inside a grapheme.
        if !whole {
            self.ends.pop();
        }
    }

    /// How many graphemes of the window fit. Painted widths and summed ones
    /// differ by a few characters, so the search gallops out from the seed
    /// instead of probing up from the start of the row.
    fn fitting_graphemes(&mut self, start: usize, available: Pixels, guess: usize) -> usize {
        let seed = self.ends.partition_point(|&end| end <= guess);
        if self.long {
            return seed;
        }
        let ends = &self.ends;
        let widths = &mut *self.widths;
        let mut fits = |count: usize| {
            ends.get(count.wrapping_sub(1))
                .is_some_and(|&end| widths.measure(start..end) <= available)
        };
        let count = ends.len();
        let unknown = count + 1;
        let (mut low, mut high) = (0, unknown);
        let mut probe = seed.clamp(1, count.max(1));
        let mut step = 1;
        while low + 1 < high {
            if fits(probe) {
                low = probe;
            } else {
                high = probe;
            }
            probe = if high == unknown {
                (low + step).min(count)
            } else if low == 0 {
                high.saturating_sub(step).max(1)
            } else {
                (low + high) / 2
            };
            step *= 2;
        }
        low
    }
}

fn ceil_char_boundary(text: &str, ix: usize) -> usize {
    (ix..text.len())
        .find(|&ix| text.is_char_boundary(ix))
        .unwrap_or(text.len())
}

/// The text runs of the byte range `range` of a line starting at `line_start`,
/// with the font changes of the formatting spans applied (Quill patch).
fn styled_runs(
    font: &Font,
    line_start: usize,
    range: Range<usize>,
    fonts: &[(Range<usize>, FontChange)],
) -> Vec<gpui::TextRun> {
    let abs = line_start + range.start..line_start + range.end;
    let mut cuts = vec![abs.start, abs.end];
    for (r, _) in fonts {
        if r.start < abs.end && abs.start < r.end {
            cuts.push(r.start.clamp(abs.start, abs.end));
            cuts.push(r.end.clamp(abs.start, abs.end));
        }
    }
    cuts.sort_unstable();
    cuts.dedup();
    cuts.windows(2)
        .map(|w| {
            let mut run_font = font.clone();
            for (r, change) in fonts {
                if r.start <= w[0] && w[0] < r.end {
                    change.apply(&mut run_font);
                }
            }
            gpui::TextRun {
                len: w[1] - w[0],
                font: run_font,
                color: gpui::black(),
                background_color: None,
                underline: None,
                strikethrough: None,
            }
        })
        .collect()
}

/// What the lines of one wrapper update are measured with.
pub(crate) struct TextMetrics {
    font: Font,
    font_size: Pixels,
    tab_stop: Pixels,
    tokens: Rc<[(Range<usize>, Pixels)]>,
    fonts: SpanFonts,
    shaper: WindowTextSystem,
    system: Arc<TextSystem>,
    wrappers: Vec<(Font, LineWrapperHandle)>,
}

impl TextMetrics {
    pub(crate) fn new(
        font: Font,
        font_size: Pixels,
        tab_stop: Pixels,
        tokens: Rc<[(Range<usize>, Pixels)]>,
        fonts: SpanFonts,
        system: Arc<TextSystem>,
    ) -> Self {
        Self {
            font,
            font_size,
            tab_stop,
            tokens,
            fonts,
            shaper: WindowTextSystem::new(system.clone()),
            system,
            wrappers: Vec::new(),
        }
    }

    /// The widths of the line at `line_start`, with its tokens relative to it.
    pub(crate) fn line<'a>(&'a mut self, line: &'a str, line_start: usize) -> LineMetrics<'a> {
        let line_end = line_start + line.len();
        let first = self.tokens.partition_point(|(r, _)| r.end <= line_start);
        let tokens = self
            .tokens
            .get(first..)
            .unwrap_or_default()
            .iter()
            .take_while(|(r, _)| r.start < line_end)
            .filter(|(r, _)| line_start <= r.start && r.end <= line_end)
            .map(|(r, width)| (r.start - line_start..r.end - line_start, *width))
            .filter(|(r, _)| line.is_char_boundary(r.start) && line.is_char_boundary(r.end))
            .collect();
        LineMetrics {
            text: self,
            line,
            line_start,
            tokens,
        }
    }

    fn wrapper(&mut self, font: &Font) -> Option<&mut LineWrapperHandle> {
        let found = self.wrappers.iter().position(|(known, _)| known == font);
        let ix = found.unwrap_or_else(|| {
            let wrapper = self.system.line_wrapper(font.clone(), self.font_size);
            self.wrappers.push((font.clone(), wrapper));
            self.wrappers.len() - 1
        });
        self.wrappers.get_mut(ix).map(|(_, wrapper)| wrapper)
    }
}

/// The widths of one line: its text in the fonts of its spans, tabs out to their
/// stops, and inline tokens by their measured width.
pub(crate) struct LineMetrics<'a> {
    text: &'a mut TextMetrics,
    line: &'a str,
    line_start: usize,
    tokens: Vec<(Range<usize>, Pixels)>,
}

impl LineMetrics<'_> {
    /// The ranges of the tokens, which a row never splits.
    pub(crate) fn atoms(&self) -> Vec<Range<usize>> {
        self.tokens.iter().map(|(r, _)| r.clone()).collect()
    }

    fn shaped_width(&self, range: Range<usize>) -> Pixels {
        let Some(text) = self.line.get(range.clone()).filter(|t| !t.is_empty()) else {
            return px(0.);
        };
        let TextMetrics {
            font,
            font_size,
            tab_stop,
            fonts,
            shaper,
            ..
        } = &*self.text;
        let runs = styled_runs(font, self.line_start, range, fonts);
        let layout = shaper.layout_line(text, *font_size, &runs, None);
        expanded_width(&layout, text, *tab_stop)
    }
}

impl LineWidths for LineMetrics<'_> {
    fn measure(&mut self, range: Range<usize>) -> Pixels {
        let mut width = px(0.);
        let mut at = range.start;
        for (token, token_width) in &self.tokens {
            if token.end <= range.start || token.start >= range.end {
                continue;
            }
            if at < token.start {
                width += self.shaped_width(at..token.start);
            }
            width += *token_width;
            at = token.end;
        }
        if at < range.end {
            width += self.shaped_width(at..range.end);
        }
        width
    }

    fn overflow(&mut self, start: usize, available: Pixels) -> usize {
        let first = self.tokens.partition_point(|(r, _)| r.end <= start);
        let mut tokens = self.tokens.get(first..).unwrap_or_default().iter().peekable();
        let tab_stop = self.text.tab_stop;
        let mut x = px(0.);
        let mut at = start;
        let mut past_token = start;
        while at < self.line.len() {
            let limit = ceil_char_boundary(self.line, at.saturating_add(RUN_CHUNK));
            let runs = styled_runs(&self.text.font, self.line_start, at..limit, &self.text.fonts);
            for run in runs {
                let Some(wrapper) = self.text.wrapper(&run.font) else {
                    return at;
                };
                let text = self.line.get(at..at + run.len).unwrap_or_default();
                for (ix, c) in text.char_indices() {
                    let offset = at + ix;
                    if offset < past_token {
                        continue;
                    }
                    if let Some((token, width)) = tokens.next_if(|(r, _)| r.start == offset) {
                        past_token = token.end;
                        x += *width;
                    } else if c == '\t' && tab_stop > px(0.) {
                        x = tab_stop * ((x / tab_stop).floor() + 1.);
                    } else {
                        x += wrapper.width_for_char(c);
                    }
                    if x > available {
                        return offset;
                    }
                }
                at += run.len;
            }
        }
        self.line.len()
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Test widths over a plain measure: `overflow` says nothing, so a row
    /// probes up from its first grapheme.
    struct Measured<'a, F> {
        text: &'a str,
        measure: F,
    }

    impl<F: FnMut(&str) -> Pixels> LineWidths for Measured<'_, F> {
        fn measure(&mut self, range: Range<usize>) -> Pixels {
            (self.measure)(&self.text[range])
        }

        fn overflow(&mut self, start: usize, _: Pixels) -> usize {
            start
        }
    }

    pub(crate) fn measured_wrap_boundaries(
        text: &str,
        width: Pixels,
        wrapping_indent: WrappingIndent,
        measure: impl FnMut(&str) -> Pixels,
    ) -> Vec<Boundary> {
        let mut widths = Measured { text, measure };
        wrap_rows(text, width, wrapping_indent, &[], None, &mut widths, false)
    }

    fn char_width(c: char) -> f32 {
        match c {
            'i' | 'l' | ' ' => 3.,
            '\u{300}'..='\u{36f}' | '\u{200d}' => 0.,
            c if c.is_ascii() => 7.,
            c if (c as u32) < 0x800 => 6.,
            _ => 13.,
        }
    }

    /// Widths that behave like a shaper: pairs kern, so the painted width sits a
    /// little under the sum, and every byte handed to either call is counted.
    struct Counted<'a> {
        text: &'a str,
        measured: usize,
        walked: usize,
    }

    impl<'a> Counted<'a> {
        fn new(text: &'a str) -> Self {
            Self {
                text,
                measured: 0,
                walked: 0,
            }
        }
    }

    impl LineWidths for Counted<'_> {
        fn measure(&mut self, range: Range<usize>) -> Pixels {
            self.measured += range.len();
            let text = &self.text[range];
            let sum: f32 = text.chars().map(char_width).sum();
            let kerned = text.as_bytes().windows(2).filter(|p| p == b"Te").count();
            px(sum - kerned as f32)
        }

        fn overflow(&mut self, start: usize, available: Pixels) -> usize {
            let mut x = 0.;
            for (ix, c) in self.text[start..].char_indices() {
                self.walked += c.len_utf8();
                x += char_width(c);
                if x > f32::from(available) {
                    return start + ix;
                }
            }
            self.text.len()
        }
    }

    fn rows_of(boundaries: &[Boundary], len: usize) -> Vec<Range<usize>> {
        let mut start = 0;
        let mut rows = Vec::new();
        for boundary in boundaries {
            rows.push(start..boundary.ix);
            start = boundary.ix;
        }
        rows.push(start..len);
        rows
    }

    fn fresh(text: &str, width: f32, indent: WrappingIndent) -> Vec<Boundary> {
        wrap_rows(text, px(width), indent, &[], None, &mut Counted::new(text), false)
    }

    /// A small deterministic generator, so a failure names a seed.
    struct Lcg(u64);

    impl Lcg {
        fn next(&mut self, below: usize) -> usize {
            self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            ((self.0 >> 33) as usize) % below.max(1)
        }

        fn pick<'a>(&mut self, words: &[&'a str]) -> &'a str {
            words[self.next(words.len())]
        }
    }

    const WORDS: [&str; 14] = [
        "the ", "Telegram ", "illicit ", "wrapping, ", "a ", "(see) ", "مرحبا ", "بالعالم ", "日本語",
        "نص ", "e\u{301}e\u{301} ", "👩\u{200d}💻 ", "https://x.y/a/b?c=1 ", "0123456789abcdef",
    ];

    fn sentence(rng: &mut Lcg, len: usize) -> String {
        let mut text = String::new();
        while text.len() < len {
            text.push_str(rng.pick(&WORDS));
        }
        text
    }

    fn char_boundary(text: &str, ix: usize) -> usize {
        ceil_char_boundary(text, ix)
    }

    /// Applies random edits, wrapping each from the last wrap, and compares with
    /// a wrap from scratch of the same text.
    fn check_edits(seed: u64, len: usize, indent: &str, width: f32) {
        let rng = &mut Lcg(seed);
        let mode = WrappingIndent::Same;
        let mut text = format!("{indent}{}", sentence(rng, len));
        let mut rows = rows_of(&fresh(&text, width, mode), text.len());
        for edit in 0..200 {
            let start = char_boundary(&text, indent.len() + 1 + rng.next(text.len() - indent.len()));
            let removed = match rng.next(3) {
                0 => 0,
                1 => rng.next(12),
                _ => rng.next(400),
            };
            let end = char_boundary(&text, start.saturating_add(removed));
            let inserted = match rng.next(4) {
                0 => "",
                1 => " ",
                2 => rng.pick(&WORDS),
                _ => "x",
            };
            let mut next = text.clone();
            next.replace_range(start..end, inserted);
            let resume = Resume::of(&rows, indent.len(), start..end, inserted.len(), next.len());
            let incremental = wrap_rows(
                &next,
                px(width),
                mode,
                &[],
                resume.as_ref(),
                &mut Counted::new(&next),
            );
            let expected = fresh(&next, width, mode);
            assert_eq!(
                rows_of(&incremental, next.len()),
                rows_of(&expected, next.len()),
                "seed {seed} edit {edit}: {start}..{end} -> {inserted:?}"
            );
            rows = rows_of(&expected, next.len());
            text = next;
        }
    }

    #[test]
    fn an_edit_wraps_like_a_fresh_wrap_on_a_measured_line() {
        for seed in 0..12 {
            check_edits(seed, 3000, "", 120.);
            check_edits(seed + 100, 3000, "  ", 120.);
        }
    }

    #[test]
    fn an_edit_wraps_like_a_fresh_wrap_on_a_long_line() {
        for seed in 0..3 {
            check_edits(seed + 200, 2 * LONG_LINE, "", 120.);
            check_edits(seed + 300, 2 * LONG_LINE, "    ", 400.);
        }
    }

    /// Replaces the middle byte of an ASCII line, as one keystroke does, and
    /// reports the bytes that were measured and walked.
    fn keystroke(text: &str, width: f32) -> (usize, usize) {
        let mode = WrappingIndent::None;
        let rows = rows_of(&fresh(text, width, mode), text.len());
        let start = text.len() / 2;
        let mut next = text.to_string();
        next.replace_range(start..start, "x");
        let resume = Resume::of(&rows, 0, start..start, 1, next.len());
        let mut widths = Counted::new(&next);
        wrap_rows(&next, px(width), mode, &[], resume.as_ref(), &mut widths, false);
        (widths.measured, widths.walked)
    }

    #[test]
    fn a_keystroke_costs_the_same_on_any_length_of_line() {
        let words = |len: usize| "wrapping a line ".repeat(len / 16);
        let (short_measured, short_walked) = keystroke(&words(8 * 1024), 800.);
        let (near_measured, near_walked) = keystroke(&words(LONG_LINE - 16), 800.);
        assert!(near_measured < 2 * short_measured.max(1024), "{near_measured}");
        assert!(near_walked < 2 * short_walked.max(1024), "{near_walked}");

        let (measured, walked) = keystroke(&words(LONG_LINE), 800.);
        let (far_measured, far_walked) = keystroke(&words(1024 * 1024), 800.);
        assert_eq!((measured, far_measured), (0, 0));
        assert!(far_walked < 2 * walked.max(1024), "{walked} {far_walked}");
    }

    #[test]
    fn a_wrap_of_a_long_line_never_shapes_even_when_the_tail_cannot_settle() {
        let text = "0".repeat(100 * 1024);
        let (measured, walked) = keystroke(&text, 700.);
        assert_eq!(measured, 0);
        assert!(walked >= text.len() / 2, "the digits all move: {walked}");
    }

    #[test]
    fn a_row_of_a_long_line_ends_at_the_summed_width() {
        let text = "wrapping a line ".repeat(LONG_LINE / 16);
        let mut widths = Counted::new(&text);
        let boundaries = wrap_rows(&text, px(200.), WrappingIndent::None, &[], None, &mut widths, false);
        assert_eq!(widths.measured, 0);
        for row in rows_of(&boundaries, text.len()) {
            let sum: f32 = text[row.clone()].chars().map(char_width).sum();
            assert!(sum <= 200., "{row:?} is {sum} wide");
            assert!(row.end == text.len() || text[..row.end].ends_with(' '));
        }
    }

    #[test]
    fn an_atom_stays_whole_in_a_long_line() {
        let text = "abcdefghij".repeat(2 * LONG_LINE / 10);
        let atoms: Vec<_> = (0..text.len() / 40).map(|i| i * 40 + 3..i * 40 + 37).collect();
        let boundaries = wrap_rows(
            &text,
            px(150.),
            WrappingIndent::None,
            &atoms,
            None,
            &mut Counted::new(&text),
        );
        assert!(!boundaries.is_empty());
        for boundary in boundaries {
            assert!(!atoms.iter().any(|a| a.start < boundary.ix && boundary.ix < a.end));
        }
    }

    #[test]
    fn crossing_the_long_line_mark_wraps_everything_again() {
        let rows = [0..LONG_LINE - 1];
        assert!(Resume::of(&rows, 0, 5..5, 1, LONG_LINE).is_none());
        assert!(Resume::of(&rows, 0, 5..5, 0, LONG_LINE - 1).is_some());
        let rows = [0..LONG_LINE];
        assert!(Resume::of(&rows, 0, 5..6, 0, LONG_LINE - 1).is_none());
    }

    #[test]
    fn a_short_line_of_a_long_text_never_shapes() {
        let text = "a word and another word and yet another word";
        let mut widths = Counted::new(text);
        wrap_rows(text, px(100.), WrappingIndent::None, &[], None, &mut widths, true);
        assert_eq!(widths.measured, 0);
    }

    #[test]
    fn an_edit_reaching_the_indent_wraps_everything_again() {
        let rows = [0..10, 10..20];
        assert!(Resume::of(&rows, 2, 2..2, 1, 21).is_none());
        assert!(Resume::of(&rows, 2, 3..3, 1, 21).is_some());
    }

    #[test]
    fn a_grapheme_wider_than_the_row_takes_a_row_of_its_own() {
        let text = "日".repeat(LONG_LINE);
        let boundaries = fresh(&text, 5., WrappingIndent::None);
        assert_eq!(boundaries.len(), LONG_LINE - 1);
        assert!(boundaries.iter().enumerate().all(|(i, b)| b.ix == (i + 1) * 3));
    }
}
