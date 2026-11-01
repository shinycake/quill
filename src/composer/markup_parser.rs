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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Seam {
    pub byte: usize,
    pub utf16_units: i32,
    pub outside_spans: bool,
}

pub(crate) struct MarkupScanner<'t> {
    ahead: Lookahead<'t>,
    seams: Vec<Seam>,
}

impl<'t> MarkupScanner<'t> {
    pub(crate) fn new(markup: &'t str) -> Self {
        Self {
            ahead: Lookahead::new(markup),
            seams: Vec::new(),
        }
    }

    pub(crate) fn rest(&self) -> &'t str {
        let ahead = &self.ahead;
        ahead.text.get(ahead.parse_start..).unwrap_or_default()
    }

    pub(crate) fn advance(&mut self, bytes: usize) {
        self.ahead.parse_start += bytes;
    }

    pub(crate) fn scan(&mut self, budget: i32) -> Option<&[Seam]> {
        let rest = self.rest();
        let capacity = usize::try_from(budget).unwrap_or(0).min(rest.len());
        self.seams.clear();
        let mut parser = MarkupParser::new(rest, &mut self.ahead, capacity);
        parser.scan = Some(SeamScan {
            budget,
            stopped: false,
            seams: std::mem::take(&mut self.seams),
        });
        parser.parse_top();
        let scan = parser.scan.take()?;
        self.seams = scan.seams;
        scan.stopped.then_some(self.seams.as_slice())
    }
}

const SEAMS_BETWEEN_LINES: usize = 2;

struct SeamScan {
    budget: i32,
    stopped: bool,
    seams: Vec<Seam>,
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
    parse_start: usize,
    matches: [NextMatch; PLAIN_TARGETS + INLINE_MARKERS.len()],
}

impl<'t> Lookahead<'t> {
    fn new(text: &'t str) -> Self {
        Self {
            text,
            parse_start: 0,
            matches: Default::default(),
        }
    }

    fn find(&mut self, target: Target, from: usize, to: usize) -> Option<usize> {
        let text = self.text;
        let start = self.parse_start + from;
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
        let at = at.checked_sub(self.parse_start)?;
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
    scan: Option<SeamScan>,
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
            scan: None,
        }
    }

    fn parse_top(&mut self) {
        let end = self.text.len();
        self.parse_span(0, end, SpanLevel::Top);
        if !self.stop_if_over_budget(end) {
            self.note_seam(end, SpanLevel::Top);
        }
        self.entities
            .sort_by_key(|e| (e.offset, std::cmp::Reverse(e.length)));
    }

    fn stop_if_over_budget(&mut self, at: usize) -> bool {
        let utf16_units = self.out16;
        let Some(scan) = &mut self.scan else {
            return false;
        };
        if !scan.stopped && utf16_units > scan.budget {
            scan.stopped = true;
            scan.seams.push(Seam {
                byte: at,
                utf16_units,
                outside_spans: false,
            });
        }
        scan.stopped
    }

    fn note_seam(&mut self, at: usize, level: SpanLevel) {
        if level != SpanLevel::Inline
            && let Some(scan) = &mut self.scan
        {
            scan.seams.push(Seam {
                byte: at,
                utf16_units: self.out16,
                outside_spans: level == SpanLevel::Top,
            });
        }
    }

    fn push_quote_line(&mut self, start: OutPosition, first_seam: usize) {
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
            if let Some(scan) = &mut self.scan {
                let between = first_seam.saturating_sub(SEAMS_BETWEEN_LINES)..first_seam;
                for seam in scan.seams.get_mut(between).unwrap_or_default() {
                    seam.outside_spans = false;
                }
            }
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
            if self.stop_if_over_budget(i) {
                return;
            }
            self.note_seam(i, level);
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
                let first_seam = self.scan.as_ref().map_or(0, |scan| scan.seams.len());
                self.parse_span(content_start, line_end, SpanLevel::QuoteLine);
                if self.out16 > start.utf16 {
                    self.push_quote_line(start, first_seam);
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
        let kind = if mention_user_id(url).is_some() {
            FormatKind::MentionName
        } else if date_link_time(url).is_some() {
            FormatKind::DateTime
        } else {
            FormatKind::TextUrl
        };
        let start = self.position();
        self.parse_span(label_start, middle, SpanLevel::Inline);
        self.push_entity(start, kind, url.to_string(), String::new());
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
        for (at, ch) in text.char_indices() {
            scan_work::add(ch.len_utf8());
            if self.stop_if_over_budget(from + at) {
                return;
            }
            self.push_char(ch);
        }
    }

    fn push_char(&mut self, ch: char) {
        if ch == markup_boundary::BARRIER {
            return;
        }
        self.out.push(ch);
        self.out16 += ch.len_utf16() as i32;
    }
}

#[cfg(test)]
mod tests {
    use super::{MarkupScanner, parse_format_markup};

    const MARKUPS: [&str; 6] = [
        "plain words with no markers in them at all",
        "**bold** then _italic_ then `code` then ~~gone~~ and more text",
        "> first quoted line\n> second quoted line\nand a plain line after",
        "[a link](https://t.me/x) and ![😀](tg://emoji?id=42) and more text",
        "```rust\nlet a = 1;\n```\nthen plain text after the fence ends",
        "*never closed and [never linked and `never coded with more text",
    ];

    fn units(markup: &str) -> i32 {
        parse_format_markup(markup).0.encode_utf16().count() as i32
    }

    #[test]
    fn a_scan_stops_just_past_its_budget() {
        let markup = "word ".repeat(1000);
        let mut scanner = MarkupScanner::new(&markup);
        let seams = scanner.scan(10).expect("over the budget");
        assert_eq!(seams.last().map(|seam| seam.utf16_units), Some(11));
    }

    #[test]
    fn a_scan_of_text_that_fits_returns_nothing() {
        for markup in MARKUPS {
            assert_eq!(MarkupScanner::new(markup).scan(1000), None);
        }
    }

    #[test]
    fn a_clean_seam_counts_the_text_before_it() {
        for markup in MARKUPS {
            let mut scanner = MarkupScanner::new(markup);
            let seams = scanner.scan(16).expect("over the budget");
            for seam in seams.iter().filter(|seam| seam.outside_spans) {
                let head = markup.get(..seam.byte).unwrap_or_default();
                let at = seam.byte;
                assert_eq!(units(head), seam.utf16_units, "{markup:?} at {at}");
            }
        }
    }

    #[test]
    fn no_clean_seam_lies_inside_a_quote() {
        let markup = "> one\n> two\n> three and more\nplain";
        let mut scanner = MarkupScanner::new(markup);
        let seams = scanner.scan(16).expect("over the budget");
        let last_quoted = markup.len() - "\nplain".len();
        assert!(
            seams
                .iter()
                .filter(|seam| seam.byte < last_quoted)
                .all(|seam| !seam.outside_spans || seam.byte == 0)
        );
    }

    #[test]
    fn a_shared_scanner_sees_a_suffix_as_a_fresh_one_does() {
        for markup in MARKUPS {
            let mut shared = MarkupScanner::new(markup);
            for ch in markup.chars() {
                let rest = shared.rest();
                let fresh = MarkupScanner::new(rest).scan(8).map(<[_]>::to_vec);
                let seen = shared.scan(8).map(<[_]>::to_vec);
                assert_eq!(seen, fresh, "{rest:?}");
                shared.advance(ch.len_utf8());
            }
        }
    }
}
