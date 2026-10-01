//! Spellcheck engine for the message composer (parity:platform-spellcheck).
//!
//! Pure Rust, no system dependencies: an embedded frequency-ordered
//! English wordlist (~48k words, `assets/spellcheck/en.txt`) backs
//! [`SpellChecker::is_correct`], and [`SpellChecker::suggestions`]
//! generates Damerau-Levenshtein edits at distance 1 (falling back to
//! distance 2) filtered through the wordlist, ranked by word frequency.
//!
//! Only ASCII-alphabetic tokens are checked — anything containing
//! non-ASCII letters is skipped outright, so Hebrew/Arabic/CJK text is
//! never flagged (English-only dictionaries; more languages are out of
//! this slice). URLs, @mentions, #hashtags, tokens with digits, and
//! ALL-CAPS acronyms are skipped too.
//!
//! The per-keystroke path is [`SpellChecker::check_words`]: tokenize +
//! hash lookups only, no suggestion generation (microseconds on a
//! 4k-char draft). Suggestions are computed lazily when the corrections
//! dialog renders.

use std::collections::{HashMap, HashSet};

/// Embedded English wordlist, one word per line, most frequent first.
const WORDLIST: &str = include_str!("../assets/spellcheck/en.txt");

/// Cap on misspellings returned per check and counted by the badge.
/// The corrections panel renders a smaller subset of this list.
pub const MAX_MISSPELLINGS: usize = 32;

/// A misspelled token: byte range into the checked text plus the word
/// as typed (for the "add to dictionary" flow).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Misspelling {
    pub word: String,
    pub start: usize,
    pub end: usize,
}

pub struct SpellChecker {
    /// Frequency rank (0 = most common) for suggestion ranking.
    rank_of: HashMap<&'static str, u32>,
    words: HashSet<&'static str>,
    /// Words the user added ("Add to dictionary"), persisted by the caller.
    custom: HashSet<String>,
    /// Session-only ignores, cleared when the draft is sent or the chat changes.
    ignored: HashSet<String>,
}

impl SpellChecker {
    pub fn new() -> Self {
        let mut rank_of = HashMap::new();
        let mut words = HashSet::new();
        for (rank, line) in WORDLIST.lines().enumerate() {
            let w = line.trim();
            if w.is_empty() {
                continue;
            }
            rank_of.insert(w, rank as u32);
            words.insert(w);
        }
        Self {
            rank_of,
            words,
            custom: HashSet::new(),
            ignored: HashSet::new(),
        }
    }

    /// Words known to the checker (dictionary size), for tests/diagnostics.
    pub fn dictionary_size(&self) -> usize {
        self.words.len()
    }

    pub fn set_custom_words(&mut self, words: impl IntoIterator<Item = String>) {
        self.custom = words.into_iter().collect();
    }

    /// Returns true when the word was newly added.
    pub fn add_custom_word(&mut self, word: &str) -> bool {
        self.custom.insert(normalize(word))
    }

    pub fn custom_words(&self) -> Vec<String> {
        let mut out: Vec<String> = self.custom.iter().cloned().collect();
        out.sort();
        out
    }

    /// Session ignore ("Ignore" in the dialog) — not persisted.
    pub fn ignore_word(&mut self, word: &str) {
        self.ignored.insert(normalize(word));
    }

    pub fn clear_ignored(&mut self) {
        self.ignored.clear();
    }

    /// True when the word needs no correction. Anything that isn't an
    /// ASCII word (other scripts, digits, URLs…) is "correct" — never
    /// flagged.
    pub fn is_correct(&self, word: &str) -> bool {
        // ALL-CAPS acronyms (NASA, FYI) are checked before lowercasing.
        if is_acronym(word) {
            return true;
        }
        let norm = normalize(word);
        if norm.is_empty() || !is_checkable(&norm) {
            return true;
        }
        self.words.contains(norm.as_str())
            || self.custom.contains(&norm)
            || self.ignored.contains(&norm)
    }

    /// Up to `limit` suggestions, closest first (edit distance, then
    /// frequency). Empty when the word is already correct.
    pub fn suggestions(&self, word: &str, limit: usize) -> Vec<String> {
        let norm = normalize(word);
        if norm.is_empty() || self.is_correct(&norm) || limit == 0 {
            return Vec::new();
        }
        // Distance-1 edits that are real words.
        let mut seen = HashSet::new();
        let mut scored: Vec<(u32, u32, String)> = Vec::new();
        for cand in edits1(&norm) {
            if !seen.insert(cand.clone()) {
                continue;
            }
            if let Some(&rank) = self.rank_of.get(cand.as_str()) {
                scored.push((1, rank, cand));
            }
        }
        // Fall back to distance 2 when distance 1 found nothing useful.
        // Hard iteration cap: gibberish words would otherwise scan
        // ~300k candidates (~50ms) with nothing to show for it.
        if scored.len() < limit {
            let ed1: Vec<String> = seen.into_iter().collect();
            let mut seen2 = HashSet::new();
            let mut scanned = 0usize;
            'outer: for mid in &ed1 {
                for cand in edits1(mid) {
                    scanned += 1;
                    if scanned > 60_000 {
                        break 'outer;
                    }
                    if !seen2.insert(cand.clone()) {
                        continue;
                    }
                    if let Some(&rank) = self.rank_of.get(cand.as_str()) {
                        scored.push((2, rank, cand));
                        if scored.len() >= limit * 4 {
                            break 'outer;
                        }
                    }
                }
            }
        }
        scored.sort();
        scored.into_iter().take(limit).map(|(_, _, w)| w).collect()
    }

    /// Cheap per-keystroke check: misspelled tokens with byte ranges,
    /// no suggestions. Capped at [`MAX_MISSPELLINGS`].
    pub fn check_words(&self, text: &str) -> Vec<Misspelling> {
        let mut out = Vec::new();
        for (word, start, end) in tokenize(text) {
            if out.len() >= MAX_MISSPELLINGS {
                break;
            }
            if !self.is_correct(word) {
                out.push(Misspelling {
                    word: word.to_string(),
                    start,
                    end,
                });
            }
        }
        out
    }
}

impl Default for SpellChecker {
    fn default() -> Self {
        Self::new()
    }
}

/// Lowercase an already-trimmed token.
fn normalize(word: &str) -> String {
    word.to_lowercase()
}

/// Match a correction to the original token's letter case.
pub fn match_capitalization(original: &str, suggestion: &str) -> String {
    let mut letters = original.chars().filter(|c| c.is_alphabetic()).peekable();
    if letters.peek().is_some() && letters.clone().all(char::is_uppercase) {
        return suggestion.to_uppercase();
    }
    if letters.next().is_some_and(char::is_uppercase)
        && let Some((start, first)) = suggestion.char_indices().find(|(_, c)| c.is_alphabetic())
    {
        let mut result = suggestion[..start].to_string();
        result.extend(first.to_uppercase());
        result.push_str(&suggestion[start + first.len_utf8()..]);
        return result;
    }
    suggestion.to_string()
}

/// ALL-CAPS tokens with 2+ letters are acronyms (NASA, FYI) — never
/// flagged. Checked before lowercasing.
fn is_acronym(word: &str) -> bool {
    let mut letters = 0;
    for c in word.chars() {
        if c.is_ascii_alphabetic() {
            letters += 1;
            if c.is_ascii_lowercase() {
                return false;
            }
        }
    }
    letters > 1
}

/// Only plain ASCII words get checked: other scripts are skipped
/// (never flagged), as are tokens with digits and anything that isn't
/// a letter, `'` or `-` (URLs, @mentions, #hashtags stay whole through
/// the tokenizer and fail here).
fn is_checkable(word: &str) -> bool {
    // ponytail: skip tokens over 64 bytes to bound quadratic edit generation.
    if word.is_empty() || word.len() > 64 {
        return false;
    }
    for c in word.chars() {
        if c.is_ascii_digit() {
            return false;
        }
        if !(c.is_ascii_alphabetic() || c == '\'' || c == '-') {
            return false;
        }
    }
    // Single letters other than "a"/"i" are initials ("J") — skip.
    // ("a" and "i" are in the dictionary anyway.)
    if word.len() == 1 {
        return word == "a" || word == "i";
    }
    true
}

/// Split text into (word, byte_start, byte_end). A word is a maximal
/// run of ASCII letters plus intra-word `'`/`-`; `@mention` and
/// `#hashtag` stay whole so `is_checkable` skips them, and URL spans
/// (`scheme://…`, `www.…`) are skipped entirely.
fn tokenize(text: &str) -> Vec<(&str, usize, usize)> {
    let mut out = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < text.len() {
        let c = bytes[i] as char;
        let word_char = token_byte(bytes[i]);
        // Keep @/# prefixes attached so mentions/hashtags are skipped.
        let prefix = (c == '@' || c == '#')
            && i + 1 < text.len()
            && (bytes[i + 1] as char).is_ascii_alphabetic();
        if word_char || prefix {
            let start = i;
            i += 1;
            while i < text.len() {
                if token_byte(bytes[i]) {
                    i += 1;
                } else {
                    break;
                }
            }
            let word = &text[start..i];
            // URL scheme ("https://…") or www. host — skip the whole URL
            // to the next whitespace; the scheme/host words must never
            // be spellchecked.
            let rest = &text[i..];
            if rest.starts_with("://")
                || (word.eq_ignore_ascii_case("www") && rest.starts_with('.'))
            {
                while i < text.len() && !(bytes[i] as char).is_whitespace() {
                    i += 1;
                }
                continue;
            }
            let trimmed = word.trim_matches(|c| c == '\'' || c == '-');
            if !trimmed.is_empty() {
                let offset = word.find(trimmed).unwrap_or(0);
                out.push((trimmed, start + offset, start + offset + trimmed.len()));
            }
        } else {
            i += 1;
        }
    }
    out
}

// Consume whole mixed-script/digit/username tokens so their ASCII suffix is not flagged.
fn token_byte(byte: u8) -> bool {
    byte >= 128 || byte.is_ascii_alphanumeric() || matches!(byte, b'\'' | b'-' | b'_')
}

/// All Damerau-Levenshtein edits at distance 1 (deletes, transposes,
/// replaces, inserts over a-z plus apostrophe and hyphen).
fn edits1(word: &str) -> Vec<String> {
    let chars: Vec<char> = word.chars().collect();
    let n = chars.len();
    let mut out = Vec::with_capacity(58 * n + 28);
    // Deletes.
    for i in 0..n {
        let mut s = String::with_capacity(n);
        s.extend(chars[..i].iter());
        s.extend(chars[i + 1..].iter());
        out.push(s);
    }
    // Transposes.
    for i in 0..n.saturating_sub(1) {
        let mut t = chars.clone();
        t.swap(i, i + 1);
        out.push(t.into_iter().collect());
    }
    // Replaces and inserts.
    for i in 0..=n {
        for c in ('a'..='z').chain(['\'', '-']) {
            if i < n {
                let mut s = String::with_capacity(n);
                s.extend(chars[..i].iter());
                s.push(c);
                s.extend(chars[i + 1..].iter());
                out.push(s);
            }
            let mut s = String::with_capacity(n + 1);
            s.extend(chars[..i].iter());
            s.push(c);
            s.extend(chars[i..].iter());
            out.push(s);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn checker() -> SpellChecker {
        SpellChecker::new()
    }

    #[test]
    fn dictionary_loads() {
        let sc = checker();
        // ~48k words from the embedded frequency list.
        assert!(
            sc.dictionary_size() > 40_000,
            "size={}",
            sc.dictionary_size()
        );
        assert!(sc.is_correct("hello"));
        assert!(sc.is_correct("Hello"));
        assert!(sc.is_correct("WORLD"));
    }

    #[test]
    fn flags_typos_not_valid_words() {
        let sc = checker();
        assert!(!sc.is_correct("teh"));
        assert!(!sc.is_correct("speling"));
        assert!(!sc.is_correct("recieve"));
        // Valid words (even rare ones in the list) are not flagged.
        assert!(sc.is_correct("quill"));
        assert!(sc.is_correct("telegram"));
    }

    #[test]
    fn skips_non_english_and_noise() {
        let sc = checker();
        // Other scripts are never flagged (English-only dictionaries).
        assert!(sc.is_correct("שלום"));
        assert!(sc.is_correct("привет"));
        assert!(sc.is_correct("日本語"));
        // Noise is never flagged either.
        assert!(sc.is_correct("hello123"));
        assert!(sc.is_correct("NASA"));
        assert!(sc.is_correct("FYI"));
        assert!(sc.is_correct("@username"));
        assert!(sc.is_correct("#hashtag"));
    }

    #[test]
    fn suggestions_rank_the_obvious_first() {
        let sc = checker();
        let s = sc.suggestions("teh", 5);
        assert_eq!(s.first().map(String::as_str), Some("the"));
        let s = sc.suggestions("speling", 5);
        assert!(s.contains(&"spelling".to_string()), "got {s:?}");
        // Already-correct words get no suggestions.
        assert!(sc.suggestions("hello", 5).is_empty());
    }

    #[test]
    fn suggestions_reach_hyphenated_dictionary_words() {
        let sc = checker();
        assert!(edits1("wellknown").contains(&"well-known".to_string()));
        assert!(edits1("wellxknown").contains(&"well-known".to_string()));
        assert!(edits1("canxt").contains(&"can't".to_string()));
        assert!(edits1("cant").contains(&"can't".to_string()));
        let suggestions = sc.suggestions("wellknown", 5);
        assert!(
            suggestions.contains(&"well-known".to_string()),
            "got {suggestions:?}"
        );
    }

    #[test]
    fn corrections_match_capitalization() {
        assert_eq!(match_capitalization("Teh", "the"), "The");
        assert_eq!(match_capitalization("THE", "the"), "THE");
        assert_eq!(match_capitalization("teh", "the"), "the");
        assert_eq!(match_capitalization("'Teh", "'the"), "'The");
        assert_eq!(
            match_capitalization("WELLKNOWN", "well-known"),
            "WELL-KNOWN"
        );
        assert_eq!(match_capitalization("---", "the"), "the");
    }

    #[test]
    fn check_words_finds_byte_ranges() {
        let sc = checker();
        let miss = sc.check_words("Hello teh world, this is a speling test");
        let words: Vec<&str> = miss.iter().map(|m| m.word.as_str()).collect();
        assert_eq!(words, vec!["teh", "speling"]);
        assert_eq!(
            &"Hello teh world, this is a speling test"[miss[0].start..miss[0].end],
            "teh"
        );
        assert_eq!(
            &"Hello teh world, this is a speling test"[miss[1].start..miss[1].end],
            "speling"
        );
        // Mentions, URLs and digits don't produce misspellings.
        let miss = sc.check_words("hey @tehuser see https://example.com/teh 123");
        assert!(miss.is_empty(), "got {miss:?}");
        assert!(
            sc.check_words("teh123 @teh_user #teh_tag café caféspeling 日本teh")
                .is_empty()
        );
        assert!(sc.suggestions(&"z".repeat(4096), 5).is_empty());
    }

    #[test]
    fn custom_words_and_ignores() {
        let mut sc = checker();
        assert!(!sc.is_correct("cryptocurrency"));
        assert!(sc.add_custom_word("cryptocurrency"));
        assert!(!sc.add_custom_word("cryptocurrency"));
        assert!(sc.is_correct("cryptocurrency"));
        assert!(!sc.is_correct("blorpt"));
        sc.ignore_word("blorpt");
        assert!(sc.is_correct("blorpt"));
        sc.clear_ignored();
        assert!(!sc.is_correct("blorpt"));
        // Custom words still work after the ignore list is cleared.
        assert!(sc.is_correct("cryptocurrency"));
    }
}
