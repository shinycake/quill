//! Rectangles over wrapped `StyledText`, one per visual row and only as wide
//! as the glyphs on it.

use gpui_kit::{Bounds, Pixels, TextLayout, point};
use std::ops::Range;

/// Rectangles covering the byte `range` of `text`, laid out as `layout`: one
/// per visual row, from the first covered glyph to the last. The spaces a row
/// wraps after are left out, so nothing reaches past the row's last glyph.
pub(super) fn range_rects(
    layout: &TextLayout,
    text: &str,
    range: Range<usize>,
) -> Vec<Bounds<Pixels>> {
    let origin = layout.bounds().origin;
    let line_height = layout.line_height();
    let mut rects = Vec::new();
    let mut top = origin.y;
    let mut line_start = 0;
    for line in layout.line_layouts() {
        if line_start >= range.end {
            break;
        }
        let unwrapped = &line.unwrapped_layout;
        // A row ends where the next one's first glyph starts.
        let breaks = line.wrap_boundaries.iter().filter_map(|wrap| {
            Some(
                unwrapped
                    .runs
                    .get(wrap.run_ix)?
                    .glyphs
                    .get(wrap.glyph_ix)?
                    .index,
            )
        });
        let mut row_start = 0;
        for row_end in breaks.chain([line.len()]) {
            let glyphs_end = text
                .get(line_start + row_start..line_start + row_end)
                .map_or(row_end, |row| row_start + row.trim_end().len());
            let start = range.start.saturating_sub(line_start).max(row_start);
            let end = range.end.saturating_sub(line_start).min(glyphs_end);
            if start < end {
                let row_x = origin.x - unwrapped.x_for_index(row_start);
                rects.push(Bounds::from_corners(
                    point(row_x + unwrapped.x_for_index(start), top),
                    point(row_x + unwrapped.x_for_index(end), top + line_height),
                ));
            }
            top += line_height;
            row_start = row_end;
        }
        line_start += line.len() + 1;
    }
    rects
}
