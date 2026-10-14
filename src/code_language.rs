//! The language of a fenced code block in the composer, after Telegram
//! Desktop's "Code Language" box (chat_helpers/message_field.cpp,
//! `EditCodeLanguageBox`): a field titled "Code Language" labelled
//! "Language for syntax highlighting.", empty meaning auto-detect, at most
//! 32 characters of letters, digits, `+` and `-`.
//!
//! The composer keeps the draft as plain text with markdown fences
//! (`` ```rust\ncode\n``` ``), so setting the language rewrites the text
//! after the opening fence. Pure logic; `src/ui/composer_shortcuts.rs` has
//! the dialog.
#![deny(clippy::indexing_slicing)]

use std::ops::Range;

/// tdesktop `kCodeLanguageLimit`.
pub const CODE_LANGUAGE_LIMIT: usize = 32;

/// A fenced block found in a draft.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeFence {
    /// Byte range of the text between the opening fence and its line end
    /// (the language, possibly empty).
    pub language: Range<usize>,
    /// Byte range of the whole block, fences included.
    pub block: Range<usize>,
}

impl CodeFence {
    /// The block's current language, trimmed; empty when auto-detected.
    pub fn current<'a>(&self, text: &'a str) -> &'a str {
        text.get(self.language.clone()).unwrap_or("").trim()
    }
}

/// Why a language was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LanguageError {
    TooLong,
    BadCharacter,
    /// The draft changed since the block was found, so its range no longer
    /// fits the text.
    Stale,
}

impl LanguageError {
    pub fn note(self) -> &'static str {
        match self {
            Self::TooLong => "A language name has at most 32 characters.",
            Self::BadCharacter => "Use letters, digits, + or - in a language name.",
            Self::Stale => "The code block changed. Try again.",
        }
    }
}

/// Trim and validate a typed language (tdesktop: `^[a-zA-Z0-9+-]*$`, 32).
pub fn validate_language(typed: &str) -> Result<&str, LanguageError> {
    let name = typed.trim();
    if name.chars().count() > CODE_LANGUAGE_LIMIT {
        return Err(LanguageError::TooLong);
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '-')
    {
        return Err(LanguageError::BadCharacter);
    }
    Ok(name)
}

/// The fenced block that contains `caret` (a byte offset; a selection's
/// start). A block counts the way the composer's parser counts it: an
/// opening fence, a language line, a body that is not blank, a closing
/// fence.
pub fn fence_at(text: &str, caret: usize) -> Option<CodeFence> {
    let mut from = 0;
    while let Some(found) = text.get(from..)?.find("```") {
        let open = from + found;
        let after = open + 3;
        let newline = text.get(after..).unwrap_or_default().find('\n')?;
        let body = after + newline + 1;
        match text.get(body..).unwrap_or_default().find("```") {
            Some(close)
                if !text
                    .get(body..body + close)
                    .unwrap_or_default()
                    .trim()
                    .is_empty() =>
            {
                let end = body + close + 3;
                if (open..=end).contains(&caret) {
                    return Some(CodeFence {
                        language: after..after + newline,
                        block: open..end,
                    });
                }
                from = end;
            }
            // Not a block (no closer, or a blank body): the parser treats
            // the backticks as text, so look past them.
            _ => from = open + 1,
        }
    }
    None
}

/// The draft with the block's language replaced, and the byte range of the
/// block afterwards. `None` language text means auto-detect.
pub fn with_language(
    text: &str,
    fence: &CodeFence,
    typed: &str,
) -> Result<(String, Range<usize>), LanguageError> {
    let name = validate_language(typed)?;
    let (Some(head), Some(tail)) = (
        text.get(..fence.language.start),
        text.get(fence.language.end..),
    ) else {
        return Err(LanguageError::Stale);
    };
    if fence.language.start > fence.language.end || fence.block.end > text.len() {
        return Err(LanguageError::Stale);
    }
    let mut out = String::with_capacity(text.len() + name.len());
    out.push_str(head);
    out.push_str(name);
    out.push_str(tail);
    let delta = name.len() as isize - fence.language.len() as isize;
    let end = fence.block.end.saturating_add_signed(delta);
    Ok((out, fence.block.start..end))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::string_slice, reason = "test code slices known-good literals")]
    use super::*;

    #[test]
    fn language_validation_matches_tdesktop() {
        assert_eq!(validate_language(""), Ok(""));
        assert_eq!(validate_language("  rust "), Ok("rust"));
        assert_eq!(validate_language("c++"), Ok("c++"));
        assert_eq!(validate_language("objective-c"), Ok("objective-c"));
        assert_eq!(validate_language("c#"), Err(LanguageError::BadCharacter));
        assert_eq!(validate_language("a b"), Err(LanguageError::BadCharacter));
        assert_eq!(validate_language("é"), Err(LanguageError::BadCharacter));
        assert_eq!(
            validate_language(&"a".repeat(32)),
            Ok("a".repeat(32).as_str())
        );
        assert_eq!(
            validate_language(&"a".repeat(33)),
            Err(LanguageError::TooLong)
        );
    }

    #[test]
    fn finds_the_block_around_the_caret() {
        let text = "intro\n```rust\nfn main() {}\n```\ntail";
        let open = text.find("```").unwrap();
        let fence = fence_at(text, open + 10).unwrap();
        assert_eq!(fence.current(text), "rust");
        assert_eq!(&text[fence.block.clone()], "```rust\nfn main() {}\n```");
        // On the fences themselves counts; outside does not.
        assert!(fence_at(text, open).is_some());
        assert!(fence_at(text, fence.block.end).is_some());
        assert!(fence_at(text, 2).is_none());
        assert!(fence_at(text, text.len()).is_none());
    }

    #[test]
    fn picks_the_right_block_of_several() {
        let text = "```a\nx\n```\n\n```b\ny\n```";
        let second = text.rfind("y").unwrap();
        assert_eq!(fence_at(text, second).unwrap().current(text), "b");
        assert_eq!(fence_at(text, 5).unwrap().current(text), "a");
    }

    #[test]
    fn unfinished_or_blank_blocks_are_not_blocks() {
        assert!(fence_at("```rust\ncode", 4).is_none());
        assert!(fence_at("```rust\n   \n```", 4).is_none());
        assert!(fence_at("```", 1).is_none());
        // A blank block followed by a real one still finds the real one.
        let text = "```\n\n```\n```py\nx\n```";
        let at = text.find('x').unwrap();
        assert_eq!(fence_at(text, at).unwrap().current(text), "py");
    }

    #[test]
    fn sets_changes_and_clears_the_language() {
        let text = "```\nlet x = 1;\n```";
        let fence = fence_at(text, 6).unwrap();
        assert_eq!(fence.current(text), "");
        let (set, block) = with_language(text, &fence, "rust").unwrap();
        assert_eq!(set, "```rust\nlet x = 1;\n```");
        assert_eq!(&set[block], "```rust\nlet x = 1;\n```");

        let fence = fence_at(&set, 12).unwrap();
        let (changed, _) = with_language(&set, &fence, " js ").unwrap();
        assert_eq!(changed, "```js\nlet x = 1;\n```");

        let fence = fence_at(&changed, 9).unwrap();
        let (cleared, block) = with_language(&changed, &fence, "").unwrap();
        assert_eq!(cleared, text);
        assert_eq!(block, 0..text.len());
    }

    #[test]
    fn a_bad_language_changes_nothing() {
        let text = "```\nx\n```";
        let fence = fence_at(text, 5).unwrap();
        assert_eq!(
            with_language(text, &fence, "no good"),
            Err(LanguageError::BadCharacter)
        );
    }

    /// A fence found in an older draft used to slice the new one out of
    /// range or inside a character.
    #[test]
    fn a_stale_fence_is_an_error_not_a_panic() {
        let text = "intro\n```rust\nfn main() {}\n```";
        let fence = fence_at(text, 12).unwrap();
        assert_eq!(
            with_language("short", &fence, "go"),
            Err(LanguageError::Stale)
        );
        let accented = "\u{e9}\u{e9}\u{e9}\u{e9}\u{e9}\u{e9}\u{e9}\u{e9}\u{e9}\u{e9}\u{e9}\u{e9}\u{e9}\u{e9}\u{e9}\u{e9}\u{e9}\u{e9}";
        assert!(!accented.is_char_boundary(fence.language.start));
        assert_eq!(
            with_language(accented, &fence, "go"),
            Err(LanguageError::Stale)
        );
    }

    #[test]
    fn the_new_language_parses_as_a_pre_entity() {
        use crate::composer::{FormatKind, parse_format_markup};
        let text = "```\nlet x = 1;\n```";
        let fence = fence_at(text, 6).unwrap();
        let (set, _) = with_language(text, &fence, "rust").unwrap();
        let (_, entities) = parse_format_markup(&set);
        assert_eq!(entities[0].kind, FormatKind::Pre);
        assert_eq!(entities[0].language, "rust");
    }
}
