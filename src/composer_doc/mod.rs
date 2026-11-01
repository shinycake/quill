//! The composer field's document, after Telegram Desktop's `TextWithTags`
//! (`InputField`, lib_ui/ui/widgets/fields/input_field.cpp): the text the
//! person sees, with no markers, plus formatting spans ("bytes 4..9 are
//! bold") and custom emoji. The field shows it as you type; markup
//! (`composer::parse_format_markup`) is how its content travels to the send,
//! draft and edit paths, so this module converts both ways and holds the
//! editing rules the field applies: toggling a format over a selection, and
//! turning typed `**markers**` into formatting when the closing marker is
//! typed (`InputField::processMarkdownReplaces`).
//!
//! Ranges are UTF-8 byte ranges into [`ComposerDoc::text`].
#![deny(clippy::indexing_slicing)]

use std::ops::Range;

use crate::composer::markup_boundary::{closer_ok, is_marker_char, opener_ok};
use crate::composer::{FormatKind, parse_format_markup, scan_work};
use crate::text::{TextEntity, TextEntityKind};

mod writer;
use writer::{Literals, Markup};

/// One kind of formatting in the field. The tag string
/// ([`ComposerTag::to_tag`]) is what the input engine stores.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ComposerTag {
    Bold,
    Italic,
    Underline,
    Strikethrough,
    Spoiler,
    Code,
    /// A code block with its language (may be empty).
    Pre(String),
    Quote,
    Link(String),
    /// A mention of a user without a username (`textEntityTypeMentionName`).
    Mention(i64),
}

impl ComposerTag {
    pub fn to_tag(&self) -> String {
        match self {
            Self::Bold => "bold".into(),
            Self::Italic => "italic".into(),
            Self::Underline => "underline".into(),
            Self::Strikethrough => "strike".into(),
            Self::Spoiler => "spoiler".into(),
            Self::Code => "code".into(),
            Self::Pre(language) if language.is_empty() => "pre".into(),
            Self::Pre(language) => format!("pre:{language}"),
            Self::Quote => "quote".into(),
            Self::Link(url) => format!("link:{url}"),
            Self::Mention(user_id) => format!("mention:{user_id}"),
        }
    }

    pub fn from_tag(tag: &str) -> Option<Self> {
        Some(match tag {
            "bold" => Self::Bold,
            "italic" => Self::Italic,
            "underline" => Self::Underline,
            "strike" => Self::Strikethrough,
            "spoiler" => Self::Spoiler,
            "code" => Self::Code,
            "pre" => Self::Pre(String::new()),
            "quote" => Self::Quote,
            _ => {
                if let Some(language) = tag.strip_prefix("pre:") {
                    Self::Pre(language.to_string())
                } else if let Some(url) = tag.strip_prefix("link:") {
                    Self::Link(url.to_string())
                } else {
                    let id = tag.strip_prefix("mention:")?;
                    Self::Mention(id.parse().ok().filter(|id: &i64| *id > 0)?)
                }
            }
        })
    }

    /// Whether a value is the same kind of formatting (links to different
    /// addresses are one kind).
    pub fn same_kind(&self, other: &Self) -> bool {
        std::mem::discriminant(self) == std::mem::discriminant(other)
    }

    fn is_link_like(&self) -> bool {
        matches!(self, Self::Link(_) | Self::Mention(_))
    }

    /// Nesting preference when two spans start together and end together:
    /// lower opens first (outside).
    fn nesting(&self) -> u8 {
        match self {
            Self::Quote => 0,
            Self::Pre(_) => 1,
            Self::Link(_) | Self::Mention(_) => 2,
            Self::Spoiler => 3,
            Self::Bold => 4,
            Self::Underline => 5,
            Self::Strikethrough => 6,
            Self::Italic => 7,
            Self::Code => 8,
        }
    }
}

/// A formatting span over a byte range of the text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocSpan {
    pub range: Range<usize>,
    pub tag: ComposerTag,
}

impl DocSpan {
    pub fn new(range: Range<usize>, tag: ComposerTag) -> Self {
        Self { range, tag }
    }
}

/// A custom emoji: its fallback emoji is the text at `range`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocEmoji {
    pub range: Range<usize>,
    pub custom_emoji_id: i64,
}

/// What the composer field holds.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ComposerDoc {
    pub text: String,
    pub spans: Vec<DocSpan>,
    pub emoji: Vec<DocEmoji>,
}

impl ComposerDoc {
    pub fn plain(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            ..Self::default()
        }
    }

    /// The document a piece of composer markup describes.
    pub fn from_markup(markup: &str) -> Self {
        let (text, entities) = parse_format_markup(markup);
        let mut doc = Self::plain(text);
        for entity in entities {
            let range = entity.bytes;
            if range.is_empty() {
                continue;
            }
            let tag = match entity.kind {
                FormatKind::Bold => ComposerTag::Bold,
                FormatKind::Italic => ComposerTag::Italic,
                FormatKind::Underline => ComposerTag::Underline,
                FormatKind::Strikethrough => ComposerTag::Strikethrough,
                FormatKind::Spoiler => ComposerTag::Spoiler,
                FormatKind::Code => ComposerTag::Code,
                FormatKind::Pre => ComposerTag::Pre(entity.language),
                FormatKind::BlockQuote => ComposerTag::Quote,
                // A date is a link to its time in the field, as in tdesktop.
                FormatKind::TextUrl | FormatKind::DateTime => ComposerTag::Link(entity.url),
                FormatKind::MentionName => match crate::composer::mention_user_id(&entity.url) {
                    Some(id) => ComposerTag::Mention(id),
                    None => continue,
                },
                FormatKind::CustomEmoji => {
                    if let Some(id) = entity
                        .url
                        .strip_prefix("tg://emoji?id=")
                        .and_then(|id| id.parse().ok())
                    {
                        doc.emoji.push(DocEmoji {
                            range,
                            custom_emoji_id: id,
                        });
                    }
                    continue;
                }
            };
            doc.spans.push(DocSpan::new(range, tag));
        }
        doc.normalize();
        doc
    }

    /// The document for a text and its TDLib entities (a draft or a message
    /// being edited). Entities markup cannot carry (URLs, `@username`
    /// mentions, hashtags…) are dropped: TDLib finds them again on send.
    pub fn from_entities(text: &str, entities: &[TextEntity]) -> Self {
        let mut doc = Self::plain(text);
        for entity in entities {
            let range = entity.utf8_start..entity.utf8_end;
            if range.start >= range.end
                || range.end > text.len()
                || !text.is_char_boundary(range.start)
                || !text.is_char_boundary(range.end)
            {
                continue;
            }
            let tag = match &entity.kind {
                TextEntityKind::Bold => ComposerTag::Bold,
                TextEntityKind::Italic => ComposerTag::Italic,
                TextEntityKind::Underline => ComposerTag::Underline,
                TextEntityKind::Strikethrough => ComposerTag::Strikethrough,
                TextEntityKind::Spoiler => ComposerTag::Spoiler,
                TextEntityKind::Code => ComposerTag::Code,
                TextEntityKind::Pre => ComposerTag::Pre(String::new()),
                TextEntityKind::PreCode { language } => ComposerTag::Pre(language.clone()),
                TextEntityKind::BlockQuote | TextEntityKind::ExpandableBlockQuote => {
                    ComposerTag::Quote
                }
                TextEntityKind::TextUrl { url } => ComposerTag::Link(url.clone()),
                TextEntityKind::MentionName { user_id } => ComposerTag::Mention(*user_id),
                TextEntityKind::DateTime { unix_time, .. } if *unix_time > 0 => {
                    ComposerTag::Link(crate::composer::date_link(*unix_time))
                }
                TextEntityKind::CustomEmoji { custom_emoji_id } => {
                    doc.emoji.push(DocEmoji {
                        range,
                        custom_emoji_id: *custom_emoji_id,
                    });
                    continue;
                }
                _ => continue,
            };
            doc.spans.push(DocSpan::new(range, tag));
        }
        doc.normalize();
        doc
    }

    /// Sort spans and emoji, merge touching spans of one tag, drop empty
    /// and overlapping emoji.
    pub fn normalize(&mut self) {
        self.spans = normalize_spans(std::mem::take(&mut self.spans));
        self.emoji.sort_by_key(|e| e.range.start);
        let mut kept: Vec<DocEmoji> = Vec::with_capacity(self.emoji.len());
        for emoji in self.emoji.drain(..) {
            if emoji.range.start < emoji.range.end
                && kept
                    .last()
                    .is_none_or(|last| last.range.end <= emoji.range.start)
            {
                kept.push(emoji);
            }
        }
        self.emoji = kept;
    }

    /// The part of the document over the byte range `range` (tdesktop
    /// `getTextWithTagsPart`): spans clipped to it, emoji inside it, all
    /// moved to start at 0. An empty document when `range` is not on char
    /// boundaries of the text.
    pub fn slice(&self, range: Range<usize>) -> Self {
        self.part(range, self.spans.iter(), self.emoji.as_slice())
    }

    /// [`Self::slice`] of each of `ranges`, which go forward and do not
    /// overlap, in one pass over the spans.
    pub fn slices(&self, ranges: &[Range<usize>]) -> Vec<Self> {
        let mut spans: Vec<&DocSpan> = self.spans.iter().collect();
        spans.sort_by_key(|span| span.range.start);
        let mut emoji: Vec<DocEmoji> = self.emoji.clone();
        emoji.sort_by_key(|emoji| emoji.range.start);
        let mut waiting = spans.into_iter().peekable();
        let mut open: Vec<&DocSpan> = Vec::new();
        ranges
            .iter()
            .map(|range| {
                while let Some(span) = waiting.next_if(|span| span.range.start < range.end) {
                    open.push(span);
                }
                open.retain(|span| span.range.end > range.start);
                scan_work::add(open.len());
                let first = emoji.partition_point(|emoji| emoji.range.start < range.start);
                let inside = emoji.get(first..).unwrap_or_default();
                let count = inside.partition_point(|emoji| emoji.range.start < range.end);
                let inside = inside.get(..count).unwrap_or_default();
                self.part(range.clone(), open.iter().copied(), inside)
            })
            .collect()
    }

    fn part<'s>(
        &self,
        range: Range<usize>,
        spans: impl Iterator<Item = &'s DocSpan>,
        emoji: &[DocEmoji],
    ) -> Self {
        let Some(text) = self.text.get(range.clone()) else {
            return Self::default();
        };
        let shift = |at: usize| at.clamp(range.start, range.end) - range.start;
        let spans = spans
            .map(|span| {
                DocSpan::new(
                    shift(span.range.start)..shift(span.range.end),
                    span.tag.clone(),
                )
            })
            .filter(|span| span.range.start < span.range.end)
            .collect();
        let emoji = emoji
            .iter()
            .filter(|emoji| emoji.range.start >= range.start && emoji.range.end <= range.end)
            .map(|emoji| DocEmoji {
                range: shift(emoji.range.start)..shift(emoji.range.end),
                custom_emoji_id: emoji.custom_emoji_id,
            })
            .collect();
        Self {
            text: text.to_string(),
            spans,
            emoji,
        }
    }

    /// The composer markup for this document; [`Self::from_markup`] of it
    /// gives the document back (spans the markup cannot express are
    /// reshaped: a quote that starts mid-line is dropped, formatting inside
    /// code or a code block is dropped, spans crossing a line break or a link
    /// are cut there).
    pub fn to_markup(&self) -> String {
        Markup::new(self, Literals::SharingALine).render()
    }

    /// The markup of a document whose text is final, such as a parsed one:
    /// [`Self::from_markup`] of it gives this text back, whatever the text
    /// holds. In a field the text is not final: a marker typed there and
    /// never turned into formatting still formats when it is sent, so
    /// [`Self::to_markup`] leaves it alone. Text that reads the same either
    /// way gets the same markup, so a draft echoed back compares equal.
    pub fn to_final_markup(&self) -> String {
        let markup = self.to_markup();
        if Self::from_markup(&markup) == *self {
            markup
        } else {
            Markup::new(self, Literals::All).render()
        }
    }
}

/// Sort by start, merge overlapping or touching spans of the same tag.
pub fn normalize_spans(mut spans: Vec<DocSpan>) -> Vec<DocSpan> {
    spans.retain(|span| span.range.start < span.range.end);
    spans.sort_by(|a, b| {
        (a.tag.to_tag(), a.range.start, a.range.end).cmp(&(
            b.tag.to_tag(),
            b.range.start,
            b.range.end,
        ))
    });
    let mut merged: Vec<DocSpan> = Vec::with_capacity(spans.len());
    for span in spans {
        match merged.last_mut() {
            Some(last) if last.tag == span.tag && span.range.start <= last.range.end => {
                last.range.end = last.range.end.max(span.range.end);
            }
            _ => merged.push(span),
        }
    }
    merged.sort_by_key(|span| (span.range.start, span.range.end, span.tag.nesting()));
    merged
}

pub(crate) struct DisjointRanges(Vec<Range<usize>>);

impl DisjointRanges {
    pub(crate) fn of(spans: impl IntoIterator<Item = DocSpan>) -> Self {
        let spans = normalize_spans(spans.into_iter().collect());
        Self(spans.into_iter().map(|span| span.range).collect())
    }

    fn overlapping(&self, range: &Range<usize>) -> &[Range<usize>] {
        let first = self.0.partition_point(|other| other.end <= range.start);
        let rest = self.0.get(first..).unwrap_or_default();
        let count = rest.partition_point(|other| other.start < range.end);
        scan_work::add(count + 1);
        rest.get(..count).unwrap_or_default()
    }

    /// The range `at` falls strictly inside.
    pub(crate) fn around(&self, at: usize) -> Option<&Range<usize>> {
        let after = self.0.partition_point(|range| range.start < at);
        let range = self.0.get(after.checked_sub(1)?)?;
        (at < range.end).then_some(range)
    }
}

/// `spans` with `range` cut out of every span whose tag `cut` selects.
fn carve(
    spans: Vec<DocSpan>,
    range: &Range<usize>,
    cut: impl Fn(&ComposerTag) -> bool,
) -> Vec<DocSpan> {
    scan_work::add(spans.len());
    let mut out = Vec::with_capacity(spans.len() + 1);
    for span in spans {
        if !cut(&span.tag) || span.range.end <= range.start || span.range.start >= range.end {
            out.push(span);
            continue;
        }
        if span.range.start < range.start {
            out.push(DocSpan::new(
                span.range.start..range.start,
                span.tag.clone(),
            ));
        }
        if span.range.end > range.end {
            out.push(DocSpan::new(range.end..span.range.end, span.tag.clone()));
        }
    }
    out
}

/// Whether every non-whitespace character of `range` carries `tag`'s kind
/// (Telegram Desktop's `HasFullTextTag`). An empty or all-blank range never
/// does.
pub fn has_tag(text: &str, spans: &[DocSpan], range: &Range<usize>, tag: &ComposerTag) -> bool {
    let Some(slice) = text.get(range.clone()) else {
        return false;
    };
    let mut any = false;
    for (ix, ch) in slice.char_indices() {
        if ch.is_whitespace() {
            continue;
        }
        any = true;
        let at = range.start + ix;
        if !spans
            .iter()
            .any(|span| span.tag.same_kind(tag) && span.range.start <= at && at < span.range.end)
        {
            return false;
        }
    }
    any
}

/// The range a block format (quote, code block) applies to: the whole lines
/// the selection touches, without the final line break.
pub fn block_range(text: &str, range: &Range<usize>) -> Range<usize> {
    // Snap both edges to char boundaries: a selection that ends inside a
    // multi-byte character must not split it.
    let start_from = text.floor_char_boundary(range.start);
    let start = text
        .get(..start_from)
        .and_then(|head| head.rfind('\n'))
        .map_or(0, |i| i + 1);
    let end_from = text.ceil_char_boundary(range.end.max(range.start));
    let end = text
        .get(end_from..)
        .and_then(|tail| tail.find('\n'))
        .map_or(text.len(), |i| end_from + i);
    start..end
}

/// Toggle `tag` over `range` (Telegram Desktop's `toggleSelectionMarkdown`):
/// remove it when the whole range already has it, add it otherwise. Adding
/// code or a code block clears the other inline formatting there (Telegram
/// cannot nest inside code); adding a link or mention replaces any link or
/// mention there.
pub fn toggle_tag(
    text: &str,
    spans: &[DocSpan],
    range: Range<usize>,
    tag: ComposerTag,
) -> Vec<DocSpan> {
    let range = if matches!(tag, ComposerTag::Quote) {
        block_range(text, &range)
    } else {
        range
    };
    if range.is_empty() {
        return spans.to_vec();
    }
    let spans = spans.to_vec();
    if has_tag(text, &spans, &range, &tag) && !tag.is_link_like() {
        return normalize_spans(carve(spans, &range, |t| t.same_kind(&tag)));
    }
    let mut spans = match &tag {
        ComposerTag::Code | ComposerTag::Pre(_) => {
            carve(spans, &range, |t| !matches!(t, ComposerTag::Quote))
        }
        ComposerTag::Link(_) | ComposerTag::Mention(_) => {
            carve(spans, &range, ComposerTag::is_link_like)
        }
        _ => spans,
    };
    spans.push(DocSpan::new(range, tag));
    normalize_spans(spans)
}

/// Remove every format from `range` (Telegram Desktop's "Clear formatting").
pub fn clear_tags(spans: &[DocSpan], range: &Range<usize>) -> Vec<DocSpan> {
    normalize_spans(carve(spans.to_vec(), range, |_| true))
}

/// The tags of the text just before `offset` (what typing there continues).
pub fn tags_at(spans: &[DocSpan], offset: usize) -> Vec<ComposerTag> {
    spans
        .iter()
        .filter(|span| span.range.start < offset && offset <= span.range.end)
        .map(|span| span.tag.clone())
        .collect()
}

/// A typed markdown span the field turns into formatting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkdownReplacement {
    /// The markers and the text between them.
    pub outer: Range<usize>,
    /// The text between the markers, which keeps its own formatting and gets
    /// `tag` on top.
    pub inner: Range<usize>,
    pub tag: ComposerTag,
}

/// Markers that turn into formatting as soon as the closing one is typed,
/// longest first. Quill's markup meaning, not Telegram Desktop's (`__` is
/// underline here, as on the send path).
type MarkerTag = fn() -> ComposerTag;
const INSTANT_MARKERS: [(&str, MarkerTag); 8] = [
    ("```", || ComposerTag::Pre(String::new())),
    ("**", || ComposerTag::Bold),
    ("__", || ComposerTag::Underline),
    ("~~", || ComposerTag::Strikethrough),
    ("||", || ComposerTag::Spoiler),
    ("`", || ComposerTag::Code),
    ("*", || ComposerTag::Italic),
    ("_", || ComposerTag::Italic),
];

/// The markdown span that typing the character just before `caret` closed,
/// after Telegram Desktop's `processMarkdownReplaces`: the closing marker
/// ends at the caret, the opening one is earlier on the same line, the text
/// between is not empty and neither starts nor ends with a space, the
/// opening marker starts a word (so `snake_case` and `2*3*4` stay as typed)
/// and is not part of a link (`https://x/__y__`), and nothing is replaced
/// inside an open `` ` `` code span. `code_ranges` are the byte ranges already
/// formatted as code or a code block, where nothing is replaced.
pub fn markdown_replacement(
    text: &str,
    caret: usize,
    code_ranges: &[Range<usize>],
) -> Option<MarkdownReplacement> {
    if caret == 0 || caret > text.len() || !text.is_char_boundary(caret) {
        return None;
    }
    let before = text.get(..caret).unwrap_or_default();
    let last = before.chars().next_back()?;
    if !matches!(last, '*' | '_' | '~' | '`' | '|') {
        return None;
    }
    if code_ranges
        .iter()
        .any(|r| r.start < caret && caret <= r.end)
    {
        return None;
    }
    let line_start = before.rfind('\n').map_or(0, |i| i + 1);
    for (marker, tag) in INSTANT_MARKERS {
        let Some(close) = before.strip_suffix(marker).map(str::len) else {
            continue;
        };
        if close < line_start {
            continue;
        }
        let Some(&marker_char) = marker.as_bytes().first() else {
            continue;
        };
        // The closing marker is the whole run of marker characters typed:
        // `***` closes neither `**` nor `*`.
        if close > 0 && text.as_bytes().get(close - 1) == Some(&marker_char) {
            continue;
        }
        let line = text.get(line_start..close).unwrap_or_default();
        // Candidate openers, nearest first.
        for (rel, _) in line.rmatch_indices(marker) {
            let open = line_start + rel;
            let inner = open + marker.len()..close;
            if inner.is_empty() {
                continue;
            }
            let Some(inner_text) = text.get(inner.clone()) else {
                continue;
            };
            if inner_text.starts_with(char::is_whitespace)
                || inner_text.ends_with(char::is_whitespace)
                || inner_text.as_bytes().first() == Some(&marker_char)
                || inner_text.as_bytes().last() == Some(&marker_char)
            {
                continue;
            }
            // The same boundary rule the send path reads markup with.
            // A half-typed `**hi*` is not italic: its opener touches the
            // marker before it.
            let touches_own =
                text.get(..open).and_then(|before| before.as_bytes().last()) == Some(&marker_char);
            if touches_own || !opener_ok(text, open, marker) || !closer_ok(text, close, marker) {
                continue;
            }
            // `scheme://host/__path__` is a link, not formatting.
            let word_start = text
                .get(..open)
                .unwrap_or_default()
                .rfind(char::is_whitespace)
                .map_or(0, |i| i + 1)
                .max(line_start);
            if text
                .get(word_start..open)
                .unwrap_or_default()
                .contains("://")
            {
                continue;
            }
            // Not inside an unclosed inline code span.
            if marker != "`"
                && marker != "```"
                && text
                    .get(line_start..open)
                    .unwrap_or_default()
                    .matches('`')
                    .count()
                    % 2
                    == 1
            {
                continue;
            }
            if code_ranges
                .iter()
                .any(|r| r.start < inner.end && inner.start < r.end)
            {
                continue;
            }
            return Some(MarkdownReplacement {
                outer: open..caret,
                inner,
                tag: tag(),
            });
        }
    }
    None
}

#[cfg(test)]
mod tests;

#[cfg(test)]
#[path = "round_trip_tests.rs"]
mod round_trip_tests;
#[cfg(test)]
pub(crate) use round_trip_tests::Random;
