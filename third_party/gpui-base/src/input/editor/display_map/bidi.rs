//! Bidirectional text for the input engine (Unicode Bidirectional Algorithm,
//! UAX #9), the way Qt's `QTextEdit` in Telegram Desktop's `InputField` lays
//! out a paragraph:
//!
//! * every paragraph (buffer line) gets its own base direction from its first
//!   strong character (rules P2/P3, `Qt::LayoutDirectionAuto`); a paragraph
//!   with no strong character is left-to-right;
//! * a right-to-left paragraph is aligned to the right edge, its caret starts
//!   there and advances leftwards;
//! * runs of the other direction (digits, Latin words) are laid out inside the
//!   paragraph in visual order, per rules L1/L2.
//!
//! GPUI's platform text systems disagree on the base direction of a line
//! (CoreText and cosmic-text pick it from the first strong character,
//! DirectWrite always lays out left-to-right) and none of them can say where a
//! *logical* offset sits when a run is right-to-left. This module therefore
//! splits a visual line into runs of one embedding level, which the engine
//! shapes one by one (a single-direction run shapes identically on every
//! platform), places them in visual order itself, and maps caret offsets, hit
//! tests and selections through that placement.
//!
//! Everything in here is pure geometry over `f32` pixels so it can be tested
//! without a text system; [`clusters_from_glyphs`] is the one bridge from a
//! shaped run.
use std::ops::Range;

use unicode_bidi::{BidiClass, Level, ParagraphBidiInfo, bidi_class};
use unicode_segmentation::UnicodeSegmentation as _;

/// Two x positions closer than this are the same place.
const EPS: f32 = 0.01;

/// A caret step must move at least this far to count as a visual move.
const STEP_EPS: f32 = 0.5;

/// Whether `text` needs the bidi path at all: it holds a character that is
/// right-to-left or can start a right-to-left run. Pure ASCII and plain
/// left-to-right scripts take the unchanged fast path.
pub(crate) fn needs_bidi(text: &str) -> bool {
    !text.is_ascii()
        && text.chars().any(|c| {
            c as u32 >= 0x590
                && matches!(
                    bidi_class(c),
                    BidiClass::R
                        | BidiClass::AL
                        | BidiClass::AN
                        | BidiClass::RLE
                        | BidiClass::RLO
                        | BidiClass::RLI
                        | BidiClass::FSI
                )
        })
}

/// A buffer line analysed for bidi: the paragraph direction plus the resolved
/// embedding levels, from which each wrapped visual line gets its runs.
pub(crate) struct Paragraph<'a> {
    info: ParagraphBidiInfo<'a>,
    rtl: bool,
}

/// One run of a visual line: a byte range of one embedding level, in the
/// order it is displayed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct VisualRun {
    pub range: Range<usize>,
    pub rtl: bool,
}

impl<'a> Paragraph<'a> {
    /// Analyse one buffer line (no `\n`). `None` when the line is plain LTR
    /// text that needs no reordering.
    pub(crate) fn analyze(text: &'a str) -> Option<Self> {
        if !needs_bidi(text) {
            return None;
        }
        let info = ParagraphBidiInfo::new(text, None);
        let rtl = info.paragraph_level.is_rtl();
        Some(Self { info, rtl })
    }

    /// Whether the paragraph's base direction is right-to-left.
    pub(crate) fn is_rtl(&self) -> bool {
        self.rtl
    }

    /// The runs of the visual line `range` (a wrapped row of the paragraph),
    /// left to right. Trailing whitespace takes the paragraph level (L1).
    ///
    /// A right-to-left run is cut so that the neutral characters at its edges (spaces,
    /// punctuation, brackets) are runs of their own, one character each: shaped inside
    /// the run they would take the direction the platform guesses for it (DirectWrite
    /// always guesses left to right and puts a trailing space on the wrong side of the
    /// word), shaped alone they cannot be misplaced.
    pub(crate) fn visual_runs(&self, range: Range<usize>) -> Vec<VisualRun> {
        if range.is_empty() {
            return Vec::new();
        }
        // Level runs are cut on grapheme boundaries only: a cluster (an emoji with its variation
        // selector, skin tone or ZWJ parts, a flag, a keycap, a letter with its marks) takes the
        // level of its first character and is shaped in one piece, or the shaper cannot pick the
        // colour emoji font for it.
        let levels = self.info.reordered_levels(range.clone());
        let text = self.info.text;
        let mut logical: Vec<(Range<usize>, Level)> = Vec::new();
        for (i, cluster) in text[range.clone()].grapheme_indices(true) {
            let start = range.start + i;
            let level = levels[start];
            match logical.last_mut() {
                Some((run, run_level)) if *run_level == level => run.end = start + cluster.len(),
                _ => logical.push((start..start + cluster.len(), level)),
            }
        }
        let run_levels: Vec<Level> = logical.iter().map(|(_, level)| *level).collect();
        let mut out = Vec::with_capacity(logical.len());
        for index in ParagraphBidiInfo::reorder_visual(&run_levels) {
            let (run, level) = &logical[index];
            if level.is_rtl() {
                out.extend(split_rtl_run(text, run.clone()));
            } else {
                out.push(VisualRun {
                    range: run.clone(),
                    rtl: false,
                });
            }
        }
        out
    }
}

/// Whether `c` is a strong right-to-left letter.
fn is_rtl_letter(c: char) -> bool {
    matches!(bidi_class(c), BidiClass::R | BidiClass::AL)
}

/// Splits one right-to-left run into its pieces, left to right: the neutral characters
/// after the last letter (one piece each, last first), the core from the first letter to
/// the last (with any combining marks that follow it), and the neutral characters before
/// the first letter (one piece each, last first). A run with no letter at all is cut into
/// single characters.
fn split_rtl_run(text: &str, run: Range<usize>) -> Vec<VisualRun> {
    let slice = &text[run.clone()];
    let first = slice.char_indices().find(|&(_, c)| is_rtl_letter(c));
    let single = |start: usize, end: usize| -> Vec<VisualRun> {
        // One piece per grapheme cluster, visually reversed.
        text[start..end]
            .grapheme_indices(true)
            .map(|(i, g)| VisualRun {
                range: start + i..start + i + g.len(),
                rtl: true,
            })
            .rev()
            .collect()
    };
    let Some((first, _)) = first else {
        return single(run.start, run.end);
    };
    let (last, last_char) = slice
        .char_indices()
        .rev()
        .find(|&(_, c)| is_rtl_letter(c))
        .unwrap_or((first, ' '));
    let mut core_end = last + last_char.len_utf8();
    // Combining marks belong to the letter before them.
    for c in slice[core_end..].chars() {
        if matches!(bidi_class(c), BidiClass::NSM | BidiClass::BN) {
            core_end += c.len_utf8();
        } else {
            break;
        }
    }
    let core = run.start + first..run.start + core_end;
    let mut pieces = single(core.end, run.end);
    pieces.push(VisualRun {
        range: core.clone(),
        rtl: true,
    });
    pieces.extend(single(run.start, core.start));
    pieces
}

/// Mirrors brackets for a run that has no strong character of its own, so the
/// platform shaper has no right-to-left context to mirror them from (a lone
/// `)` between two Latin words in a Hebrew paragraph). Only characters whose
/// mirror has the same UTF-8 length are replaced so byte ranges stay valid.
pub(crate) fn mirror_neutral_run(text: &str) -> Option<String> {
    if text.chars().any(|c| {
        matches!(
            bidi_class(c),
            BidiClass::L | BidiClass::R | BidiClass::AL | BidiClass::EN | BidiClass::AN
        )
    }) {
        return None;
    }
    let mut changed = false;
    let mirrored: String = text
        .chars()
        .map(|c| match unicode_bidi_mirroring::get_mirrored(c) {
            Some(m) if m.len_utf8() == c.len_utf8() => {
                changed = true;
                m
            }
            _ => c,
        })
        .collect();
    changed.then_some(mirrored)
}

/// The visual extent of a run of characters sharing one shaped cluster
/// (usually one character).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Cluster {
    /// Byte range within the fragment.
    pub range: Range<usize>,
    /// Visual left and right edge within the fragment.
    pub left: f32,
    pub right: f32,
}

/// Builds the clusters of one single-direction fragment from its shaped
/// glyphs, as `(left x, byte index)` pairs in any order. Glyph indices are
/// cluster starts; the right edge of a glyph is the next distinct glyph x (or
/// the fragment width). Characters that carry no advance of their own
/// (combining marks, format characters) fold into the cluster before them.
pub(crate) fn clusters_from_glyphs(
    glyphs: &[(f32, usize)],
    width: f32,
    text: &str,
) -> Vec<Cluster> {
    if text.is_empty() {
        return Vec::new();
    }
    let mut xs: Vec<f32> = glyphs.iter().map(|g| g.0).collect();
    xs.sort_by(f32::total_cmp);
    xs.dedup_by(|a, b| (*a - *b).abs() < EPS);
    let right_of = |left: f32| {
        xs.iter()
            .copied()
            .find(|&x| x > left + EPS)
            .unwrap_or(width)
            .max(left)
    };

    // index -> (left, right), merged over glyphs sharing a cluster start.
    let mut by_index: Vec<(usize, f32, f32)> = Vec::new();
    for &(left, index) in glyphs {
        if index >= text.len() || !text.is_char_boundary(index) {
            continue;
        }
        let right = right_of(left);
        match by_index.iter_mut().find(|e| e.0 == index) {
            Some(entry) => {
                entry.1 = entry.1.min(left);
                entry.2 = entry.2.max(right);
            }
            None => by_index.push((index, left, right)),
        }
    }
    if by_index.is_empty() {
        return vec![Cluster {
            range: 0..text.len(),
            left: 0.,
            right: width,
        }];
    }
    by_index.sort_by_key(|e| e.0);

    let mut clusters: Vec<Cluster> = Vec::with_capacity(by_index.len());
    for (index, left, right) in by_index {
        let carries_no_advance = text[index..]
            .chars()
            .next()
            .is_some_and(|c| matches!(bidi_class(c), BidiClass::NSM | BidiClass::BN));
        match clusters.last_mut() {
            Some(prev) if carries_no_advance => prev.range.end = text.len(),
            _ => {
                if let Some(prev) = clusters.last_mut() {
                    prev.range.end = index;
                }
                clusters.push(Cluster {
                    range: index..text.len(),
                    left,
                    right,
                });
            }
        }
    }
    // Characters before the first glyph index belong to the first cluster.
    if let Some(first) = clusters.first_mut() {
        first.range.start = 0;
    }
    // Close the chain: every cluster ends where the next starts.
    for i in 0..clusters.len().saturating_sub(1) {
        clusters[i].range.end = clusters[i + 1].range.start;
    }
    clusters
}

/// The extent of a shaped fragment: `(shift, width)`, where `shift` is how far right the
/// shaped line must be moved so that the box holding every glyph starts at the fragment's
/// left edge, and `width` is that box's width.
///
/// Platforms let trailing whitespace hang outside the width they report: CoreText puts it at
/// a negative x in a right-to-left run, and past `reported_width` in a left-to-right one.
/// `rightmost_advance` is the advance of the glyph that sits furthest right, since nothing
/// follows it to measure its right edge from.
pub(crate) fn fragment_extent(
    glyph_xs: &[f32],
    reported_width: f32,
    rightmost_advance: f32,
) -> (f32, f32) {
    let left = glyph_xs.iter().copied().fold(0f32, f32::min);
    let rightmost = glyph_xs.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let right = if rightmost.is_finite() {
        reported_width.max(rightmost + rightmost_advance)
    } else {
        reported_width
    };
    (-left, right - left)
}

/// One shaped single-direction run of a visual line, placed at `x`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Fragment {
    /// Byte range within the visual line (sub-line).
    pub range: Range<usize>,
    pub rtl: bool,
    /// Left edge of the fragment within the visual line.
    pub x: f32,
    pub width: f32,
    /// The fragment's text (for character counting inside a cluster).
    pub text: String,
    pub clusters: Vec<Cluster>,
    /// How far right the shaped line is painted from `x` so that its glyph box starts at `x`
    /// (platforms may hang trailing whitespace outside the shaped width).
    pub paint_shift: f32,
}

impl Fragment {
    pub(crate) fn new(
        range: Range<usize>,
        rtl: bool,
        x: f32,
        width: f32,
        text: String,
        clusters: Vec<Cluster>,
    ) -> Self {
        Self {
            range,
            rtl,
            x,
            width,
            text,
            clusters,
            paint_shift: 0.,
        }
    }

    fn len(&self) -> usize {
        self.range.len()
    }

    fn cluster_at(&self, local: usize) -> Option<&Cluster> {
        let ix = self.clusters.partition_point(|c| c.range.end <= local);
        self.clusters.get(ix).filter(|c| c.range.start <= local)
    }

    /// x of the caret at the character boundary `local` (a byte offset in the
    /// fragment), relative to the fragment's left edge: the leading edge of
    /// the character that starts there, and the logical end edge at the end.
    fn pos(&self, local: usize) -> f32 {
        if local >= self.len() {
            return if self.rtl { 0. } else { self.width };
        }
        // A caller can hand over an offset inside a multi-byte character; use its start.
        let local = self.text.floor_char_boundary(local);
        let Some(cluster) = self.cluster_at(local) else {
            return if self.rtl { self.width } else { 0. };
        };
        let chars = self.text[cluster.range.clone()].chars().count().max(1);
        let before = self.text[cluster.range.start..local].chars().count();
        let frac = before as f32 / chars as f32;
        let span = cluster.right - cluster.left;
        if self.rtl {
            cluster.right - span * frac
        } else {
            cluster.left + span * frac
        }
    }

    /// Character boundaries of the fragment, as byte offsets within it.
    fn boundaries(&self) -> impl Iterator<Item = usize> + '_ {
        self.text
            .char_indices()
            .map(|(i, _)| i)
            .chain(std::iter::once(self.text.len()))
    }
}

/// One visual line: its fragments in display order.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct BidiLine {
    pub len: usize,
    pub width: f32,
    /// Display order, left to right.
    pub fragments: Vec<Fragment>,
}

impl BidiLine {
    pub(crate) fn new(len: usize, fragments: Vec<Fragment>) -> Self {
        let width = fragments.last().map_or(0., |f| f.x + f.width);
        Self {
            len,
            width,
            fragments,
        }
    }

    /// x of the caret at the character boundary `offset`: the leading edge of
    /// the character that starts there, or the logical end of the line at its
    /// end (the left edge of a right-to-left run, the right edge of a
    /// left-to-right one).
    pub(crate) fn x_for_index(&self, offset: usize) -> f32 {
        if offset >= self.len {
            return self
                .fragments
                .iter()
                .max_by_key(|f| f.range.end)
                .map_or(0., |f| f.x + f.pos(f.len()));
        }
        match self
            .fragments
            .iter()
            .find(|f| f.range.start <= offset && offset < f.range.end)
        {
            Some(f) => f.x + f.pos(offset - f.range.start),
            None => self.width,
        }
    }

    /// x of the caret at `offset` seen from the character *before* it: that character's
    /// trailing edge. Equal to [`Self::x_for_index`] unless the two characters around `offset`
    /// sit in runs of different directions, where an offset has two places on screen.
    pub(crate) fn x_for_index_trailing(&self, offset: usize) -> f32 {
        if offset == 0 || offset > self.len {
            return self.x_for_index(offset);
        }
        match self
            .fragments
            .iter()
            .find(|f| f.range.start < offset && offset <= f.range.end)
        {
            Some(f) => f.x + f.pos(offset - f.range.start),
            None => self.x_for_index(offset),
        }
    }

    /// Whether `offset` is drawn in two different places: after the character before it and
    /// before the character after it (a direction boundary).
    pub(crate) fn has_two_stops(&self, offset: usize) -> bool {
        offset > 0
            && offset < self.len
            && (self.x_for_index(offset) - self.x_for_index_trailing(offset)).abs() > STEP_EPS
    }

    /// x of a caret stop: `trailing` is the edge of the character before `offset`.
    pub(crate) fn stop_x(&self, offset: usize, trailing: bool) -> f32 {
        if trailing {
            self.x_for_index_trailing(offset)
        } else {
            self.x_for_index(offset)
        }
    }

    /// The fragment a caret stop hangs on: the one holding the character before a trailing
    /// stop, else the one holding the character after it (the last one at the end).
    fn stop_owner(&self, offset: usize, trailing: bool) -> Option<usize> {
        if trailing {
            self.fragments
                .iter()
                .position(|f| f.range.start < offset && offset <= f.range.end)
        } else {
            self.fragments
                .iter()
                .position(|f| f.range.start <= offset && offset < f.range.end)
                .or_else(|| self.fragments.iter().position(|f| f.range.end == offset))
        }
    }

    /// The character boundary closest to `x`.
    pub(crate) fn closest_index_for_x(&self, x: f32) -> usize {
        self.closest_stop_for_x(x).0
    }

    /// The caret stop closest to `x`: the boundary offset and whether it is the trailing edge
    /// of the character before it. The stop belongs to the fragment under `x`, so a click at a
    /// direction boundary lands on the side that was clicked.
    pub(crate) fn closest_stop_for_x(&self, x: f32) -> (usize, bool) {
        let Some(first) = self.fragments.first() else {
            return (0, false);
        };
        // The fragment under `x`, clamping to the outermost ones.
        let fragment = if x < first.x {
            first
        } else {
            self.fragments
                .iter()
                .find(|f| x < f.x + f.width)
                .unwrap_or_else(|| self.fragments.last().unwrap_or(first))
        };
        fragment
            .boundaries()
            .map(|local| {
                let at = fragment.x + fragment.pos(local);
                let offset = fragment.range.start + local;
                let trailing = local > 0 && local == fragment.len() && self.has_two_stops(offset);
                ((offset, trailing), (at - x).abs())
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map_or((0, false), |(stop, _)| stop)
    }

    /// The character under `x`, as the byte offset it starts at; `None` past
    /// the end of the line.
    pub(crate) fn index_for_x(&self, x: f32) -> Option<usize> {
        if x >= self.width {
            return None;
        }
        let fragment = self
            .fragments
            .iter()
            .find(|f| x < f.x + f.width)
            .or(self.fragments.first())?;
        let local = (x - fragment.x).max(0.);
        let cluster = fragment
            .clusters
            .iter()
            .find(|c| local >= c.left && local < c.right)
            .or_else(|| fragment.clusters.first())?;
        Some(fragment.range.start + cluster.range.start)
    }

    /// The visual rectangles `[left, right]` covering the byte `range`, left
    /// to right with touching neighbours merged. A logical range spans several
    /// disjoint rectangles when it crosses a direction change.
    pub(crate) fn range_rects(&self, range: Range<usize>) -> Vec<(f32, f32)> {
        let mut rects: Vec<(f32, f32)> = Vec::new();
        for f in &self.fragments {
            let start = range.start.max(f.range.start);
            let end = range.end.min(f.range.end);
            if start >= end {
                continue;
            }
            let a = f.x + f.pos(start - f.range.start);
            let b = f.x + f.pos(end - f.range.start);
            let (left, right) = if a <= b { (a, b) } else { (b, a) };
            match rects.last_mut() {
                Some(last) if left <= last.1 + EPS => last.1 = last.1.max(right),
                _ => rects.push((left, right)),
            }
        }
        rects
    }

    /// The caret stop one step visually left (`left`) or right of the stop `(offset,
    /// trailing)`; `None` at the visual edge of the line.
    ///
    /// Where a boundary offset has two places on screen both are stops, and a place that
    /// several offsets share (the end of a run and the start of its neighbour touch) resolves
    /// to the stop of the run the caret is travelling in, so a caret walking right through
    /// "ab" + Hebrew visits the end of "ab" as offset 2 and later the other side of that
    /// offset, instead of jumping to the end of the Hebrew run.
    pub(crate) fn visual_step_stop(
        &self,
        offset: usize,
        trailing: bool,
        left: bool,
    ) -> Option<(usize, bool)> {
        let here = self.stop_x(offset, trailing);
        let owner = self.stop_owner(offset, trailing);
        let mut best: Option<((usize, bool), f32, bool)> = None;
        for (fi, f) in self.fragments.iter().enumerate() {
            for local in f.boundaries() {
                let boundary = f.range.start + local;
                let stop_trailing = local > 0 && local == f.len() && self.has_two_stops(boundary);
                if (boundary, stop_trailing) == (offset, trailing) {
                    continue;
                }
                let at = f.x + f.pos(local);
                let ahead = if left {
                    at < here - STEP_EPS
                } else {
                    at > here + STEP_EPS
                };
                if !ahead {
                    continue;
                }
                let same_owner = Some(fi) == owner;
                let better = match best {
                    None => true,
                    Some(((b, bt), bx, b_same)) => {
                        let nearer = if left { at > bx + EPS } else { at < bx - EPS };
                        let tie = (at - bx).abs() <= EPS
                            && ((same_owner && !b_same)
                                || (same_owner == b_same
                                    && (boundary.abs_diff(offset) < b.abs_diff(offset)
                                        || (boundary == b && bt && !stop_trailing))));
                        nearer || tie
                    }
                };
                if better {
                    best = Some(((boundary, stop_trailing), at, same_owner));
                }
            }
        }
        best.map(|(stop, ..)| stop)
    }

    /// The boundary one caret step visually left (`left`) or right of `offset`; `None` at the
    /// visual edge of the line.
    #[cfg(test)]
    pub(crate) fn visual_step(&self, offset: usize, left: bool) -> Option<usize> {
        self.visual_step_stop(offset, false, left).map(|(o, _)| o)
    }

    /// The character the caret passes over going from x `a` to x `b` (any order): the first
    /// character of the cluster that lies between them.
    pub(crate) fn char_between(&self, a: f32, b: f32) -> Option<char> {
        let mid = (a + b) / 2.;
        let f = self
            .fragments
            .iter()
            .find(|f| mid >= f.x && mid <= f.x + f.width)?;
        let local = mid - f.x;
        let cluster = f
            .clusters
            .iter()
            .find(|c| local >= c.left && local <= c.right)?;
        f.text[cluster.range.clone()].chars().next()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Cluster per char, `adv` wide each, laid out for `rtl` (first char at
    /// the right edge) or LTR.
    fn chars_fragment(text: &str, rtl: bool, start: usize, x: f32, adv: f32) -> Fragment {
        let n = text.chars().count();
        let width = adv * n as f32;
        let mut clusters = Vec::new();
        for (k, (i, c)) in text.char_indices().enumerate() {
            let (left, right) = if rtl {
                (width - adv * (k as f32 + 1.), width - adv * k as f32)
            } else {
                (adv * k as f32, adv * (k as f32 + 1.))
            };
            clusters.push(Cluster {
                range: i..i + c.len_utf8(),
                left,
                right,
            });
        }
        Fragment::new(
            start..start + text.len(),
            rtl,
            x,
            width,
            text.to_string(),
            clusters,
        )
    }

    /// Base direction of a paragraph per UAX #9 P2/P3.
    fn paragraph_is_rtl(text: &str) -> bool {
        Paragraph::analyze(text).is_some_and(|p| p.is_rtl())
    }

    #[test]
    fn direction_is_the_first_strong_character() {
        assert!(!paragraph_is_rtl("hello שלום"));
        assert!(paragraph_is_rtl("שלום hello"));
        assert!(paragraph_is_rtl("123 שלום"));
        assert!(paragraph_is_rtl("(שלום"));
        assert!(paragraph_is_rtl("مرحبا"));
    }

    #[test]
    fn neutral_only_paragraphs_are_left_to_right() {
        assert!(!paragraph_is_rtl(""));
        assert!(!paragraph_is_rtl("123 456"));
        assert!(!paragraph_is_rtl("... !?"));
        assert!(!paragraph_is_rtl("😀"));
    }

    #[test]
    fn plain_ltr_text_skips_the_bidi_path() {
        assert!(!needs_bidi("hello world"));
        assert!(!needs_bidi("héllo wörld 世界"));
        assert!(needs_bidi("hello שלום"));
        assert!(needs_bidi("مرحبا"));
        assert!(Paragraph::analyze("hello").is_none());
        assert!(Paragraph::analyze("שלום").is_some());
    }

    #[test]
    fn pure_hebrew_is_one_rtl_run() {
        let p = Paragraph::analyze("שלום").unwrap();
        assert!(p.is_rtl());
        assert_eq!(
            p.visual_runs(0..8),
            vec![VisualRun {
                range: 0..8,
                rtl: true
            }]
        );
    }

    #[test]
    fn mixed_runs_are_reordered_for_an_rtl_paragraph() {
        // א ב ␠ h e l l o ␠ ג ד (Hebrew letters are two bytes)
        let text = "אב hello גד";
        let p = Paragraph::analyze(text).unwrap();
        assert!(p.is_rtl());
        let runs = p.visual_runs(0..text.len());
        // Visual left-to-right: [גד], [hello], [אב␠]… with neutrals taking the
        // paragraph level: ג ד at the left end, "hello" in the middle.
        let hello = text.find("hello").unwrap();
        let ltr: Vec<_> = runs.iter().filter(|r| !r.rtl).collect();
        assert_eq!(ltr.len(), 1);
        assert_eq!(ltr[0].range, hello..hello + 5);
        // Logical first run is displayed last (rightmost).
        assert_eq!(runs.last().unwrap().range.start, 0);
        assert!(runs.first().unwrap().range.end == text.len());
    }

    #[test]
    fn ltr_paragraph_with_a_hebrew_word() {
        let text = "say שלום now";
        let p = Paragraph::analyze(text).unwrap();
        assert!(!p.is_rtl());
        let runs = p.visual_runs(0..text.len());
        assert_eq!(runs.len(), 3);
        assert!(!runs[0].rtl && runs[1].rtl && !runs[2].rtl);
        assert_eq!(runs[0].range.start, 0);
    }

    #[test]
    fn digits_inside_hebrew_stay_in_reading_order() {
        let text = "אב 123 גד";
        let p = Paragraph::analyze(text).unwrap();
        let runs = p.visual_runs(0..text.len());
        let digits = text.find("123").unwrap();
        let run = runs.iter().find(|r| r.range.start == digits).unwrap();
        assert!(!run.rtl, "digits are a left-to-right run");
        assert_eq!(run.range, digits..digits + 3);
    }

    fn ranges(runs: &[VisualRun]) -> Vec<Range<usize>> {
        runs.iter().map(|r| r.range.clone()).collect()
    }

    #[test]
    fn neutral_characters_at_the_edges_of_an_rtl_run_are_runs_of_their_own() {
        // "אב. ": the full stop and the space follow the letters in typing
        // order and so sit to their left, last first.
        let text = "אב. ";
        let p = Paragraph::analyze(text).unwrap();
        let runs = p.visual_runs(0..text.len());
        assert_eq!(ranges(&runs), vec![5..6, 4..5, 0..4]);
        assert!(runs.iter().all(|r| r.rtl));

        // Leading neutrals sit to the right of the letters.
        let text = "(אב";
        let p = Paragraph::analyze(text).unwrap();
        assert_eq!(ranges(&p.visual_runs(0..text.len())), vec![1..5, 0..1]);
    }

    #[test]
    fn neutrals_between_letters_stay_in_the_core() {
        let text = "אב, גד";
        let p = Paragraph::analyze(text).unwrap();
        let runs = p.visual_runs(0..text.len());
        assert_eq!(ranges(&runs), vec![0..text.len()]);
    }

    #[test]
    fn combining_marks_stay_with_the_last_letter() {
        // Bet + dagesh + qamats then a full stop: the points belong to the
        // letter, the stop does not.
        let text = "בּ\u{05B8}.";
        let p = Paragraph::analyze(text).unwrap();
        let end = text.len() - 1;
        assert_eq!(
            ranges(&p.visual_runs(0..text.len())),
            vec![end..text.len(), 0..end]
        );
    }

    #[test]
    fn a_run_without_letters_is_cut_into_single_characters() {
        // Inside "a (b) שלום" the brackets around the Latin letter sit in the
        // right-to-left paragraph level; each is a run of its own.
        let text = "שלום (a)";
        let p = Paragraph::analyze(text).unwrap();
        assert!(p.is_rtl());
        let runs = p.visual_runs(0..text.len());
        let open = text.find('(').unwrap();
        let close = text.find(')').unwrap();
        let single: Vec<_> = runs
            .iter()
            .filter(|r| r.rtl && r.range.len() == 1)
            .map(|r| r.range.start)
            .collect();
        assert!(single.contains(&open) && single.contains(&close));
        // Visually the close bracket comes first (leftmost).
        let at = |ix: usize| runs.iter().position(|r| r.range.start == ix).unwrap();
        assert!(at(close) < at(open));
    }

    /// Every run starts and ends on a grapheme cluster boundary.
    fn assert_clusters_whole(text: &str, runs: &[VisualRun]) {
        let boundaries: Vec<usize> = text
            .grapheme_indices(true)
            .map(|(i, _)| i)
            .chain(std::iter::once(text.len()))
            .collect();
        for run in runs {
            assert!(
                boundaries.contains(&run.range.start) && boundaries.contains(&run.range.end),
                "run {:?} cuts a cluster of {text:?}",
                run.range
            );
        }
        // And the runs still cover the text exactly once.
        let mut covered: Vec<_> = runs.iter().map(|r| r.range.clone()).collect();
        covered.sort_by_key(|r| r.start);
        let mut at = 0;
        for r in covered {
            assert_eq!(r.start, at);
            at = r.end;
        }
        assert_eq!(at, text.len());
    }

    const EMOJI: [&str; 5] = [
        "\u{261D}\u{FE0F}",
        "\u{1F44D}\u{1F3FD}",
        "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}",
        "\u{1F1EE}\u{1F1F1}",
        "1\u{FE0F}\u{20E3}",
    ];

    #[test]
    fn emoji_sequences_are_never_cut_by_a_run() {
        for emoji in EMOJI {
            // At the end, at the start, in the middle, and next to Latin text.
            for text in [
                format!("שלום עולם {emoji}"),
                format!("{emoji} שלום עולם"),
                format!("שלום {emoji} עולם"),
                format!("שלום עולם abc {emoji}"),
                format!("abc {emoji} שלום"),
                format!("שלום{emoji}"),
            ] {
                let p = Paragraph::analyze(&text).unwrap();
                assert_clusters_whole(&text, &p.visual_runs(0..text.len()));
            }
        }
    }

    #[test]
    fn a_trailing_emoji_is_one_run_at_the_left_of_an_rtl_paragraph() {
        let emoji = EMOJI[0];
        let text = format!("שלום עולם {emoji}");
        let p = Paragraph::analyze(&text).unwrap();
        let runs = p.visual_runs(0..text.len());
        let at = text.find(emoji).unwrap();
        // The emoji resolves to the right-to-left level (between a Hebrew letter and the
        // end of the paragraph) and is the first run from the left, in one piece.
        assert_eq!(runs[0].range, at..text.len());
        assert!(runs[0].rtl);
    }

    #[test]
    fn a_flag_or_keycap_inside_hebrew_stays_in_one_run() {
        for emoji in ["\u{1F1EE}\u{1F1F1}", "1\u{FE0F}\u{20E3}"] {
            let text = format!("שלום {emoji} עולם");
            let p = Paragraph::analyze(&text).unwrap();
            let runs = p.visual_runs(0..text.len());
            let at = text.find(emoji).unwrap();
            let run = runs
                .iter()
                .find(|r| r.range.contains(&at))
                .expect("the emoji is in a run");
            assert!(run.range.end >= at + emoji.len(), "{emoji:?}: {run:?}");
        }
    }

    #[test]
    fn wrapped_rows_keep_the_paragraph_direction() {
        // Second row starts with a Latin word, but the paragraph is RTL: the
        // trailing space of the first row sits at the paragraph's end (L1).
        let text = "אב גד hello";
        let p = Paragraph::analyze(text).unwrap();
        assert!(p.is_rtl());
        let second = text.find("hello").unwrap();
        let runs = p.visual_runs(second..text.len());
        assert_eq!(runs.len(), 1);
        assert!(!runs[0].rtl);
    }

    #[test]
    fn lone_neutral_runs_are_mirrored_for_the_shaper() {
        assert_eq!(mirror_neutral_run(")").as_deref(), Some("("));
        assert_eq!(mirror_neutral_run(" (").as_deref(), Some(" )"));
        assert_eq!(mirror_neutral_run("«»").as_deref(), Some("»«"));
        // A run with a strong character carries its own context.
        assert_eq!(mirror_neutral_run("(שלום)"), None);
        assert_eq!(mirror_neutral_run("(1)"), None);
        // No bracket, nothing to do.
        assert_eq!(mirror_neutral_run(" "), None);
    }

    #[test]
    fn clusters_follow_glyph_positions_in_an_rtl_run() {
        // Three RTL glyphs, visually left to right: indices 4, 2, 0.
        let text = "אבג";
        let glyphs = [(0., 4), (10., 2), (20., 0)];
        let clusters = clusters_from_glyphs(&glyphs, 30., text);
        assert_eq!(clusters.len(), 3);
        assert_eq!(clusters[0].range, 0..2);
        assert_eq!((clusters[0].left, clusters[0].right), (20., 30.));
        assert_eq!(clusters[2].range, 4..6);
        assert_eq!((clusters[2].left, clusters[2].right), (0., 10.));
    }

    #[test]
    fn combining_marks_fold_into_their_base() {
        // Hebrew letter + point (niqqud) + letter: glyph for the point sits on
        // the base and must not become a caret stop of its own.
        let text = "בְּר";
        // Visually left to right: the second letter (index 6), then the first
        // (index 0) with its two points (indices 2 and 4) stacked on it.
        let glyphs: Vec<(f32, usize)> = vec![(0., 6), (10., 0), (10., 2), (10., 4)];
        let clusters = clusters_from_glyphs(&glyphs, 20., text);
        assert_eq!(clusters.len(), 2);
        assert_eq!(clusters[0].range, 0..6, "base owns its marks");
        assert_eq!((clusters[0].left, clusters[0].right), (10., 20.));
    }

    #[test]
    fn rtl_caret_starts_at_the_right_edge_and_moves_left() {
        let f = chars_fragment("אבג", true, 0, 0., 10.);
        let line = BidiLine::new(6, vec![f]);
        assert_eq!(line.width, 30.);
        // Logical order a(0) b(2) c(4) end(6): x strictly decreases.
        let xs: Vec<f32> = [0, 2, 4, 6].iter().map(|&b| line.x_for_index(b)).collect();
        assert_eq!(xs, vec![30., 20., 10., 0.]);
    }

    #[test]
    fn ltr_fragment_caret_advances_right() {
        let line = BidiLine::new(3, vec![chars_fragment("abc", false, 0, 0., 8.)]);
        let xs: Vec<f32> = [0, 1, 2, 3].iter().map(|&b| line.x_for_index(b)).collect();
        assert_eq!(xs, vec![0., 8., 16., 24.]);
    }

    /// "אב hello גד" laid out in an RTL paragraph at 10px per glyph, fragments
    /// placed in the visual order the paragraph analysis gives.
    fn mixed_line() -> (String, BidiLine) {
        let text = "אב hello גד".to_string();
        let p = Paragraph::analyze(&text).unwrap();
        let mut x = 0.;
        let mut frags = Vec::new();
        for run in p.visual_runs(0..text.len()) {
            let slice = &text[run.range.clone()];
            let f = chars_fragment(slice, run.rtl, run.range.start, x, 10.);
            x += f.width;
            frags.push(f);
        }
        (text.clone(), BidiLine::new(text.len(), frags))
    }

    #[test]
    fn mixed_line_places_runs_in_visual_order() {
        let (text, line) = mixed_line();
        // Logical start of the paragraph is at the right edge.
        assert_eq!(line.x_for_index(0), line.width);
        // First logical char is further right than any later Hebrew char.
        let hello = text.find("hello").unwrap();
        assert!(line.x_for_index(hello) < line.x_for_index(0));
        // Inside the Latin word the caret advances rightwards.
        assert!(line.x_for_index(hello + 1) > line.x_for_index(hello));
        assert!(line.x_for_index(hello + 4) > line.x_for_index(hello + 3));
    }

    #[test]
    fn hit_testing_lands_on_the_boundary_it_was_aimed_at() {
        let (text, line) = mixed_line();
        for b in text.char_indices().map(|(i, _)| i) {
            let x = line.x_for_index(b);
            let hit = line.closest_index_for_x(x);
            // Two logical boundaries can share one x where the direction
            // changes; either is the caret the user pointed at.
            assert!(
                (line.x_for_index(hit) - x).abs() < 0.01,
                "boundary {b} at x={x} hit {hit} at x={}",
                line.x_for_index(hit)
            );
        }
    }

    #[test]
    fn clicks_clamp_to_the_ends() {
        let (_, line) = mixed_line();
        let left = line.closest_index_for_x(-50.);
        let right = line.closest_index_for_x(line.width + 50.);
        assert_eq!(right, 0, "far right of an RTL line is its logical start");
        assert!(left > 0);
    }

    #[test]
    fn selection_across_a_direction_change_is_disjoint() {
        let (text, line) = mixed_line();
        let hello = text.find("hello").unwrap();
        // "ב hel": the last Hebrew letter, the space and the start of the Latin
        // word. Five glyphs of 10px, whatever the number of pieces.
        let rects = line.range_rects(2..hello + 3);
        assert!(!rects.is_empty());
        let total: f32 = rects.iter().map(|r| r.1 - r.0).sum();
        assert!((total - 50.).abs() < 0.01, "rects {rects:?}");
        for r in &rects {
            assert!(r.0 < r.1);
        }
    }

    #[test]
    fn whole_line_selection_is_one_rectangle() {
        let (text, line) = mixed_line();
        let rects = line.range_rects(0..text.len());
        assert_eq!(rects, vec![(0., line.width)]);
    }

    #[test]
    fn arrows_move_visually_in_an_rtl_run() {
        let line = BidiLine::new(6, vec![chars_fragment("אבג", true, 0, 0., 10.)]);
        // Caret before the first letter sits at the right edge: Left moves to
        // the next letter logically, Right has nowhere to go.
        assert_eq!(line.visual_step(0, true), Some(2));
        assert_eq!(line.visual_step(0, false), None);
        assert_eq!(line.visual_step(2, false), Some(0));
        assert_eq!(line.visual_step(6, false), Some(4));
        assert_eq!(line.visual_step(6, true), None);
    }

    #[test]
    fn arrows_move_visually_in_an_ltr_run() {
        let line = BidiLine::new(3, vec![chars_fragment("abc", false, 0, 0., 8.)]);
        assert_eq!(line.visual_step(0, false), Some(1));
        assert_eq!(line.visual_step(3, true), Some(2));
        assert_eq!(line.visual_step(0, true), None);
    }

    #[test]
    fn arrows_walk_every_stop_of_a_mixed_line_without_trapping() {
        let (_, line) = mixed_line();
        // Starting at the visual left edge and stepping right must visit
        // strictly increasing x and terminate at the right edge.
        let mut at = line.closest_stop_for_x(0.);
        let mut x = line.stop_x(at.0, at.1);
        let mut steps = 0;
        while let Some(next) = line.visual_step_stop(at.0, at.1, false) {
            let nx = line.stop_x(next.0, next.1);
            assert!(nx > x, "x must increase: {x} -> {nx}");
            at = next;
            x = nx;
            steps += 1;
            assert!(steps < 64, "visual stepping must terminate");
        }
        assert!(steps >= 5);
        // And the other way back.
        while let Some(next) = line.visual_step_stop(at.0, at.1, true) {
            assert!(line.stop_x(next.0, next.1) < x);
            x = line.stop_x(next.0, next.1);
            at = next;
            steps -= 1;
            assert!(steps >= -64);
        }
    }

    /// "abשג" at 10px a glyph: a Latin run and a Hebrew run side by side. Offset 2 sits
    /// between them and has two places on screen: x=20 (after "b") and x=40 (before the
    /// Hebrew "ש", the run's right edge).
    fn ab_hebrew() -> BidiLine {
        BidiLine::new(
            6,
            vec![
                chars_fragment("ab", false, 0, 0., 10.),
                chars_fragment("שג", true, 2, 20., 10.),
            ],
        )
    }

    fn walk(line: &BidiLine, from: (usize, bool), left: bool) -> Vec<(usize, bool)> {
        let mut stops = vec![from];
        while let Some(next) =
            line.visual_step_stop(stops.last().unwrap().0, stops.last().unwrap().1, left)
        {
            stops.push(next);
            assert!(stops.len() < 64);
        }
        stops
    }

    #[test]
    fn a_direction_boundary_has_two_places() {
        let line = ab_hebrew();
        assert!(line.has_two_stops(2));
        assert_eq!(line.x_for_index(2), 40.);
        assert_eq!(line.x_for_index_trailing(2), 20.);
        // Inside a run, and at the ends, there is only one.
        assert!(!line.has_two_stops(1));
        assert!(!line.has_two_stops(4));
        assert!(!line.has_two_stops(0));
        assert!(!line.has_two_stops(6));
        assert_eq!(line.x_for_index_trailing(1), line.x_for_index(1));
    }

    #[test]
    fn walking_right_visits_both_places_of_the_boundary_offset() {
        let line = ab_hebrew();
        // Offset 2 first after "b" (hanging on it), then before the Hebrew letter.
        assert_eq!(
            walk(&line, (0, false), false),
            vec![(0, false), (1, false), (2, true), (4, false), (2, false)]
        );
    }

    #[test]
    fn walking_left_goes_back_through_the_hebrew_run() {
        let line = ab_hebrew();
        assert_eq!(
            walk(&line, (2, false), true),
            vec![(2, false), (4, false), (6, false), (1, false), (0, false)]
        );
    }

    #[test]
    fn clicks_pick_the_side_of_the_boundary() {
        let line = ab_hebrew();
        // Just left of the join is "b": the caret hangs on it.
        assert_eq!(line.closest_stop_for_x(19.), (2, true));
        // Right edge of the line: before the first Hebrew letter.
        assert_eq!(line.closest_stop_for_x(41.), (2, false));
        // Mid-letter picks the nearer edge of that letter.
        assert_eq!(line.closest_stop_for_x(33.), (4, false));
    }

    #[test]
    fn the_character_between_two_stops_is_the_one_in_the_gap() {
        let line = ab_hebrew();
        assert_eq!(line.char_between(10., 20.), Some('b'));
        assert_eq!(line.char_between(30., 40.), Some('ש'));
        assert_eq!(line.char_between(20., 30.), Some('ג'));
    }

    #[test]
    fn index_for_x_names_the_character_under_the_pointer() {
        let line = BidiLine::new(6, vec![chars_fragment("אבג", true, 0, 0., 10.)]);
        // Rightmost glyph is the first character.
        assert_eq!(line.index_for_x(25.), Some(0));
        assert_eq!(line.index_for_x(15.), Some(2));
        assert_eq!(line.index_for_x(5.), Some(4));
        assert_eq!(line.index_for_x(31.), None);
    }

    #[test]
    fn empty_line_has_a_caret_at_the_origin() {
        let line = BidiLine::new(0, Vec::new());
        assert_eq!(line.x_for_index(0), 0.);
        assert_eq!(line.closest_index_for_x(5.), 0);
        assert!(line.range_rects(0..0).is_empty());
    }

    #[test]
    fn hanging_whitespace_widens_the_fragment() {
        // CoreText: a trailing space in an RTL run at x = -3.8, outside the
        // reported width.
        let (shift, width) = fragment_extent(&[-3.8, 0., 10.], 20., 10.);
        assert!((shift - 3.8).abs() < 1e-4);
        assert!((width - 23.8).abs() < 1e-4);
        // A trailing space of an LTR run hangs past the reported width.
        let (shift, width) = fragment_extent(&[0., 10., 20.], 20., 4.);
        assert_eq!(shift, 0.);
        assert_eq!(width, 24.);
        // Nothing hangs: the reported width stands.
        let (shift, width) = fragment_extent(&[0., 10.], 20., 10.);
        assert_eq!((shift, width), (0., 20.));
        // No glyphs at all.
        assert_eq!(fragment_extent(&[], 0., 0.), (0., 0.));
    }

    #[test]
    fn offsets_inside_a_character_use_its_start() {
        let line = BidiLine::new(6, vec![chars_fragment("אבג", true, 0, 0., 10.)]);
        assert_eq!(line.x_for_index(3), line.x_for_index(2));
        assert_eq!(line.x_for_index(5), line.x_for_index(4));
    }

    #[test]
    fn ligature_clusters_split_the_caret_evenly() {
        // Two characters shaped into one 20px glyph (lam-alef style).
        let f = Fragment::new(
            0..4,
            true,
            0.,
            20.,
            "לא".to_string(),
            vec![Cluster {
                range: 0..4,
                left: 0.,
                right: 20.,
            }],
        );
        assert_eq!(f.pos(0), 20.);
        assert_eq!(f.pos(2), 10.);
        assert_eq!(f.pos(4), 0.);
    }
}
