//! macOS "Text Replacements" (System Settings → Keyboard → Text Input),
//! as tdesktop applies them in its message field
//! (`InputField::processSystemTextReplaces`, `applySystemTextReplace`,
//! `Platform::FindSystemTextReplace`): when a character that ends a word
//! is typed, the text before it goes to `NSSpellChecker` with
//! `NSTextCheckingTypeReplacement`; a replacement whose match ends right
//! there swaps the typed shortcut for its phrase. The check runs off the
//! UI thread, so the answer is applied only while the text it was asked
//! about is still in place. Pure, so it is unit tested; the system call is
//! the UI crate's (`spellcheck_mac`).
#![deny(clippy::indexing_slicing)]

use crate::emoji_replace::{InstantReplace, typed_one_char};

/// How far back the shortcut may start (tdesktop's `lookBack`).
const LOOK_BACK_CHARS: usize = 100;

/// A check to run: `text` is what ends right before the typed character.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Check {
    /// Up to [`LOOK_BACK_CHARS`] characters before the typed one.
    pub text: String,
    /// The field's text up to the typed character (byte offset
    /// `prefix.len()`); the answer applies only while it is unchanged.
    pub prefix: String,
}

/// A replacement the system found: the match is the last `match_utf16`
/// UTF-16 units of [`Check::text`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub match_utf16: usize,
    pub replacement: String,
}

/// The check to run after a keystroke, if any. `prev` is the text before
/// it, `new` after it, `caret` the byte offset after the typed character.
/// Only a single typed character that is not a letter or digit asks (a
/// space or punctuation ends the shortcut), and never with nothing before
/// it.
pub fn check_for(prev: &str, new: &str, caret: usize) -> Option<Check> {
    if !typed_one_char(prev, new, caret) {
        return None;
    }
    let head = new.get(..caret)?;
    let typed = head.chars().next_back()?;
    if typed.is_alphanumeric() {
        return None;
    }
    let prefix = head.get(..head.len() - typed.len_utf8())?;
    let from = prefix
        .char_indices()
        .rev()
        .nth(LOOK_BACK_CHARS - 1)
        .map_or(0, |(at, _)| at);
    let text = prefix.get(from..)?;
    (!text.is_empty()).then(|| Check {
        text: text.to_string(),
        prefix: prefix.to_string(),
    })
}

/// The edit to make in `current` for `found`, or `None` when the text
/// before the typed character changed meanwhile or the match does not fit.
pub fn edit_for(current: &str, check: &Check, found: &Found) -> Option<InstantReplace> {
    if found.match_utf16 == 0 || current.get(..check.prefix.len())? != check.prefix {
        return None;
    }
    let end = check.prefix.len();
    let mut units = 0;
    let mut start = end;
    for (at, ch) in check.text.char_indices().rev() {
        if units >= found.match_utf16 {
            break;
        }
        units += ch.len_utf16();
        start = end - (check.text.len() - at);
    }
    if units != found.match_utf16 {
        return None;
    }
    Some(InstantReplace {
        range: start..end,
        with: found.replacement.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn typed(prev: &str, ch: char) -> (String, Option<Check>) {
        let new = format!("{prev}{ch}");
        let check = check_for(prev, &new, new.len());
        (new, check)
    }

    #[test]
    fn a_space_after_a_word_asks_about_the_text_before_it() {
        let (_, check) = typed("say omw", ' ');
        assert_eq!(
            check,
            Some(Check {
                text: "say omw".into(),
                prefix: "say omw".into(),
            })
        );
    }

    #[test]
    fn letters_digits_and_empty_fields_do_not_ask() {
        assert_eq!(typed("om", 'w').1, None);
        assert_eq!(typed("om", '5').1, None);
        assert_eq!(typed("", ' ').1, None);
    }

    #[test]
    fn pastes_do_not_ask() {
        assert_eq!(check_for("a", "a omw ", "a omw ".len()), None);
    }

    #[test]
    fn the_text_sent_is_capped_at_100_characters() {
        let long = "é".repeat(150);
        let (_, check) = typed(&long, '.');
        let check = check.expect("asks");
        assert_eq!(check.text.chars().count(), 100);
        assert_eq!(check.prefix, long);
    }

    #[test]
    fn the_match_is_replaced_before_the_typed_character() {
        let (new, check) = typed("say omw", ' ');
        let check = check.expect("asks");
        let found = Found {
            match_utf16: 3,
            replacement: "On my way!".into(),
        };
        let edit = edit_for(&new, &check, &found).expect("applies");
        assert_eq!(edit.range, 4..7);
        assert_eq!(edit.with, "On my way!");
    }

    #[test]
    fn utf16_lengths_map_to_byte_ranges() {
        // "😀" is two UTF-16 units and four bytes.
        let (new, check) = typed("hi 😀x", ' ');
        let check = check.expect("asks");
        let found = Found {
            match_utf16: 3,
            replacement: "smile".into(),
        };
        let edit = edit_for(&new, &check, &found).expect("applies");
        assert_eq!(new.get(edit.range), Some("😀x"));
    }

    #[test]
    fn typing_on_after_the_check_keeps_the_answer() {
        let (_, check) = typed("omw", ' ');
        let check = check.expect("asks");
        let found = Found {
            match_utf16: 3,
            replacement: "On my way!".into(),
        };
        let edit = edit_for("omw there", &check, &found).expect("applies");
        assert_eq!(edit.range, 0..3);
    }

    #[test]
    fn an_edit_before_the_typed_character_drops_the_answer() {
        let (_, check) = typed("omw", ' ');
        let check = check.expect("asks");
        let found = Found {
            match_utf16: 3,
            replacement: "On my way!".into(),
        };
        assert_eq!(edit_for("om ", &check, &found), None);
        assert_eq!(edit_for("xomw ", &check, &found), None);
    }

    #[test]
    fn a_match_longer_than_the_text_is_ignored() {
        let (new, check) = typed("omw", ' ');
        let check = check.expect("asks");
        let found = Found {
            match_utf16: 9,
            replacement: "x".into(),
        };
        assert_eq!(edit_for(&new, &check, &found), None);
    }
}
