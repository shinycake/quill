use crate::{FontId, GlyphId, Pixels, PlatformTextSystem, Point, SharedString, Size, point, px};
use collections::{FxHashMap, FxHashSet};
use itertools::Itertools;
use smallvec::SmallVec;
use std::{
    borrow::Borrow,
    cell::RefCell,
    hash::{Hash, Hasher},
    ops::Range,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use super::LineWrapper;

/// A laid out and styled line of text
#[derive(Default, Debug)]
pub struct LineLayout {
    /// The font size for this line
    pub font_size: Pixels,
    /// The width of the line
    pub width: Pixels,
    /// The ascent of the line
    pub ascent: Pixels,
    /// The descent of the line
    pub descent: Pixels,
    /// The shaped runs that make up this line
    pub runs: Vec<ShapedRun>,
    /// The length of the line in utf-8 bytes
    pub len: usize,
}

/// A run of text that has been shaped .
#[derive(Debug, Clone)]
pub struct ShapedRun {
    /// The font id for this run
    pub font_id: FontId,
    /// The glyphs that make up this run
    pub glyphs: Vec<ShapedGlyph>,
}

/// A single glyph, ready to paint.
#[derive(Clone, Debug)]
pub struct ShapedGlyph {
    /// The ID for this glyph, as determined by the text system.
    pub id: GlyphId,

    /// The position of this glyph in its containing line.
    pub position: Point<Pixels>,

    /// The index of this glyph in the original text.
    pub index: usize,

    /// Whether this glyph is an emoji
    pub is_emoji: bool,
}

/// QUILL: one character cluster of a line, for hit testing right-to-left text.
struct GlyphCluster {
    /// Byte index of the character that starts the cluster.
    index: usize,
    /// Byte index of the next character in the text, or the line's length.
    end: usize,
    left: Pixels,
    right: Pixels,
    rtl: bool,
}

impl LineLayout {
    /// The index for the character at the given x coordinate
    pub fn index_for_x(&self, x: Pixels) -> Option<usize> {
        if x >= self.width {
            None
        } else if let Some(clusters) = self.unordered_clusters() {
            // QUILL: right-to-left glyphs (see `unordered_clusters`).
            clusters
                .iter()
                .rev()
                .find(|cluster| cluster.left <= x)
                .or(clusters.first())
                .map(|cluster| cluster.index)
        } else {
            for run in self.runs.iter().rev() {
                for glyph in run.glyphs.iter().rev() {
                    if glyph.position.x <= x {
                        return Some(glyph.index);
                    }
                }
            }
            Some(0)
        }
    }

    /// closest_index_for_x returns the character boundary closest to the given x coordinate
    /// (e.g. to handle aligning up/down arrow keys)
    pub fn closest_index_for_x(&self, x: Pixels) -> usize {
        if let Some(clusters) = self.unordered_clusters() {
            // QUILL: the caret stop nearest to x, each cluster's leading edge being its
            // start and its trailing edge the start of the character after it.
            let mut best = (Pixels::MAX, self.len);
            for cluster in &clusters {
                let (leading, trailing) = if cluster.rtl {
                    (cluster.right, cluster.left)
                } else {
                    (cluster.left, cluster.right)
                };
                for (edge, index) in [(leading, cluster.index), (trailing, cluster.end)] {
                    let distance = (edge - x).abs();
                    if distance < best.0 {
                        best = (distance, index);
                    }
                }
            }
            return best.1;
        }
        let mut prev_index = 0;
        let mut prev_x = px(0.);

        for run in self.runs.iter() {
            for glyph in run.glyphs.iter() {
                if glyph.position.x >= x {
                    if glyph.position.x - x < x - prev_x {
                        return glyph.index;
                    } else {
                        return prev_index;
                    }
                }
                prev_index = glyph.index;
                prev_x = glyph.position.x;
            }
        }

        if self.len == 1 {
            if x > self.width / 2. {
                return 1;
            } else {
                return 0;
            }
        }

        self.len
    }

    /// The x position of the character at the given index
    pub fn x_for_index(&self, index: usize) -> Pixels {
        if let Some(clusters) = self.unordered_clusters() {
            // QUILL: the leading edge of the character at `index`, which is the right edge
            // in right-to-left text; past the end, the trailing edge of the last character.
            let containing = clusters
                .iter()
                .filter(|cluster| cluster.index <= index)
                .max_by_key(|cluster| cluster.index);
            return match containing {
                Some(cluster) if index < cluster.end => {
                    if cluster.rtl {
                        cluster.right
                    } else {
                        cluster.left
                    }
                }
                Some(cluster) => {
                    if cluster.rtl {
                        cluster.left
                    } else {
                        cluster.right
                    }
                }
                None => self.width,
            };
        }
        for run in &self.runs {
            for glyph in &run.glyphs {
                if glyph.index >= index {
                    return glyph.position.x;
                }
            }
        }
        self.width
    }

    /// QUILL: the line's character clusters left to right, when its glyphs are not in byte
    /// order (right-to-left text), so the hit tests above can tell a character's leading
    /// edge from its trailing one; `None` for byte-ordered lines, which keep the upstream
    /// walks. A cluster reads right to left when the cluster after it on screen starts
    /// earlier in the text (or, last on the line, the one before it starts later).
    fn unordered_clusters(&self) -> Option<Vec<GlyphCluster>> {
        if super::glyphs_in_byte_order(self) {
            return None;
        }
        let mut starts: Vec<(Pixels, usize)> = self
            .runs
            .iter()
            .flat_map(|run| &run.glyphs)
            .map(|glyph| (glyph.position.x, glyph.index))
            .collect();
        starts.sort_by(|a, b| a.1.cmp(&b.1).then(f32::from(a.0).total_cmp(&f32::from(b.0))));
        // One entry per character start, at its leftmost glyph.
        starts.dedup_by_key(|(_, index)| *index);
        let mut by_index: Vec<usize> = starts.iter().map(|(_, index)| *index).collect();
        by_index.sort_unstable();
        starts.sort_by(|a, b| f32::from(a.0).total_cmp(&f32::from(b.0)));
        let mut clusters: Vec<GlyphCluster> = Vec::with_capacity(starts.len());
        for (ix, &(left, index)) in starts.iter().enumerate() {
            let right = starts
                .get(ix + 1)
                .map_or(self.width, |(next, _)| *next)
                .max(left);
            let next_start = by_index.partition_point(|&start| start <= index);
            let end = by_index.get(next_start).copied().unwrap_or(self.len);
            let rtl = match (starts.get(ix + 1), ix.checked_sub(1).and_then(|p| starts.get(p))) {
                (Some((_, next)), _) => *next < index,
                (None, Some((_, previous))) => *previous > index,
                (None, None) => false,
            };
            clusters.push(GlyphCluster {
                index,
                end,
                left,
                right,
                rtl,
            });
        }
        Some(clusters)
    }

    /// QUILL: where a line whose glyphs are not in byte order (it holds right-to-left
    /// text) breaks into rows of at most `wrap_width`: the byte offsets at which the rows
    /// after the first start. Rows follow the text, not the screen, so the first row holds
    /// the start of a right-to-left paragraph; upstream's walk over the glyphs on screen
    /// put its end there. Breaks fall where UAX #14 allows them, a word too long for a row
    /// is cut where it overflows, and spaces may hang past the row's end.
    fn logical_row_starts(
        &self,
        text: &str,
        wrap_width: Pixels,
        max_lines: Option<usize>,
    ) -> Vec<usize> {
        let Some(mut clusters) = self.unordered_clusters() else {
            return Vec::new();
        };
        clusters.sort_by_key(|cluster| cluster.index);
        let breaks: Vec<usize> = LineWrapper::break_indices(text).collect();
        let mut widths = Vec::with_capacity(clusters.len() + 1);
        let mut total = px(0.);
        widths.push(total);
        for cluster in &clusters {
            total += cluster.right - cluster.left;
            widths.push(total);
        }
        let width_of = |from: usize, through: usize| {
            widths.get(through + 1).copied().unwrap_or(total)
                - widths.get(from).copied().unwrap_or(px(0.))
        };
        let mut starts = Vec::new();
        let mut row_first = 0;
        let mut candidate = None;
        for (ix, cluster) in clusters.iter().enumerate() {
            if ix > row_first && breaks.binary_search(&cluster.index).is_ok() {
                candidate = Some(ix);
            }
            let blank = text
                .get(cluster.index..)
                .and_then(|rest| rest.chars().next())
                .is_none_or(char::is_whitespace);
            if blank || ix == row_first || width_of(row_first, ix) <= wrap_width {
                continue;
            }
            if max_lines.is_some_and(|max| starts.len() + 1 >= max) {
                break;
            }
            let cut = candidate.take().unwrap_or(ix);
            let Some(start) = clusters.get(cut).map(|cluster| cluster.index) else {
                break;
            };
            starts.push(start);
            row_first = cut;
        }
        starts
    }

    /// The corresponding Font at the given index
    pub fn font_id_for_index(&self, index: usize) -> Option<FontId> {
        for run in &self.runs {
            for glyph in &run.glyphs {
                if glyph.index >= index {
                    return Some(run.font_id);
                }
            }
        }
        None
    }

    /// Split this layout at a byte index, returning `(prefix, suffix)`.
    ///
    /// - `prefix` contains glyphs for bytes `[0, byte_index)` with original positions.
    ///   Its width equals the x-advance up to the split point.
    /// - `suffix` contains glyphs for bytes `[byte_index, len)` with positions
    ///   shifted left so the first glyph starts at x=0, and byte indices rebased to 0.
    /// - `font_size`, `ascent`, and `descent` are copied to both halves.
    pub fn split_at(&self, byte_index: usize) -> (LineLayout, LineLayout) {
        let x_offset = self.x_for_index(byte_index);

        // Partition glyph runs. A single run may contribute glyphs to both halves.
        let mut left_runs = Vec::new();
        let mut right_runs = Vec::new();

        for run in &self.runs {
            let split_pos = run.glyphs.partition_point(|g| g.index < byte_index);

            if split_pos > 0 {
                left_runs.push(ShapedRun {
                    font_id: run.font_id,
                    glyphs: run.glyphs[..split_pos].to_vec(),
                });
            }

            if split_pos < run.glyphs.len() {
                let right_glyphs = run.glyphs[split_pos..]
                    .iter()
                    .map(|g| ShapedGlyph {
                        id: g.id,
                        position: point(g.position.x - x_offset, g.position.y),
                        index: g.index - byte_index,
                        is_emoji: g.is_emoji,
                    })
                    .collect();
                right_runs.push(ShapedRun {
                    font_id: run.font_id,
                    glyphs: right_glyphs,
                });
            }
        }

        let left = LineLayout {
            font_size: self.font_size,
            width: x_offset,
            ascent: self.ascent,
            descent: self.descent,
            runs: left_runs,
            len: byte_index,
        };

        let right = LineLayout {
            font_size: self.font_size,
            width: self.width - x_offset,
            ascent: self.ascent,
            descent: self.descent,
            runs: right_runs,
            len: self.len - byte_index,
        };

        (left, right)
    }

    fn compute_wrap_boundaries(
        &self,
        text: &str,
        wrap_width: Pixels,
        max_lines: Option<usize>,
        scratch: &mut WrapScratch,
    ) -> SmallVec<[WrapBoundary; 1]> {
        let mut boundaries = SmallVec::new();
        let mut current_segment: Option<Range<usize>> = None;
        let mut last_candidate_ix = None;
        let mut last_candidate_x = px(0.);
        let mut last_boundary = WrapBoundary {
            run_ix: 0,
            glyph_ix: 0,
        };
        let mut last_boundary_x = px(0.);
        // Shaping can combine characters across a line break opportunity, so we ignore those
        // break opportunities that would separate them.
        let WrapScratch {
            glyph_indices,
            break_indices,
        } = scratch;
        glyph_indices.clear();
        glyph_indices.extend(
            self.runs
                .iter()
                .flat_map(|run| run.glyphs.iter().map(|glyph| glyph.index)),
        );
        break_indices.clear();
        break_indices
            .extend(LineWrapper::break_indices(text).filter(|index| glyph_indices.contains(index)));
        let mut glyphs = self
            .runs
            .iter()
            .enumerate()
            .flat_map(move |(run_ix, run)| {
                run.glyphs.iter().enumerate().map(move |(glyph_ix, glyph)| {
                    (
                        WrapBoundary { run_ix, glyph_ix },
                        glyph.index,
                        glyph.position.x,
                    )
                })
            })
            .dedup_by(|(_, previous_index, ..), (_, index, ..)| previous_index == index)
            .peekable();

        while let Some((boundary, index, x)) = glyphs.next() {
            let byte = text.as_bytes()[index];
            if byte == b'\n' {
                continue;
            }

            if byte != b' '
                && current_segment
                    .as_ref()
                    .is_none_or(|segment| !segment.contains(&index))
            {
                // Detect transitions between line-break segments rather than matching glyph
                // indices directly, to accommodate RTL text. Workaround for the lack of complete
                // UAX #9 support.
                let segment = break_indices.partition_point(|break_index| *break_index <= index);
                if current_segment.is_some() {
                    last_candidate_ix = Some(boundary);
                    last_candidate_x = x;
                }
                let start = segment
                    .checked_sub(1)
                    .and_then(|previous| break_indices.get(previous))
                    .copied()
                    .unwrap_or(0);
                let end = break_indices.get(segment).copied().unwrap_or(text.len());
                current_segment = Some(start..end);
            }

            let next_x = glyphs.peek().map_or(self.width, |(_, _, x)| *x);
            let width = next_x - last_boundary_x;

            if width > wrap_width && boundary > last_boundary {
                // When used line_clamp, we should limit the number of lines.
                if let Some(max_lines) = max_lines
                    && boundaries.len() >= max_lines.saturating_sub(1)
                {
                    break;
                }

                if let Some(last_candidate_ix) = last_candidate_ix.take() {
                    last_boundary = last_candidate_ix;
                    last_boundary_x = last_candidate_x;
                } else {
                    last_boundary = boundary;
                    last_boundary_x = x;
                }
                boundaries.push(last_boundary);
            }
        }

        boundaries
    }
}

/// A line of text that has been wrapped to fit a given width
#[derive(Default, Debug)]
pub struct WrappedLineLayout {
    /// The line layout, pre-wrapping.
    pub unwrapped_layout: Arc<LineLayout>,

    /// The boundaries at which the line was wrapped
    pub wrap_boundaries: SmallVec<[WrapBoundary; 1]>,

    /// The width of the line, if it was wrapped
    pub wrap_width: Option<Pixels>,
}

/// A boundary at which a line was wrapped
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct WrapBoundary {
    /// The index in the run just before the line was wrapped
    pub run_ix: usize,
    /// The index of the glyph just before the line was wrapped
    pub glyph_ix: usize,
}

impl WrappedLineLayout {
    /// The length of the underlying text, in utf8 bytes.
    pub fn len(&self) -> usize {
        self.unwrapped_layout.len
    }

    /// The width of this line, in pixels, whether or not it was wrapped.
    pub fn width(&self) -> Pixels {
        self.wrap_width
            .unwrap_or(Pixels::MAX)
            .min(self.unwrapped_layout.width)
    }

    /// The size of the whole wrapped text, for the given line_height.
    /// can span multiple lines if there are multiple wrap boundaries.
    pub fn size(&self, line_height: Pixels) -> Size<Pixels> {
        Size {
            width: self.width(),
            height: line_height * (self.wrap_boundaries.len() + 1),
        }
    }

    /// The ascent of a line in this layout
    pub fn ascent(&self) -> Pixels {
        self.unwrapped_layout.ascent
    }

    /// The descent of a line in this layout
    pub fn descent(&self) -> Pixels {
        self.unwrapped_layout.descent
    }

    /// The wrap boundaries in this layout
    pub fn wrap_boundaries(&self) -> &[WrapBoundary] {
        &self.wrap_boundaries
    }

    /// The font size of this layout
    pub fn font_size(&self) -> Pixels {
        self.unwrapped_layout.font_size
    }

    /// The runs in this layout, sans wrapping
    pub fn runs(&self) -> &[ShapedRun] {
        &self.unwrapped_layout.runs
    }

    /// The index corresponding to a given position in this layout for the given line height.
    ///
    /// See also [`Self::closest_index_for_position`].
    pub fn index_for_position(
        &self,
        position: Point<Pixels>,
        line_height: Pixels,
    ) -> Result<usize, usize> {
        self._index_for_position(position, line_height, false)
    }

    /// The closest index to a given position in this layout for the given line height.
    ///
    /// Closest means the character boundary closest to the given position.
    ///
    /// See also [`LineLayout::closest_index_for_x`].
    pub fn closest_index_for_position(
        &self,
        position: Point<Pixels>,
        line_height: Pixels,
    ) -> Result<usize, usize> {
        self._index_for_position(position, line_height, true)
    }

    fn _index_for_position(
        &self,
        position: Point<Pixels>,
        line_height: Pixels,
        closest: bool,
    ) -> Result<usize, usize> {
        if self.has_unordered_rows() {
            return self.row_index_for_position(position, line_height, closest);
        }
        let wrapped_line_ix = (position.y / line_height) as usize;

        let wrapped_line_start_index;
        let wrapped_line_start_x;
        if wrapped_line_ix > 0 {
            let Some(line_start_boundary) = self.wrap_boundaries.get(wrapped_line_ix - 1) else {
                return Err(0);
            };
            let run = &self.unwrapped_layout.runs[line_start_boundary.run_ix];
            let glyph = &run.glyphs[line_start_boundary.glyph_ix];
            wrapped_line_start_index = glyph.index;
            wrapped_line_start_x = glyph.position.x;
        } else {
            wrapped_line_start_index = 0;
            wrapped_line_start_x = Pixels::ZERO;
        };

        let wrapped_line_end_index;
        let wrapped_line_end_x;
        if wrapped_line_ix < self.wrap_boundaries.len() {
            let next_wrap_boundary_ix = wrapped_line_ix;
            let next_wrap_boundary = self.wrap_boundaries[next_wrap_boundary_ix];
            let run = &self.unwrapped_layout.runs[next_wrap_boundary.run_ix];
            let glyph = &run.glyphs[next_wrap_boundary.glyph_ix];
            wrapped_line_end_index = glyph.index;
            wrapped_line_end_x = glyph.position.x;
        } else {
            wrapped_line_end_index = self.unwrapped_layout.len;
            wrapped_line_end_x = self.unwrapped_layout.width;
        };

        let mut position_in_unwrapped_line = position;
        position_in_unwrapped_line.x += wrapped_line_start_x;
        if position_in_unwrapped_line.x < wrapped_line_start_x {
            Err(wrapped_line_start_index)
        } else if position_in_unwrapped_line.x >= wrapped_line_end_x {
            Err(wrapped_line_end_index)
        } else {
            if closest {
                Ok(self
                    .unwrapped_layout
                    .closest_index_for_x(position_in_unwrapped_line.x))
            } else {
                // The shaper can place a trailing zero-width wrap boundary glyph slightly past
                // the line's width, so the row can extend past where `index_for_x` has glyphs.
                self.unwrapped_layout
                    .index_for_x(position_in_unwrapped_line.x)
                    .ok_or(wrapped_line_end_index)
            }
        }
    }

    /// QUILL: whether this is a wrapped line holding right-to-left text, whose rows
    /// follow the text rather than the screen (`LineLayoutCache::layout_rows`), so the hit
    /// tests below go row by row.
    fn has_unordered_rows(&self) -> bool {
        !self.wrap_boundaries.is_empty() && !super::glyphs_in_byte_order(&self.unwrapped_layout)
    }

    /// QUILL: `_index_for_position` for a line with [`Self::has_unordered_rows`].
    fn row_index_for_position(
        &self,
        position: Point<Pixels>,
        line_height: Pixels,
        closest: bool,
    ) -> Result<usize, usize> {
        let rows = super::wrapped_rows(&self.unwrapped_layout, &self.wrap_boundaries);
        let row_ix = (position.y / line_height).max(0.) as usize;
        let Some(row) = rows.get(row_ix) else {
            return Err(0);
        };
        if position.x < px(0.) {
            Err(row.closest_index_for_x(px(0.)))
        } else if position.x >= row.width {
            Err(row.closest_index_for_x(row.width))
        } else if closest {
            Ok(row.closest_index_for_x(position.x))
        } else {
            row.index_for_x(position.x).ok_or(row.len)
        }
    }

    /// Returns the pixel position for the given byte index.
    pub fn position_for_index(&self, index: usize, line_height: Pixels) -> Option<Point<Pixels>> {
        if self.has_unordered_rows() {
            let rows = super::wrapped_rows(&self.unwrapped_layout, &self.wrap_boundaries);
            let last = rows.len().saturating_sub(1);
            let mut row_start = 0;
            for (row_ix, row) in rows.iter().enumerate() {
                if index >= row_start && (index < row.len || row_ix == last) {
                    return (index <= self.len()).then(|| {
                        point(row.x_for_index(index), line_height * row_ix as f32)
                    });
                }
                row_start = row.len;
            }
            return None;
        }
        let mut line_start_ix = 0;
        let line_end_indices = self
            .wrap_boundaries
            .iter()
            .map(|wrap_boundary| {
                let run = &self.unwrapped_layout.runs[wrap_boundary.run_ix];
                let glyph = &run.glyphs[wrap_boundary.glyph_ix];
                glyph.index
            })
            .chain([self.len()])
            .enumerate();
        for (ix, line_end_ix) in line_end_indices {
            let line_y = ix as f32 * line_height;
            if index < line_start_ix {
                break;
            } else if index > line_end_ix {
                line_start_ix = line_end_ix;
                continue;
            } else {
                let line_start_x = self.unwrapped_layout.x_for_index(line_start_ix);
                let x = self.unwrapped_layout.x_for_index(index) - line_start_x;
                return Some(point(x, line_y));
            }
        }

        None
    }
}

pub(crate) struct LineLayoutCache {
    state: RefCell<LineLayoutCacheState>,
    wrap_scratch: RefCell<WrapScratch>,
    platform_text_system: Arc<dyn PlatformTextSystem>,
    /// Advances when [`TextSystem::add_fonts`] successfully changes the font database.
    font_generation: Arc<AtomicUsize>,
    /// Records the generation represented by both frame caches.
    cached_font_generation: AtomicUsize,
}

#[derive(Default)]
struct WrapScratch {
    glyph_indices: FxHashSet<usize>,
    break_indices: Vec<usize>,
}

#[derive(Default)]
struct LineLayoutCacheState {
    previous_frame: FrameCache,
    current_frame: FrameCache,
}

#[derive(Default)]
struct FrameCache {
    lines: FxHashMap<Arc<CacheKey>, Arc<LineLayout>>,
    wrapped_lines: FxHashMap<Arc<CacheKey>, Arc<WrappedLineLayout>>,
    used_lines: Vec<Arc<CacheKey>>,
    used_wrapped_lines: Vec<Arc<CacheKey>>,
}

#[derive(Clone, Default)]
pub(crate) struct LineLayoutIndex {
    font_generation: usize,
    lines_index: usize,
    wrapped_lines_index: usize,
}

impl LineLayoutCache {
    pub fn new(
        platform_text_system: Arc<dyn PlatformTextSystem>,
        font_generation: Arc<AtomicUsize>,
    ) -> Self {
        let cached_font_generation = font_generation.load(Ordering::Acquire);
        Self {
            state: RefCell::default(),
            wrap_scratch: RefCell::default(),
            platform_text_system,
            font_generation,
            cached_font_generation: AtomicUsize::new(cached_font_generation),
        }
    }

    pub fn layout_index(&self) -> LineLayoutIndex {
        let font_generation = self.clear_if_font_generation_changed();
        let state = self.state.borrow();
        let frame = &state.current_frame;
        LineLayoutIndex {
            font_generation,
            lines_index: frame.used_lines.len(),
            wrapped_lines_index: frame.used_wrapped_lines.len(),
        }
    }

    pub fn reuse_layouts(&self, range: Range<LineLayoutIndex>) {
        let font_generation = self.clear_if_font_generation_changed();
        if range.start.font_generation != font_generation
            || range.end.font_generation != font_generation
        {
            return;
        }
        let mut state = self.state.borrow_mut();
        let LineLayoutCacheState {
            previous_frame,
            current_frame,
        } = &mut *state;

        for key in &previous_frame.used_lines[range.start.lines_index..range.end.lines_index] {
            if let Some((key, line)) = previous_frame.lines.remove_entry(key) {
                current_frame.lines.insert(key, line);
            }
            current_frame.used_lines.push(key.clone());
        }

        for key in &previous_frame.used_wrapped_lines
            [range.start.wrapped_lines_index..range.end.wrapped_lines_index]
        {
            if let Some((key, line)) = previous_frame.wrapped_lines.remove_entry(key) {
                current_frame.wrapped_lines.insert(key, line);
            }
            current_frame.used_wrapped_lines.push(key.clone());
        }
    }

    pub fn truncate_layouts(&self, index: LineLayoutIndex) {
        let font_generation = self.clear_if_font_generation_changed();
        if index.font_generation != font_generation {
            return;
        }
        let mut state = self.state.borrow_mut();
        let current_frame = &mut state.current_frame;
        current_frame.used_lines.truncate(index.lines_index);
        current_frame
            .used_wrapped_lines
            .truncate(index.wrapped_lines_index);
    }

    pub fn finish_frame(&self) {
        let _font_generation = self.clear_if_font_generation_changed();
        let LineLayoutCacheState {
            previous_frame,
            current_frame,
        } = &mut *self.state.borrow_mut();
        std::mem::swap(previous_frame, current_frame);
        current_frame.lines.clear();
        current_frame.wrapped_lines.clear();
        current_frame.used_lines.clear();
        current_frame.used_wrapped_lines.clear();
    }

    pub fn layout_wrapped_line<Text>(
        &self,
        text: Text,
        font_size: Pixels,
        runs: &[FontRun],
        wrap_width: Option<Pixels>,
        max_lines: Option<usize>,
    ) -> Arc<WrappedLineLayout>
    where
        Text: AsRef<str>,
        SharedString: From<Text>,
    {
        let _font_generation = self.clear_if_font_generation_changed();
        let key = &CacheKeyRef {
            text: text.as_ref(),
            font_size,
            runs,
            wrap_width,
            force_width: None,
        } as &dyn AsCacheKeyRef;

        let state = self.state.borrow();
        if let Some(layout) = state.current_frame.wrapped_lines.get(key) {
            return layout.clone();
        }
        drop(state);

        let mut state = self.state.borrow_mut();
        let previous_frame_entry = state.previous_frame.wrapped_lines.remove_entry(key);
        if let Some((key, layout)) = previous_frame_entry {
            state
                .current_frame
                .wrapped_lines
                .insert(key.clone(), layout.clone());
            state.current_frame.used_wrapped_lines.push(key);
            layout
        } else {
            drop(state);
            let text = SharedString::from(text);
            let unwrapped_layout = self.layout_line::<&SharedString>(&text, font_size, runs, None);
            let (unwrapped_layout, wrap_boundaries) = match wrap_width {
                // QUILL: a line holding right-to-left text wraps in text order, each row
                // laid out on its own (`logical_row_starts`, `layout_rows`).
                Some(wrap_width) if !super::glyphs_in_byte_order(&unwrapped_layout) => {
                    let starts =
                        unwrapped_layout.logical_row_starts(text.as_ref(), wrap_width, max_lines);
                    if starts.is_empty() {
                        (unwrapped_layout, SmallVec::new())
                    } else {
                        self.layout_rows(text.as_ref(), font_size, runs, &starts)
                    }
                }
                Some(wrap_width) => {
                    let boundaries = unwrapped_layout.compute_wrap_boundaries(
                        text.as_ref(),
                        wrap_width,
                        max_lines,
                        &mut self.wrap_scratch.borrow_mut(),
                    );
                    (unwrapped_layout, boundaries)
                }
                None => (unwrapped_layout, SmallVec::new()),
            };
            let layout = Arc::new(WrappedLineLayout {
                unwrapped_layout,
                wrap_boundaries,
                wrap_width,
            });
            let key = Arc::new(CacheKey {
                text,
                font_size,
                runs: SmallVec::from(runs),
                wrap_width,
                force_width: None,
            });

            let mut state = self.state.borrow_mut();
            state
                .current_frame
                .wrapped_lines
                .insert(key.clone(), layout.clone());
            state.current_frame.used_wrapped_lines.push(key);

            layout
        }
    }

    /// QUILL: lays the rows of a wrapped line holding right-to-left text out one by one,
    /// each in the paragraph's direction, and joins them into one line whose rows follow
    /// each other left to right, with a wrap boundary at the first glyph of each row after
    /// the first. `starts` are the byte offsets the rows after the first start at.
    fn layout_rows(
        &self,
        text: &str,
        font_size: Pixels,
        runs: &[FontRun],
        starts: &[usize],
    ) -> (Arc<LineLayout>, SmallVec<[WrapBoundary; 1]>) {
        let rtl = unicode_bidi::get_base_direction_full(text) == unicode_bidi::Direction::Rtl;
        let mut line = LineLayout {
            font_size,
            len: text.len(),
            ..Default::default()
        };
        let mut boundaries = SmallVec::new();
        let mut x = px(0.);
        let mut start = 0;
        for end in starts.iter().copied().chain([text.len()]) {
            let row = self.layout_row(text, start..end, font_size, runs, rtl);
            let first_run = line.runs.len();
            for mut run in row.runs {
                for glyph in &mut run.glyphs {
                    glyph.position.x += x;
                    glyph.index += start;
                }
                if !run.glyphs.is_empty() {
                    line.runs.push(run);
                }
            }
            if start > 0 && line.runs.len() > first_run {
                boundaries.push(WrapBoundary {
                    run_ix: first_run,
                    glyph_ix: 0,
                });
            }
            if row.ascent > line.ascent {
                line.ascent = row.ascent;
            }
            if row.descent > line.descent {
                line.descent = row.descent;
            }
            x += row.width;
            start = end;
        }
        line.width = x;
        (Arc::new(line), boundaries)
    }

    /// One row of a wrapped line: the bytes `range` of `text` laid out in the paragraph's
    /// direction (`rtl`) whatever character the row starts with, glyph indices relative
    /// to the row. A row whose own first strong character reads the other way is shaped
    /// behind a direction mark, which is then taken out again.
    fn layout_row(
        &self,
        text: &str,
        range: Range<usize>,
        font_size: Pixels,
        runs: &[FontRun],
        rtl: bool,
    ) -> LineLayout {
        let row_text = text.get(range.clone()).unwrap_or_default();
        let row_runs = font_runs_in(runs, range);
        let plain = self.layout_line(row_text.to_string(), font_size, &row_runs, None);
        let row_rtl =
            unicode_bidi::get_base_direction_full(row_text) == unicode_bidi::Direction::Rtl;
        if row_rtl == rtl || row_text.is_empty() {
            return copy_layout(&plain);
        }
        let mark = if rtl { '\u{200F}' } else { '\u{200E}' };
        let mark_len = mark.len_utf8();
        let mut marked_runs = row_runs;
        if let Some(first) = marked_runs.first_mut() {
            first.len += mark_len;
        }
        let marked = self.layout_line(format!("{mark}{row_text}"), font_size, &marked_runs, None);
        // The mark sits at the row's start edge: the right end of a right-to-left row,
        // the left end of a left-to-right one. Whatever room it took goes.
        let advance = (marked.width - plain.width).max(px(0.));
        let mut row = copy_layout(&marked);
        for run in &mut row.runs {
            run.glyphs.retain(|glyph| glyph.index >= mark_len);
            for glyph in &mut run.glyphs {
                glyph.index -= mark_len;
                if !rtl {
                    glyph.position.x -= advance;
                }
            }
        }
        row.runs.retain(|run| !run.glyphs.is_empty());
        row.width = marked.width - advance;
        row.len = row_text.len();
        row
    }

    pub fn layout_line<Text>(
        &self,
        text: Text,
        font_size: Pixels,
        runs: &[FontRun],
        force_width: Option<Pixels>,
    ) -> Arc<LineLayout>
    where
        Text: AsRef<str>,
        SharedString: From<Text>,
    {
        let _font_generation = self.clear_if_font_generation_changed();
        let key = &CacheKeyRef {
            text: text.as_ref(),
            font_size,
            runs,
            wrap_width: None,
            force_width,
        } as &dyn AsCacheKeyRef;

        let state = self.state.borrow();
        if let Some(layout) = state.current_frame.lines.get(key) {
            return layout.clone();
        }
        drop(state);

        let mut state = self.state.borrow_mut();
        let LineLayoutCacheState {
            previous_frame,
            current_frame,
        } = &mut *state;
        if let Some((key, layout)) = previous_frame.lines.remove_entry(key) {
            current_frame.lines.insert(key.clone(), layout.clone());
            current_frame.used_lines.push(key);
            layout
        } else {
            let text = SharedString::from(text);
            let mut layout = self
                .platform_text_system
                .layout_line(&text, font_size, runs);

            if let Some(force_width) = force_width {
                apply_force_width_to_layout(&mut layout, force_width);
            }

            let key = Arc::new(CacheKey {
                text,
                font_size,
                runs: SmallVec::from(runs),
                wrap_width: None,
                force_width,
            });
            let layout = Arc::new(layout);
            current_frame.lines.insert(key.clone(), layout.clone());
            current_frame.used_lines.push(key);
            layout
        }
    }

    fn clear_if_font_generation_changed(&self) -> usize {
        let font_generation = self.font_generation.load(Ordering::Acquire);
        if self.cached_font_generation.load(Ordering::Acquire) == font_generation {
            return font_generation;
        }

        let mut state = self.state.borrow_mut();
        if self.cached_font_generation.load(Ordering::Acquire) == font_generation {
            return font_generation;
        }

        *state = LineLayoutCacheState::default();
        self.cached_font_generation
            .store(font_generation, Ordering::Release);
        font_generation
    }
}

// Combining marks (e.g. Thai vowel signs, Arabic diacritics) are shaped by
// HarfBuzz at the same x position as their base character. The force-width
// loop must not advance the cell counter for these zero-advance glyphs,
// otherwise they get displaced into the next cell. We detect them by checking
// whether shaped x has advanced by at least half a cell beyond the last base.
fn apply_force_width_to_layout(layout: &mut LineLayout, force_width: Pixels) {
    let mut glyph_pos: usize = 0;
    // NEG_INFINITY ensures the first glyph is always classified as a base.
    let mut last_base_shaped_x = px(f32::NEG_INFINITY);
    let mut last_base_actual_x = px(0.);

    for run in layout.runs.iter_mut() {
        for glyph in run.glyphs.iter_mut() {
            let shaped_x = glyph.position.x;

            if shaped_x > last_base_shaped_x + force_width * 0.5 {
                let forced_x = glyph_pos * force_width;
                if (shaped_x - forced_x).abs() > px(1.) {
                    glyph.position.x = forced_x;
                }
                last_base_shaped_x = shaped_x;
                last_base_actual_x = glyph.position.x;
                glyph_pos += 1;
            } else {
                glyph.position.x = last_base_actual_x + (shaped_x - last_base_shaped_x);
            }
        }
    }
}

/// A run of text with a single font.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
#[expect(missing_docs)]
pub struct FontRun {
    pub len: usize,
    pub font_id: FontId,
}

trait AsCacheKeyRef {
    fn as_cache_key_ref(&self) -> CacheKeyRef<'_>;
}

#[derive(Clone, Debug, Eq)]
struct CacheKey {
    text: SharedString,
    font_size: Pixels,
    runs: SmallVec<[FontRun; 1]>,
    wrap_width: Option<Pixels>,
    force_width: Option<Pixels>,
}

#[derive(Copy, Clone, PartialEq, Eq, Hash)]
struct CacheKeyRef<'a> {
    text: &'a str,
    font_size: Pixels,
    runs: &'a [FontRun],
    wrap_width: Option<Pixels>,
    force_width: Option<Pixels>,
}

impl PartialEq for dyn AsCacheKeyRef + '_ {
    fn eq(&self, other: &dyn AsCacheKeyRef) -> bool {
        self.as_cache_key_ref() == other.as_cache_key_ref()
    }
}

impl Eq for dyn AsCacheKeyRef + '_ {}

impl Hash for dyn AsCacheKeyRef + '_ {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_cache_key_ref().hash(state)
    }
}

impl AsCacheKeyRef for CacheKey {
    fn as_cache_key_ref(&self) -> CacheKeyRef<'_> {
        CacheKeyRef {
            text: &self.text,
            font_size: self.font_size,
            runs: self.runs.as_slice(),
            wrap_width: self.wrap_width,
            force_width: self.force_width,
        }
    }
}

impl PartialEq for CacheKey {
    fn eq(&self, other: &Self) -> bool {
        self.as_cache_key_ref().eq(&other.as_cache_key_ref())
    }
}

impl Hash for CacheKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_cache_key_ref().hash(state);
    }
}

impl<'a> Borrow<dyn AsCacheKeyRef + 'a> for Arc<CacheKey> {
    fn borrow(&self) -> &(dyn AsCacheKeyRef + 'a) {
        self.as_ref() as &dyn AsCacheKeyRef
    }
}

impl AsCacheKeyRef for CacheKeyRef<'_> {
    fn as_cache_key_ref(&self) -> CacheKeyRef<'_> {
        *self
    }
}

/// QUILL: a copy of a line layout (it is not `Clone` upstream).
fn copy_layout(layout: &LineLayout) -> LineLayout {
    LineLayout {
        font_size: layout.font_size,
        width: layout.width,
        ascent: layout.ascent,
        descent: layout.descent,
        runs: layout.runs.clone(),
        len: layout.len,
    }
}

/// QUILL: the font runs that cover the bytes `range` of a line, cut to it.
fn font_runs_in(runs: &[FontRun], range: Range<usize>) -> Vec<FontRun> {
    let mut out = Vec::new();
    let mut start = 0;
    for run in runs {
        let end = start + run.len;
        let from = start.max(range.start);
        let to = end.min(range.end);
        if from < to {
            out.push(FontRun {
                len: to - from,
                font_id: run.font_id,
            });
        }
        start = end;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{GlyphId, NoopTextSystem, TextRun, TextSystem, WindowTextSystem, font};

    #[test]
    fn shape_line_shares_its_text_with_the_layout_cache() {
        let text_system =
            WindowTextSystem::new(Arc::new(TextSystem::new(Arc::new(NoopTextSystem::new()))));
        // Long enough to be stored on the heap rather than inline.
        let text = SharedString::from("let value = compute_something(argument_one, argument_two);");
        let runs = [TextRun {
            len: text.len(),
            font: font(".ZedMono"),
            ..Default::default()
        }];

        text_system.shape_line(text.clone(), px(14.), &runs, None);
        let state = text_system.line_layout_cache.state.borrow();
        let (key, _) = state.current_frame.lines.iter().next().unwrap();
        assert_eq!(key.text.as_ptr(), text.as_ptr());
    }

    fn glyph_at(x: f32, index: usize) -> ShapedGlyph {
        ShapedGlyph {
            id: GlyphId(0),
            position: point(px(x), px(0.)),
            index,
            is_emoji: false,
        }
    }

    fn make_layout(glyphs: Vec<ShapedGlyph>) -> LineLayout {
        LineLayout {
            font_size: px(16.),
            width: px(100.),
            ascent: px(12.),
            descent: px(4.),
            runs: vec![ShapedRun {
                font_id: FontId(0),
                glyphs,
            }],
            len: 0,
        }
    }

    fn glyph_x_positions(layout: &LineLayout) -> Vec<f32> {
        layout.runs[0]
            .glyphs
            .iter()
            .map(|g| f32::from(g.position.x))
            .collect()
    }

    #[test]
    fn test_wrap_preserves_shaped_clusters() {
        let mut layout = make_layout(vec![
            glyph_at(0., 0),
            glyph_at(5., 0),
            glyph_at(10., 3),
            glyph_at(15., 3),
        ]);
        layout.width = px(20.);
        layout.len = "你好".len();
        assert_eq!(
            layout
                .compute_wrap_boundaries("你好", px(15.), None, &mut WrapScratch::default())
                .as_slice(),
            &[WrapBoundary {
                run_ix: 0,
                glyph_ix: 2
            }],
        );

        let mut layout = make_layout(vec![glyph_at(0., 0), glyph_at(10., 0)]);
        layout.width = px(20.);
        layout.len = 1;
        assert!(
            layout
                .compute_wrap_boundaries("a", px(10.), None, &mut WrapScratch::default())
                .is_empty()
        );
    }

    #[test]
    fn test_wrap_right_to_left_text() {
        for (text, indices, expected_boundaries) in [
            ("אבג דהו", vec![11, 9, 7, 6, 4, 2, 0], vec![4]),
            ("a אבג b", vec![0, 1, 6, 4, 2, 8, 9], vec![2, 6]),
        ] {
            let mut layout = make_layout(
                indices
                    .iter()
                    .enumerate()
                    .map(|(position, index)| glyph_at(position as f32 * 10., *index))
                    .collect(),
            );
            layout.width = px(indices.len() as f32 * 10.);
            layout.len = text.len();
            assert_eq!(
                layout.compute_wrap_boundaries(text, px(40.), None, &mut WrapScratch::default()),
                expected_boundaries
                    .into_iter()
                    .map(|glyph_ix| WrapBoundary {
                        run_ix: 0,
                        glyph_ix
                    })
                    .collect::<SmallVec<[WrapBoundary; 1]>>(),
                "{text:?}",
            );
        }
    }

    #[test]
    fn test_wrap_ignores_breaks_inside_shaped_clusters() {
        let text = "x a-bcde";
        let mut layout = make_layout(
            [0, 1, 2, 5, 6, 7]
                .into_iter()
                .map(|index| glyph_at(index as f32 * 10., index))
                .collect(),
        );
        layout.width = px(80.);
        layout.len = text.len();
        assert_eq!(
            layout
                .compute_wrap_boundaries(text, px(60.), None, &mut WrapScratch::default())
                .as_slice(),
            &[WrapBoundary {
                run_ix: 0,
                glyph_ix: 2
            }],
        );
    }

    #[test]
    fn test_force_width_latin_unchanged() {
        let cell_width = px(8.);
        let mut layout = make_layout(vec![glyph_at(0., 0), glyph_at(8., 1), glyph_at(16., 2)]);

        apply_force_width_to_layout(&mut layout, cell_width);

        let positions = glyph_x_positions(&layout);
        assert_eq!(positions, vec![0., 8., 16.]);
    }

    #[test]
    fn test_force_width_combining_marks_not_advanced() {
        let cell_width = px(8.);
        // Simulates Thai "กี" — base consonant at x=0, combining vowel also at x=0
        let mut layout = make_layout(vec![
            glyph_at(0., 0), // ก (base)
            glyph_at(0., 3), // ี (combining mark, same x)
        ]);

        apply_force_width_to_layout(&mut layout, cell_width);

        let positions = glyph_x_positions(&layout);
        assert_eq!(positions, vec![0., 0.]);
    }

    #[test]
    fn test_force_width_base_after_combining_mark() {
        let cell_width = px(8.);
        let mut layout = make_layout(vec![glyph_at(0., 0), glyph_at(0., 3), glyph_at(8., 6)]);

        apply_force_width_to_layout(&mut layout, cell_width);

        let positions = glyph_x_positions(&layout);
        assert_eq!(positions, vec![0., 0., 8.]);
    }

    #[test]
    fn test_force_width_multiple_combining_marks() {
        let cell_width = px(8.);
        // Simulates "ก้" — base + vowel + tone mark (two combining marks stacked)
        let mut layout = make_layout(vec![
            glyph_at(0., 0), // ก (base)
            glyph_at(0., 3), // vowel (combining)
            glyph_at(0., 6), // tone mark (combining)
            glyph_at(8., 9), // next base
        ]);

        apply_force_width_to_layout(&mut layout, cell_width);

        let positions = glyph_x_positions(&layout);
        assert_eq!(positions, vec![0., 0., 0., 8.]);
    }

    #[test]
    fn test_force_width_corrects_drifted_base_positions() {
        let cell_width = px(8.);
        // Font metrics don't perfectly match cell grid — glyphs drift >1px from cell boundary
        let mut layout = make_layout(vec![
            glyph_at(0.5, 0),  // within 1px tolerance, kept as-is
            glyph_at(10.2, 1), // >1px off from 8.0, corrected
            glyph_at(19.8, 2), // >1px off from 16.0, corrected
        ]);

        apply_force_width_to_layout(&mut layout, cell_width);

        let positions = glyph_x_positions(&layout);
        assert_eq!(positions, vec![0.5, 8., 16.]);
    }

    #[test]
    fn test_force_width_combining_mark_after_within_tolerance_base() {
        let cell_width = px(8.);
        // Base glyph is within 1px of grid so it keeps its shaped position.
        // The combining mark must align to the base's actual position, not the grid slot.
        let mut layout = make_layout(vec![glyph_at(0.5, 0), glyph_at(0.5, 3)]);

        apply_force_width_to_layout(&mut layout, cell_width);

        let positions = glyph_x_positions(&layout);
        assert_eq!(positions, vec![0.5, 0.5]);
    }
}
