//! Words in the composer that have an emoji, shown with a soft highlight;
//! hovering one offers the same strip as typing `:word`.
//!
//! Telegram Desktop has no such feature. The closest one is iOS Messages'
//! emoji replacement (iOS 10): with the emoji keyboard open, words that
//! have an emoji turn orange and a tap offers them. Here every word that is
//! an emoji keyword lights up as soon as it is finished; a word the user
//! does not want is turned off from its context menu and listed in
//! Settings (docs/decisions/emoji-word-highlights.md):
//!
//! - it is finished: the caret or the selection is not touching it;
//! - it is a plain word outside code, links and mentions, and not one
//!   tdesktop's `SkipExactKeyword` drops (most one- and two-letter words;
//!   "ok" stays);
//! - it is a whole emoji keyword.
//!
//! The lookups are TDLib's on-device keyword search, so nothing typed
//! leaves the device. Pure, so it is unit tested.
#![deny(clippy::indexing_slicing)]

use std::ops::Range;

use crate::emoji::keyword_lookup::{keyword_emoji, normalize_query, skip_exact_keyword};
use crate::telegram::envelope_emoji::EmojiKeyword;

/// Only this much of a long draft is looked at.
pub const SCAN_BYTES: usize = 4096;

/// A word worth a lookup: where it is and its normalized form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub range: Range<usize>,
    pub normalized: String,
}

/// A highlighted word and the emoji it offers, best first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Highlight {
    pub range: Range<usize>,
    pub normalized: String,
    pub emoji: Vec<String>,
}

/// Words looked at in one draft, the nearest to the caret: a long paste
/// would otherwise send a lookup for each of its words.
pub const MAX_CANDIDATES: usize = 48;

/// The words of `text` (bytes, `char` boundaries) that may be highlighted:
/// letters and digits (an inner `'` or `-` is kept, "don't", "t-rex"),
/// not dropped by [`skip_exact_keyword`], not touching `selection`, not overlapping
/// `skip` (code, links, mentions), not part of a tag, mention, command,
/// `:query`, link or path. At most [`MAX_CANDIDATES`], the nearest to the
/// selection. Also the total word count, for the density limit.
pub fn candidates(
    text: &str,
    selection: Range<usize>,
    skip: &[Range<usize>],
) -> (Vec<Candidate>, usize) {
    let scan = floor_boundary(text, SCAN_BYTES);
    let head = text.get(..scan).unwrap_or_default();
    let tokens = tokens(head);
    let mut token = tokens.iter().peekable();
    let mut out = Vec::new();
    let mut words = 0;
    for range in word_ranges(head) {
        words += 1;
        while token.next_if(|t| t.range.end < range.end).is_some() {}
        let marked = token.peek().is_some_and(|t| t.marked);
        let Some(word) = head.get(range.clone()) else {
            continue;
        };
        let touches_selection = range.start <= selection.end && selection.start <= range.end;
        let normalized = normalize_query(word);
        if touches_selection
            || marked
            || skip_exact_keyword(&normalized)
            || !word
                .chars()
                .all(|c| c.is_alphanumeric() || is_mark(c) || c == '\'' || c == '-')
            || skip
                .iter()
                .any(|r| r.start < range.end && range.start < r.end)
        {
            continue;
        }
        out.push(Candidate { range, normalized });
    }
    if out.len() > MAX_CANDIDATES {
        let distance = |c: &Candidate| {
            if c.range.end < selection.start {
                selection.start - c.range.end
            } else {
                c.range.start.saturating_sub(selection.end)
            }
        };
        out.sort_by_key(|c| (distance(c), c.range.start));
        out.truncate(MAX_CANDIDATES);
        out.sort_by_key(|c| c.range.start);
    }
    (out, words)
}

/// The highlights among `candidates`, given the keyword answers TDLib has
/// sent (`answer` is `None` while one is on its way): every word that is a
/// whole keyword. A lone word gets none: the composer already offers its
/// emoji above the field.
pub fn pick<'a>(
    candidates: &[Candidate],
    words: usize,
    answer: impl Fn(&str) -> Option<&'a [EmojiKeyword]>,
) -> Vec<Highlight> {
    if words < 2 {
        return Vec::new();
    }
    candidates
        .iter()
        .filter_map(|c| {
            let emoji = keyword_emoji(&c.normalized, true, answer(&c.normalized)?);
            (!emoji.is_empty()).then(|| Highlight {
                range: c.range.clone(),
                normalized: c.normalized.clone(),
                emoji,
            })
        })
        .collect()
}

/// A whitespace-delimited token and whether it is a tag, mention,
/// command, `:query`, link, address or path: it holds `#`, `@`, `/`, `:`,
/// `_` or a `.` between two letters or digits.
struct Token {
    range: Range<usize>,
    marked: bool,
}

fn tokens(text: &str) -> Vec<Token> {
    let mut out = Vec::new();
    let mut start: Option<usize> = None;
    let mut marked = false;
    let mut last = ['\0'; 2];
    for (at, c) in text.char_indices() {
        if c.is_whitespace() {
            if let Some(s) = start.take() {
                out.push(Token {
                    range: s..at,
                    marked,
                });
            }
            continue;
        }
        if start.is_none() {
            start = Some(at);
            marked = false;
            last = ['\0'; 2];
        }
        marked |= matches!(c, '#' | '@' | '/' | ':' | '_')
            || (last[1] == '.' && last[0].is_alphanumeric() && c.is_alphanumeric());
        last = [last[1], c];
    }
    if let Some(s) = start {
        out.push(Token {
            range: s..text.len(),
            marked,
        });
    }
    out
}

/// Combining marks (an accent typed or pasted decomposed, "cafe\u{301}")
/// belong to the letter before them.
fn is_mark(c: char) -> bool {
    matches!(
        c,
        '\u{300}'..='\u{36F}'
            | '\u{1AB0}'..='\u{1AFF}'
            | '\u{1DC0}'..='\u{1DFF}'
            | '\u{20D0}'..='\u{20FF}'
            | '\u{FE20}'..='\u{FE2F}'
    )
}

/// Byte ranges of the words: runs of letters and digits (with their
/// combining marks), with `'` or `-` between two letters kept inside.
fn word_ranges(text: &str) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    let mut start: Option<usize> = None;
    let mut chars = text.char_indices().peekable();
    while let Some((at, c)) = chars.next() {
        let next_letter = chars.peek().is_some_and(|(_, n)| n.is_alphabetic());
        let inner = (c == '\'' || c == '-' || c == '\u{2019}') && start.is_some() && next_letter;
        if c.is_alphanumeric() || inner || (is_mark(c) && start.is_some()) {
            start.get_or_insert(at);
        } else if let Some(s) = start.take() {
            out.push(s..at);
        }
    }
    if let Some(s) = start {
        out.push(s..text.len());
    }
    out
}

fn floor_boundary(text: &str, at: usize) -> usize {
    let mut at = at.min(text.len());
    while !text.is_char_boundary(at) {
        at -= 1;
    }
    at
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::string_slice,
        clippy::single_range_in_vec_init,
        reason = "test code slices known-good literals"
    )]
    use super::*;

    fn kw(emoji: &str, keyword: &str) -> EmojiKeyword {
        EmojiKeyword {
            emoji: emoji.into(),
            keyword: keyword.into(),
        }
    }

    fn words(text: &str, caret: usize) -> Vec<&str> {
        candidates(text, caret..caret, &[])
            .0
            .iter()
            .map(|c| &text[c.range.clone()])
            .collect()
    }

    #[test]
    fn only_finished_plain_words_are_candidates() {
        // The word at the caret is still being typed.
        assert_eq!(words("eat pizza", 9), ["eat"]);
        assert_eq!(words("eat pizza ", 10), ["eat", "pizza"]);
        // Tags, mentions, commands, `:queries`, paths and most one- and
        // two-letter words are not; "ok" and common words are.
        assert_eq!(
            words(
                "I go to the #party with @friend /start a:b x.y/z ok 100",
                usize::MAX
            ),
            ["the", "with", "ok", "100"]
        );
        assert_eq!(
            words("don't t-rex rock'n", usize::MAX),
            ["don't", "t-rex", "rock'n"]
        );
        assert_eq!(words("Ünïcode Привет", usize::MAX), ["Ünïcode", "Привет"]);
        // A decomposed accent stays with its word.
        assert_eq!(
            words("cafe\u{301} time", usize::MAX),
            ["cafe\u{301}", "time"]
        );
        // A selection covers every word it touches, not only its end.
        let (found, _) = candidates("pizza and cake ", 0..9, &[]);
        let picked: Vec<&str> = found.iter().map(|c| c.normalized.as_str()).collect();
        assert_eq!(picked, ["cake"]);
    }

    #[test]
    fn a_long_draft_looks_only_near_the_caret() {
        let text = "pizza ".repeat(200);
        let caret = text.len();
        let (found, count) = candidates(&text, caret..caret, &[]);
        assert_eq!(count, 200);
        assert_eq!(found.len(), MAX_CANDIDATES);
        assert!(
            found
                .iter()
                .all(|c| c.range.start >= caret - 6 * MAX_CANDIDATES)
        );
    }

    #[test]
    fn skipped_spans_hold_no_candidates() {
        let text = "see `pizza` here at pizza.com";
        let (found, count) = candidates(text, 0..0, &[4..11]);
        assert!(found.iter().all(|c| &text[c.range.clone()] != "pizza"));
        assert_eq!(count, 6);
    }

    #[test]
    fn every_word_with_an_emoji_lights_up() {
        let pizza = [kw("🍕", "pizza")];
        let ok = [kw("🆗", "ok"), kw("👌", "ok")];
        let funny: Vec<EmojiKeyword> = [
            "😂", "🤣", "😆", "😹", "🙃", "😝", "🤪", "😜", "😛", "😁", "😄", "🤡",
        ]
        .iter()
        .map(|e| kw(e, "funny"))
        .collect();
        // TDLib's prefix search: "pizzeria" is no "pizz" keyword.
        let lookup = |q: &str| match q {
            "pizza" => Some(&pizza[..]),
            "ok" => Some(&ok[..]),
            "funny" => Some(funny.as_slice()),
            _ => Some(&[][..]),
        };
        let text = "ok that pizza was funny ";
        let (found, count) = candidates(text, text.len()..text.len(), &[]);
        let lit: Vec<&str> = pick(&found, count, lookup)
            .iter()
            .map(|h| &text[h.range.clone()])
            .collect();
        assert_eq!(lit, ["ok", "pizza", "funny"]);
        // A word twice lights twice.
        let text = "pizza or pizza ";
        let (found, count) = candidates(text, text.len()..text.len(), &[]);
        assert_eq!(pick(&found, count, lookup).len(), 2);
    }

    #[test]
    fn a_lone_word_or_a_pending_answer_shows_nothing() {
        let one = [kw("🍕", "pizza")];
        let (found, count) = candidates("pizza ", 6..6, &[]);
        assert!(pick(&found, count, |_| Some(&one[..])).is_empty());
        let (found, count) = candidates("hot pizza ", 10..10, &[]);
        assert!(pick(&found, count, |_| None).is_empty());
        assert_eq!(pick(&found, count, |_| Some(&one[..])).len(), 1);
    }

    #[test]
    fn a_huge_draft_is_scanned_only_at_its_head() {
        let text = "é".repeat(SCAN_BYTES) + " pizza";
        let (found, _) = candidates(&text, 0..0, &[]);
        assert!(found.iter().all(|c| c.normalized != "pizza"));
    }
}
