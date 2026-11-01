use super::markup::{ComposerEntity, FormatKind, date_link_time, mention_user_id};
use super::{markup_boundary, scan_work};

/// M1: parse composer markup into clean text + entities. Returns the text
/// with markers stripped and entities with UTF-16 offsets into it.
pub fn parse_format_markup(text: &str) -> (String, Vec<ComposerEntity>) {
    let mut ahead = Lookahead::new(text);
    let mut parser = MarkupParser::new(text, &mut ahead, text.len());
    parser.parse_top();
    (parser.out, parser.entities)
}

#[derive(Clone, Copy)]
struct OutPosition {
    utf16: i32,
    byte: usize,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SpanLevel {
    Top,
    QuoteLine,
    Inline,
}

const INLINE_MARKERS: [(&str, FormatKind); 7] = [
    ("**", FormatKind::Bold),
    ("__", FormatKind::Underline),
    ("~~", FormatKind::Strikethrough),
    ("||", FormatKind::Spoiler),
    ("`", FormatKind::Code),
    ("*", FormatKind::Italic),
    ("_", FormatKind::Italic),
];

#[derive(Clone, Copy)]
enum Target {
    LinkMiddle,
    CloseParen,
    Fence,
    Newline,
    NewlineAfterUrl,
    Closer { index: usize, marker: &'static str },
}

const PLAIN_TARGETS: usize = 5;

impl Target {
    fn slot(self) -> usize {
        match self {
            Self::LinkMiddle => 0,
            Self::CloseParen => 1,
            Self::Fence => 2,
            Self::Newline => 3,
            Self::NewlineAfterUrl => 4,
            Self::Closer { index, .. } => PLAIN_TARGETS + index,
        }
    }

    fn needle(self) -> &'static str {
        match self {
            Self::LinkMiddle => "](",
            Self::CloseParen => ")",
            Self::Fence => "```",
            Self::Newline | Self::NewlineAfterUrl => "\n",
            Self::Closer { marker, .. } => marker,
        }
    }

    fn first_match(self, start: usize) -> usize {
        match self {
            Self::Closer { .. } => start + 1,
            _ => start,
        }
    }
}

#[derive(Clone, Copy)]
struct SearchResult {
    from: usize,
    found: Option<usize>,
}

#[derive(Clone, Copy, Default)]
struct NextMatch {
    last: Option<SearchResult>,
}

impl NextMatch {
    fn get(&mut self, from: usize, search: impl FnOnce() -> Option<usize>) -> Option<usize> {
        if let Some(last) = self.last
            && last.from <= from
            && last.found.is_none_or(|at| from <= at)
        {
            return last.found;
        }
        let found = search();
        self.last = Some(SearchResult { from, found });
        found
    }
}

struct Lookahead<'t> {
    text: &'t str,
    matches: [NextMatch; PLAIN_TARGETS + INLINE_MARKERS.len()],
}

impl<'t> Lookahead<'t> {
    fn new(text: &'t str) -> Self {
        Self {
            text,
            matches: Default::default(),
        }
    }

    fn find(&mut self, target: Target, from: usize, to: usize) -> Option<usize> {
        let text = self.text;
        let start = from;
        let needle = target.needle();
        let at = self
            .matches
            .get_mut(target.slot())?
            .get(target.first_match(start), || match target {
                Target::Closer { .. } => {
                    markup_boundary::find_closer(text, start, text.len(), needle)
                }
                _ => {
                    let found = text.get(start..)?.find(needle).map(|rel| start + rel);
                    scan_work::add(found.unwrap_or(text.len()).saturating_sub(start));
                    found
                }
            })?;
        (at + needle.len() <= to).then_some(at)
    }
}

struct MarkupParser<'t, 'a> {
    text: &'t str,
    ahead: &'a mut Lookahead<'t>,
    out: String,
    /// UTF-16 code-unit length of `out`.
    out16: i32,
    entities: Vec<ComposerEntity>,
    last_quote: Option<usize>,
}

impl<'t, 'a> MarkupParser<'t, 'a> {
    fn new(text: &'t str, ahead: &'a mut Lookahead<'t>, capacity: usize) -> Self {
        Self {
            text,
            ahead,
            out: String::with_capacity(capacity),
            out16: 0,
            entities: Vec::new(),
            last_quote: None,
        }
    }

    fn parse_top(&mut self) {
        let end = self.text.len();
        self.parse_span(0, end, SpanLevel::Top);
        self.entities
            .sort_by_key(|e| (e.offset, std::cmp::Reverse(e.length)));
    }

    fn push_quote_line(&mut self, start: OutPosition) {
        if let Some(prev) = self.last_quote.and_then(|ix| self.entities.get_mut(ix))
            && prev.offset + prev.length + 1 == start.utf16
            && start
                .byte
                .checked_sub(1)
                .and_then(|ix| self.out.as_bytes().get(ix))
                == Some(&b'\n')
        {
            prev.length = self.out16 - prev.offset;
            prev.bytes.end = self.out.len();
            return;
        }
        self.last_quote = Some(self.entities.len());
        self.push_entity(start, FormatKind::BlockQuote, String::new(), String::new());
    }

    /// Parse `self.text[from..to]`. Block syntax (`> ` quote lines) only at
    /// the top level; inline spans nest (`**a *b* c**`), except inside code,
    /// which stays literal.
    fn parse_span(&mut self, from: usize, to: usize, level: SpanLevel) {
        let bytes = self.text.as_bytes();
        let mut i = from;
        while i < to {
            scan_work::add(1);
            if self
                .text
                .get(i..to)
                .unwrap_or_default()
                .starts_with(markup_boundary::ESCAPE)
            {
                i += markup_boundary::ESCAPE.len_utf8();
                if let Some(ch) = self.text.get(i..to).and_then(|rest| rest.chars().next()) {
                    self.push_char(ch);
                    i += ch.len_utf8();
                }
                continue;
            }
            let line_start = i == 0 || bytes.get(i - 1) == Some(&b'\n');
            if level == SpanLevel::Top
                && line_start
                && self.text.get(i..to).unwrap_or_default().starts_with("> ")
            {
                let content_start = i + 2;
                let line_end = self
                    .ahead
                    .find(Target::Newline, content_start, to)
                    .unwrap_or(to);
                let start = self.position();
                self.parse_span(content_start, line_end, SpanLevel::QuoteLine);
                if self.out16 > start.utf16 {
                    self.push_quote_line(start);
                }
                i = line_end;
                continue;
            }
            if let Some(consumed) = self.try_inline(i, to) {
                i = consumed;
                continue;
            }
            if let Some(consumed) = self.try_custom_emoji(i, to) {
                i = consumed;
                continue;
            }
            if let Some(consumed) = self.try_link(i, to) {
                i = consumed;
                continue;
            }
            // `i` always advances by whole chars, so it stays on a boundary.
            let Some(ch) = self.text.get(i..).and_then(|rest| rest.chars().next()) else {
                break;
            };
            self.push_char(ch);
            i += ch.len_utf8();
        }
    }

    /// Try a paired delimiter at byte offset `i`. Returns the offset just
    /// past the closing delimiter on success. Check order matters: longer
    /// delimiters first (`**` before `*`, `__` before `_`, ` ``` ` before
    /// `` ` ``).
    fn try_inline(&mut self, i: usize, to: usize) -> Option<usize> {
        let rest = self.text.get(i..to).unwrap_or_default();
        // Fenced code block first (may span lines, optional language).
        if rest.starts_with("```") {
            return self.try_fenced(i, to);
        }
        for (index, (open, kind)) in INLINE_MARKERS.into_iter().enumerate() {
            if rest.starts_with(open) {
                // A lone `*`/`_` that continues a `**`/`__` pair never opens
                // italic: an unmatched `**bold` must not re-parse the second
                // `*` as an italic opener (unmatched delimiters stay literal).
                if open.len() == 1
                    && i > 0
                    && self.text.as_bytes().get(i - 1) == open.as_bytes().first()
                {
                    continue;
                }
                // A marker inside a word is plain text.
                if !markup_boundary::opener_ok(self.text, i, open) {
                    return None;
                }
                // Inline spans stay on one line (Telegram entities do).
                let inner_start = i + open.len();
                let line_end = self
                    .ahead
                    .find(Target::Newline, inner_start, to)
                    .unwrap_or(to);
                let closer = Target::Closer {
                    index,
                    marker: open,
                };
                if let Some(inner_end) = self.ahead.find(closer, inner_start, line_end) {
                    let start = self.position();
                    if kind == FormatKind::Code {
                        self.push_range(inner_start, inner_end);
                    } else {
                        self.parse_span(inner_start, inner_end, SpanLevel::Inline);
                    }
                    self.push_entity(start, kind, String::new(), String::new());
                    return Some(inner_end + open.len());
                }
                return None;
            }
        }
        None
    }

    fn try_fenced(&mut self, i: usize, to: usize) -> Option<usize> {
        let fence_end = i + "```".len();
        // Language = first line after the fence (may be empty).
        let first_nl = self.ahead.find(Target::Newline, fence_end, to)?;
        let language = self
            .text
            .get(fence_end..first_nl)
            .unwrap_or_default()
            .trim()
            .to_string();
        let body_start = first_nl + 1;
        let close = self.ahead.find(Target::Fence, body_start, to)?;
        let inner = self.text.get(body_start..close).unwrap_or_default();
        // A fenced block with only whitespace is not formatting.
        if inner.trim().is_empty() {
            return None;
        }
        // The newline before the closing fence is block syntax, not content.
        let inner = inner
            .strip_suffix("\r\n")
            .or_else(|| inner.strip_suffix('\n'))
            .unwrap_or(inner);
        let inner_start = body_start;
        let start = self.position();
        self.push_range(inner_start, inner_start + inner.len());
        self.push_entity(start, FormatKind::Pre, String::new(), language);
        Some(close + "```".len())
    }

    /// `![emoji](tg://emoji?id=N)` — Telegram's markup for a custom emoji.
    fn try_custom_emoji(&mut self, i: usize, to: usize) -> Option<usize> {
        if !self.text.get(i..to).unwrap_or_default().starts_with("![") {
            return None;
        }
        let fallback_start = i + "![".len();
        let middle = self.ahead.find(Target::LinkMiddle, fallback_start, to)?;
        let url_start = middle + "](".len();
        let close_paren = self.ahead.find(Target::CloseParen, url_start, to)?;
        let url = self.text.get(url_start..close_paren).unwrap_or_default();
        let id = url.strip_prefix("tg://emoji?id=")?;
        if fallback_start == middle || id.is_empty() || id.parse::<i64>().is_err() {
            return None;
        }
        let start = self.position();
        self.push_range(fallback_start, middle);
        self.push_entity(
            start,
            FormatKind::CustomEmoji,
            url.to_string(),
            String::new(),
        );
        Some(close_paren + 1)
    }

    /// `[label](url)`; a `tg://user?id=N` target is a mention of a user
    /// without a username. The label may hold other inline formatting.
    fn try_link(&mut self, i: usize, to: usize) -> Option<usize> {
        if !self.text.get(i..to).unwrap_or_default().starts_with('[') {
            return None;
        }
        let label_start = i + "[".len();
        let middle = self.ahead.find(Target::LinkMiddle, label_start, to)?;
        let url_start = middle + "](".len();
        // URL stays on one line.
        let line_end = self
            .ahead
            .find(Target::NewlineAfterUrl, url_start, to)
            .unwrap_or(to);
        let close_paren = self.ahead.find(Target::CloseParen, url_start, line_end)?;
        let url = self.text.get(url_start..close_paren).unwrap_or_default();
        if label_start == middle || url.is_empty() {
            return None;
        }
        let url = markup_boundary::strip_barrier(url);
        let kind = if mention_user_id(&url).is_some() {
            FormatKind::MentionName
        } else if date_link_time(&url).is_some() {
            FormatKind::DateTime
        } else {
            FormatKind::TextUrl
        };
        let start = self.position();
        self.parse_span(label_start, middle, SpanLevel::Inline);
        self.push_entity(start, kind, url, String::new());
        Some(close_paren + 1)
    }

    fn position(&self) -> OutPosition {
        OutPosition {
            utf16: self.out16,
            byte: self.out.len(),
        }
    }

    fn push_entity(&mut self, start: OutPosition, kind: FormatKind, url: String, language: String) {
        self.entities.push(ComposerEntity {
            offset: start.utf16,
            length: self.out16 - start.utf16,
            bytes: start.byte..self.out.len(),
            kind,
            url,
            language,
        });
    }

    fn push_range(&mut self, from: usize, to: usize) {
        let text = self.text.get(from..to).unwrap_or_default();
        for ch in text.chars() {
            scan_work::add(ch.len_utf8());
            self.push_char(ch);
        }
    }

    fn push_char(&mut self, ch: char) {
        if matches!(ch, markup_boundary::BARRIER | markup_boundary::ESCAPE) {
            return;
        }
        self.out.push(ch);
        self.out16 += ch.len_utf16() as i32;
    }
}
