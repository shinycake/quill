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

use std::ops::Range;

use crate::composer::{FormatKind, parse_format_markup};
use crate::text::{TextEntity, TextEntityKind, utf16_to_utf8_offset};

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
            let (Ok(start), Ok(end)) = (
                utf16_to_utf8_offset(&doc.text, entity.offset),
                utf16_to_utf8_offset(&doc.text, entity.offset + entity.length),
            ) else {
                continue;
            };
            if start >= end {
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
                FormatKind::TextUrl => ComposerTag::Link(entity.url),
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
                            range: start..end,
                            custom_emoji_id: id,
                        });
                    }
                    continue;
                }
            };
            doc.spans.push(DocSpan::new(start..end, tag));
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

    /// The composer markup for this document; [`Self::from_markup`] of it
    /// gives the document back (spans the markup cannot express are
    /// reshaped: a quote that starts mid-line is dropped, formatting inside
    /// code or a code block is dropped, spans crossing a line break or a link
    /// are cut there).
    pub fn to_markup(&self) -> String {
        Markup::new(self).render()
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

/// `spans` with `range` cut out of every span whose tag `cut` selects.
fn carve(
    spans: Vec<DocSpan>,
    range: &Range<usize>,
    cut: impl Fn(&ComposerTag) -> bool,
) -> Vec<DocSpan> {
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
    let start = text[..range.start.min(text.len())]
        .rfind('\n')
        .map_or(0, |i| i + 1);
    let end_from = range.end.max(range.start).min(text.len());
    let end = text[end_from..]
        .find('\n')
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
    let before = &text[..caret];
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
        let marker_char = marker.as_bytes()[0];
        // The closing marker is the whole run of marker characters typed:
        // `***` closes neither `**` nor `*`.
        if close > 0 && text.as_bytes()[close - 1] == marker_char {
            continue;
        }
        let line = &text[line_start..close];
        // Candidate openers, nearest first.
        for (rel, _) in line.rmatch_indices(marker) {
            let open = line_start + rel;
            let inner = open + marker.len()..close;
            if inner.is_empty() {
                continue;
            }
            let inner_text = &text[inner.clone()];
            if inner_text.starts_with(char::is_whitespace)
                || inner_text.ends_with(char::is_whitespace)
                || inner_text.as_bytes()[0] == marker_char
                || inner_text.as_bytes()[inner_text.len() - 1] == marker_char
            {
                continue;
            }
            // The opener starts a word: nothing or a non-word character
            // before it, and not another marker character.
            let prev = text[..open].chars().next_back();
            if prev.is_some_and(|c| c.is_alphanumeric() || c as u32 == marker_char as u32) {
                continue;
            }
            // `scheme://host/__path__` is a link, not formatting.
            let word_start = text[..open]
                .rfind(char::is_whitespace)
                .map_or(0, |i| i + 1)
                .max(line_start);
            if text[word_start..open].contains("://") {
                continue;
            }
            // Not inside an unclosed inline code span.
            if marker != "`"
                && marker != "```"
                && text[line_start..open].matches('`').count() % 2 == 1
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

/// Writes a document as markup: a walk over segments of the text between
/// span boundaries, with a stack of open inline spans that is closed and
/// reopened where spans cross (`**a [b**](u)[c](u)`), so the result always
/// nests.
struct Markup<'a> {
    doc: &'a ComposerDoc,
    /// Inline spans after reshaping (see [`ComposerDoc::to_markup`]).
    inline: Vec<DocSpan>,
    pre: Vec<DocSpan>,
    quote: Vec<Range<usize>>,
}

impl<'a> Markup<'a> {
    fn new(doc: &'a ComposerDoc) -> Self {
        let text = &doc.text;
        let mut spans: Vec<DocSpan> = doc
            .spans
            .iter()
            .filter_map(|span| {
                let range = snap_out_of_emoji(&doc.emoji, span.range.clone());
                (range.start < range.end && range.end <= text.len()).then(|| DocSpan {
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
        for block in &pre {
            spans = carve(spans, &block.range, |_| true);
        }
        let mut quote: Vec<Range<usize>> = Vec::new();
        let mut inline: Vec<DocSpan> = Vec::new();
        for span in spans {
            if span.tag == ComposerTag::Quote {
                quote.push(span.range);
            } else {
                inline.push(span);
            }
        }
        // Inline formatting stays on one line.
        let mut lines: Vec<DocSpan> = Vec::new();
        for span in inline {
            let mut start = span.range.start;
            for (ix, _) in text[span.range.clone()].match_indices('\n') {
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
        let code: Vec<Range<usize>> = lines
            .iter()
            .filter(|span| span.tag == ComposerTag::Code)
            .map(|span| span.range.clone())
            .collect();
        for code_range in &code {
            lines = lines
                .into_iter()
                .flat_map(|span| {
                    let holds =
                        span.range.start <= code_range.start && code_range.end <= span.range.end;
                    if span.tag == ComposerTag::Code || holds {
                        vec![span]
                    } else {
                        carve(vec![span], code_range, |_| true)
                    }
                })
                .collect();
        }
        Self {
            doc,
            inline: normalize_spans(lines),
            pre,
            quote,
        }
    }

    fn render(&self) -> String {
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
        cuts.retain(|&c| c <= text.len());
        cuts.sort_unstable();
        cuts.dedup();

        let mut out = String::with_capacity(text.len() + 16);
        // Indices into `self.inline` of the open spans, outermost first.
        let mut stack: Vec<usize> = Vec::new();
        let mut i = 0;
        while i + 1 < cuts.len() {
            let (a, b) = (cuts[i], cuts[i + 1]);
            i += 1;
            if let Some(block) = self.pre.iter().find(|p| p.range.start == a) {
                self.close_all(&mut stack, &mut out);
                self.quote_prefix(a, &mut out);
                let language = match &block.tag {
                    ComposerTag::Pre(language) => language.as_str(),
                    _ => "",
                };
                out.push_str("```");
                out.push_str(language);
                out.push('\n');
                out.push_str(&text[block.range.clone()]);
                out.push_str("\n```");
                // Continue from the end of the block.
                while i + 1 < cuts.len() && cuts[i] < block.range.end {
                    i += 1;
                }
                continue;
            }
            let active: Vec<usize> = (0..self.inline.len())
                .filter(|&ix| {
                    let r = &self.inline[ix].range;
                    r.start <= a && a < r.end
                })
                .collect();
            // Close from the first open span that is no longer active.
            if let Some(k) = stack.iter().position(|ix| !active.contains(ix)) {
                while stack.len() > k {
                    let ix = stack.pop().expect("stack holds k+1 spans");
                    out.push_str(&self.closer(ix));
                }
            }
            self.quote_prefix(a, &mut out);
            let mut opening: Vec<usize> = active
                .into_iter()
                .filter(|ix| !stack.contains(ix))
                .collect();
            opening.sort_by_key(|&ix| {
                let span = &self.inline[ix];
                (std::cmp::Reverse(span.range.end), span.tag.nesting())
            });
            for ix in opening {
                out.push_str(&self.opener(ix));
                stack.push(ix);
            }
            match self.doc.emoji.iter().find(|e| e.range.start == a) {
                Some(emoji) if emoji.range.end == b => {
                    out.push_str(&crate::composer::custom_emoji_markup(
                        &text[a..b],
                        emoji.custom_emoji_id,
                    ));
                }
                _ => out.push_str(&text[a..b]),
            }
        }
        self.close_all(&mut stack, &mut out);
        out
    }

    /// `> ` when `at` starts a line inside a quote.
    fn quote_prefix(&self, at: usize, out: &mut String) {
        let text = &self.doc.text;
        let line_start = at == 0 || text.as_bytes()[at - 1] == b'\n';
        if line_start && self.quote.iter().any(|q| q.start <= at && at < q.end) {
            out.push_str("> ");
        }
    }

    fn close_all(&self, stack: &mut Vec<usize>, out: &mut String) {
        while let Some(ix) = stack.pop() {
            out.push_str(&self.closer(ix));
        }
    }

    fn opener(&self, ix: usize) -> String {
        match &self.inline[ix].tag {
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
        match &self.inline[ix].tag {
            ComposerTag::Bold => "**".into(),
            ComposerTag::Italic => self.italic_marker(ix).into(),
            ComposerTag::Underline => "__".into(),
            ComposerTag::Strikethrough => "~~".into(),
            ComposerTag::Spoiler => "||".into(),
            ComposerTag::Code => "`".into(),
            ComposerTag::Link(url) => format!("]({url})"),
            ComposerTag::Mention(user_id) => format!("](tg://user?id={user_id})"),
            ComposerTag::Pre(_) | ComposerTag::Quote => String::new(),
        }
    }

    /// `*` unless the italic text holds `*`, touches a `*`, or shares text
    /// with bold (whose `**` an italic `*` would be read into); then `_`.
    fn italic_marker(&self, ix: usize) -> &'static str {
        let text = &self.doc.text;
        let range = &self.inline[ix].range;
        let touches_star = text[range.clone()].contains('*')
            || text[..range.start].ends_with('*')
            || text[range.end..].starts_with('*');
        let with_bold = self.inline.iter().any(|span| {
            span.tag == ComposerTag::Bold
                && span.range.start < range.end
                && range.start < span.range.end
        });
        if touches_star || with_bold { "_" } else { "*" }
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(text: &str, spans: &[(&str, ComposerTag)]) -> ComposerDoc {
        let mut doc = ComposerDoc::plain(text);
        for (part, tag) in spans {
            let start = text.find(part).expect("part in text");
            doc.spans
                .push(DocSpan::new(start..start + part.len(), tag.clone()));
        }
        doc.normalize();
        doc
    }

    /// The document survives markup both ways, and the markup reads as
    /// expected.
    fn round_trip(doc: &ComposerDoc, markup: &str) {
        assert_eq!(doc.to_markup(), markup, "markup of {doc:?}");
        assert_eq!(
            &ComposerDoc::from_markup(markup),
            doc,
            "document of {markup}"
        );
    }

    #[test]
    fn every_tag_round_trips_through_markup() {
        use ComposerTag as T;
        let cases: Vec<(ComposerDoc, &str)> = vec![
            (doc("a bold b", &[("bold", T::Bold)]), "a **bold** b"),
            (doc("a it b", &[("it", T::Italic)]), "a *it* b"),
            (doc("a un b", &[("un", T::Underline)]), "a __un__ b"),
            (doc("a st b", &[("st", T::Strikethrough)]), "a ~~st~~ b"),
            (doc("a sp b", &[("sp", T::Spoiler)]), "a ||sp|| b"),
            (doc("a co b", &[("co", T::Code)]), "a `co` b"),
            (
                doc("see docs", &[("docs", T::Link("https://t.me".into()))]),
                "see [docs](https://t.me)",
            ),
            (
                doc("hi Ann!", &[("Ann", T::Mention(42))]),
                "hi [Ann](tg://user?id=42)!",
            ),
            (
                doc("x\nlet a;\ny", &[("let a;", T::Pre("rust".into()))]),
                "x\n```rust\nlet a;\n```\ny",
            ),
            (doc("quoted", &[("quoted", T::Quote)]), "> quoted"),
        ];
        for (doc, markup) in cases {
            round_trip(&doc, markup);
        }
    }

    #[test]
    fn nested_and_overlapping_spans_round_trip() {
        use ComposerTag as T;
        // Italic inside bold uses `_`, so `**` is never read into it.
        round_trip(
            &doc(
                "a very bold b",
                &[("very bold", T::Bold), ("bold", T::Italic)],
            ),
            "a **very _bold_** b",
        );
        // Bold inside a link label.
        round_trip(
            &doc(
                "go home now",
                &[
                    ("home now", T::Link("https://x.y".into())),
                    ("home", T::Bold),
                ],
            ),
            "go [**home** now](https://x.y)",
        );
        // Crossing spans: the inner one is closed and reopened, the text
        // and formatting per character are kept.
        let crossing = doc("abcdef", &[("abcd", T::Bold), ("cdef", T::Strikethrough)]);
        let markup = crossing.to_markup();
        assert_eq!(markup, "**ab~~cd~~**~~ef~~");
        assert_eq!(ComposerDoc::from_markup(&markup), crossing);
    }

    #[test]
    fn a_quote_over_several_lines_is_one_span() {
        use ComposerTag as T;
        round_trip(
            &doc("one\ntwo\nafter", &[("one\ntwo", T::Quote)]),
            "> one\n> two\nafter",
        );
        // With bold inside a quoted line.
        round_trip(
            &doc("a b", &[("a b", T::Quote), ("b", T::Bold)]),
            "> a **b**",
        );
    }

    #[test]
    fn spans_across_lines_split_at_the_line_break() {
        let doc = doc("ab\ncd", &[("ab\ncd", ComposerTag::Bold)]);
        let markup = doc.to_markup();
        assert_eq!(markup, "**ab**\n**cd**");
        let back = ComposerDoc::from_markup(&markup);
        assert_eq!(back.text, "ab\ncd");
        assert_eq!(
            back.spans,
            [
                DocSpan::new(0..2, ComposerTag::Bold),
                DocSpan::new(3..5, ComposerTag::Bold)
            ]
        );
    }

    #[test]
    fn custom_emoji_round_trip_inside_formatting() {
        let mut d = doc("a 🔠 b", &[("a 🔠 b", ComposerTag::Bold)]);
        d.emoji.push(DocEmoji {
            range: 2..6,
            custom_emoji_id: 7,
        });
        round_trip(&d, "**a ![🔠](tg://emoji?id=7) b**");
    }

    #[test]
    fn formatting_inside_code_is_dropped() {
        let d = doc(
            "x code y",
            &[("code", ComposerTag::Code), ("od", ComposerTag::Bold)],
        );
        assert_eq!(d.to_markup(), "x `code` y");
        // Formatting around code keeps it.
        let d = doc(
            "x code y",
            &[("x code y", ComposerTag::Bold), ("code", ComposerTag::Code)],
        );
        round_trip(&d, "**x `code` y**");
    }

    #[test]
    fn markup_and_entities_round_trip_through_the_document() {
        // markup -> document -> TDLib entities -> document -> markup.
        let markup = "hi **bold _it_** [Ann](tg://user?id=5) ![😀](tg://emoji?id=9)\n> q";
        let doc = ComposerDoc::from_markup(markup);
        let entities: Vec<TextEntity> = doc
            .spans
            .iter()
            .map(|span| TextEntity {
                utf8_start: span.range.start,
                utf8_end: span.range.end,
                kind: match &span.tag {
                    ComposerTag::Bold => TextEntityKind::Bold,
                    ComposerTag::Italic => TextEntityKind::Italic,
                    ComposerTag::Mention(user_id) => {
                        TextEntityKind::MentionName { user_id: *user_id }
                    }
                    ComposerTag::Quote => TextEntityKind::BlockQuote,
                    other => panic!("unexpected {other:?}"),
                },
            })
            .chain(doc.emoji.iter().map(|e| TextEntity {
                utf8_start: e.range.start,
                utf8_end: e.range.end,
                kind: TextEntityKind::CustomEmoji {
                    custom_emoji_id: e.custom_emoji_id,
                },
            }))
            .collect();
        let back = ComposerDoc::from_entities(&doc.text, &entities);
        assert_eq!(back, doc);
        assert_eq!(back.to_markup(), markup);
        // And the send path sees a mention-name entity.
        let (_, parsed) = parse_format_markup(markup);
        assert!(parsed.iter().any(|e| e.kind == FormatKind::MentionName));
    }

    #[test]
    fn toggling_adds_then_removes_and_respects_conflicts() {
        let text = "one two three";
        let spans = toggle_tag(text, &[], 4..7, ComposerTag::Bold);
        assert_eq!(spans, [DocSpan::new(4..7, ComposerTag::Bold)]);
        // Over a wider selection that is only partly bold: add.
        let spans = toggle_tag(text, &spans, 0..7, ComposerTag::Bold);
        assert_eq!(spans, [DocSpan::new(0..7, ComposerTag::Bold)]);
        // Fully bold: remove from the selection only.
        let spans = toggle_tag(text, &spans, 0..3, ComposerTag::Bold);
        assert_eq!(spans, [DocSpan::new(3..7, ComposerTag::Bold)]);
        // Code clears other inline formatting under it.
        let spans = toggle_tag(text, &spans, 4..13, ComposerTag::Code);
        assert_eq!(
            spans,
            [
                DocSpan::new(3..4, ComposerTag::Bold),
                DocSpan::new(4..13, ComposerTag::Code)
            ]
        );
        // A quote takes whole lines.
        let text = "ab\ncd\nef";
        let spans = toggle_tag(text, &[], 4..4, ComposerTag::Quote);
        assert_eq!(spans, [DocSpan::new(3..5, ComposerTag::Quote)]);
        // Clearing removes everything in the range.
        let spans = vec![
            DocSpan::new(0..8, ComposerTag::Italic),
            DocSpan::new(3..5, ComposerTag::Link("u".into())),
        ];
        assert_eq!(
            clear_tags(&spans, &(2..6)),
            [
                DocSpan::new(0..2, ComposerTag::Italic),
                DocSpan::new(6..8, ComposerTag::Italic)
            ]
        );
    }

    #[test]
    fn typed_markdown_turns_into_formatting_when_closed() {
        let check = |text: &str| markdown_replacement(text, text.len(), &[]);
        let r = check("say **hi**").unwrap();
        assert_eq!((r.outer, r.inner, r.tag), (4..10, 6..8, ComposerTag::Bold));
        let r = check("an *it*").unwrap();
        assert_eq!(r.tag, ComposerTag::Italic);
        assert_eq!(check("x ~~gone~~").unwrap().tag, ComposerTag::Strikethrough);
        assert_eq!(check("x ||hide||").unwrap().tag, ComposerTag::Spoiler);
        assert_eq!(check("x `code`").unwrap().tag, ComposerTag::Code);
        assert_eq!(check("x __u__").unwrap().tag, ComposerTag::Underline);
        assert_eq!(
            check("```let a```").unwrap().tag,
            ComposerTag::Pre(String::new())
        );
        // Not yet closed, or not formatting.
        assert_eq!(check("say **hi*"), None);
        assert_eq!(check("say ** hi**"), None);
        assert_eq!(check("say **hi **"), None);
        assert_eq!(check("2*3*"), None);
        assert_eq!(check("snake_case_"), None);
        assert_eq!(check("https://x.org/__a__"), None);
        assert_eq!(check("a `b *c*"), None);
        assert_eq!(check("**\nx**"), None);
        // Not at the end of the text: the caret decides.
        let text = "a *b* c";
        assert!(markdown_replacement(text, 5, &[]).is_some());
        assert_eq!(markdown_replacement(text, 7, &[]), None);
        // Nothing inside text already formatted as code.
        assert_eq!(
            markdown_replacement("x *y*", 5, std::slice::from_ref(&(0..5))),
            None
        );
    }

    #[test]
    fn tags_and_has_tag_follow_the_spans() {
        let spans = vec![DocSpan::new(0..3, ComposerTag::Bold)];
        assert_eq!(tags_at(&spans, 3), [ComposerTag::Bold]);
        assert!(tags_at(&spans, 0).is_empty());
        assert!(has_tag("abc d", &spans, &(0..4), &ComposerTag::Bold));
        assert!(!has_tag("abc d", &spans, &(0..5), &ComposerTag::Bold));
        assert!(!has_tag("   ", &[], &(0..3), &ComposerTag::Bold));
    }

    #[test]
    fn tags_survive_their_string_form() {
        for tag in [
            ComposerTag::Bold,
            ComposerTag::Pre("rust".into()),
            ComposerTag::Pre(String::new()),
            ComposerTag::Link("https://a.b/c?d=e:f".into()),
            ComposerTag::Mention(77),
        ] {
            assert_eq!(ComposerTag::from_tag(&tag.to_tag()), Some(tag));
        }
        assert_eq!(ComposerTag::from_tag("mention:x"), None);
        assert_eq!(ComposerTag::from_tag("nope"), None);
    }
}
