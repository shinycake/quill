// Modified by the Quill project (2026) from gpui-base 0.7.1 (Apache-2.0):
// bidirectional text support in the input engine. See third_party/gpui-base/QUILL-CHANGES.md.
//! A visual row whose offsets remain in source UTF-8 bytes.
use super::bidi::{BidiLine, Fragment, clusters_from_glyphs, fragment_extent};
use gpui::{App, Pixels, Point, ShapedLine, SharedString, TextAlign, Window, point, px};
use std::ops::Range;

pub(crate) struct InlineFragment {
    pub(crate) range: Range<usize>,
    pub(crate) x: Pixels,
    pub(crate) width: Pixels,
    pub(crate) text: Option<ShapedLine>,
}

pub(crate) struct InputLine {
    pub(crate) len: usize,
    pub(crate) width: Pixels,
    pub(crate) text: SharedString,
    content: Content,
}
// Keep the ordinary shaped row inline: boxing it would add an allocation to
// every existing plain-text row. Only token rows allocate fragment storage.
#[allow(clippy::large_enum_variant)]
enum Content {
    Text(ShapedLine),
    Inline(Vec<InlineFragment>),
    /// A row with right-to-left content: single-direction runs shaped one by
    /// one and placed in visual order (`geo.fragments[i]` ↔ `lines[i]`).
    Bidi {
        geo: BidiLine,
        lines: Vec<ShapedLine>,
    },
}
impl From<ShapedLine> for InputLine {
    fn from(line: ShapedLine) -> Self {
        Self {
            len: line.len,
            width: line.width,
            text: line.text.clone(),
            content: Content::Text(line),
        }
    }
}
impl InputLine {
    pub(crate) fn inline(text: SharedString, fragments: Vec<InlineFragment>) -> Self {
        let width = fragments.last().map_or(px(0.), |f| f.x + f.width);
        Self {
            len: text.len(),
            text,
            width,
            content: Content::Inline(fragments),
        }
    }
    /// A row whose runs were shaped separately and laid out by [`BidiLine`].
    pub(crate) fn bidi(text: SharedString, geo: BidiLine, lines: Vec<ShapedLine>) -> Self {
        debug_assert_eq!(geo.fragments.len(), lines.len());
        Self {
            len: text.len(),
            width: px(geo.width),
            text,
            content: Content::Bidi { geo, lines },
        }
    }

    /// Whether this row holds bidi-laid-out content.
    pub(crate) fn is_bidi(&self) -> bool {
        matches!(self.content, Content::Bidi { .. })
    }

    /// The visual rectangles `[left, right]` covering the byte `range` of this
    /// row. One rectangle, except for a bidi row where a logical range can
    /// cross a direction change.
    pub(crate) fn range_rects(&self, range: Range<usize>) -> Vec<(Pixels, Pixels)> {
        match &self.content {
            Content::Bidi { geo, .. } => geo
                .range_rects(range)
                .into_iter()
                .map(|(l, r)| (px(l), px(r)))
                .collect(),
            _ => vec![(self.x_for_index(range.start), self.x_for_index(range.end))],
        }
    }

    /// One caret step visually left or right of the stop `(ix, trailing)` on a bidi row.
    pub(crate) fn visual_step(
        &self,
        ix: usize,
        trailing: bool,
        left: bool,
    ) -> Option<(usize, bool)> {
        match &self.content {
            Content::Bidi { geo, .. } => geo.visual_step_stop(ix, trailing, left),
            _ => None,
        }
    }

    /// x of the caret at `ix` seen from the character before it (the same as
    /// [`Self::x_for_index`] unless `ix` is a direction boundary of a bidi row).
    pub(crate) fn x_for_index_trailing(&self, ix: usize) -> Pixels {
        match &self.content {
            Content::Bidi { geo, .. } => px(geo.x_for_index_trailing(ix)),
            _ => self.x_for_index(ix),
        }
    }

    /// The caret stop closest to `x` and whether it hangs on the character before it.
    pub(crate) fn closest_stop_for_x(&self, x: Pixels) -> (usize, bool) {
        match &self.content {
            Content::Bidi { geo, .. } => geo.closest_stop_for_x(x.as_f32()),
            _ => (self.closest_index_for_x(x), false),
        }
    }

    /// The character the caret passes between two x positions of a bidi row.
    pub(crate) fn char_between(&self, a: Pixels, b: Pixels) -> Option<char> {
        match &self.content {
            Content::Bidi { geo, .. } => geo.char_between(a.as_f32(), b.as_f32()),
            _ => None,
        }
    }

    /// The x of stop `(ix, trailing)`.
    pub(crate) fn stop_x(&self, ix: usize, trailing: bool) -> Pixels {
        if trailing {
            self.x_for_index_trailing(ix)
        } else {
            self.x_for_index(ix)
        }
    }

    pub(crate) fn x_for_index(&self, ix: usize) -> Pixels {
        match &self.content {
            Content::Bidi { geo, .. } => px(geo.x_for_index(ix)),
            Content::Text(line) => line.x_for_index(ix),
            Content::Inline(fragments) => {
                for f in fragments {
                    if ix < f.range.end {
                        return f.x
                            + f.text.as_ref().map_or(
                                if ix <= f.range.start { px(0.) } else { f.width },
                                |line| line.x_for_index(ix.saturating_sub(f.range.start)),
                            );
                    }
                }
                self.width
            }
        }
    }
    pub(crate) fn closest_index_for_x(&self, x: Pixels) -> usize {
        match &self.content {
            Content::Bidi { geo, .. } => geo.closest_index_for_x(x.as_f32()),
            Content::Text(line) => line.closest_index_for_x(x),
            Content::Inline(fragments) => {
                for f in fragments {
                    if x <= f.x + f.width {
                        return f.text.as_ref().map_or(
                            if x - f.x < f.width / 2. {
                                f.range.start
                            } else {
                                f.range.end
                            },
                            |line| f.range.start + line.closest_index_for_x(x - f.x),
                        );
                    }
                }
                self.len
            }
        }
    }
    pub(crate) fn index_for_x(&self, x: Pixels) -> Option<usize> {
        match &self.content {
            Content::Bidi { geo, .. } => geo.index_for_x(x.as_f32()),
            Content::Text(line) => line.index_for_x(x),
            Content::Inline(_) => {
                (x >= px(0.) && x <= self.width).then(|| self.closest_index_for_x(x))
            }
        }
    }
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn paint(
        &self,
        pos: Point<Pixels>,
        height: Pixels,
        align: TextAlign,
        width: Option<Pixels>,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.paint_pass(pos, height, align, width, false, window, cx);
    }
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn paint_background(
        &self,
        pos: Point<Pixels>,
        height: Pixels,
        align: TextAlign,
        width: Option<Pixels>,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.paint_pass(pos, height, align, width, true, window, cx);
    }
    #[allow(clippy::too_many_arguments)]
    fn paint_pass(
        &self,
        pos: Point<Pixels>,
        height: Pixels,
        align: TextAlign,
        width: Option<Pixels>,
        background: bool,
        window: &mut Window,
        cx: &mut App,
    ) {
        let paint = |line: &ShapedLine, pos, align, width, window: &mut Window, cx: &mut App| {
            if background {
                let _ = line.paint_background(pos, height, align, width, window, cx);
            } else {
                let _ = line.paint(pos, height, align, width, window, cx);
            }
        };
        match &self.content {
            Content::Text(line) => paint(line, pos, align, width, window, cx),
            Content::Bidi { geo, lines } => {
                let remaining = (width.unwrap_or(self.width) - self.width).max(px(0.));
                let offset = match align {
                    TextAlign::Right => remaining,
                    TextAlign::Center => remaining / 2.,
                    _ => px(0.),
                };
                for (fragment, line) in geo.fragments.iter().zip(lines) {
                    paint(
                        line,
                        pos + point(offset + px(fragment.x + fragment.paint_shift), px(0.)),
                        TextAlign::Left,
                        None,
                        window,
                        cx,
                    );
                }
            }
            Content::Inline(fragments) => {
                let remaining = (width.unwrap_or(self.width) - self.width).max(px(0.));
                let offset = match align {
                    TextAlign::Right => remaining,
                    TextAlign::Center => remaining / 2.,
                    _ => px(0.),
                };
                for f in fragments {
                    if let Some(line) = &f.text {
                        paint(
                            line,
                            pos + point(offset + f.x, px(0.)),
                            TextAlign::Left,
                            None,
                            window,
                            cx,
                        );
                    }
                }
            }
        }
    }
}

/// The bidi [`Fragment`] for one shaped single-direction run of a row.
///
/// `range` is the run's byte range within the row, `x` where it is placed.
/// Cluster geometry is read back from the shaped glyphs, whatever order the
/// platform text system emitted them in. A platform may let trailing whitespace
/// hang outside the line's reported width (CoreText puts it at a negative x in
/// a right-to-left run), so the fragment's extent is the box that holds every
/// glyph, and `paint_shift` moves the shaped line so that box starts at `x`.
pub(crate) fn fragment_from_shaped(
    shaped: &ShapedLine,
    range: Range<usize>,
    rtl: bool,
    x: Pixels,
    text: &str,
    text_system: &gpui::TextSystem,
) -> Fragment {
    let glyphs: Vec<(f32, usize)> = shaped
        .runs
        .iter()
        .flat_map(|run| run.glyphs.iter())
        .map(|glyph| (glyph.position.x.as_f32(), glyph.index))
        .collect();
    // Advance of the rightmost glyph: nothing follows it to measure its right edge from.
    let rightmost = shaped
        .runs
        .iter()
        .flat_map(|run| run.glyphs.iter().map(move |glyph| (run.font_id, glyph)))
        .max_by(|a, b| a.1.position.x.as_f32().total_cmp(&b.1.position.x.as_f32()));
    let rightmost_advance = rightmost
        .and_then(|(font_id, glyph)| {
            let ch = text[glyph.index.min(text.len())..].chars().next()?;
            text_system.advance(font_id, shaped.font_size, ch).ok()
        })
        .map_or(0., |size| size.width.as_f32());
    let xs: Vec<f32> = glyphs.iter().map(|g| g.0).collect();
    let (shift, width) = fragment_extent(&xs, shaped.width.as_f32(), rightmost_advance);
    let shifted: Vec<(f32, usize)> = glyphs.iter().map(|&(gx, ix)| (gx + shift, ix)).collect();
    let mut fragment = Fragment::new(
        range,
        rtl,
        x.as_f32(),
        width,
        text.to_owned(),
        clusters_from_glyphs(&shifted, width, text),
    );
    fragment.paint_shift = shift;
    fragment
}
