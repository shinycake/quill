/// M1: composer text formatting. The composer stays plain text; formatting
/// is authored as lightweight markup (Telegram X `InputView` format menu /
/// tdesktop markdown behavior) and converted to TDLib `textEntities` on
/// the send path by `parse_format_markup`. Paired delimiters only;
/// unmatched delimiters stay literal; inline spans nest (`**a *b* c**`),
/// code stays literal:
/// `**bold**` `*italic*` (or `_italic_`) `__underline__` `~~strike~~`
/// `` `code` `` `||spoiler||` `[label](url)`, `[name](tg://user?id=N)` for a
/// mention of a user without a username; fenced ` ```lang? ` blocks;
/// `> ` line prefix for quotes. The composer field itself shows formatting
/// as you type (`composer_doc`); markup is how its content travels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormatKind {
    Bold,
    Italic,
    Underline,
    Strikethrough,
    Code,
    Pre,
    Spoiler,
    BlockQuote,
    TextUrl,
    /// `![😀](tg://emoji?id=…)` — a custom emoji over its fallback emoji
    /// (`url` keeps the `tg://emoji?id=` link).
    CustomEmoji,
    /// `[Name](tg://user?id=N)` — a mention of a user without a username,
    /// sent as `textEntityTypeMentionName` (`url` keeps the link).
    MentionName,
    /// `[text](tg://time?unix=N)` — a date shown in each reader's time
    /// zone (tdesktop's "Date" formatting, a `custom-date://` tag there),
    /// sent as `textEntityTypeDateTime` (`url` keeps the link).
    DateTime,
}

/// M1: one parsed entity. Offsets are UTF-16 code units — the units TDLib
/// `textEntity` uses (`src/text.rs` maps them back for rendering).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComposerEntity {
    pub offset: i32,
    pub length: i32,
    /// The same text as UTF-8 byte offsets.
    pub bytes: std::ops::Range<usize>,
    pub kind: FormatKind,
    /// `TextUrl` target.
    pub url: String,
    /// `Pre` language; empty renders as plain `textEntityTypePre`.
    pub language: String,
}

/// M1: a formatting action the toolbar / shortcut applies to the composer
/// selection (byte range; empty = cursor).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormatAction {
    Bold,
    Italic,
    Underline,
    Strikethrough,
    Code,
    Pre,
    Spoiler,
    BlockQuote,
    Link(String),
}

impl FormatAction {
    fn open_marker(&self) -> &'static str {
        match self {
            FormatAction::Bold => "**",
            FormatAction::Italic => "*",
            FormatAction::Underline => "__",
            FormatAction::Strikethrough => "~~",
            FormatAction::Code => "`",
            FormatAction::Pre => "```\n",
            FormatAction::Spoiler => "||",
            FormatAction::BlockQuote => "> ",
            FormatAction::Link(_) => "[",
        }
    }

    fn close_marker(&self) -> &'static str {
        match self {
            FormatAction::Bold => "**",
            FormatAction::Italic => "*",
            FormatAction::Underline => "__",
            FormatAction::Strikethrough => "~~",
            FormatAction::Code => "`",
            FormatAction::Pre => "\n```",
            FormatAction::Spoiler => "||",
            FormatAction::BlockQuote => "",
            FormatAction::Link(_) => "]()",
        }
    }
}

/// Composer markup for `text` and its TDLib entities: the inverse of
/// [`parse_format_markup`](super::parse_format_markup), used to restore drafts that come back from
/// Telegram. Kinds the markup can't express (URLs, mentions, …) stay plain
/// text; TDLib detects those again on send.
pub fn entities_to_markup(text: &str, entities: &[crate::text::TextEntity]) -> String {
    crate::composer_doc::ComposerDoc::from_entities(text, entities).to_final_markup()
}

/// M1: apply a formatting action to `range` (UTF-8 byte range; snapped to
/// char boundaries). Returns the new text and the new selection: the
/// wrapped region for a selection, the cursor between markers when empty.
pub fn apply_format_markup(
    text: &str,
    range: std::ops::Range<usize>,
    action: &FormatAction,
) -> (String, std::ops::Range<usize>) {
    let (start, end) = snap_range(text, range);
    if *action == FormatAction::BlockQuote {
        return apply_block_quote(text, start, end);
    }
    let open = action.open_marker();
    let close = action.close_marker();
    if start == end {
        // Empty selection: insert the marker pair, cursor between them.
        // Link inserts `[` + `](url)` and lands the cursor in the URL slot.
        let (insert, cursor_off) = match action {
            FormatAction::Link(url) if url.is_empty() => ("[]()".to_string(), 3),
            FormatAction::Link(url) => (format!("[]({url})"), 3 + url.len()),
            _ => (format!("{open}{close}"), open.len()),
        };
        let mut new_text = String::with_capacity(text.len() + insert.len());
        new_text.push_str(text.get(..start).unwrap_or_default());
        new_text.push_str(&insert);
        new_text.push_str(text.get(start..).unwrap_or_default());
        let cursor = start + cursor_off;
        (new_text, cursor..cursor)
    } else {
        let selected = text.get(start..end).unwrap_or_default();
        let wrapped = match action {
            FormatAction::Link(url) => format!("[{selected}]({url})"),
            _ => format!("{open}{selected}{close}"),
        };
        let mut new_text = String::with_capacity(text.len() + wrapped.len());
        new_text.push_str(text.get(..start).unwrap_or_default());
        new_text.push_str(&wrapped);
        new_text.push_str(text.get(end..).unwrap_or_default());
        let new_start = start + open.len();
        let new_end = new_start + selected.len();
        (new_text, new_start..new_end)
    }
}

/// M1: strip all markup markers in `range` (whole text when empty),
/// keeping the inner text. `[label](url)` collapses to `label`.
pub fn clear_format_markup(text: &str, range: std::ops::Range<usize>) -> String {
    let (start, end) = snap_range(text, range);
    let (start, end) = if start == end {
        (0, text.len())
    } else {
        (start, end)
    };
    let mut result = String::with_capacity(text.len());
    result.push_str(text.get(..start).unwrap_or_default());
    result.push_str(&strip_markup(text.get(start..end).unwrap_or_default()));
    result.push_str(text.get(end..).unwrap_or_default());
    result
}

fn snap_range(text: &str, range: std::ops::Range<usize>) -> (usize, usize) {
    let len = text.len();
    let mut start = range.start.min(len);
    let mut end = range.end.min(len);
    while start > 0 && !text.is_char_boundary(start) {
        start -= 1;
    }
    while end < len && !text.is_char_boundary(end) {
        end += 1;
    }
    if start > end {
        std::mem::swap(&mut start, &mut end);
    }
    (start, end)
}

/// M1: `> ` prefix on every line intersecting the range.
fn apply_block_quote(text: &str, start: usize, end: usize) -> (String, std::ops::Range<usize>) {
    let line_start = text
        .get(..start)
        .unwrap_or_default()
        .rfind('\n')
        .map(|i| i + 1)
        .unwrap_or(0);
    let line_end = text
        .get(end..)
        .unwrap_or_default()
        .find('\n')
        .map(|i| end + i)
        .unwrap_or(text.len());
    let mut new_text = String::with_capacity(text.len() + 8);
    new_text.push_str(text.get(..line_start).unwrap_or_default());
    let mut added_total = 0;
    for line in text
        .get(line_start..line_end)
        .unwrap_or_default()
        .split_inclusive('\n')
    {
        new_text.push_str("> ");
        added_total += 2;
        new_text.push_str(line);
    }
    new_text.push_str(text.get(line_end..).unwrap_or_default());
    // Select the quoted lines (predictable; the user can keep typing).
    (new_text, line_start..line_end + added_total)
}

/// The user id of a `tg://user?id=N` mention link.
pub fn mention_user_id(url: &str) -> Option<i64> {
    url.strip_prefix("tg://user?id=")?
        .parse::<i64>()
        .ok()
        .filter(|id| *id > 0)
}

/// The link a date carries in the composer: `tg://time?unix=N`.
pub fn date_link(unix_time: i32) -> String {
    format!("tg://time?unix={unix_time}")
}

/// The time of a date link (see [`date_link`]); TDLib takes a positive
/// `int32` (tdesktop's date box allows 1 to 2114380800).
pub fn date_link_time(url: &str) -> Option<i32> {
    url.strip_prefix("tg://time?unix=")?
        .parse::<i32>()
        .ok()
        .filter(|time| *time > 0)
}

/// M1: strip markup without producing entities (clear-formatting).
fn strip_markup(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    strip_range(text, 0, text.len(), &mut out);
    out
}

/// [`strip_markup`] for `text[from..to]`; markers are judged against the
/// whole `text`, so a span's neighbours count at every depth.
fn strip_range(text: &str, from: usize, to: usize, out: &mut String) {
    let mut i = from;
    while i < to {
        let rest = text.get(i..to).unwrap_or_default();
        let line_start = i == 0 || text.as_bytes().get(i - 1) == Some(&b'\n');
        if line_start && rest.starts_with("> ") {
            i += 2;
            continue;
        }
        if let Some(after) = rest.strip_prefix("```")
            && let Some(first_nl) = after.find('\n')
        {
            let body_start = first_nl + 1;
            if let Some(close_rel) = after.get(body_start..).unwrap_or_default().find("```") {
                let body = i + 3 + body_start;
                strip_range(text, body, body + close_rel, out);
                i = body + close_rel + 3;
                continue;
            }
        }
        let mut stripped = false;
        for open in ["**", "__", "~~", "||", "`", "*", "_"] {
            let Some(after_open) = rest.strip_prefix(open) else {
                continue;
            };
            // The second `*` of an unmatched `**` never opens italic.
            let continues_marker = open.len() == 1
                && i > from
                && text.as_bytes().get(i - 1) == open.as_bytes().first();
            if continues_marker || !super::markup_boundary::opener_ok(text, i, open) {
                continue;
            }
            let inner_start = i + open.len();
            let line_end = after_open.find('\n').unwrap_or(after_open.len());
            if let Some(close) =
                super::markup_boundary::find_closer(text, inner_start, inner_start + line_end, open)
            {
                strip_range(text, inner_start, close, out);
                i = close + open.len();
                stripped = true;
                break;
            }
        }
        if stripped {
            continue;
        }
        // `[label](url)` → `label`.
        if let Some(after_bracket) = rest.strip_prefix('[')
            && let Some(close_bracket) = after_bracket.find("](")
        {
            let after_paren = after_bracket.get(close_bracket + 2..).unwrap_or_default();
            let line_end = after_paren.find('\n').unwrap_or(after_paren.len());
            if let Some(close_paren) = after_paren.get(..line_end).unwrap_or_default().find(')') {
                strip_range(text, i + 1, i + 1 + close_bracket, out);
                i += 1 + close_bracket + 2 + close_paren + 1;
                continue;
            }
        }
        let Some(ch) = text.get(i..).and_then(|rest| rest.chars().next()) else {
            break;
        };
        if ch != super::markup_boundary::BARRIER {
            out.push(ch);
        }
        i += ch.len_utf8();
    }
}

/// MED4: find `http(s)://` URLs in composer text (TDLib's `getLinkPreview`
/// takes the raw text, but the composer preview chip needs to know when
/// to appear at all). Stdlib scan — no URL parser dependency; trailing
/// punctuation (`,.;:!?)]'"`) is trimmed the way clients do.
pub fn find_urls(text: &str) -> Vec<String> {
    text.split_whitespace()
        .filter_map(|token| {
            let starts = |scheme: &str| {
                let head = token.as_bytes().get(..scheme.len());
                head.is_some_and(|head| head.eq_ignore_ascii_case(scheme.as_bytes()))
            };
            if !starts("http://") && !starts("https://") {
                return None;
            }
            let trimmed =
                token.trim_end_matches([',', '.', ';', ':', '!', '?', ')', ']', '\'', '"', '…']);
            (!trimmed.is_empty()).then(|| trimmed.to_string())
        })
        .collect()
}

/// Composer markup for a custom emoji: Telegram's `![fallback](tg://emoji?id=N)`.
pub fn custom_emoji_markup(fallback: &str, custom_emoji_id: i64) -> String {
    let fallback = if fallback.is_empty() { "⭐" } else { fallback };
    format!("![{fallback}](tg://emoji?id={custom_emoji_id})")
}

#[cfg(test)]
mod tests {
    #![allow(clippy::string_slice, reason = "test code slices known-good literals")]
    use super::*;
    use crate::composer::parse_format_markup;
    use crate::text::{TextEntity, TextEntityKind as K};

    #[test]
    fn custom_emoji_markup_parses_to_an_entity_over_the_fallback() {
        let text = format!("hi {}!", custom_emoji_markup("😀", 5368324170671202286));
        let (plain, entities) = parse_format_markup(&text);
        assert_eq!(plain, "hi 😀!");
        assert_eq!(entities.len(), 1);
        assert_eq!(entities[0].kind, FormatKind::CustomEmoji);
        assert_eq!(entities[0].offset, 3);
        assert_eq!(entities[0].length, 2);
        assert_eq!(entities[0].url, "tg://emoji?id=5368324170671202286");
        // Not a numeric id: never a custom emoji (the plain link rule may
        // still apply to the bracketed part).
        let (_, entities) = parse_format_markup("![x](tg://emoji?id=abc)");
        assert!(entities.iter().all(|e| e.kind != FormatKind::CustomEmoji));
    }
    // M1: the markup parser emits TDLib UTF-16 offsets directly. One test
    // per entity kind, plus the documented edge cases (unmatched stays
    // literal, no nesting).
    #[test]
    fn markup_parses_every_entity_kind() {
        let cases: &[(&str, &str, FormatKind, i32, i32)] = &[
            ("**bold**", "bold", FormatKind::Bold, 0, 4),
            ("*italic*", "italic", FormatKind::Italic, 0, 6),
            ("_italic_", "italic", FormatKind::Italic, 0, 6),
            ("__under__", "under", FormatKind::Underline, 0, 5),
            ("~~strike~~", "strike", FormatKind::Strikethrough, 0, 6),
            ("`code`", "code", FormatKind::Code, 0, 4),
            ("||spoiler||", "spoiler", FormatKind::Spoiler, 0, 7),
            (
                "[label](https://example.com)",
                "label",
                FormatKind::TextUrl,
                0,
                5,
            ),
        ];
        for (input, clean, kind, offset, length) in cases {
            let (text, entities) = parse_format_markup(input);
            assert_eq!(&text, clean, "clean text for {input}");
            assert_eq!(entities.len(), 1, "entity count for {input}");
            assert_eq!(entities[0].kind, *kind);
            assert_eq!(entities[0].offset, *offset);
            assert_eq!(entities[0].length, *length);
        }
        // Pre without language and with language.
        let (text, entities) = parse_format_markup("```\nlet x = 1;\n```");
        assert_eq!(text, "let x = 1;");
        assert_eq!(entities[0].kind, FormatKind::Pre);
        assert!(entities[0].language.is_empty());
        let (text, entities) = parse_format_markup("```rust\nlet x = 1;\n```");
        assert_eq!(text, "let x = 1;");
        assert_eq!(entities[0].kind, FormatKind::Pre);
        assert_eq!(entities[0].language, "rust");
        // Block quote is a `> ` line prefix.
        let (text, entities) = parse_format_markup("> quoted");
        assert_eq!(text, "quoted");
        assert_eq!(entities[0].kind, FormatKind::BlockQuote);
        assert_eq!((entities[0].offset, entities[0].length), (0, 6));
        // URL lands on the entity.
        let (_, entities) = parse_format_markup("[t](https://t.me/x)");
        assert_eq!(entities[0].url, "https://t.me/x");
    }

    #[test]
    fn markup_unmatched_delimiters_stay_literal() {
        let (text, entities) = parse_format_markup("a **bold and *half");
        assert_eq!(text, "a **bold and *half");
        assert!(entities.is_empty());
    }

    #[test]
    fn markup_nests_inline_spans() {
        // The WYSIWYG composer can put italic inside bold, and its markup
        // says so; code stays literal.
        let (text, entities) = parse_format_markup("**a *b* c**");
        assert_eq!(text, "a b c");
        let spans: Vec<_> = entities
            .iter()
            .map(|e| (e.kind, e.offset, e.length))
            .collect();
        assert_eq!(
            spans,
            [(FormatKind::Bold, 0, 5), (FormatKind::Italic, 2, 1)]
        );
        let (text, entities) = parse_format_markup("`a **b**`");
        assert_eq!(text, "a **b**");
        assert_eq!(entities.len(), 1);
        // A mention of a user without a username.
        let (text, entities) = parse_format_markup("hi [Ann **B**](tg://user?id=12)");
        assert_eq!(text, "hi Ann B");
        assert_eq!(entities[0].kind, FormatKind::MentionName);
        assert_eq!((entities[0].offset, entities[0].length), (3, 5));
        assert_eq!(entities[1].kind, FormatKind::Bold);
    }

    #[test]
    fn markup_offsets_are_utf16_with_emoji() {
        // 😀 is one char but two UTF-16 code units; TDLib counts those.
        let (text, entities) = parse_format_markup("😀 **bold**");
        assert_eq!(text, "😀 bold");
        assert_eq!(entities.len(), 1);
        assert_eq!((entities[0].offset, entities[0].length), (3, 4));
        // Emoji inside the formatted span shifts the length too.
        let (text, entities) = parse_format_markup("**a😀b**");
        assert_eq!(text, "a😀b");
        assert_eq!(entities.len(), 1);
        assert_eq!((entities[0].offset, entities[0].length), (0, 4));
        // BMP text after a surrogate pair keeps counting in code units.
        let (text, entities) = parse_format_markup("😀😀 *it*");
        assert_eq!(text, "😀😀 it");
        assert_eq!((entities[0].offset, entities[0].length), (5, 2));
    }

    #[test]
    fn apply_format_wraps_selection_and_places_cursor() {
        // Selection: wrap, selection covers the inner text only.
        let (text, sel) = apply_format_markup("hello world", 6..11, &FormatAction::Bold);
        assert_eq!(text, "hello **world**");
        assert_eq!(&text[sel], "world");
        // Empty selection: marker pair inserted, cursor between markers.
        let (text, sel) = apply_format_markup("hi", 2..2, &FormatAction::Italic);
        assert_eq!(text, "hi**");
        assert_eq!(sel, 3..3);
        // Link with empty selection lands the cursor in the URL slot.
        let (text, sel) = apply_format_markup("hi", 2..2, &FormatAction::Link(String::new()));
        assert_eq!(text, "hi[]()");
        assert_eq!(sel, 5..5);
        // Block quote prefixes each selected line.
        let (text, _) = apply_format_markup("a\nb", 0..3, &FormatAction::BlockQuote);
        assert_eq!(text, "> a\n> b");
    }

    #[test]
    fn clear_format_strips_markers_keeps_text() {
        assert_eq!(
            clear_format_markup("**bold** and `code`", 0..0),
            "bold and code"
        );
        assert_eq!(clear_format_markup("[label](https://x)", 0..0), "label");
        // A selection only strips inside itself.
        assert_eq!(clear_format_markup("**a** **b**", 0..5), "a **b**");
    }

    /// MED4: composer URL detection for the preview chip.
    #[test]
    fn find_urls_detects_http_links() {
        assert!(find_urls("no links here").is_empty());
        assert_eq!(
            find_urls("see https://example.com/a, and http://x.org."),
            vec!["https://example.com/a", "http://x.org"]
        );
        // Bare "www." is not a URL for the chip (TDLib may still preview
        // it server-side; the chip only tracks explicit schemes).
        assert!(find_urls("see www.example.com").is_empty());
        assert_eq!(
            find_urls("https://EXAMPLE.com/Path"),
            vec!["https://EXAMPLE.com/Path"]
        );
    }
    fn entity(text: &str, part: &str, kind: K) -> TextEntity {
        let start = text.find(part).unwrap();
        TextEntity {
            utf8_start: start,
            utf8_end: start + part.len(),
            kind,
        }
    }

    #[test]
    fn rebuilds_markup_that_parses_back_to_the_same_text() {
        let text = "hi very bold text 🔠 link";
        let entities = vec![
            entity(text, "very bold text", K::Bold),
            entity(text, "bold", K::Italic),
            entity(
                text,
                "🔠",
                K::CustomEmoji {
                    custom_emoji_id: 42,
                },
            ),
            entity(
                text,
                "link",
                K::TextUrl {
                    url: "https://t.me".into(),
                },
            ),
        ];
        let markup = entities_to_markup(text, &entities);
        assert_eq!(
            markup,
            "hi **very _bold_ text** ![🔠](tg://emoji?id=42) [link](https://t.me)"
        );
        let (clean, parsed) = parse_format_markup(&markup);
        assert_eq!(clean, text);
        let kinds: Vec<_> = parsed.iter().map(|e| e.kind).collect();
        assert!(kinds.contains(&FormatKind::CustomEmoji));
        assert!(kinds.contains(&FormatKind::Bold));
        assert!(kinds.contains(&FormatKind::TextUrl));
        // Formatting nests: a custom emoji inside bold keeps both, and so
        // does italic inside bold (written with `_`).
        let text = "a 🔠 b";
        let markup = entities_to_markup(
            text,
            &[
                entity(text, "a 🔠 b", K::Bold),
                entity(text, "🔠", K::CustomEmoji { custom_emoji_id: 7 }),
            ],
        );
        assert_eq!(markup, "**a ![🔠](tg://emoji?id=7) b**");
        let text = "x name y";
        let markup =
            entities_to_markup(text, &[entity(text, "name", K::MentionName { user_id: 3 })]);
        assert_eq!(markup, "x [name](tg://user?id=3) y");
    }

    #[test]
    fn quotes_code_and_unknown_kinds() {
        let text = "a\nb\nc x";
        let entities = vec![
            entity(text, "a\nb", K::BlockQuote),
            entity(text, "x", K::Url),
        ];
        assert_eq!(entities_to_markup(text, &entities), "> a\n> b\nc x");
        let code = "run it";
        let entities = vec![entity(code, "it", K::Code)];
        assert_eq!(entities_to_markup(code, &entities), "run `it`");
        assert_eq!(entities_to_markup("plain", &[]), "plain");
    }
}
