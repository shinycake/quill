//! Wrapped, painted and hit-tested bidirectional text outside the input engine.
//!
//! GPUI wraps a paragraph by walking its shaped glyphs left to right, which for a
//! right-to-left run is visual order, so a long Hebrew message breaks into rows that hold
//! the *end* of the sentence first. This module wraps in typing order (Telegram Desktop's
//! `Ui::Text` does the same), then orders each row visually (UAX #9 L1/L2) with the same
//! run shaping the input engine uses (`input::editor::display_map::bidi`), and answers the
//! geometry questions text selection and link clicks need.
//!
//! Only text that holds right-to-left characters takes this path
//! ([`BidiParagraph::layout`] returns `None` otherwise); plain text keeps GPUI's own
//! layout.

use std::ops::Range;
use std::rc::Rc;

use gpui::{
    App, Bounds, Pixels, Point, SharedString, TextAlign, TextRun, Window, point, px, size,
};

use crate::RunGeometry;
use super::display_map::{
    BidiLine, InputLine, Paragraph, fragment_from_shaped, mirror_neutral_run,
};

/// One visual row of the paragraph.
struct Row {
    /// The text it covers, including the break space(s) after it and a hard line's `\n`.
    range: Range<usize>,
    /// The part that is shaped: `range` without trailing break whitespace.
    shaped_len: usize,
    line: InputLine,
    /// Whether its hard line's base direction is right-to-left.
    rtl: bool,
}

/// A paragraph laid out for a wrap width, in typing-order wrapping and visual row order.
pub struct BidiParagraph {
    rows: Vec<Row>,
    line_height: Pixels,
    wrap_width: Option<Pixels>,
    width: Pixels,
    len: usize,
}

/// Is `c` a place a row may end after?
fn breaks_after(c: char) -> bool {
    c == ' ' || c == '\t'
}

/// `runs` restricted to `range`.
fn runs_in(runs: &[TextRun], range: &Range<usize>) -> Vec<TextRun> {
    let mut out = Vec::new();
    let mut cursor = 0;
    for run in runs {
        let (start, end) = (cursor, cursor + run.len);
        cursor = end;
        if end <= range.start {
            continue;
        }
        if start >= range.end {
            break;
        }
        let len = end.min(range.end) - start.max(range.start);
        if len > 0 {
            out.push(TextRun {
                len,
                ..run.clone()
            });
        }
    }
    out
}

impl BidiParagraph {
    /// Lays `text` out, or `None` when it holds no right-to-left character (the caller keeps
    /// GPUI's layout for it). `wrap_width` of `None` means no wrapping.
    pub fn layout(
        text: &SharedString,
        runs: &[TextRun],
        font_size: Pixels,
        line_height: Pixels,
        wrap_width: Option<Pixels>,
        window: &mut Window,
    ) -> Option<Self> {
        if !super::display_map::needs_bidi(text) {
            return None;
        }
        let mut rows = Vec::new();
        let mut start = 0;
        for hard in text.split('\n') {
            let end = start + hard.len();
            let next = (end + 1).min(text.len());
            let hard_range = start..end;
            let paragraph = Paragraph::analyze(hard);
            let rtl = paragraph.as_ref().is_some_and(Paragraph::is_rtl);
            let breaks = Self::wrap(text, runs, &hard_range, font_size, wrap_width, window);
            let mut row_start = start;
            for row_end in breaks.iter().copied().chain(std::iter::once(end)) {
                // A hard line's last row also owns its `\n`.
                let covered_end = if row_end == end { next } else { row_end };
                let core_end = Self::trim_break_space(text, row_start, row_end);
                let line = Self::shape_row(
                    paragraph.as_ref(),
                    text,
                    runs,
                    start,
                    row_start..core_end,
                    font_size,
                    window,
                );
                rows.push(Row {
                    range: row_start..covered_end,
                    shaped_len: core_end - row_start,
                    line,
                    rtl,
                });
                row_start = row_end;
            }
            start = end + 1;
        }
        let width = rows
            .iter()
            .map(|row| row.line.width)
            .max()
            .unwrap_or_default();
        Some(Self {
            rows,
            line_height,
            wrap_width,
            width,
            len: text.len(),
        })
    }

    /// `row_end` moved back over trailing spaces: a row's break whitespace takes no room.
    fn trim_break_space(text: &str, row_start: usize, row_end: usize) -> usize {
        let trimmed = text[row_start..row_end].trim_end_matches(breaks_after);
        row_start + trimmed.len()
    }

    /// Row end offsets (exclusive, before the last row) for the hard line `range`, wrapped in
    /// typing order at `wrap_width`.
    fn wrap(
        text: &str,
        runs: &[TextRun],
        range: &Range<usize>,
        font_size: Pixels,
        wrap_width: Option<Pixels>,
        window: &mut Window,
    ) -> Vec<usize> {
        let Some(wrap_width) = wrap_width else {
            return Vec::new();
        };
        // Words with the spaces that follow them; a word's width is what must fit.
        let mut words: Vec<Range<usize>> = Vec::new();
        let mut word_start = range.start;
        let mut in_space = false;
        for (i, c) in text[range.clone()].char_indices() {
            let i = range.start + i;
            if breaks_after(c) {
                in_space = true;
            } else if in_space {
                words.push(word_start..i);
                word_start = i;
                in_space = false;
            }
        }
        words.push(word_start..range.end);

        let measure = |r: Range<usize>, window: &mut Window| -> Pixels {
            if r.is_empty() {
                return px(0.);
            }
            let slice = &text[r.clone()];
            let line_runs = runs_in(runs, &r);
            window
                .text_system()
                .shape_line(slice.to_string().into(), font_size, &line_runs, None)
                .width
        };

        let mut breaks = Vec::new();
        let mut row_width = px(0.);
        for word in words {
            let core_end = Self::trim_break_space(text, word.start, word.end);
            let ink = measure(word.start..core_end, window);
            let full = measure(word.clone(), window);
            if row_width > px(0.) && row_width + ink > wrap_width {
                breaks.push(word.start);
                row_width = px(0.);
            }
            if ink > wrap_width {
                // A word wider than the row: break it at characters.
                let mut piece_width = px(0.);
                let word_text = &text[word.start..core_end];
                for (i, c) in word_text.char_indices() {
                    let at = word.start + i;
                    let cw = measure(at..at + c.len_utf8(), window);
                    if piece_width > px(0.) && piece_width + cw > wrap_width {
                        breaks.push(at);
                        piece_width = px(0.);
                    }
                    piece_width += cw;
                }
                row_width = piece_width + (full - ink);
            } else {
                row_width += full;
            }
        }
        breaks
    }

    /// Shapes one row's `range` run by run in visual order.
    #[allow(clippy::too_many_arguments)]
    fn shape_row(
        paragraph: Option<&Paragraph<'_>>,
        text: &str,
        runs: &[TextRun],
        hard_start: usize,
        range: Range<usize>,
        font_size: Pixels,
        window: &mut Window,
    ) -> InputLine {
        let row_text: SharedString = text[range.clone()].to_string().into();
        let visual: Vec<(Range<usize>, bool)> = match paragraph {
            // Offsets within the hard line, which the analysis was made on.
            Some(paragraph) => paragraph
                .visual_runs(range.start - hard_start..range.end - hard_start)
                .into_iter()
                .map(|r| {
                    (
                        r.range.start + hard_start..r.range.end + hard_start,
                        r.rtl,
                    )
                })
                .collect(),
            None if range.is_empty() => Vec::new(),
            None => vec![(range.clone(), false)],
        };
        let mut fragments = Vec::new();
        let mut shaped_lines = Vec::new();
        let mut x = px(0.);
        for (run_range, rtl) in visual {
            let slice = &text[run_range.clone()];
            let run_runs = runs_in(runs, &run_range);
            let shaped_text: SharedString = if rtl {
                mirror_neutral_run(slice).unwrap_or_else(|| slice.to_string())
            } else {
                slice.to_string()
            }
            .into();
            let shaped = window
                .text_system()
                .shape_line(shaped_text, font_size, &run_runs, None);
            let local = run_range.start - range.start..run_range.end - range.start;
            let fragment =
                fragment_from_shaped(&shaped, local, rtl, x, slice, window.text_system());
            x += px(fragment.width);
            fragments.push(fragment);
            shaped_lines.push(shaped);
        }
        let geometry = BidiLine::new(row_text.len(), fragments);
        InputLine::bidi(row_text, geometry, shaped_lines)
    }

    /// The size the paragraph takes: the widest row, or the whole wrap width when a row is
    /// right-aligned (so the text can reach the right edge).
    pub fn size(&self) -> gpui::Size<Pixels> {
        let any_rtl = self.rows.iter().any(|row| row.rtl);
        let width = match (any_rtl, self.wrap_width) {
            (true, Some(wrap)) => wrap.max(self.width),
            _ => self.width,
        };
        size(width, self.line_height * self.rows.len().max(1) as f32)
    }

    /// Row height.
    pub fn line_height(&self) -> Pixels {
        self.line_height
    }

    /// Byte length of the text.
    pub fn len(&self) -> usize {
        self.len
    }

    /// Whether the text is empty.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    fn row_offset(&self, row: &Row, width: Pixels) -> Pixels {
        if row.rtl {
            (width - row.line.width).max(px(0.))
        } else {
            px(0.)
        }
    }

    fn row_at_index(&self, index: usize) -> usize {
        self.rows
            .iter()
            .position(|row| index < row.range.end)
            .unwrap_or(self.rows.len().saturating_sub(1))
    }

    /// Caret position for byte offset `index`, relative to the paragraph's top-left, for a
    /// paragraph `width` wide.
    pub fn position_for_index(&self, index: usize, width: Pixels) -> Option<Point<Pixels>> {
        if self.rows.is_empty() {
            return None;
        }
        let ix = self.row_at_index(index.min(self.len));
        let row = &self.rows[ix];
        let local = (index.saturating_sub(row.range.start)).min(row.shaped_len);
        Some(point(
            self.row_offset(row, width) + row.line.x_for_index(local),
            self.line_height * ix as f32,
        ))
    }

    fn row_at_y(&self, y: Pixels) -> usize {
        ((y / self.line_height).floor().max(0.) as usize).min(self.rows.len().saturating_sub(1))
    }

    /// The byte offset under `position` (`Ok`), else the nearest caret offset (`Err`).
    pub fn index_for_position(&self, position: Point<Pixels>, width: Pixels) -> Result<usize, usize> {
        if self.rows.is_empty() {
            return Err(0);
        }
        let ix = self.row_at_y(position.y);
        let row = &self.rows[ix];
        let x = position.x - self.row_offset(row, width);
        let inside = position.y >= px(0.)
            && position.y < self.line_height * self.rows.len() as f32
            && x >= px(0.);
        match (inside, row.line.index_for_x(x)) {
            (true, Some(local)) => Ok(row.range.start + local),
            _ => Err(row.range.start + row.line.closest_index_for_x(x).min(row.shaped_len)),
        }
    }

    /// The caret offset nearest `position`, wherever it is.
    pub fn closest_index(&self, position: Point<Pixels>, width: Pixels) -> usize {
        if self.rows.is_empty() {
            return 0;
        }
        match self.index_for_position(position, width) {
            _ => {
                // Over the left half of a character the caret goes before it, over the
                // right half after it: the nearest boundary, not the character.
                let row = &self.rows[self.row_at_y(position.y)];
                let x = position.x - self.row_offset(row, width);
                row.range.start + row.line.closest_index_for_x(x).min(row.shaped_len)
            }
        }
    }

    /// The rectangles covering the byte `range`, relative to the paragraph's top-left.
    pub fn range_rects(&self, range: Range<usize>, width: Pixels) -> Vec<Bounds<Pixels>> {
        let mut out = Vec::new();
        for (ix, row) in self.rows.iter().enumerate() {
            let start = range.start.max(row.range.start);
            let end = range.end.min(row.range.end);
            if start >= end {
                continue;
            }
            let offset = self.row_offset(row, width);
            let local = start - row.range.start..(end - row.range.start).min(row.shaped_len);
            if local.start >= local.end {
                continue;
            }
            for (left, right) in row.line.range_rects(local) {
                out.push(Bounds::from_corners(
                    point(offset + left, self.line_height * ix as f32),
                    point(offset + right, self.line_height * (ix + 1) as f32),
                ));
            }
        }
        out
    }

    /// Paints the paragraph with its top-left at `origin`, `width` wide.
    pub fn paint(&self, origin: Point<Pixels>, width: Pixels, window: &mut Window, cx: &mut App) {
        for (ix, row) in self.rows.iter().enumerate() {
            let at = origin + point(px(0.), self.line_height * ix as f32);
            let align = if row.rtl {
                TextAlign::Right
            } else {
                TextAlign::Left
            };
            row.line
                .paint_background(at, self.line_height, align, Some(width), window, cx);
            row.line
                .paint(at, self.line_height, align, Some(width), window, cx);
        }
    }

    /// Selection geometry for text laid out by this paragraph, at window position `origin`
    /// and `width` wide.
    pub fn geometry(self: &Rc<Self>, origin: Point<Pixels>, width: Pixels) -> Rc<dyn RunGeometry> {
        Rc::new(Geometry {
            paragraph: self.clone(),
            origin,
            width,
        })
    }
}

struct Geometry {
    paragraph: Rc<BidiParagraph>,
    origin: Point<Pixels>,
    width: Pixels,
}

impl RunGeometry for Geometry {
    fn len(&self) -> usize {
        self.paragraph.len
    }

    fn line_height(&self) -> Pixels {
        self.paragraph.line_height
    }

    fn bounds(&self) -> Bounds<Pixels> {
        Bounds::new(self.origin, self.paragraph.size())
    }

    fn position_for_index(&self, index: usize) -> Option<Point<Pixels>> {
        self.paragraph
            .position_for_index(index, self.width)
            .map(|p| p + self.origin)
    }

    fn index_for_position(&self, position: Point<Pixels>) -> Result<usize, usize> {
        self.paragraph
            .index_for_position(position - self.origin, self.width)
    }

    fn rows_extent(&self, _line_height: Pixels) -> (Pixels, Pixels) {
        (
            self.origin.y,
            self.origin.y + self.paragraph.line_height * self.paragraph.rows.len() as f32,
        )
    }

    fn selected_range(
        &self,
        start: Point<Pixels>,
        end: Point<Pixels>,
    ) -> Option<Option<Range<usize>>> {
        // The logical range between the two endpoints, as in any editor: a drag across a
        // right-to-left row selects right to left in reading order.
        let a = self
            .paragraph
            .closest_index(start - self.origin, self.width);
        let b = self.paragraph.closest_index(end - self.origin, self.width);
        let (from, to) = (a.min(b), a.max(b));
        // The band can lie wholly above or below the paragraph.
        let (top, bottom) = self.rows_extent(self.paragraph.line_height);
        let band_top = start.y.min(end.y);
        let band_bottom = start.y.max(end.y);
        if band_bottom < top || band_top >= bottom {
            return Some(None);
        }
        // Endpoints outside the rows extend to the paragraph's edge.
        let from = if band_top < top { 0 } else { from };
        let to = if band_bottom >= bottom {
            self.paragraph.len
        } else {
            to
        };
        Some((from < to).then_some(from..to))
    }
}
