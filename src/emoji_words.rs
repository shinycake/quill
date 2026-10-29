//! Words in the composer that have an emoji, shown with a soft highlight;
//! hovering one offers the same strip as typing `:word`.
//!
//! Telegram Desktop has no such feature. The closest one is iOS Messages'
//! emoji replacement (iOS 10): with the emoji keyboard open, words that
//! have an emoji turn orange and a tap offers them. That lights up every
//! match at once and only on request; here the highlight is always there
//! and has to stay rare, so a word must earn it (docs/decisions/
//! emoji-word-highlights.md):
//!
//! - it is finished: the caret or the selection is not touching it;
//! - it is a plain word of [`MIN_WORD_CHARS`] letters or more, outside
//!   code, links and mentions, not a [`COMMON_WORDS`] word;
//! - it is a whole emoji keyword with at most [`MAX_EMOJI_PER_WORD`]
//!   emoji: "pizza" has one, "face" has dozens and says nothing;
//! - one word in [`WORDS_PER_HIGHLIGHT`] (at least one, never more than
//!   [`MAX_HIGHLIGHTS`]), words lit before keeping their place, then the
//!   most specific words.
//!
//! The lookups are TDLib's on-device keyword search, so nothing typed
//! leaves the device. Pure, so it is unit tested.
#![deny(clippy::indexing_slicing)]

use std::ops::Range;

use crate::emoji::keyword_lookup::{keyword_emoji, normalize_query};
use crate::telegram::envelope_emoji::EmojiKeyword;

/// Shortest word that may be highlighted.
pub const MIN_WORD_CHARS: usize = 3;
/// A keyword with more emoji than this is too vague to offer.
pub const MAX_EMOJI_PER_WORD: usize = 8;
/// At most one highlight for this many words of the message.
pub const WORDS_PER_HIGHLIGHT: usize = 6;
/// Highlights in one message, however long.
pub const MAX_HIGHLIGHTS: usize = 3;
/// Only this much of a long draft is looked at.
pub const SCAN_BYTES: usize = 4096;

/// Words that are emoji keywords but almost never meant as one: articles,
/// pronouns, helpers and the vaguest keywords.
pub const COMMON_WORDS: &[&str] = &[
    "about", "after", "again", "all", "also", "and", "any", "are", "back", "been", "before", "but",
    "can", "come", "could", "day", "did", "does", "done", "down", "end", "even", "for", "from",
    "get", "give", "going", "good", "got", "had", "has", "have", "her", "here", "him", "his",
    "how", "into", "its", "just", "know", "last", "let", "like", "look", "made", "make", "man",
    "many", "mark", "more", "most", "much", "need", "new", "next", "not", "now", "off", "one",
    "only", "other", "our", "out", "over", "person", "put", "really", "right", "said", "same",
    "say", "see", "she", "sign", "some", "soon", "still", "symbol", "take", "than", "that", "the",
    "their", "them", "then", "there", "they", "thing", "think", "this", "time", "too", "top",
    "two", "use", "very", "want", "was", "way", "well", "were", "what", "when", "where", "which",
    "who", "why", "will", "with", "woman", "would", "yes", "you", "your",
];

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
/// letters only (an inner `'` or `-` is kept, "don't", "t-rex"), at
/// least [`MIN_WORD_CHARS`], not touching `selection`, not overlapping
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
            || word.chars().filter(|c| !is_mark(*c)).count() < MIN_WORD_CHARS
            || !word
                .chars()
                .all(|c| c.is_alphabetic() || is_mark(c) || c == '\'' || c == '-')
            || COMMON_WORDS.contains(&normalized.as_str())
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
/// sent (`answer` is `None` while one is on its way). A lone word gets
/// none: the composer already offers its emoji above the field. Words
/// that were lit before (`was_lit`) keep their place, so finishing a
/// more specific word does not move a highlight the user has seen.
pub fn pick<'a>(
    candidates: &[Candidate],
    words: usize,
    was_lit: impl Fn(&str) -> bool,
    answer: impl Fn(&str) -> Option<&'a [EmojiKeyword]>,
) -> Vec<Highlight> {
    if words < 2 {
        return Vec::new();
    }
    let budget = (words / WORDS_PER_HIGHLIGHT).clamp(1, MAX_HIGHLIGHTS);
    let mut scored: Vec<(bool, Highlight)> = candidates
        .iter()
        .filter_map(|c| {
            let emoji = keyword_emoji(&c.normalized, true, answer(&c.normalized)?);
            (!emoji.is_empty() && emoji.len() <= MAX_EMOJI_PER_WORD).then(|| {
                (
                    !was_lit(&c.normalized),
                    Highlight {
                        range: c.range.clone(),
                        normalized: c.normalized.clone(),
                        emoji,
                    },
                )
            })
        })
        .collect();
    // Lit before first; then fewest emoji, the most specific word;
    // earlier wins a tie.
    scored.sort_by_key(|(new, h)| (*new, h.emoji.len(), h.range.start));
    let mut chosen: Vec<Highlight> = Vec::new();
    for (_, h) in scored {
        if chosen.len() == budget {
            break;
        }
        // One word of the same text is enough ("pizza ... pizza").
        if chosen.iter().all(|c| c.normalized != h.normalized) {
            chosen.push(h);
        }
    }
    chosen.sort_by_key(|h| h.range.start);
    chosen
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
        // Short and common words, digits, tags, mentions, commands, paths.
        assert_eq!(
            words("I go to the #party with @friend /start a:b 3pm x ok", 0),
            Vec::<&str>::new()
        );
        assert_eq!(
            words("don't t-rex rock'n", usize::MAX),
            ["don't", "t-rex", "rock'n"]
        );
        assert_eq!(words("Ünïcode Привет", usize::MAX), ["Ünïcode", "Привет"]);
        // A decomposed accent stays with its word.
        assert_eq!(words("cafe\u{301} time", usize::MAX), ["cafe\u{301}"]);
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
    fn a_lit_word_keeps_its_place() {
        let pizza = [kw("🍕", "pizza")];
        let cake = [kw("🍰", "cake"), kw("🎂", "cake")];
        let lookup = |q: &str| match q {
            "pizza" => Some(&pizza[..]),
            "cake" => Some(&cake[..]),
            _ => Some(&[][..]),
        };
        let text = "we had some cake and then the pizza ";
        let (found, count) = candidates(text, text.len()..text.len(), &[]);
        let fresh = pick(&found, count, |_| false, lookup);
        assert_eq!(fresh[0].normalized, "pizza");
        let kept = pick(&found, count, |w| w == "cake", lookup);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].normalized, "cake");
    }

    #[test]
    fn skipped_spans_hold_no_candidates() {
        let text = "see `pizza` here at pizza.com";
        let (found, count) = candidates(text, 0..0, &[4..11]);
        assert!(found.iter().all(|c| &text[c.range.clone()] != "pizza"));
        assert_eq!(count, 6);
    }

    #[test]
    fn specific_words_win_and_density_is_capped() {
        let text = "we ordered pizza and some cake at the party tonight friends ";
        let (found, count) = candidates(text, text.len()..text.len(), &[]);
        let answers = |q: &str| -> Option<Vec<EmojiKeyword>> {
            match q {
                "pizza" => Some(vec![kw("🍕", "pizza")]),
                "cake" => Some(vec![kw("🍰", "cake"), kw("🎂", "cake")]),
                "party" => Some(vec![
                    kw("🎉", "party"),
                    kw("🥳", "party"),
                    kw("🎊", "party"),
                ]),
                "tonight" => Some(
                    (0..20)
                        .map(|n| kw(&char::from_u32(0x1F300 + n).unwrap().to_string(), "tonight"))
                        .collect(),
                ),
                _ => Some(Vec::new()),
            }
        };
        let store: Vec<(String, Vec<EmojiKeyword>)> = found
            .iter()
            .map(|c| (c.normalized.clone(), answers(&c.normalized).unwrap()))
            .collect();
        let lookup = |q: &str| {
            store
                .iter()
                .find(|(k, _)| k == q)
                .map(|(_, v)| v.as_slice())
        };
        let picked: Vec<&str> = pick(&found, count, |_| false, lookup)
            .iter()
            .map(|h| &text[h.range.clone()])
            .collect();
        // 11 words: one highlight for every six, the most specific.
        assert_eq!(picked, ["pizza"]);
        // "tonight" has too many emoji to ever show.
        let long = "pizza cake party tonight ".repeat(10);
        let (found, count) = candidates(&long, long.len()..long.len(), &[]);
        let picked: Vec<String> = pick(&found, count, |_| false, lookup)
            .into_iter()
            .map(|h| h.normalized)
            .collect();
        assert_eq!(picked, ["pizza", "cake", "party"]);
    }

    #[test]
    fn a_lone_word_or_a_pending_answer_shows_nothing() {
        let one = [kw("🍕", "pizza")];
        let (found, count) = candidates("pizza ", 6..6, &[]);
        assert!(pick(&found, count, |_| false, |_| Some(&one[..])).is_empty());
        let (found, count) = candidates("hot pizza ", 10..10, &[]);
        assert!(pick(&found, count, |_| false, |_| None).is_empty());
        assert_eq!(pick(&found, count, |_| false, |_| Some(&one[..])).len(), 1);
    }

    #[test]
    fn a_huge_draft_is_scanned_only_at_its_head() {
        let text = "é".repeat(SCAN_BYTES) + " pizza";
        let (found, _) = candidates(&text, 0..0, &[]);
        assert!(found.iter().all(|c| c.normalized != "pizza"));
    }
}
