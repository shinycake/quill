//! Writes a [`ComposerDoc`] as composer markup.
#![deny(clippy::indexing_slicing)]

use std::ops::Range;

use super::{ComposerDoc, ComposerTag, DisjointRanges, DocEmoji, DocSpan, carve, normalize_spans};
use crate::composer::markup_boundary::{BARRIER, ESCAPE, is_marker_char, needs_barrier};
use crate::composer::scan_work;

/// Writes a document as markup: a walk over segments of the text between
/// span boundaries, with a stack of open inline spans that is closed and
/// reopened where spans cross (`**a [b**](u)[c](u)`), so the result always
/// nests.
pub(super) struct Markup<'a> {
    doc: &'a ComposerDoc,
    literals: Literals,
    line_markers: LineMarkers,
    /// Inline spans after reshaping (see [`ComposerDoc::to_markup`]).
    inline: Vec<DocSpan>,
    bold: DisjointRanges,
    underline: DisjointRanges,
    pre: Vec<DocSpan>,
    quote: DisjointRanges,
}

/// Which marker characters of the text a [`Markup`] escapes.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Literals {
    /// Those on a line where the same character is written as a marker, which
    /// it would pair with.
    SharingALine,
    /// Every one.
    All,
}

const STAR: u8 = 1;
const UNDERSCORE: u8 = 1 << 1;
const TILDE: u8 = 1 << 2;
const PIPE: u8 = 1 << 3;
const BACKTICK: u8 = 1 << 4;
const BRACKET: u8 = 1 << 5;
const EVERY_MARKER: u8 = STAR | UNDERSCORE | TILDE | PIPE | BACKTICK | BRACKET;

fn marker_bit(ch: char) -> u8 {
    match ch {
        '*' => STAR,
        '_' => UNDERSCORE,
        '~' => TILDE,
        '|' => PIPE,
        '`' => BACKTICK,
        '[' => BRACKET,
        _ => 0,
    }
}

/// `text` with every character in `markers` escaped.
fn escaped(text: &str, markers: u8) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        if marker_bit(ch) & markers != 0 {
            out.push(ESCAPE);
        }
        out.push(ch);
    }
    out
}

/// `marker` after `out`, kept apart from a character of its own: next to one
/// it would not open or close.
fn push_marker(out: &mut String, marker: &str) {
    let first = marker.chars().next();
    if first.is_some_and(is_marker_char) && out.chars().next_back() == first {
        out.push(BARRIER);
    }
    out.push_str(marker);
}

/// The markers written on each line of a text.
#[derive(Default)]
struct LineMarkers {
    line_starts: Vec<usize>,
    markers: Vec<u8>,
}

impl LineMarkers {
    fn at(&self, offset: usize) -> u8 {
        let lines = self.line_starts.partition_point(|&start| start <= offset);
        lines
            .checked_sub(1)
            .and_then(|line| self.markers.get(line))
            .copied()
            .unwrap_or(0)
    }
}

impl<'a> Markup<'a> {
    pub(super) fn new(doc: &'a ComposerDoc, literals: Literals) -> Self {
        let text = &doc.text;
        let mut spans: Vec<DocSpan> = doc
            .spans
            .iter()
            .filter_map(|span| {
                let range = snap_out_of_emoji(&doc.emoji, span.range.clone());
                // Off a char boundary means an edit left the span stale:
                // drop it rather than split a character.
                (range.start < range.end && text.get(range.clone()).is_some()).then(|| DocSpan {
                    range,
                    tag: span.tag.clone(),
                })
            })
            .collect();
        let pre: Vec<DocSpan> = normalize_spans(
            spans
                .iter()
                .filter(|span| matches!(span.tag, ComposerTag::Pre(_)))
                .cloned()
                .collect(),
        )
        .into_iter()
        // Overlapping code blocks of different languages: the first wins.
        .fold(Vec::new(), |mut kept: Vec<DocSpan>, span| {
            if kept
                .last()
                .is_none_or(|last| last.range.end <= span.range.start)
            {
                kept.push(span);
            }
            kept
        });
        spans.retain(|span| !matches!(span.tag, ComposerTag::Pre(_)));
        // Nothing else inside a code block.
        let blocks = DisjointRanges::of(pre.iter().cloned());
        let spans = spans.into_iter().flat_map(|span| {
            blocks
                .overlapping(&span.range)
                .iter()
                .fold(vec![span], |pieces, block| carve(pieces, block, |_| true))
        });
        let (quote, inline): (Vec<DocSpan>, Vec<DocSpan>) =
            spans.partition(|span| span.tag == ComposerTag::Quote);
        // Inline formatting stays on one line.
        let mut lines: Vec<DocSpan> = Vec::new();
        for span in inline {
            let mut start = span.range.start;
            let body = text.get(span.range.clone()).unwrap_or_default();
            for (ix, _) in body.match_indices('\n') {
                let at = span.range.start + ix;
                if start < at {
                    lines.push(DocSpan::new(start..at, span.tag.clone()));
                }
                start = at + 1;
            }
            if start < span.range.end {
                lines.push(DocSpan::new(start..span.range.end, span.tag.clone()));
            }
        }
        // Nothing inside code, unless it holds the whole code span.
        let code = DisjointRanges::of(
            lines
                .iter()
                .filter(|span| span.tag == ComposerTag::Code)
                .cloned(),
        );
        let lines = lines.into_iter().flat_map(|span| {
            if span.tag == ComposerTag::Code {
                return vec![span];
            }
            code.overlapping(&span.range)
                .iter()
                .fold(vec![span], |pieces, code_range| {
                    pieces
                        .into_iter()
                        .flat_map(|piece| {
                            let holds = piece.range.start <= code_range.start
                                && code_range.end <= piece.range.end;
                            if holds {
                                vec![piece]
                            } else {
                                carve(vec![piece], code_range, |_| true)
                            }
                        })
                        .collect()
                })
        });
        let inline = normalize_spans(lines.collect());
        let tagged = |tag: ComposerTag| {
            DisjointRanges::of(inline.iter().filter(|span| span.tag == tag).cloned())
        };
        let bold = tagged(ComposerTag::Bold);
        let underline = tagged(ComposerTag::Underline);
        let mut markup = Self {
            doc,
            literals,
            line_markers: LineMarkers::default(),
            inline,
            bold,
            underline,
            pre,
            quote: DisjointRanges::of(quote),
        };
        if literals == Literals::SharingALine {
            markup.line_markers = markup.line_markers();
        }
        markup
    }

    /// The characters to escape in text written at `at`.
    fn escapes_at(&self, at: usize) -> u8 {
        match self.literals {
            Literals::SharingALine => self.line_markers.at(at),
            Literals::All => EVERY_MARKER,
        }
    }

    fn line_markers(&self) -> LineMarkers {
        let text = &self.doc.text;
        let line_starts: Vec<usize> = std::iter::once(0)
            .chain(text.match_indices('\n').map(|(ix, _)| ix + 1))
            .collect();
        let mut markers = vec![0; line_starts.len()];
        for (ix, span) in self.inline.iter().enumerate() {
            let lines = line_starts.partition_point(|&start| start <= span.range.start);
            let line = lines.checked_sub(1).and_then(|line| markers.get_mut(line));
            if let Some(line) = line {
                *line |= self.opener(ix).chars().next().map_or(0, marker_bit);
            }
        }
        LineMarkers {
            line_starts,
            markers,
        }
    }

    pub(super) fn render(&self) -> String {
        let text = &self.doc.text;
        let mut cuts: Vec<usize> = vec![0, text.len()];
        for span in self.inline.iter().chain(&self.pre) {
            cuts.push(span.range.start);
            cuts.push(span.range.end);
        }
        for emoji in &self.doc.emoji {
            cuts.push(emoji.range.start);
            cuts.push(emoji.range.end);
        }
        for (ix, _) in text.match_indices('\n') {
            cuts.push(ix);
            cuts.push(ix + 1);
        }
        // Spans and emoji should stay on char boundaries after every edit;
        // drop any cut that does not, so a stale range cannot split a
        // character here.
        cuts.retain(|&c| text.is_char_boundary(c));
        cuts.sort_unstable();
        cuts.dedup();

        let mut emoji: Vec<&DocEmoji> = self.doc.emoji.iter().collect();
        emoji.sort_by_key(|e| e.range.start);
        let mut emoji = emoji.as_slice();
        let mut out = String::with_capacity(text.len() + 16);
        // Indices into `self.inline` of the open spans, outermost first.
        let mut stack: Vec<usize> = Vec::new();
        let mut spans_over_cut: Vec<usize> = Vec::new();
        let mut first_unreached = 0;
        let mut i = 0;
        while let (Some(&a), Some(&b)) = (cuts.get(i), cuts.get(i + 1)) {
            i += 1;
            scan_work::add(spans_over_cut.len() + 1);
            let block = self
                .pre
                .binary_search_by_key(&a, |block| block.range.start)
                .ok()
                .and_then(|ix| self.pre.get(ix));
            if let Some(block) = block {
                self.close_all(&mut stack, &mut out);
                self.quote_prefix(a, &mut out);
                let language = match &block.tag {
                    ComposerTag::Pre(language) => language.as_str(),
                    _ => "",
                };
                out.push_str("```");
                out.push_str(language);
                out.push('\n');
                out.push_str(text.get(block.range.clone()).unwrap_or_default());
                out.push_str("\n```");
                // Continue from the end of the block.
                while cuts.get(i).is_some_and(|&c| c < block.range.end) {
                    i += 1;
                }
                continue;
            }
            while self
                .inline
                .get(first_unreached)
                .is_some_and(|span| span.range.start <= a)
            {
                spans_over_cut.push(first_unreached);
                first_unreached += 1;
            }
            spans_over_cut.retain(|&ix| self.inline.get(ix).is_some_and(|span| a < span.range.end));
            // Close from the first open span that is no longer active.
            let mut closed_marker = false;
            if let Some(k) = stack.iter().position(|ix| !spans_over_cut.contains(ix)) {
                for ix in stack.drain(k..).rev() {
                    push_marker(&mut out, &self.closer(ix));
                }
                closed_marker = out.chars().next_back().is_some_and(is_marker_char);
            }
            self.quote_prefix(a, &mut out);
            let mut opening: Vec<usize> = spans_over_cut
                .iter()
                .copied()
                .filter(|ix| !stack.contains(ix))
                .collect();
            opening.sort_by_key(|&ix| {
                self.inline
                    .get(ix)
                    .map(|span| (std::cmp::Reverse(span.range.end), span.tag.nesting()))
            });
            // A marker next to a word character would read as plain text
            // (`composer::markup_boundary`); a barrier keeps a span that
            // starts or ends inside a word.
            if let Some(&first) = opening.first()
                && self
                    .opener(first)
                    .chars()
                    .next()
                    .is_some_and(is_marker_char)
                && needs_barrier(out.chars().next_back())
            {
                out.push(BARRIER);
            }
            let opens = !opening.is_empty();
            for ix in opening {
                push_marker(&mut out, &self.opener(ix));
                stack.push(ix);
            }
            while emoji.first().is_some_and(|e| e.range.start < a) {
                emoji = emoji.get(1..).unwrap_or_default();
            }
            let written = match emoji.first().filter(|e| e.range.start == a) {
                Some(emoji) => {
                    // A custom emoji is one piece, whatever its text holds.
                    while cuts.get(i).is_some_and(|&c| c < emoji.range.end) {
                        i += 1;
                    }
                    let piece = text.get(emoji.range.clone()).unwrap_or_default();
                    crate::composer::custom_emoji_markup(piece, emoji.custom_emoji_id)
                }
                None => {
                    let piece = text.get(a..b).unwrap_or_default();
                    if self.literals == Literals::All
                        && at_line_start(text, a)
                        && !opens
                        && piece.starts_with("> ")
                    {
                        out.push(ESCAPE);
                    }
                    escaped(piece, self.escapes_at(a))
                }
            };
            if closed_marker && !opens && needs_barrier(written.chars().next()) {
                out.push(BARRIER);
            }
            out.push_str(&written);
        }
        self.close_all(&mut stack, &mut out);
        out
    }

    /// `> ` when `at` starts a line inside a quote.
    fn quote_prefix(&self, at: usize, out: &mut String) {
        if at_line_start(&self.doc.text, at) && !self.quote.overlapping(&(at..at + 1)).is_empty() {
            out.push_str("> ");
        }
    }

    fn close_all(&self, stack: &mut Vec<usize>, out: &mut String) {
        while let Some(ix) = stack.pop() {
            push_marker(out, &self.closer(ix));
        }
    }

    fn opener(&self, ix: usize) -> String {
        let Some(span) = self.inline.get(ix) else {
            return String::new();
        };
        match &span.tag {
            ComposerTag::Bold => "**".into(),
            ComposerTag::Italic => self.italic_marker(ix).into(),
            ComposerTag::Underline => "__".into(),
            ComposerTag::Strikethrough => "~~".into(),
            ComposerTag::Spoiler => "||".into(),
            ComposerTag::Code => "`".into(),
            ComposerTag::Link(_) | ComposerTag::Mention(_) => "[".into(),
            ComposerTag::Pre(_) | ComposerTag::Quote => String::new(),
        }
    }

    fn closer(&self, ix: usize) -> String {
        let Some(span) = self.inline.get(ix) else {
            return String::new();
        };
        match &span.tag {
            ComposerTag::Bold => "**".into(),
            ComposerTag::Italic => self.italic_marker(ix).into(),
            ComposerTag::Underline => "__".into(),
            ComposerTag::Strikethrough => "~~".into(),
            ComposerTag::Spoiler => "||".into(),
            ComposerTag::Code => "`".into(),
            ComposerTag::Link(url) => {
                // A link ends at the first `)`, and a marker in it would pair.
                let url = url.replace(')', "%29");
                format!("]({})", escaped(&url, self.escapes_at(span.range.start)))
            }
            ComposerTag::Mention(user_id) => format!("](tg://user?id={user_id})"),
            ComposerTag::Pre(_) | ComposerTag::Quote => String::new(),
        }
    }

    /// `_` where the italic text shares text with bold, whose `**` an italic
    /// `*` would be read into, unless an underline starts or ends inside it,
    /// whose `__` an italic `_` would be.
    fn italic_marker(&self, ix: usize) -> &'static str {
        let Some(range) = self.inline.get(ix).map(|span| &span.range) else {
            return "*";
        };
        let with_bold = !self.bold.overlapping(range).is_empty();
        let underline_inside = self
            .underline
            .overlapping(range)
            .iter()
            .any(|other| other.start > range.start || other.end < range.end);
        if with_bold && !underline_inside {
            "_"
        } else {
            "*"
        }
    }
}

fn at_line_start(text: &str, at: usize) -> bool {
    at == 0 || text.as_bytes().get(at - 1) == Some(&b'\n')
}

/// `range` with an end that falls inside a custom emoji moved past it.
fn snap_out_of_emoji(emoji: &[DocEmoji], range: Range<usize>) -> Range<usize> {
    let snap = |at: usize| {
        emoji
            .iter()
            .find(|e| e.range.start < at && at < e.range.end)
            .map_or(at, |e| e.range.end)
    };
    let start = emoji
        .iter()
        .find(|e| e.range.start < range.start && range.start < e.range.end)
        .map_or(range.start, |e| e.range.start);
    start..snap(range.end)
}
