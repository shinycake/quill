// Added by the Quill project (2026) to gpui-pre 0.3.12; licensed Apache-2.0 like the rest
// of this crate. See third_party/gpui-pre/QUILL-CHANGES.md.
//! Decorations on a line whose glyphs are not in byte order.
//!
//! `paint_line` walks the decoration runs forward as it meets glyphs left to right,
//! which only works while glyph byte indices rise along the line. In right-to-left
//! text they fall, so the walk skipped to the last run: a link in the middle of a
//! Hebrew sentence took the color of the sentence's end, or the whole line took the
//! link's. A [`DecorationPlan`] instead gives each glyph the run whose byte range holds
//! it, and each underline, strikethrough and background the span of the glyphs that
//! carry it on screen.

use std::ops::Range;

use crate::{Hsla, LineLayout, Pixels, ShapedRun, StrikethroughStyle, UnderlineStyle, WrapBoundary, px};

use super::DecorationRun;

/// Whether the glyphs of `layout` come in byte order, left to right, so the forward
/// walk of `paint_line` finds each glyph's decoration.
pub fn glyphs_in_byte_order(layout: &LineLayout) -> bool {
    let mut last = 0;
    for glyph in layout.runs.iter().flat_map(|run| &run.glyphs) {
        if glyph.index < last {
            return false;
        }
        last = glyph.index;
    }
    true
}

/// The rows of a wrapped line, each as a line of its own: its glyphs moved so the row
/// starts at x = 0, byte indices kept, and `len` the byte offset the next row starts at.
/// The rows of a line holding right-to-left text follow each other left to right in
/// `layout`, one wrap boundary at the first glyph of each row after the first.
pub fn wrapped_rows(layout: &LineLayout, boundaries: &[WrapBoundary]) -> Vec<LineLayout> {
    let row_x = |boundary: &WrapBoundary| {
        layout
            .runs
            .get(boundary.run_ix)
            .and_then(|run| run.glyphs.get(boundary.glyph_ix))
            .map(|glyph| glyph.position.x)
    };
    let mut rows: Vec<LineLayout> = (0..=boundaries.len())
        .map(|row| {
            let start = row
                .checked_sub(1)
                .and_then(|ix| boundaries.get(ix))
                .and_then(row_x)
                .unwrap_or(px(0.));
            let end = boundaries.get(row).and_then(row_x).unwrap_or(layout.width);
            LineLayout {
                font_size: layout.font_size,
                width: end - start,
                ascent: layout.ascent,
                descent: layout.descent,
                runs: Vec::new(),
                len: layout.len,
            }
        })
        .collect();
    let mut starts = vec![px(0.); rows.len()];
    for (row, start) in starts.iter_mut().enumerate().skip(1) {
        *start = boundaries
            .get(row - 1)
            .and_then(row_x)
            .unwrap_or(px(0.));
    }
    // The run of `layout` each row's last shaped run came from.
    let mut last_run: Vec<Option<usize>> = vec![None; rows.len()];
    for (run_ix, run) in layout.runs.iter().enumerate() {
        for (glyph_ix, glyph) in run.glyphs.iter().enumerate() {
            let row = boundaries.partition_point(|boundary| {
                (boundary.run_ix, boundary.glyph_ix) <= (run_ix, glyph_ix)
            });
            let (Some(target), Some(start), Some(last)) =
                (rows.get_mut(row), starts.get(row), last_run.get_mut(row))
            else {
                continue;
            };
            let mut glyph = glyph.clone();
            glyph.position.x -= *start;
            match target.runs.last_mut() {
                Some(shaped) if *last == Some(run_ix) => shaped.glyphs.push(glyph),
                _ => {
                    target.runs.push(ShapedRun {
                        font_id: run.font_id,
                        glyphs: vec![glyph],
                    });
                    *last = Some(run_ix);
                }
            }
        }
    }
    // Each row ends where the text of the next one starts.
    let firsts: Vec<Option<usize>> = rows
        .iter()
        .map(|row| row.runs.iter().flat_map(|run| &run.glyphs).map(|g| g.index).min())
        .collect();
    for (row, layout_row) in rows.iter_mut().enumerate() {
        if let Some(Some(next)) = firsts.get(row + 1) {
            layout_row.len = *next;
        }
    }
    rows
}

/// One glyph to paint: where it sits in `layout.runs` and the decoration run it takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlannedGlyph {
    /// Index into `layout.runs`.
    pub run: usize,
    /// Index into that run's glyphs.
    pub glyph: usize,
    /// Index into the decoration runs, `None` past their end.
    pub decoration: Option<usize>,
}

/// A horizontal stretch of the line that carries one decoration, in line coordinates
/// (x from the line's left edge).
#[derive(Clone, Debug, PartialEq)]
pub struct DecorationSpan<Style> {
    /// The bytes of the text the span covers, first to last byte of its glyphs.
    pub text: Range<usize>,
    /// Left edge.
    pub start: Pixels,
    /// Right edge.
    pub end: Pixels,
    /// How to draw it.
    pub style: Style,
}

/// Colors and decoration spans for every glyph of a line, by byte range rather than by
/// the order the glyphs come in.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DecorationPlan {
    /// Every glyph, left to right on screen.
    pub glyphs: Vec<PlannedGlyph>,
    /// Underlines, left to right.
    pub underlines: Vec<DecorationSpan<UnderlineStyle>>,
    /// Strikethroughs, left to right.
    pub strikethroughs: Vec<DecorationSpan<StrikethroughStyle>>,
    /// Backgrounds, left to right.
    pub backgrounds: Vec<DecorationSpan<Hsla>>,
}

impl DecorationPlan {
    /// The plan for `layout` styled by `decorations`.
    pub fn new(layout: &LineLayout, decorations: &[DecorationRun]) -> Self {
        let mut ends = Vec::with_capacity(decorations.len());
        let mut end = 0usize;
        for run in decorations {
            end += run.len as usize;
            ends.push(end);
        }
        let decoration_of = |index: usize| {
            let ix = ends.partition_point(|&end| end <= index);
            (ix < decorations.len()).then_some(ix)
        };

        let mut glyphs: Vec<(Pixels, PlannedGlyph, usize)> = layout
            .runs
            .iter()
            .enumerate()
            .flat_map(|(run_ix, run)| {
                run.glyphs.iter().enumerate().map(move |(glyph_ix, glyph)| {
                    (glyph.position.x, run_ix, glyph_ix, glyph.index)
                })
            })
            .map(|(x, run, glyph, index)| {
                (
                    x,
                    PlannedGlyph {
                        run,
                        glyph,
                        decoration: decoration_of(index),
                    },
                    index,
                )
            })
            .collect();
        // Backends hand glyphs over left to right; sort anyway so a backend that does
        // not still gets spans that do not overlap.
        glyphs.sort_by(|a, b| f32::from(a.0).total_cmp(&f32::from(b.0)));

        let mut plan = DecorationPlan {
            glyphs: glyphs.iter().map(|(_, glyph, _)| *glyph).collect(),
            ..Default::default()
        };
        for (ix, (x, glyph, index)) in glyphs.iter().enumerate() {
            let Some(decoration) = glyph.decoration.and_then(|ix| decorations.get(ix)) else {
                continue;
            };
            // A glyph reaches to the next glyph on screen, the last one to the line's end.
            let right = glyphs
                .get(ix + 1)
                .map(|(next, ..)| *next)
                .unwrap_or(layout.width)
                .max(*x);
            let bytes = *index..index + 1;
            if let Some(style) = decoration.underline {
                let style = UnderlineStyle {
                    color: Some(style.color.unwrap_or(decoration.color)),
                    ..style
                };
                extend(&mut plan.underlines, bytes.clone(), *x, right, style);
            }
            if let Some(style) = decoration.strikethrough {
                let style = StrikethroughStyle {
                    color: Some(style.color.unwrap_or(decoration.color)),
                    ..style
                };
                extend(&mut plan.strikethroughs, bytes.clone(), *x, right, style);
            }
            if let Some(color) = decoration.background_color {
                extend(&mut plan.backgrounds, bytes, *x, right, color);
            }
        }
        plan
    }
}

/// Grows the last span when it ends where this glyph starts and draws the same way,
/// otherwise starts a new one.
fn extend<Style: PartialEq>(
    spans: &mut Vec<DecorationSpan<Style>>,
    bytes: Range<usize>,
    left: Pixels,
    right: Pixels,
    style: Style,
) {
    if let Some(last) = spans.last_mut()
        && last.style == style
        && (f32::from(last.end) - f32::from(left)).abs() < 0.5
    {
        last.end = last.end.max(right);
        last.text.start = last.text.start.min(bytes.start);
        last.text.end = last.text.end.max(bytes.end);
        return;
    }
    spans.push(DecorationSpan {
        text: bytes,
        start: left,
        end: right,
        style,
    });
}
