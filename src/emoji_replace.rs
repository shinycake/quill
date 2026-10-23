//! "Replace emoji automatically" (tdesktop `Ui::InstantReplaces`,
//! `InputField::processInstantReplaces`): typing a trigger such as `:-)`,
//! `<3` or `:rocket:` swaps it for the emoji as the last character lands.
//! The message field installs `InstantReplaces::Default()` and binds the
//! setting to `setInstantReplacesEnabled`, so with the setting off nothing
//! is replaced at all, dashes and guillemets included (`TextOnly` is only
//! used by the member tag field). Pure, so it is unit tested.
#![deny(clippy::indexing_slicing)]

use std::ops::Range;

/// A replacement to apply to the composer text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstantReplace {
    /// Bytes of the new text to replace.
    pub range: Range<usize>,
    pub with: String,
}

/// The text replacements of `InstantReplaces::Default`.
const TEXT_ONLY: &[(&str, &str)] = &[
    ("--", "\u{2014}"),
    ("<<", "\u{ab}"),
    (">>", "\u{bb}"),
    (":shrug:", "\u{af}\\_(\u{30c4})_/\u{af}"),
];

/// Emoticons of `InstantReplaces::Default`. `:o ` and `xD ` come straight
/// from `InstantReplaces::Default()`; they need the trailing space so the
/// text `:o` and `xD` can be typed on.
const EMOTICONS: &[(&str, &str)] = &[
    (":o ", "\u{1f628}"),
    ("xD ", "\u{1f606}"),
    ("<3", "\u{2764}\u{fe0f}"),
    (":-)", "\u{1f642}"),
    (";-)", "\u{1f609}"),
    (":-(", "\u{1f61e}"),
    (":-P", "\u{1f61b}"),
    (":-D", "\u{1f600}"),
    ("8-)", "\u{1f60e}"),
    (":'(", "\u{1f622}"),
    (":-O", "\u{1f62e}"),
    (":-*", "\u{1f618}"),
    (":-|", "\u{1f610}"),
    (":-/", "\u{1f615}"),
];

/// Whether `new` is `prev` with exactly one character typed right before
/// `caret` (a paste or an IME commit never replaces, as in tdesktop).
pub(crate) fn typed_one_char(prev: &str, new: &str, caret: usize) -> bool {
    let Some(head) = new.get(..caret) else {
        return false;
    };
    let Some(typed) = head.chars().next_back() else {
        return false;
    };
    let start = caret - typed.len_utf8();
    new.len() == prev.len() + typed.len_utf8()
        && prev.get(..start) == new.get(..start)
        && prev.get(start..) == new.get(caret..)
}

fn ends_with_ignore_case(head: &str, pattern: &str) -> bool {
    head.len() >= pattern.len()
        && head.is_char_boundary(head.len() - pattern.len())
        && head
            .get(head.len() - pattern.len()..)
            .unwrap_or_default()
            .eq_ignore_ascii_case(pattern)
}

/// The triggers `InstantReplaces::Default()` adds by hand are matched
/// letter for letter (its trie compares characters exactly); the ones
/// that come from the emoji replacement table ignore case.
fn matches_trigger(head: &str, what: &str) -> bool {
    if matches!(what, ":o " | "xD ") {
        head.ends_with(what)
    } else {
        ends_with_ignore_case(head, what)
    }
}

/// The replacement for the character just typed, if any. `prev` is the
/// text before the keystroke, `new` after it, `caret` the byte offset
/// after the typed character. `enabled` is the "Replace emoji
/// automatically" setting: off replaces nothing (`_instantReplacesEnabled`).
pub fn instant_replacement(
    prev: &str,
    new: &str,
    caret: usize,
    enabled: bool,
) -> Option<InstantReplace> {
    if !enabled || !typed_one_char(prev, new, caret) {
        return None;
    }
    let head = new.get(..caret).unwrap_or_default();
    // `processInstantReplaces` walks the trie back from the typed character
    // and stops at the first node that holds a replacement: the shortest
    // matching ending wins.
    let best = TEXT_ONLY
        .iter()
        .chain(EMOTICONS)
        .filter(|(what, _)| matches_trigger(head, what))
        .min_by_key(|(what, _)| what.len());
    if let Some((what, with)) = best {
        return Some(InstantReplace {
            range: caret - what.len()..caret,
            with: (*with).to_string(),
        });
    }
    shortcode(head)
}

/// `:name:` closed by the typed colon.
fn shortcode(head: &str) -> Option<InstantReplace> {
    let body = head.strip_suffix(':')?;
    let open = body.rfind(':')?;
    let name = body.get(open + 1..).unwrap_or_default();
    if name.is_empty()
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '+' | '-'))
    {
        return None;
    }
    let emoji = crate::suggest::exact_emoji(name)?;
    Some(InstantReplace {
        range: open..head.len(),
        with: emoji.to_string(),
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::string_slice, reason = "test code slices known-good literals")]
    use super::instant_replacement;

    fn typed(prev: &str, ch: char, replace_emoji: bool) -> Option<String> {
        let new = format!("{prev}{ch}");
        let rep = instant_replacement(prev, &new, new.len(), replace_emoji)?;
        Some(format!(
            "{}{}{}",
            &new[..rep.range.start],
            rep.with,
            &new[rep.range.end..]
        ))
    }

    #[test]
    fn dashes_and_guillemets_replace_with_the_setting_on() {
        assert_eq!(typed("a-", '-', true).as_deref(), Some("a\u{2014}"));
        assert_eq!(typed("<", '<', true).as_deref(), Some("\u{ab}"));
        assert_eq!(typed(">", '>', true).as_deref(), Some("\u{bb}"));
        assert_eq!(typed("a", '-', true), None);
    }

    #[test]
    fn the_setting_off_replaces_nothing() {
        // `setInstantReplacesEnabled`: the message field turns every
        // replacement off, dashes and guillemets too.
        assert_eq!(typed("a-", '-', false), None);
        assert_eq!(typed("<", '<', false), None);
        assert_eq!(typed(":-", ')', false), None);
        assert_eq!(typed(":shrug", ':', false), None);
        assert_eq!(typed("go :rocket", ':', false), None);
    }

    #[test]
    fn emoticons_replace() {
        assert_eq!(typed(":-", ')', true).as_deref(), Some("\u{1f642}"));
        assert_eq!(
            typed("I <", '3', true).as_deref(),
            Some("I \u{2764}\u{fe0f}")
        );
    }

    #[test]
    fn hand_added_triggers_match_exactly_and_table_ones_ignore_case() {
        assert_eq!(typed("so xD", ' ', true).as_deref(), Some("so \u{1f606}"));
        // The trie compares characters exactly: no match for `XD `.
        assert_eq!(typed("XD", ' ', true), None);
        // There is no word-start rule: `taxD ` still matches.
        assert_eq!(typed("taxD", ' ', true).as_deref(), Some("ta\u{1f606}"));
        assert_eq!(typed(":-", 'p', true).as_deref(), Some("\u{1f61b}"));
    }

    #[test]
    fn shortcodes_close_on_the_second_colon() {
        assert_eq!(
            typed("go :rocket", ':', true).as_deref(),
            Some("go \u{1f680}")
        );
        assert_eq!(typed("go :rocket", ':', false), None);
        assert_eq!(typed("go :not a name", ':', true), None);
        assert_eq!(typed("12:30", ':', true), None);
    }

    #[test]
    fn shrug_is_a_text_replacement() {
        assert_eq!(
            typed(":shrug", ':', true).as_deref(),
            Some("\u{af}\\_(\u{30c4})_/\u{af}")
        );
    }

    #[test]
    fn pastes_and_edits_in_the_middle_are_left_alone() {
        assert_eq!(instant_replacement("a", "a--", 3, true), None);
        // A character typed before existing text still replaces.
        let rep = instant_replacement("a-b", "a--b", 3, true).unwrap();
        assert_eq!(rep.range, 1..3);
        // Deleting is not typing.
        assert_eq!(instant_replacement("a--", "a-", 2, true), None);
    }
}
