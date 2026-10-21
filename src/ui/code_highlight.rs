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
/// number / constant, string, operator, keyword / function, class name. Its
/// "inserted" slot has no source here: the kit names a diff's added lines
/// no differently from other text.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Token {
    Comment,
    Punctuation,
    Literal,
    String,
    Operator,
    Keyword,
    Type,
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

/// The slot colors on the code block's background, each held to WCAG AA
/// there. Hues follow tdesktop's palette (light blue comments, red
/// punctuation and literals, orange strings, blue keywords, purple class
/// names) through Quill's tokens.
pub(crate) fn color_of(token: Token, background: Rgba) -> Rgba {
    let base = match token {
        Token::Comment => chat_theme::accent_info(),
        Token::Punctuation => chat_theme::danger_soft(),
        Token::Literal => chat_theme::danger_bright(),
        Token::String => chat_theme::warning_orange(),
        Token::Operator => chat_theme::danger_pale(),
        Token::Keyword => chat_theme::accent(),
        Token::Type => purple(),
    };
    let base_hex = rgb_hex(base);
    let readable = quill::theme_colorize::readable(
        base_hex,
        rgb_hex(background),
        quill::theme_colorize::WCAG_AA,
    );
    gpui_kit::rgb(readable)
}

fn purple() -> Rgba {
    gpui_kit::rgb(if chat_theme::is_dark_palette() {
        0xd2a8ff
    } else {
        0x8250df
    })
}

fn rgb_hex(color: Rgba) -> u32 {
    let channel = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u32;
    (channel(color.r) << 16) | (channel(color.g) << 8) | channel(color.b)
}

/// `code` as styled text with `spans` colored on `background`. High contrast
/// keeps one text color.
pub(crate) fn styled(code: &str, spans: &[Span], background: Rgba) -> StyledText {
    let highlights: Vec<(Range<usize>, HighlightStyle)> = if chat_theme::high_contrast() {
        Vec::new()
    } else {
        spans
            .iter()
            .filter(|(range, _)| range.end <= code.len())
            .map(|(range, token)| {
                let color: Hsla = color_of(*token, background).into();
                (
                    range.clone(),
                    HighlightStyle {
                        color: Some(color),
                        ..HighlightStyle::default()
                    },
                )
            })
            .collect()
    };
    StyledText::new(code.to_owned()).with_highlights(highlights)
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

    #[test]
    fn slot_colors_are_readable_on_the_code_background() {
        use super::{color_of, rgb_hex};
        let background = crate::ui::chat_theme::bg_code();
        for token in [
            Token::Comment,
            Token::Punctuation,
            Token::Literal,
            Token::String,
            Token::Operator,
            Token::Keyword,
            Token::Type,
        ] {
            let ratio = quill::theme_colorize::contrast(
                rgb_hex(color_of(token, background)),
                rgb_hex(background),
            );
            assert!(ratio >= quill::theme_colorize::WCAG_AA, "{token:?} {ratio}");
        }
    }
}
