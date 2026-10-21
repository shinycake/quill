//! Syntax highlighting of `pre` blocks in messages, after Telegram Desktop
//! (`lib_spellcheck`'s `TryHighlightSyntax`): a block with a language is
//! tokenized off the main thread, the colored spans are cached by the block's
//! text and language, and the block draws plain until they are ready. A block
//! without a language, or with one the highlighter does not know, stays plain.
//!
//! Tokenizing is the kit's syntect highlighter (`gpui_component::highlighter`,
//! pure Rust `fancy-regex`, syntaxes loaded once on first use). Its highlight
//! names fold into the eight slots tdesktop colors (`SyntaxHighlightColors`),
//! and the slots take their colors from Quill's theme tokens when the block
//! draws, so a theme change recolors cached spans without tokenizing again.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::chat_theme;
use gpui_kit::component::highlighter::capture_names;
use gpui_kit::{Context, HighlightStyle, Hsla, Rgba, StyledText};
use std::collections::{HashMap, VecDeque};
use std::hash::{Hash, Hasher};
use std::ops::Range;
use std::sync::{Arc, LazyLock, Mutex, PoisonError};

/// tdesktop `kCacheLimit`.
const CACHE_LIMIT: usize = 256;

/// tdesktop `kCodeLanguageLimit`: longer names never reach the highlighter.
const LANGUAGE_LIMIT: usize = 32;

/// The slots tdesktop colors, in its order (`colors` in
/// `spellcheck_highlight_syntax.cpp`): comment, punctuation, property /
/// number / constant, string, operator, keyword / function, class name,
/// inserted. The kit names no "inserted" token, so only [`diff_spans`] sets
/// it; tdesktop's "deleted" shares the red `Literal` slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Token {
    Comment,
    Punctuation,
    Literal,
    String,
    Operator,
    Keyword,
    Type,
    Inserted,
}

/// A colored stretch of a block's text, in bytes.
pub(crate) type Span = (Range<usize>, Token);

/// The token a kit highlight name colors as, or `None` for plain text.
pub(crate) fn token_for(name: &str) -> Option<Token> {
    let is = |prefix: &str| {
        name == prefix
            || name
                .strip_prefix(prefix)
                .is_some_and(|rest| rest.starts_with('.'))
    };
    Some(if is("comment") {
        Token::Comment
    } else if is("punctuation") {
        Token::Punctuation
    } else if is("property")
        || is("tag")
        || is("boolean")
        || is("number")
        || is("constant")
        || is("variable.special")
    {
        Token::Literal
    } else if is("string") || is("attribute") || is("label") {
        Token::String
    } else if is("operator") || is("link_uri") {
        Token::Operator
    } else if is("keyword") || is("function") || is("preproc") || is("title") {
        Token::Keyword
    } else if is("type") {
        Token::Type
    } else {
        return None;
    })
}

/// The language name the highlighter is asked for, or `None` when the block
/// has none (tdesktop highlights only blocks that name a language). tdesktop
/// aliases `patch` to its `git` grammar; the kit's diff grammar is `diff`.
pub(crate) fn language_for(raw: &str) -> Option<String> {
    let name = raw.trim().to_lowercase();
    if name.is_empty() || name.chars().count() > LANGUAGE_LIMIT {
        return None;
    }
    Some(match name.as_str() {
        "patch" => "diff".to_owned(),
        "c++" => "cpp".to_owned(),
        _ => name,
    })
}

/// The colored spans of `text` in `language`: ordered, not overlapping, each
/// on character boundaries. Empty for a language the highlighter does not
/// know. Pure, but loads the syntaxes on first use, so it runs off the main
/// thread.
pub(crate) fn tokenize(language: &str, text: &str) -> Vec<Span> {
    if is_diff(language) {
        return diff_spans(text);
    }
    let mut spans: Vec<Span> = Vec::new();
    let mut end_of_last = 0;
    for (range, name) in capture_names(language, text) {
        let Some(token) = token_for(name.as_ref()) else {
            continue;
        };
        let fits = range.start >= end_of_last
            && range.start < range.end
            && text.is_char_boundary(range.start)
            && text.is_char_boundary(range.end);
        if !fits {
            continue;
        }
        end_of_last = range.end;
        match spans.last_mut() {
            Some((last, last_token)) if last.end == range.start && *last_token == token => {
                last.end = range.end;
            }
            _ => spans.push((range, token)),
        }
    }
    spans
}

/// Languages tdesktop gives its diff grammar (`diff` and `patch` alias to
/// it); [`language_for`] already folds `patch` into `diff`.
fn is_diff(language: &str) -> bool {
    matches!(language, "diff" | "git")
}

/// The token a diff line takes from its first character, as libprisma's
/// diff grammar does: `+` is inserted, `-` is deleted, which tdesktop colors
/// like literals, and `#` is a comment. (tdesktop's compiled grammar stores
/// the deleted class as `[-\x13]`, so an en dash never matches.)
pub(crate) fn diff_line_token(line: &str) -> Option<Token> {
    match line.chars().next()? {
        '+' => Some(Token::Inserted),
        '-' => Some(Token::Literal),
        '#' => Some(Token::Comment),
        _ => None,
    }
}

/// One span per colored line of a diff, without its line break.
fn diff_spans(text: &str) -> Vec<Span> {
    let mut spans = Vec::new();
    let mut start = 0;
    for line in text.split_inclusive('\n') {
        let end = start + line.len();
        let content = line.strip_suffix('\n').unwrap_or(line);
        let content = content.strip_suffix('\r').unwrap_or(content);
        if let Some(token) = diff_line_token(content) {
            spans.push((start..start + content.len(), token));
        } else {
            // libprisma lists `string` before `command`, `coord` and
            // `commit-sha1`, which have no color, so quoted strings color on
            // every line the line rules leave.
            for range in quoted_strings(content) {
                spans.push((start + range.start..start + range.end, Token::String));
            }
        }
        start = end;
    }
    spans
}

/// The byte ranges of the `"…"` and `'…'` strings of one line, as libprisma's
/// `("|')(?:\\.|(?!\1)[^\\\r\n])*\1`: a backslash takes the next character,
/// and a quote that never closes matches nothing.
fn quoted_strings(line: &str) -> Vec<Range<usize>> {
    let mut found = Vec::new();
    let mut chars = line.char_indices().peekable();
    while let Some((open, quote)) = chars.next() {
        if quote != '"' && quote != '\'' {
            continue;
        }
        let mut probe = chars.clone();
        let mut closed = None;
        while let Some((at, c)) = probe.next() {
            if c == quote {
                closed = Some(at + c.len_utf8());
                break;
            }
            if c == '\\' {
                if probe.next().is_none() {
                    break;
                }
            } else if c == '\r' || c == '\n' {
                break;
            }
        }
        if let Some(close) = closed {
            found.push(open..close);
            while chars.peek().is_some_and(|(at, _)| *at < close) {
                chars.next();
            }
        }
    }
    found
}

enum Entry {
    Pending,
    Ready(Arc<[Span]>),
}

#[derive(Default)]
struct Cache {
    entries: HashMap<u64, Entry>,
    order: VecDeque<u64>,
}

impl Cache {
    fn finish(&mut self, key: u64, spans: Arc<[Span]>) {
        self.entries.insert(key, Entry::Ready(spans));
        self.order.push_back(key);
        while self.order.len() > CACHE_LIMIT {
            if let Some(oldest) = self.order.pop_front() {
                self.entries.remove(&oldest);
            }
        }
    }
}

/// Shared by every window; locked only for a lookup or an insert.
static CACHE: LazyLock<Mutex<Cache>> = LazyLock::new(Mutex::default);

fn cache() -> std::sync::MutexGuard<'static, Cache> {
    CACHE.lock().unwrap_or_else(PoisonError::into_inner)
}

fn key_of(language: &str, text: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    language.hash(&mut hasher);
    text.hash(&mut hasher);
    hasher.finish()
}

/// What a block should draw now.
pub(crate) enum Lookup {
    /// No language: draw the text as it is.
    Plain,
    /// The spans are being worked out: draw plain until they arrive.
    Pending,
    Ready(Arc<[Span]>),
}

/// A highlight the caller must start with [`start`].
pub(crate) struct Job {
    key: u64,
    language: String,
    text: String,
}

/// The spans of `text` for the block's `language`. The first time a pair is
/// asked for, a [`Job`] comes back with `Pending`; it is asked for once.
pub(crate) fn lookup(language: &str, text: &str) -> (Lookup, Option<Job>) {
    let Some(language) = language_for(language) else {
        return (Lookup::Plain, None);
    };
    if text.trim().is_empty() {
        return (Lookup::Plain, None);
    }
    let key = key_of(&language, text);
    let mut cache = cache();
    match cache.entries.get(&key) {
        Some(Entry::Ready(spans)) => (Lookup::Ready(spans.clone()), None),
        Some(Entry::Pending) => (Lookup::Pending, None),
        None => {
            cache.entries.insert(key, Entry::Pending);
            let job = Job {
                key,
                language,
                text: text.to_owned(),
            };
            (Lookup::Pending, Some(job))
        }
    }
}

/// The ready spans for a pair, if any (tests and diagnostics).
#[cfg(test)]
#[cfg(feature = "demo-capture")]
pub(crate) fn ready(language: &str, text: &str) -> Option<Arc<[Span]>> {
    let language = language_for(language)?;
    match cache().entries.get(&key_of(&language, text)) {
        Some(Entry::Ready(spans)) => Some(spans.clone()),
        _ => None,
    }
}

/// Tokenizes `job` on the background executor, stores the spans and asks the
/// window to draw again. The text does not change size, so nothing is
/// measured again.
pub(crate) fn start(job: Job, cx: &mut Context<QuillApp>) {
    cx.spawn(async move |this, cx| {
        let Job {
            key,
            language,
            text,
        } = job;
        let spans = cx
            .background_executor()
            .spawn(async move { Arc::<[Span]>::from(tokenize(&language, &text)) })
            .await;
        cache().finish(key, spans);
        let _ = this.update(cx, |_, cx| cx.notify());
    })
    .detach();
}

impl Token {
    const ALL: [Token; 8] = [
        Token::Comment,
        Token::Punctuation,
        Token::Literal,
        Token::String,
        Token::Operator,
        Token::Keyword,
        Token::Type,
        Token::Inserted,
    ];
}

/// The slot colors on the code block's background, each held to WCAG AA
/// there, in [`Token::ALL`] order. Slots take tdesktop's chart palette
/// (`quill::stat_chart::ColorKey`: red punctuation, literals and operators,
/// orange strings, blue keywords, purple class names). Comments are Quill's
/// muted text instead of tdesktop's light blue: held to AA, the two blues
/// come out the same color on a light background.
pub(crate) fn palette(background: Rgba) -> [Rgba; 8] {
    use quill::stat_chart::ColorKey;
    let background_hex = rgb_hex(background);
    Token::ALL.map(|token| {
        let base = match token {
            Token::Comment => rgb_hex(chat_theme::text_muted()),
            Token::Punctuation | Token::Literal | Token::Operator => ColorKey::Red.rgb(),
            Token::String => ColorKey::Orange.rgb(),
            Token::Keyword => ColorKey::Blue.rgb(),
            Token::Type => ColorKey::Purple.rgb(),
            Token::Inserted => ColorKey::Green.rgb(),
        };
        gpui_kit::rgb(quill::theme_colorize::readable(
            base,
            background_hex,
            quill::theme_colorize::WCAG_AA,
        ))
    })
}

#[cfg(test)]
fn color_of(token: Token, background: Rgba) -> Rgba {
    let index = Token::ALL.iter().position(|t| *t == token).unwrap_or(0);
    palette(background)
        .get(index)
        .copied()
        .unwrap_or(background)
}

fn rgb_hex(color: Rgba) -> u32 {
    let channel = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u32;
    (channel(color.r) << 16) | (channel(color.g) << 8) | channel(color.b)
}

/// The spans that are safe to hand to the text shaper for `code`: not empty,
/// on character boundaries, in order. The cache is keyed by a hash, so a
/// collision could bring spans made for other text.
fn usable(code: &str, spans: &[Span]) -> Vec<(Range<usize>, Token)> {
    let mut end_of_last = 0;
    let mut kept = Vec::with_capacity(spans.len());
    for (range, token) in spans {
        let fits = range.start < range.end
            && range.start >= end_of_last
            && range.end <= code.len()
            && code.is_char_boundary(range.start)
            && code.is_char_boundary(range.end);
        if fits {
            end_of_last = range.end;
            kept.push((range.clone(), *token));
        }
    }
    kept
}

fn highlights(
    code: &str,
    spans: &[Span],
    colors: &[Rgba; 8],
) -> Vec<(Range<usize>, HighlightStyle)> {
    usable(code, spans)
        .into_iter()
        .filter_map(|(range, token)| {
            let index = Token::ALL.iter().position(|t| *t == token)?;
            let color: Hsla = (*colors.get(index)?).into();
            Some((
                range,
                HighlightStyle {
                    color: Some(color),
                    ..HighlightStyle::default()
                },
            ))
        })
        .collect()
}

/// `code` as styled text with `spans` colored on `background`. High contrast
/// keeps one text color.
pub(crate) fn styled(code: &str, spans: &[Span], background: Rgba) -> StyledText {
    let runs = if chat_theme::high_contrast() {
        Vec::new()
    } else {
        highlights(code, spans, &palette(background))
    };
    #[cfg(test)]
    DRAWN_RUNS.with(|drawn| drawn.set(drawn.get() + runs.len()));
    StyledText::new(code.to_owned()).with_highlights(runs)
}

#[cfg(test)]
thread_local! {
    /// Tests: the colored runs [`styled`] handed to the text on this thread.
    pub(crate) static DRAWN_RUNS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
mod tests {
    use super::{Lookup, Token, language_for, lookup, token_for, tokenize};

    fn text_of<'a>(code: &'a str, spans: &[super::Span], token: Token) -> Vec<&'a str> {
        spans
            .iter()
            .filter(|(_, t)| *t == token)
            .filter_map(|(range, _)| code.get(range.clone()))
            .map(str::trim_end)
            .collect()
    }

    #[test]
    fn language_names_fold_to_the_highlighters() {
        assert_eq!(language_for("Rust").as_deref(), Some("rust"));
        assert_eq!(language_for("  python ").as_deref(), Some("python"));
        assert_eq!(language_for("patch").as_deref(), Some("diff"));
        assert_eq!(language_for("C++").as_deref(), Some("cpp"));
    }

    #[test]
    fn no_language_is_plain() {
        assert_eq!(language_for(""), None);
        assert_eq!(language_for("   "), None);
        assert_eq!(language_for(&"a".repeat(33)), None);
        assert!(matches!(lookup("", "let x = 1;").0, Lookup::Plain));
        assert!(matches!(lookup("rust", "  \n").0, Lookup::Plain));
    }

    #[test]
    fn unknown_language_has_no_spans() {
        assert!(tokenize("klingon-script", "qapla' 42 \"x\"").is_empty());
        assert!(tokenize("text", "fn main() {}").is_empty());
    }

    #[test]
    fn names_fold_into_the_eight_slots() {
        assert_eq!(token_for("comment.doc"), Some(Token::Comment));
        assert_eq!(token_for("punctuation.bracket"), Some(Token::Punctuation));
        assert_eq!(token_for("number"), Some(Token::Literal));
        assert_eq!(token_for("string.escape"), Some(Token::String));
        assert_eq!(token_for("operator"), Some(Token::Operator));
        assert_eq!(token_for("function"), Some(Token::Keyword));
        assert_eq!(token_for("type"), Some(Token::Type));
        assert_eq!(token_for("variable"), None);
        assert_eq!(token_for("commentary"), None);
    }

    #[test]
    fn rust_sample_gets_spans() {
        let code = "// add\nfn add(a: i32) -> i32 { a + 42 }\nlet s = \"hi\";";
        let spans = tokenize("rust", code);
        assert!(text_of(code, &spans, Token::Comment).contains(&"// add"));
        assert!(text_of(code, &spans, Token::Keyword).contains(&"fn"));
        assert!(text_of(code, &spans, Token::Literal).contains(&"42"));
        assert!(text_of(code, &spans, Token::String).contains(&"\"hi\""));
        let ordered = spans.windows(2).all(|pair| match pair {
            [(a, _), (b, _)] => a.end <= b.start,
            _ => true,
        });
        assert!(ordered, "spans do not overlap");
    }

    #[test]
    fn python_sample_gets_spans() {
        let code = "def add(a):\n    return a + 1  # one";
        let spans = tokenize("python", code);
        assert!(text_of(code, &spans, Token::Keyword).contains(&"def"));
        assert!(text_of(code, &spans, Token::Comment).contains(&"# one"));
    }

    #[test]
    fn spans_stay_on_char_boundaries() {
        let code = "let s = \"héllo wörld\"; // ünï";
        for (range, _) in tokenize("rust", code) {
            assert!(code.is_char_boundary(range.start));
            assert!(code.is_char_boundary(range.end));
        }
    }

    fn each_mode(check: impl Fn()) {
        use quill::app_theme::BuiltinTheme;
        crate::ui::chat_theme::set_dark_for_test(true);
        check();
        crate::ui::chat_theme::set_dark_for_test(false);
        check();
        for theme in [BuiltinTheme::Night, BuiltinTheme::Day] {
            crate::ui::app_theme::install_for_test(Some(theme.palette().colors()));
            check();
        }
        crate::ui::app_theme::install_for_test(None);
        crate::ui::chat_theme::set_dark_for_test(true);
    }

    #[test]
    fn slot_colors_are_readable_on_the_code_background() {
        use super::{palette, rgb_hex};
        each_mode(|| {
            let background = crate::ui::chat_theme::bg_code();
            for (token, color) in Token::ALL.iter().zip(palette(background)) {
                let ratio = quill::theme_colorize::contrast(rgb_hex(color), rgb_hex(background));
                assert!(ratio >= quill::theme_colorize::WCAG_AA, "{token:?} {ratio}");
            }
        });
    }

    #[test]
    fn comments_and_keywords_are_told_apart_in_every_mode() {
        use super::{color_of, rgb_hex};
        each_mode(|| {
            let background = crate::ui::chat_theme::bg_code();
            let comment = rgb_hex(color_of(Token::Comment, background));
            let keyword = rgb_hex(color_of(Token::Keyword, background));
            let distance: u32 = [16, 8, 0]
                .iter()
                .map(|shift| ((comment >> shift) & 0xff).abs_diff((keyword >> shift) & 0xff))
                .sum();
            assert!(distance >= 90, "{comment:06x} vs {keyword:06x}");
        });
    }

    #[test]
    fn diff_lines_are_classified_by_their_first_character() {
        use super::diff_line_token;
        assert_eq!(diff_line_token("+added"), Some(Token::Inserted));
        assert_eq!(diff_line_token("-removed"), Some(Token::Literal));
        assert_eq!(diff_line_token("\u{2013}dash"), None);
        assert_eq!(diff_line_token("# note"), Some(Token::Comment));
        assert_eq!(diff_line_token(" context"), None);
        assert_eq!(diff_line_token("@@ -1 +1 @@"), None);
        assert_eq!(diff_line_token(""), None);
    }

    #[test]
    fn strings_color_on_diff_lines_without_a_line_token() {
        let code = "@@ -1 +1 @@ \"hunk\"\n say(\"Hi\", 'a\\'b', \"open\n+ add(\"no\")\n \"é\" 'x";
        let spans = tokenize("diff", code);
        assert_eq!(
            text_of(code, &spans, Token::String),
            vec!["\"hunk\"", "\"Hi\"", "'a\\'b'", "\"é\""]
        );
        assert_eq!(
            text_of(code, &spans, Token::Inserted),
            vec!["+ add(\"no\")"]
        );
        for (range, _) in &spans {
            assert!(code.is_char_boundary(range.start) && code.is_char_boundary(range.end));
        }
    }

    #[test]
    fn diff_blocks_color_added_and_removed_lines() {
        let code = "@@ -1,3 +1,3 @@\n fn a() {\n-    old();\r\n+    new();\n }\n+é";
        for language in ["diff", "git"] {
            let spans = tokenize(language, code);
            assert_eq!(
                text_of(code, &spans, Token::Inserted),
                vec!["+    new();", "+é"]
            );
            assert_eq!(text_of(code, &spans, Token::Literal), vec!["-    old();"]);
            for (range, _) in &spans {
                assert!(code.is_char_boundary(range.start) && code.is_char_boundary(range.end));
            }
        }
        assert!(tokenize("diff", "").is_empty());
        assert!(tokenize("diff", "\n\n").is_empty());
    }

    #[test]
    fn inserted_green_is_told_apart_from_deleted_red_in_every_mode() {
        use super::{color_of, rgb_hex};
        each_mode(|| {
            let background = crate::ui::chat_theme::bg_code();
            let inserted = rgb_hex(color_of(Token::Inserted, background));
            let deleted = rgb_hex(color_of(Token::Literal, background));
            let (green, red) = (inserted >> 8 & 0xff, deleted >> 8 & 0xff);
            let (inserted_red, deleted_red) = (inserted >> 16 & 0xff, deleted >> 16 & 0xff);
            assert!(green > inserted_red, "{inserted:06x} is green");
            assert!(deleted_red > red, "{deleted:06x} is red");
        });
    }

    #[test]
    fn spans_that_do_not_fit_the_text_are_dropped() {
        use super::{highlights, palette};
        let code = "é = 1 // ünï";
        let colors = palette(crate::ui::chat_theme::bg_code());
        let spans = vec![
            (1..2, Token::Keyword),   // cuts the "é"
            (3..3, Token::Keyword),   // empty
            (0..2, Token::Literal),   // fits
            (4..5, Token::Literal),   // fits
            (3..4, Token::String),    // overlaps nothing but is out of order
            (6..400, Token::Comment), // past the end
        ];
        let kept: Vec<_> = highlights(code, &spans, &colors)
            .into_iter()
            .map(|(range, _)| range)
            .collect();
        assert_eq!(kept, vec![0..2, 4..5]);
    }
}
