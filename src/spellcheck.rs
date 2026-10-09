//! Spellcheck engine for the message composer (codex:spellcheck-native).
//!
//! Mirrors Telegram Desktop's `lib_spellcheck` split:
//!
//! - **Segmentation + skip rules** (pure, here): Unicode word boundaries
//!   (UAX #29, so `don't` stays one word), minus everything tdesktop never
//!   checks — links, `@mentions`, `#hashtags`, `$cashtags`, `/commands`,
//!   e-mail addresses, bare domains, `` `code` `` / ```` ```pre``` ````
//!   spans, words with digits or `_`, mixed-script words, words longer
//!   than [`MAX_WORD_CHARS`], and (Quill extra) ALL-CAPS acronyms.
//! - **A backend** ([`SpellBackend`]) that judges single words. On macOS
//!   the UI installs the system `NSSpellChecker` (dictionaries, languages
//!   and "Learn Spelling" shared with every other Mac app — exactly what
//!   tdesktop does there). Elsewhere [`WordlistBackend`] keeps the old
//!   embedded English list (~48k words) with Damerau-Levenshtein
//!   suggestions.
//! - [`SpellChecker`] glues the two together with a per-word result cache
//!   (re-checking a long draft only asks the backend about new words),
//!   session ignores and app-level learned words. It is `Send + Sync`, so
//!   the UI runs whole-draft checks on a background thread.

use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::sync::{Arc, Mutex, RwLock};

use unicode_segmentation::UnicodeSegmentation;

/// Embedded English wordlist, one word per line, most frequent first.
const WORDLIST: &str = include_str!("../assets/spellcheck/en.txt");

/// tdesktop `kMaxWordSize`: longer tokens are never checked.
pub const MAX_WORD_CHARS: usize = 99;
/// tdesktop `kMaxSuggestions`.
pub const MAX_SUGGESTIONS: usize = 5;
/// Hard cap on misspellings reported for one draft.
pub const MAX_MISSPELLINGS: usize = 256;
/// The per-word cache is dropped wholesale past this many entries.
const CACHE_LIMIT: usize = 20_000;

/// A misspelled word: byte range into the checked text plus the word.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Misspelling {
    pub word: String,
    pub start: usize,
    pub end: usize,
}

impl Misspelling {
    pub fn range(&self) -> Range<usize> {
        self.start..self.end
    }
}

/// Writing systems the skip rules and the language → script mapping
/// distinguish (a small subset of Unicode scripts; anything else is
/// [`Script::Other`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Script {
    Latin,
    Greek,
    Cyrillic,
    Armenian,
    Hebrew,
    Arabic,
    Devanagari,
    Bengali,
    Thai,
    Georgian,
    Hangul,
    /// Han ideographs: no spaces between words, never checked.
    Han,
    /// Hiragana / Katakana: never checked (tdesktop skips Japanese).
    Kana,
    Other,
}

impl Script {
    /// Scripts tdesktop never spellchecks (no word separators).
    pub fn is_checkable(self) -> bool {
        !matches!(self, Script::Han | Script::Kana | Script::Thai)
    }
}

/// Script of a letter; `None` for anything that isn't a letter (digits,
/// punctuation, emoji) and for combining marks, which inherit the script
/// of their base letter.
pub fn char_script(c: char) -> Option<Script> {
    if !c.is_alphabetic() {
        return None;
    }
    let u = c as u32;
    Some(match u {
        0x0041..=0x024F | 0x1E00..=0x1EFF | 0x2C60..=0x2C7F | 0xA720..=0xA7FF | 0xFF21..=0xFF5A => {
            Script::Latin
        }
        0x0300..=0x036F => return None,
        0x0370..=0x03FF | 0x1F00..=0x1FFF => Script::Greek,
        0x0400..=0x052F | 0x2DE0..=0x2DFF | 0xA640..=0xA69F | 0x1C80..=0x1C8F => Script::Cyrillic,
        0x0530..=0x058F => Script::Armenian,
        0x0590..=0x05FF | 0xFB1D..=0xFB4F => Script::Hebrew,
        0x0600..=0x06FF | 0x0750..=0x077F | 0x08A0..=0x08FF | 0xFB50..=0xFDFF | 0xFE70..=0xFEFF => {
            Script::Arabic
        }
        0x0900..=0x097F => Script::Devanagari,
        0x0980..=0x09FF => Script::Bengali,
        0x0E00..=0x0E7F => Script::Thai,
        0x10A0..=0x10FF | 0x2D00..=0x2D2F => Script::Georgian,
        0x1100..=0x11FF | 0x3130..=0x318F | 0xAC00..=0xD7AF => Script::Hangul,
        0x3040..=0x30FF | 0x31F0..=0x31FF | 0xFF66..=0xFF9F => Script::Kana,
        0x2E80..=0x2FDF
        | 0x3400..=0x4DBF
        | 0x4E00..=0x9FFF
        | 0xF900..=0xFAFF
        | 0x20000..=0x3FFFF => Script::Han,
        _ => Script::Other,
    })
}

/// The single script of a word, or `None` when it has no letters or mixes
/// scripts (tdesktop `IsWordSkippable`: a word whose letters are not all
/// in the first letter's script is skipped).
pub fn word_script(word: &str) -> Option<Script> {
    let mut script = None;
    for c in word.chars() {
        if let Some(s) = char_script(c) {
            match script {
                None => script = Some(s),
                Some(prev) if prev != s => return None,
                Some(_) => {}
            }
        }
    }
    script
}

/// Script for a locale identifier (`en`, `en_US`, `pt-BR`), from the
/// language subtag — the part of tdesktop's `kLocaleScriptList` covering
/// the languages macOS ships spelling dictionaries for.
pub fn locale_script(locale: &str) -> Option<Script> {
    let subtag = locale
        .split(['_', '-'])
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    Some(match subtag.as_str() {
        "af" | "ca" | "cs" | "cy" | "da" | "de" | "en" | "es" | "et" | "eu" | "fi" | "fil"
        | "fo" | "fr" | "ga" | "gl" | "hr" | "hu" | "id" | "is" | "it" | "lt" | "lv" | "ms"
        | "mt" | "nb" | "nl" | "nn" | "no" | "pl" | "pt" | "ro" | "sk" | "sl" | "sq" | "sv"
        | "sw" | "tl" | "tr" | "vi" | "az" | "bs" | "la" | "lb" => Script::Latin,
        "el" => Script::Greek,
        "ru" | "uk" | "be" | "bg" | "mk" | "sr" | "kk" | "ky" | "mn" | "tg" | "tt" => {
            Script::Cyrillic
        }
        "hy" => Script::Armenian,
        "he" | "iw" | "yi" => Script::Hebrew,
        "ar" | "fa" | "ur" | "ps" => Script::Arabic,
        "hi" | "mr" | "ne" => Script::Devanagari,
        "bn" => Script::Bengali,
        "th" => Script::Thai,
        "ka" => Script::Georgian,
        "ko" => Script::Hangul,
        "zh" => Script::Han,
        "ja" => Script::Kana,
        _ => return None,
    })
}

/// Spelling dictionaries for the user's system languages (tdesktop
/// `SystemLanguages` + `PreferredRegionalVariant`): each preferred
/// language (`en-GB`, `he-IL`, `pt-BR`) maps to the available dictionary
/// with the same region (`en_GB`), else the bare language (`en`), else
/// any regional one (`en_US`). Order kept, duplicates dropped.
pub fn spelling_languages(system: &[String], available: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for lang in system {
        let full = lang.replace('-', "_");
        let mut parts = full.split('_');
        let base = parts.next().unwrap_or("");
        // Drop a script subtag (`zh_Hans_CN` → region `CN`).
        let region = parts.find(|p| p.len() == 2 || p.chars().all(|c| c.is_ascii_digit()));
        let regional = region.map(|r| format!("{base}_{r}"));
        let pick = regional
            .as_ref()
            .and_then(|r| available.iter().find(|a| a.eq_ignore_ascii_case(r)))
            .or_else(|| available.iter().find(|a| a.eq_ignore_ascii_case(base)))
            .or_else(|| {
                available.iter().find(|a| {
                    a.split(['_', '-'])
                        .next()
                        .is_some_and(|b| b.eq_ignore_ascii_case(base))
                })
            });
        if let Some(pick) = pick
            && !out.contains(pick)
        {
            out.push(pick.clone());
        }
    }
    out
}

/// Judges single words. Implementations must be cheap to call from a
/// background thread; [`SpellChecker`] caches their answers.
pub trait SpellBackend: Send + Sync {
    /// Whether this backend can judge words in `script`. Words it can't
    /// judge are never flagged (tdesktop: scripts without an enabled
    /// dictionary are skipped).
    fn handles(&self, script: Script, word: &str) -> bool;
    fn is_correct(&self, word: &str) -> bool;
    /// Up to `limit` replacements, best first.
    fn suggestions(&self, word: &str, limit: usize) -> Vec<String>;
    /// Learn into the backend's own persistent dictionary. `false` when
    /// the backend has none (the caller then keeps the word itself).
    fn learn(&self, _word: &str) -> bool {
        false
    }
    /// Forget a word learned with [`Self::learn`]; `false` when unknown.
    fn unlearn(&self, _word: &str) -> bool {
        false
    }
    fn has_learned(&self, _word: &str) -> bool {
        false
    }
    /// Tell the backend the word is ignored for this app session.
    fn ignore(&self, _word: &str) {}
    /// Load any lazily parsed data now. Called once from a background
    /// thread at startup so the first check or suggestion never parses a
    /// large dictionary on the thread that asked.
    fn warm(&self) {}
}

/// Where "Add to Dictionary" stored the word.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LearnedIn {
    /// The backend's own dictionary (macOS: the system "Learn Spelling"
    /// list, shared with other apps).
    System,
    /// Quill's app word list — the caller persists [`SpellChecker::app_words`].
    App,
}

pub struct SpellChecker {
    backend: Arc<dyn SpellBackend>,
    /// Words the user added and the backend can't store (lowercased).
    app_words: RwLock<HashSet<String>>,
    /// Session ignores ("Ignore"), lowercased. Like tdesktop's
    /// `ignoreWord:inSpellDocumentWithTag:0` they last until quit.
    ignored: RwLock<HashSet<String>>,
    /// Backend answers per exact word (case matters: "Paris" ≠ "paris").
    cache: Mutex<HashMap<String, bool>>,
}

impl SpellChecker {
    pub fn new(backend: Arc<dyn SpellBackend>) -> Self {
        Self {
            backend,
            app_words: RwLock::new(HashSet::new()),
            ignored: RwLock::new(HashSet::new()),
            cache: Mutex::new(HashMap::new()),
        }
    }

    /// The portable fallback: embedded English wordlist.
    pub fn wordlist() -> Self {
        Self::new(Arc::new(WordlistBackend::new()))
    }

    pub fn set_app_words(&self, words: impl IntoIterator<Item = String>) {
        let words = words.into_iter().map(|w| normalize(&w)).collect();
        *write(&self.app_words) = words;
        self.clear_cache();
    }

    /// The app-level learned words, sorted (for persisting).
    pub fn app_words(&self) -> Vec<String> {
        let mut out: Vec<String> = read(&self.app_words).iter().cloned().collect();
        out.sort();
        out
    }

    /// True when the word needs no correction: skipped words (other
    /// scripts, acronyms, digits…) are always "correct".
    pub fn is_correct(&self, word: &str) -> bool {
        let word = normalize_apostrophes(word);
        if is_skippable_word(&word) {
            return true;
        }
        let Some(script) = word_script(&word) else {
            return true;
        };
        if !script.is_checkable() || !self.backend.handles(script, &word) {
            return true;
        }
        let lower = normalize(&word);
        if read(&self.ignored).contains(&lower) || read(&self.app_words).contains(&lower) {
            return true;
        }
        if let Some(&known) = lock(&self.cache).get(&*word) {
            return known;
        }
        let correct = self.backend.is_correct(&word);
        let mut cache = lock(&self.cache);
        if cache.len() >= CACHE_LIMIT {
            cache.clear();
        }
        cache.insert(word.into_owned(), correct);
        correct
    }

    /// Every misspelled word in `text` (capped at [`MAX_MISSPELLINGS`]).
    /// Cost is segmentation plus one backend call per *new* word.
    pub fn check_text(&self, text: &str) -> Vec<Misspelling> {
        let mut out = Vec::new();
        for range in checkable_words(text) {
            if out.len() >= MAX_MISSPELLINGS {
                break;
            }
            let word = &text[range.clone()];
            if !self.is_correct(word) {
                out.push(Misspelling {
                    word: word.to_string(),
                    start: range.start,
                    end: range.end,
                });
            }
        }
        out
    }

    /// Up to `limit` replacements for a misspelled word.
    pub fn suggestions(&self, word: &str, limit: usize) -> Vec<String> {
        if limit == 0 || self.is_correct(word) {
            return Vec::new();
        }
        let word = normalize_apostrophes(word);
        let mut out: Vec<String> = Vec::new();
        for s in self.backend.suggestions(&word, limit) {
            if s != *word && !out.contains(&s) {
                out.push(s);
            }
        }
        out.truncate(limit);
        out
    }

    /// "Add to Dictionary".
    pub fn learn(&self, word: &str) -> LearnedIn {
        let word = normalize_apostrophes(word);
        let learned = if self.backend.learn(&word) {
            LearnedIn::System
        } else {
            write(&self.app_words).insert(normalize(&word));
            LearnedIn::App
        };
        self.clear_cache();
        learned
    }

    /// "Remove from Dictionary". `Some` with where it was removed from.
    pub fn unlearn(&self, word: &str) -> Option<LearnedIn> {
        let word = normalize_apostrophes(word);
        let removed = if write(&self.app_words).remove(&normalize(&word)) {
            Some(LearnedIn::App)
        } else if self.backend.unlearn(&word) {
            Some(LearnedIn::System)
        } else {
            None
        };
        self.clear_cache();
        removed
    }

    /// Whether the user added this word (offers "Remove from Dictionary").
    pub fn is_learned(&self, word: &str) -> bool {
        let word = normalize_apostrophes(word);
        read(&self.app_words).contains(&normalize(&word)) || self.backend.has_learned(&word)
    }

    /// "Ignore": accepted for the rest of the app session.
    pub fn ignore(&self, word: &str) {
        let word = normalize_apostrophes(word);
        write(&self.ignored).insert(normalize(&word));
        self.backend.ignore(&word);
    }

    fn clear_cache(&self) {
        lock(&self.cache).clear();
    }
}

impl Default for SpellChecker {
    fn default() -> Self {
        Self::wordlist()
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}
fn read<T>(m: &RwLock<T>) -> std::sync::RwLockReadGuard<'_, T> {
    m.read().unwrap_or_else(|e| e.into_inner())
}
fn write<T>(m: &RwLock<T>) -> std::sync::RwLockWriteGuard<'_, T> {
    m.write().unwrap_or_else(|e| e.into_inner())
}

/// Lowercase for ignore / app-word comparison.
fn normalize(word: &str) -> String {
    word.to_lowercase()
}

/// tdesktop `NormalizeApostrophes`: U+2019 → `'` so `don’t` is checked as
/// `don't`.
pub fn normalize_apostrophes(word: &str) -> std::borrow::Cow<'_, str> {
    if word.contains('\u{2019}') {
        std::borrow::Cow::Owned(word.replace('\u{2019}', "'"))
    } else {
        std::borrow::Cow::Borrowed(word)
    }
}

/// Word-level skips that don't depend on the surrounding text: digits,
/// underscores, over-long words and ALL-CAPS acronyms (2+ letters).
fn is_skippable_word(word: &str) -> bool {
    if word.chars().count() > MAX_WORD_CHARS {
        return true;
    }
    if word.chars().any(|c| c.is_numeric() || c == '_') {
        return true;
    }
    let mut letters = 0;
    for c in word.chars().filter(|c| c.is_alphabetic()) {
        if !c.is_uppercase() {
            return false;
        }
        letters += 1;
    }
    letters > 1
}

/// Byte ranges of the words in `text` that should be spellchecked:
/// UAX #29 words with at least one letter, outside [`skip_ranges`].
pub fn checkable_words(text: &str) -> Vec<Range<usize>> {
    let skips = skip_ranges(text);
    let mut skip_ix = 0;
    let mut out = Vec::new();
    // UAX #29 joins `end.Start` / `a:b` into one word (MidNumLet /
    // MidLetter); split those so a missing space doesn't hide two words.
    let segments = text
        .split_word_bound_indices()
        .flat_map(|(start, segment)| {
            let mut offset = 0;
            segment.split(['.', ':']).map(move |part| {
                let at = start + offset;
                offset += part.len() + 1;
                (at, part)
            })
        });
    for (start, segment) in segments {
        if !segment.chars().any(char::is_alphabetic) {
            continue;
        }
        // UAX #29 keeps intra-word apostrophes; trim stray edge ones.
        let trimmed = segment.trim_matches(|c| c == '\'' || c == '\u{2019}');
        if trimmed.is_empty() {
            continue;
        }
        let start = start + (segment.len() - segment.trim_start_matches(['\'', '\u{2019}']).len());
        let end = start + trimmed.len();
        while skip_ix < skips.len() && skips[skip_ix].end <= start {
            skip_ix += 1;
        }
        if skips
            .get(skip_ix)
            .is_some_and(|s| s.start < end && start < s.end)
        {
            continue;
        }
        if is_skippable_word(trimmed) {
            continue;
        }
        out.push(start..end);
    }
    out
}

/// The word (as [`checkable_words`] sees it) touching byte `offset`:
/// inside it, or ending exactly at it (the caret just after a word).
pub fn word_at(text: &str, offset: usize) -> Option<Range<usize>> {
    checkable_words(text)
        .into_iter()
        .find(|r| r.start <= offset && offset <= r.end)
}

/// Common TLDs for bare-domain detection (`example.com` is a link in
/// Telegram; `end.Start` is a typo).
const TLDS: &[&str] = &[
    "com", "org", "net", "io", "me", "ru", "de", "uk", "dev", "app", "co", "ai", "il", "gov",
    "edu", "info", "xyz", "ly", "gg", "tv", "fm", "us", "ca", "fr", "it", "es", "nl", "eu", "be",
    "ch", "at", "se", "no", "fi", "dk", "pl", "cz", "ua", "by", "kz", "in", "jp", "cn", "kr", "br",
    "ar", "mx", "au", "nz", "ir", "tr", "gr", "pt", "biz", "to", "sh", "so", "cc", "ws", "site",
    "online", "tech", "store", "blog", "news", "page", "link", "live", "pro", "one",
];

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Byte ranges never spellchecked, sorted and non-overlapping: code
/// spans, links (scheme URLs, `www.`, bare domains, e-mails), and the
/// entities tdesktop parses with `TextParseMentions | TextParseHashtags
/// | TextParseBotCommands` (+ cashtags).
pub fn skip_ranges(text: &str) -> Vec<Range<usize>> {
    let mut out = code_spans(text);
    let mut i = 0;
    let bytes = text.as_bytes();
    // Whitespace-separated runs.
    while i < text.len() {
        if bytes[i].is_ascii_whitespace() {
            i += 1;
            continue;
        }
        let run_start = i;
        while i < text.len() && !bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        // `i` stops on ASCII whitespace or the end: always a char boundary.
        run_skips(text, run_start, i, &mut out);
    }
    out.sort_by_key(|r| (r.start, r.end));
    // Merge overlaps.
    let mut merged: Vec<Range<usize>> = Vec::with_capacity(out.len());
    for r in out {
        if let Some(last) = merged.last_mut()
            && r.start <= last.end
        {
            last.end = last.end.max(r.end);
            continue;
        }
        merged.push(r);
    }
    merged
}

/// ```` ```pre``` ```` and `` `code` `` spans (Telegram markdown). An
/// unclosed fence runs to the end; an unclosed single backtick is text.
fn code_spans(text: &str) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(off) = text[i..].find('`') {
        let start = i + off;
        if text[start..].starts_with("```") {
            let body = start + 3;
            let end = text[body..]
                .find("```")
                .map_or(text.len(), |e| body + e + 3);
            out.push(start..end);
            i = end;
        } else if let Some(e) = text[start + 1..].find('`') {
            let end = start + 1 + e + 1;
            out.push(start..end);
            i = end;
        } else {
            break;
        }
        if i >= text.len() {
            break;
        }
    }
    out
}

/// Skips inside one whitespace-free run `text[start..end]`.
fn run_skips(text: &str, start: usize, end: usize, out: &mut Vec<Range<usize>>) {
    let run = &text[start..end];
    // Scheme URLs: from the scheme to the end of the run.
    if let Some(pos) = run.find("://") {
        let scheme_start = run[..pos]
            .char_indices()
            .rev()
            .take_while(|(_, c)| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
            .last()
            .map_or(pos, |(ix, _)| ix);
        out.push(start + scheme_start..end);
    }
    // `tg:`/`mailto:` style schemes without slashes.
    for scheme in ["mailto:", "tg:", "tel:"] {
        if let Some(pos) = find_ascii_ci(run, scheme)
            && run[..pos].chars().last().is_none_or(|c| !is_word_char(c))
        {
            out.push(start + pos..end);
        }
    }
    // `www.` hosts.
    if let Some(pos) = find_ascii_ci(run, "www.")
        && run[..pos].chars().last().is_none_or(|c| !is_word_char(c))
    {
        out.push(start + pos..end);
    }
    // Bare domains and e-mails: the run minus surrounding punctuation.
    let core_start = run
        .char_indices()
        .find(|(_, c)| !matches!(c, '(' | '[' | '{' | '"' | '\'' | '<' | '«' | '“'))
        .map_or(run.len(), |(ix, _)| ix);
    let core_end = run
        .char_indices()
        .rev()
        .find(|(_, c)| {
            !matches!(
                c,
                ')' | ']' | '}' | '"' | '\'' | '>' | '.' | ',' | '!' | '?' | ':' | ';' | '»' | '”'
            )
        })
        .map_or(core_start, |(ix, c)| ix + c.len_utf8());
    if core_start < core_end {
        let core = &run[core_start..core_end];
        if is_email(core) || is_domain(core) {
            out.push(start + core_start..start + core_end);
        }
    }
    // Entities: @mention, #hashtag, $cashtag, /command.
    let mut prev: Option<char> = None;
    for (ix, c) in run.char_indices() {
        let after = &run[ix + c.len_utf8()..];
        let boundary = prev.is_none_or(|p| !is_word_char(p) && p != '@' && p != '#');
        let entity = match c {
            '@' | '#' => boundary,
            '$' => boundary && after.starts_with(|n: char| n.is_ascii_uppercase()),
            // Bot commands only at the start of a run ("and/or" is text).
            '/' => prev.is_none(),
            _ => false,
        };
        if entity {
            let len: usize = after
                .chars()
                .take_while(|&n| is_word_char(n) || (c == '/' && n == '@'))
                .map(char::len_utf8)
                .sum();
            if len > 0 {
                out.push(start + ix..start + ix + c.len_utf8() + len);
            }
        }
        prev = Some(c);
    }
}

fn find_ascii_ci(haystack: &str, needle: &str) -> Option<usize> {
    let h = haystack.as_bytes();
    let n = needle.as_bytes();
    if n.len() > h.len() {
        return None;
    }
    (0..=h.len() - n.len())
        .find(|&i| haystack.is_char_boundary(i) && h[i..i + n.len()].eq_ignore_ascii_case(n))
}

fn is_email(s: &str) -> bool {
    let Some((local, domain)) = s.split_once('@') else {
        return false;
    };
    !local.is_empty()
        && local
            .chars()
            .all(|c| is_word_char(c) || matches!(c, '.' | '+' | '-'))
        && domain.contains('.')
        && domain
            .split('.')
            .all(|l| !l.is_empty() && l.chars().all(|c| c.is_alphanumeric() || c == '-'))
}

fn is_domain(s: &str) -> bool {
    let host = s.split(['/', '?', '#']).next().unwrap_or("");
    let host = host.split(':').next().unwrap_or("");
    let labels: Vec<&str> = host.split('.').collect();
    if labels.len() < 2 {
        return false;
    }
    let ok_labels = labels
        .iter()
        .all(|l| !l.is_empty() && l.chars().all(|c| c.is_alphanumeric() || c == '-'));
    let tld = labels[labels.len() - 1].to_ascii_lowercase();
    ok_labels && TLDS.contains(&tld.as_str())
}

/// Carry misspellings across an edit from `old` to `new` without
/// re-checking: ranges before the edit stay, ranges after it shift, and
/// any range the edit touches is dropped until the next check. Typing
/// punctuation or a space right after a flagged word keeps its underline.
pub fn shift_misspellings(old: &str, new: &str, list: &[Misspelling]) -> Vec<Misspelling> {
    if old == new || list.is_empty() {
        return list.to_vec();
    }
    let (prefix, old_end, new_end) = edit_window(old, new);
    let delta = new_end as isize - old_end as isize;
    list.iter()
        .filter_map(|m| {
            let (start, end) = if m.end <= prefix {
                (m.start, m.end)
            } else if m.start >= old_end {
                (
                    (m.start as isize + delta) as usize,
                    (m.end as isize + delta) as usize,
                )
            } else {
                return None;
            };
            // Still the same, whole word: no letters glued onto either side.
            let same = new.get(start..end) == Some(m.word.as_str())
                && new[..start]
                    .chars()
                    .next_back()
                    .is_none_or(|c| !is_word_char(c))
                && new[end..].chars().next().is_none_or(|c| !is_word_char(c));
            same.then(|| Misspelling {
                word: m.word.clone(),
                start,
                end,
            })
        })
        .collect()
}

/// Whether the edit from `old` to `new` is mid-word typing: it inserted
/// text ending in a letter or digit. tdesktop re-checks those after its
/// "cold" timeout and everything else (space, punctuation, deletion,
/// paste ending in a separator) right away.
pub fn is_typing_word(old: &str, new: &str) -> bool {
    let (prefix, _, new_end) = edit_window(old, new);
    new[prefix..new_end]
        .chars()
        .next_back()
        .is_some_and(is_word_char)
}

/// `(prefix, old_end, new_end)`: `old[prefix..old_end]` was replaced by
/// `new[prefix..new_end]`; all on char boundaries.
fn edit_window(old: &str, new: &str) -> (usize, usize, usize) {
    let (a, b) = (old.as_bytes(), new.as_bytes());
    let mut prefix = a.iter().zip(b).take_while(|(x, y)| x == y).count();
    while !old.is_char_boundary(prefix) || !new.is_char_boundary(prefix) {
        prefix -= 1;
    }
    let max_suffix = (a.len() - prefix).min(b.len() - prefix);
    let mut suffix = a
        .iter()
        .rev()
        .zip(b.iter().rev())
        .take(max_suffix)
        .take_while(|(x, y)| x == y)
        .count();
    while !old.is_char_boundary(a.len() - suffix) || !new.is_char_boundary(b.len() - suffix) {
        suffix -= 1;
    }
    (prefix, a.len() - suffix, b.len() - suffix)
}

/// Match a correction to the original word's letter case.
pub fn match_capitalization(original: &str, suggestion: &str) -> String {
    let mut letters = original.chars().filter(|c| c.is_alphabetic()).peekable();
    if letters.peek().is_some() && letters.clone().all(char::is_uppercase) && original.len() > 1 {
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

/// The portable backend: embedded English wordlist, ASCII words only
/// (accented or other-script words are never flagged), suggestions from
/// Damerau-Levenshtein edits ranked by word frequency.
pub struct WordlistBackend {
    /// Frequency rank (0 = most common) for suggestion ranking.
    rank_of: HashMap<&'static str, u32>,
}

impl WordlistBackend {
    pub fn new() -> Self {
        let mut rank_of = HashMap::new();
        for (rank, line) in WORDLIST.lines().enumerate() {
            let w = line.trim();
            if !w.is_empty() {
                rank_of.insert(w, rank as u32);
            }
        }
        Self { rank_of }
    }

    /// Words known to the checker (dictionary size), for tests/diagnostics.
    pub fn dictionary_size(&self) -> usize {
        self.rank_of.len()
    }

    fn known(&self, lower: &str) -> bool {
        self.rank_of.contains_key(lower)
    }
}

impl Default for WordlistBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl SpellBackend for WordlistBackend {
    fn handles(&self, script: Script, word: &str) -> bool {
        script == Script::Latin
            && word
                .chars()
                .all(|c| c.is_ascii_alphabetic() || c == '\'' || c == '-')
            && word.len() > 1
    }

    fn is_correct(&self, word: &str) -> bool {
        let lower = word.to_lowercase();
        // The list has no contractions: accept `don't` via `dont`, and
        // possessives / `'ll` forms via their stem.
        self.known(&lower)
            || (lower.contains('\'')
                && (self.known(&lower.replace('\'', ""))
                    || lower
                        .split('\'')
                        .next()
                        .is_some_and(|stem| self.known(stem))))
    }

    fn suggestions(&self, word: &str, limit: usize) -> Vec<String> {
        let norm = word.to_lowercase();
        if norm.is_empty() || norm.len() > 64 || self.known(&norm) || limit == 0 {
            return Vec::new();
        }
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
        // Distance 2 only when distance 1 found too little. Hard
        // iteration cap: gibberish would otherwise scan ~300k candidates.
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
        scored.dedup_by(|a, b| a.2 == b.2);
        scored
            .into_iter()
            .take(limit)
            .map(|(_, _, w)| match_capitalization(word, &w))
            .collect()
    }
}

/// All Damerau-Levenshtein edits at distance 1 (deletes, transposes,
/// replaces, inserts over a-z plus apostrophe and hyphen).
fn edits1(word: &str) -> Vec<String> {
    let chars: Vec<char> = word.chars().collect();
    let n = chars.len();
    let mut out = Vec::with_capacity(58 * n + 28);
    for i in 0..n {
        let mut s = String::with_capacity(n);
        s.extend(chars[..i].iter());
        s.extend(chars[i + 1..].iter());
        out.push(s);
    }
    for i in 0..n.saturating_sub(1) {
        let mut t = chars.clone();
        t.swap(i, i + 1);
        out.push(t.into_iter().collect());
    }
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
        SpellChecker::wordlist()
    }

    fn words(text: &str) -> Vec<&str> {
        checkable_words(text)
            .into_iter()
            .map(|r| &text[r])
            .collect()
    }

    fn flagged(text: &str) -> Vec<String> {
        checker()
            .check_text(text)
            .into_iter()
            .map(|m| m.word)
            .collect()
    }

    #[test]
    fn dictionary_loads() {
        let backend = WordlistBackend::new();
        assert!(backend.dictionary_size() > 40_000);
        let sc = checker();
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
        assert!(sc.is_correct("quill"));
        assert!(sc.is_correct("telegram"));
    }

    #[test]
    fn segmentation_keeps_apostrophes_and_splits_hyphens() {
        assert_eq!(words("don't stop"), vec!["don't", "stop"]);
        assert_eq!(words("don\u{2019}t"), vec!["don\u{2019}t"]);
        assert_eq!(words("'quoted' word"), vec!["quoted", "word"]);
        assert_eq!(words("well-known"), vec!["well", "known"]);
        assert_eq!(words("Hello, world!"), vec!["Hello", "world"]);
        // Typographic apostrophes are checked as plain ones.
        assert!(checker().is_correct("don\u{2019}t"));
    }

    #[test]
    fn skips_links_and_entities() {
        let text = "teh https://teh.example/teh?x=teh www.tehh.org/a tehh.com \
                    mail tehh@tehh.com @tehuser #tehtag $TEHH /tehcmd and/or";
        assert_eq!(words(text), vec!["teh", "mail", "and", "or"]);
        // Markdown link: the label is text, the target is a link.
        assert_eq!(words("[teh](https://x.y/teh)"), vec!["teh"]);
        // Custom-emoji markup is a tg:// link.
        assert_eq!(words("![🔠](tg://emoji?id=1) ok"), vec!["ok"]);
        // A dotted typo is not a domain ("Start" isn't a TLD).
        assert_eq!(words("end.Start"), vec!["end", "Start"]);
    }

    #[test]
    fn skips_code_spans() {
        assert_eq!(words("a `tehh code` b"), vec!["a", "b"]);
        assert_eq!(words("x ```\nfn tehh()\n``` y"), vec!["x", "y"]);
        // Unclosed fence: everything after it is code.
        assert_eq!(words("ok ```tehh"), vec!["ok"]);
        // A lone backtick is just text.
        assert_eq!(words("it`s"), vec!["it", "s"]);
    }

    #[test]
    fn skips_noise_words() {
        assert_eq!(words("teh123 teh_user NASA FYI 😀 x2"), Vec::<&str>::new());
        // Mixed script words are skipped; pure other-script words segment.
        assert_eq!(word_script("caféteh"), Some(Script::Latin));
        assert_eq!(word_script("teшh"), None);
        assert!(checker().is_correct("teшh"));
        let long = "a".repeat(MAX_WORD_CHARS + 1);
        assert!(words(&long).is_empty());
    }

    #[test]
    fn other_scripts_are_never_flagged_by_the_wordlist() {
        assert!(flagged("שלום привет 日本語 café naïve").is_empty());
        assert_eq!(char_script('日'), Some(Script::Han));
        assert_eq!(char_script('ש'), Some(Script::Hebrew));
        assert_eq!(char_script('1'), None);
        assert_eq!(locale_script("en_US"), Some(Script::Latin));
        assert_eq!(locale_script("pt-BR"), Some(Script::Latin));
        assert_eq!(locale_script("ru"), Some(Script::Cyrillic));
        assert_eq!(locale_script("he"), Some(Script::Hebrew));
        assert_eq!(locale_script("xx"), None);
    }

    #[test]
    fn check_text_finds_byte_ranges() {
        let text = "Hello teh world, this is a speling test";
        let miss = checker().check_text(text);
        let found: Vec<&str> = miss.iter().map(|m| &text[m.range()]).collect();
        assert_eq!(found, vec!["teh", "speling"]);
        // Byte ranges stay valid after multi-byte characters.
        let text = "😀 日本 teh";
        let miss = checker().check_text(text);
        assert_eq!(&text[miss[0].range()], "teh");
    }

    #[test]
    fn suggestions_rank_the_obvious_first() {
        let sc = checker();
        assert_eq!(
            sc.suggestions("teh", 5).first().map(String::as_str),
            Some("the")
        );
        assert_eq!(
            sc.suggestions("Teh", 5).first().map(String::as_str),
            Some("The")
        );
        assert!(
            sc.suggestions("speling", 5)
                .contains(&"spelling".to_string())
        );
        assert!(sc.suggestions("hello", 5).is_empty());
        assert!(sc.suggestions(&"z".repeat(4096), 5).is_empty());
        assert!(
            sc.suggestions("wellknown", 5)
                .contains(&"well-known".to_string())
        );
    }

    #[test]
    fn corrections_match_capitalization() {
        assert_eq!(match_capitalization("Teh", "the"), "The");
        assert_eq!(match_capitalization("THE", "the"), "THE");
        assert_eq!(match_capitalization("teh", "the"), "the");
        assert_eq!(match_capitalization("'Teh", "'the"), "'The");
        assert_eq!(match_capitalization("---", "the"), "the");
    }

    #[test]
    fn learn_unlearn_and_ignore() {
        let sc = checker();
        assert!(!sc.is_correct("blorpt"));
        // The wordlist has no dictionary of its own: the app keeps it.
        assert_eq!(sc.learn("blorpt"), LearnedIn::App);
        assert!(sc.is_correct("blorpt"));
        assert!(sc.is_correct("Blorpt"));
        assert!(sc.is_learned("blorpt"));
        assert_eq!(sc.app_words(), vec!["blorpt".to_string()]);
        assert_eq!(sc.unlearn("blorpt"), Some(LearnedIn::App));
        assert!(!sc.is_correct("blorpt"));
        assert_eq!(sc.unlearn("blorpt"), None);
        sc.ignore("Zorbl");
        assert!(sc.is_correct("zorbl"));
        assert!(!sc.is_learned("zorbl"));
        sc.set_app_words(["Frobz".to_string()]);
        assert!(sc.is_correct("frobz"));
    }

    struct CountingBackend(std::sync::atomic::AtomicUsize);
    impl SpellBackend for CountingBackend {
        fn handles(&self, _: Script, _: &str) -> bool {
            true
        }
        fn is_correct(&self, word: &str) -> bool {
            self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            word != "bad"
        }
        fn suggestions(&self, _: &str, _: usize) -> Vec<String> {
            vec!["bad".into(), "good".into(), "good".into()]
        }
    }

    #[test]
    fn backend_answers_are_cached_per_word() {
        let backend = Arc::new(CountingBackend(Default::default()));
        let sc = SpellChecker::new(backend.clone());
        let text = "bad good bad good bad";
        assert_eq!(sc.check_text(text).len(), 3);
        assert_eq!(sc.check_text(text).len(), 3);
        // Two distinct words, two backend calls, across both checks.
        assert_eq!(backend.0.load(std::sync::atomic::Ordering::SeqCst), 2);
        // The word itself and duplicates are dropped from suggestions.
        assert_eq!(sc.suggestions("bad", 5), vec!["good".to_string()]);
    }

    fn miss(text: &str, word: &str) -> Misspelling {
        let start = text.find(word).unwrap();
        Misspelling {
            word: word.into(),
            start,
            end: start + word.len(),
        }
    }

    #[test]
    fn shift_misspellings_follows_edits() {
        let old = "teh cat speling";
        let list = vec![miss(old, "teh"), miss(old, "speling")];
        // Typing in front shifts both.
        let new = "Oh teh cat speling";
        assert_eq!(
            shift_misspellings(old, new, &list),
            vec![miss(new, "teh"), miss(new, "speling")]
        );
        // Editing inside a word drops just that word.
        let new = "tehx cat speling";
        assert_eq!(
            shift_misspellings(old, new, &list),
            vec![miss(new, "speling")]
        );
        // Punctuation right after a flagged word keeps it.
        let new = "teh cat speling,";
        assert_eq!(
            shift_misspellings(old, new, &list),
            vec![miss(new, "teh"), miss(new, "speling")]
        );
        // Letters right after it drop it (the word changed).
        let new = "teh cat spelingx";
        assert_eq!(shift_misspellings(old, new, &list), vec![miss(new, "teh")]);
        // Multi-byte edits stay on char boundaries.
        let new = "😀teh cat speling";
        assert_eq!(
            shift_misspellings(old, new, &list),
            vec![miss(new, "teh"), miss(new, "speling")]
        );
        assert_eq!(edit_window("aé", "aè"), (1, 3, 3));
    }

    #[test]
    fn spelling_languages_follow_system_languages() {
        let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        let available = s(&["en", "en_GB", "en_AU", "he", "cs", "pt_BR", "pt_PT", "ru"]);
        assert_eq!(
            spelling_languages(&s(&["en-US", "he-IL"]), &available),
            s(&["en", "he"])
        );
        assert_eq!(
            spelling_languages(&s(&["en-GB"]), &available),
            s(&["en_GB"])
        );
        assert_eq!(
            spelling_languages(&s(&["pt-BR", "pt"]), &available),
            s(&["pt_BR"])
        );
        assert_eq!(spelling_languages(&s(&["pt"]), &available), s(&["pt_BR"]));
        assert_eq!(
            spelling_languages(&s(&["ja-JP"]), &available),
            Vec::<String>::new()
        );
        assert_eq!(
            spelling_languages(&s(&["en", "en-US"]), &available),
            s(&["en"])
        );
    }

    #[test]
    fn typing_word_detection() {
        assert!(is_typing_word("hel", "hell"));
        assert!(is_typing_word("", "a"));
        assert!(!is_typing_word("hell", "hell "));
        assert!(!is_typing_word("hell", "hell,"));
        assert!(!is_typing_word("hello", "hell"));
        assert!(!is_typing_word("same", "same"));
    }

    #[test]
    fn word_at_offset() {
        let text = "hello teh world";
        assert_eq!(word_at(text, 7), Some(6..9));
        assert_eq!(word_at(text, 9), Some(6..9));
        assert_eq!(word_at(text, 5), Some(0..5));
        assert_eq!(word_at(text, 6), Some(6..9));
        assert_eq!(word_at("a @mention", 5), None);
    }
}
