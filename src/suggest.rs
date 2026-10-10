//! Composer autocomplete that needs no server round trip: `#hashtag`
//! suggestions from the locally remembered sent hashtags, and `:emoji`
//! suggestions from the bundled emoji names.
//!
//! Behavior follows Telegram Desktop: the trigger rules are
//! `ParseMentionHashtagBotCommandQuery` (`chat_helpers/message_field.cpp`)
//! for `#` and `SuggestionsController::getEmojiQuery`
//! (`chat_helpers/emoji_suggestions_widget.cpp`) for `:`; the hashtag
//! list is `cRecentWriteHashtags` with `Local::incrementRecentHashtag`
//! (`storage/localstorage.cpp`).

use std::ops::Range;

use serde::{Deserialize, Serialize};

/// Which popup a composer query belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SuggestKind {
    Hashtag,
    Emoji,
}

/// A trigger found at the caret: `range` covers the trigger character and
/// everything typed after it (`#tag` / `:smi`), `query` is the part after.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SuggestQuery {
    pub kind: SuggestKind,
    pub range: Range<usize>,
    pub query: String,
}

/// tdesktop stops scanning for `#` after 63 chars of tag.
const HASHTAG_MAX: usize = 64;
/// How far back from the caret a `:` may sit (tdesktop's emoji limit is
/// the longest keyword; 32 covers every name we match).
const EMOJI_QUERY_MAX: usize = 32;
/// Fewest typed characters after `:` before emoji are suggested.
pub const EMOJI_MIN_QUERY: usize = 2;
/// How many emoji one query offers.
pub const EMOJI_LIMIT: usize = 10;
/// How many recent hashtags one query offers.
pub const HASHTAG_LIMIT: usize = 8;

fn is_tag_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// The trigger at `caret` (a byte offset), if any. `emoji` gates the `:`
/// suggestions ("Suggest emoji replacements"). `#` wins over `:` only by
/// position: whichever trigger is closest to the caret is the one found.
pub fn detect(text: &str, caret: usize, emoji: bool) -> Option<SuggestQuery> {
    hashtag_query(text, caret).or_else(|| emoji.then(|| emoji_query(text, caret)).flatten())
}

/// `#tag` being typed at `caret`: letters, digits and `_` back to a `#`
/// that does not follow a tag character, and that is not inside a URL.
pub fn hashtag_query(text: &str, caret: usize) -> Option<SuggestQuery> {
    if !text.is_char_boundary(caret) {
        return None;
    }
    let head = &text[..caret];
    let mut count = 0;
    for (at, c) in head.char_indices().rev() {
        if c == '#' {
            let before = &head[..at];
            if before.chars().next_back().is_some_and(is_tag_char) {
                return None;
            }
            // `https://site/#frag`, `a/b#c`: the word holding the `#`
            // looks like a URL or path.
            let word_start = before
                .char_indices()
                .rev()
                .find(|(_, c)| c.is_whitespace())
                .map_or(0, |(i, c)| i + c.len_utf8());
            if before[word_start..].contains('/') {
                return None;
            }
            return Some(SuggestQuery {
                kind: SuggestKind::Hashtag,
                range: at..caret,
                query: head[at + 1..].to_string(),
            });
        }
        count += 1;
        if !is_tag_char(c) || count > HASHTAG_MAX {
            return None;
        }
    }
    None
}

/// `:name` being typed at `caret`. tdesktop's rules: the `:` must not
/// follow a letter or digit (`http:`, `12:30`), must be followed by
/// something that is not a space, and the name may hold single spaces
/// ("thumbs up"). Quill also wants [`EMOJI_MIN_QUERY`] characters.
pub fn emoji_query(text: &str, caret: usize) -> Option<SuggestQuery> {
    if !text.is_char_boundary(caret) {
        return None;
    }
    let head = &text[..caret];
    let mut count = 0;
    let mut previous_space = false;
    for (at, c) in head.char_indices().rev() {
        if c == ':' {
            let query = &head[at + 1..];
            if head[..at]
                .chars()
                .next_back()
                .is_some_and(|p| p.is_alphanumeric())
                || query.chars().count() < EMOJI_MIN_QUERY
                || query.starts_with(char::is_whitespace)
                || !query.chars().next().is_some_and(char::is_alphanumeric)
                    && !query.starts_with(['+', '-'])
            {
                return None;
            }
            return Some(SuggestQuery {
                kind: SuggestKind::Emoji,
                range: at..caret,
                query: query.to_string(),
            });
        }
        count += 1;
        let space = c == ' ';
        if count > EMOJI_QUERY_MAX
            || !(c.is_alphanumeric() || matches!(c, '_' | '-' | '+') || space)
            || (space && previous_space)
        {
            return None;
        }
        previous_space = space;
    }
    None
}

// ---------------------------------------------------------------- hashtags

/// Telegram Desktop keeps 64 recent write hashtags.
const RECENT_HASHTAG_CAP: usize = 64;
/// Counts are halved when one passes this (`incrementRecentHashtag`).
const RECENT_HASHTAG_HALVE: u16 = 0x4000;

/// Hashtags the user has sent, most used first (tdesktop's
/// `RecentHashtagPack`: tag + use count, ties favour the newest).
/// Persisted as `recent_hashtags.json`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct RecentHashtags {
    #[serde(default)]
    pub tags: Vec<(String, u16)>,
}

impl RecentHashtags {
    /// Count one use of `tag` (without the `#`); port of
    /// `Local::incrementRecentHashtag`.
    pub fn bump(&mut self, tag: &str) {
        let tags = &mut self.tags;
        let mut i = match tags.iter().position(|(t, _)| t == tag) {
            Some(i) => {
                tags[i].1 = tags[i].1.saturating_add(1);
                if tags[i].1 > RECENT_HASHTAG_HALVE {
                    for (_, n) in tags.iter_mut() {
                        *n = if *n > 1 { *n / 2 } else { (*n).min(1) };
                    }
                }
                i
            }
            None => {
                tags.truncate(RECENT_HASHTAG_CAP - 1);
                tags.push((tag.to_string(), 1));
                tags.len() - 1
            }
        };
        while i > 0 && tags[i - 1].1 <= tags[i].1 {
            tags.swap(i, i - 1);
            i -= 1;
        }
    }

    /// Count every hashtag in a sent message.
    pub fn record_message(&mut self, text: &str) -> bool {
        let found = extract_hashtags(text);
        for tag in &found {
            self.bump(tag);
        }
        !found.is_empty()
    }

    /// Drop one tag (without the `#`). True when it was there.
    pub fn remove(&mut self, tag: &str) -> bool {
        let before = self.tags.len();
        self.tags.retain(|(t, _)| t != tag);
        self.tags.len() != before
    }

    /// Tags offered for the typed `filter` (text after `#`): everything
    /// when empty, otherwise a case-insensitive prefix match that skips
    /// the tag already typed in full (tdesktop `tag.size() == filter.size()`).
    pub fn matching(&self, filter: &str) -> Vec<&str> {
        let needle = filter.to_lowercase();
        self.tags
            .iter()
            .map(|(t, _)| t.as_str())
            .filter(|tag| {
                needle.is_empty()
                    || (tag.chars().count() != needle.chars().count()
                        && tag.to_lowercase().starts_with(&needle))
            })
            .take(HASHTAG_LIMIT)
            .collect()
    }
}

/// The hashtags in a message, without `#`: tag characters after a `#`
/// that does not follow a tag character, outside URLs, not all digits.
pub fn extract_hashtags(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for (at, c) in text.char_indices() {
        if c != '#' {
            continue;
        }
        let Some(tag) = hashtag_query(&text[..at + 1], at + 1) else {
            continue;
        };
        debug_assert!(tag.query.is_empty());
        let rest: String = text[at + 1..]
            .chars()
            .take_while(|c| is_tag_char(*c))
            .take(HASHTAG_MAX)
            .collect();
        if rest.is_empty() || rest.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        out.push(rest);
    }
    out
}

// ------------------------------------------------------------------- emoji

/// Popular shortcodes the Unicode names do not cover (`:smile:` is not a
/// prefix of "smiling face"). Quill's own short list; tdesktop ships a
/// full shortcode table.
const ALIASES: &[(&str, &str)] = &[
    ("😄", "smile"),
    ("😊", "blush"),
    ("😂", "joy lol"),
    ("🤣", "rofl"),
    ("😉", "wink"),
    ("😍", "heart_eyes"),
    ("😘", "kiss"),
    ("😢", "cry"),
    ("😭", "sob"),
    ("😡", "angry rage"),
    ("😎", "cool"),
    ("🤔", "thinking hmm"),
    ("😴", "sleep zzz"),
    ("🙂", "slight_smile"),
    ("🙏", "pray please thanks"),
    ("👍", "thumbsup +1 like"),
    ("👎", "thumbsdown -1 dislike"),
    ("👌", "ok"),
    ("👏", "clap"),
    ("🙌", "raised_hands hooray"),
    ("💪", "muscle strong"),
    ("👀", "eyes look"),
    ("❤️", "heart love"),
    ("💔", "broken_heart"),
    ("🔥", "fire lit"),
    ("🎉", "tada party"),
    ("✨", "sparkles"),
    ("💯", "100 hundred"),
    ("🚀", "rocket"),
    ("✅", "check ok"),
    ("❌", "x cross"),
    ("⚠️", "warning"),
    ("💩", "poop shit"),
    ("🤷", "shrug"),
    ("🤦", "facepalm"),
    ("☕", "coffee"),
    ("🍕", "pizza"),
    ("🍺", "beer"),
    ("🐶", "dog puppy"),
    ("🐱", "cat kitten"),
];

/// One suggestion: the emoji and the name shown with it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmojiHit {
    pub emoji: &'static str,
    pub name: &'static str,
}

struct Indexed {
    emoji: &'static str,
    name: &'static str,
    /// Lowercased words of the name, then of the aliases.
    words: Vec<String>,
    name_words: usize,
}

fn index() -> &'static [Indexed] {
    use std::sync::OnceLock;
    static INDEX: OnceLock<Vec<Indexed>> = OnceLock::new();
    INDEX.get_or_init(|| {
        crate::emoji_catalog::catalog()
            // Skin-tone variants would drown the base emoji.
            .filter(|e| !e.name.contains("skin tone") && !e.name.contains("hair"))
            .map(|e| {
                let mut words = tokens(e.name);
                let name_words = words.len();
                let bare = e.emoji.replace('\u{fe0f}', "");
                for (emoji, aliases) in ALIASES {
                    if emoji.replace('\u{fe0f}', "") == bare {
                        words.extend(aliases.split(' ').map(str::to_string));
                    }
                }
                Indexed {
                    emoji: e.emoji,
                    name: e.name,
                    words,
                    name_words,
                }
            })
            .collect()
    })
}

fn tokens(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| c.is_whitespace() || c == '_' || c == ':' || c == ',')
        .filter(|w| !w.is_empty())
        .map(str::to_string)
        .collect()
}

/// The emoji whose full name or shortcode is exactly `query` (`rocket`,
/// `slight_smile`), for `:name:` replacement in the composer.
pub fn exact_emoji(query: &str) -> Option<&'static str> {
    let wanted = tokens(query).join(" ");
    if wanted.is_empty() {
        return None;
    }
    index()
        .iter()
        .find(|item| {
            item.name.to_lowercase() == wanted || item.words[item.name_words..].contains(&wanted)
        })
        .map(|item| item.emoji)
}

/// Emoji whose name (or shortcode) words start with every word of
/// `query`, best first: an exact name or shortcode, then names starting
/// with the query, then matches on the first word, then the rest —
/// fewer-word names ahead within a tier (tdesktop `prepareResult`).
pub fn search_emoji(query: &str, limit: usize) -> Vec<EmojiHit> {
    let wanted = tokens(query);
    if wanted.is_empty() {
        return Vec::new();
    }
    let joined = wanted.join(" ");
    let mut hits: Vec<(u8, usize, usize, EmojiHit)> = Vec::new();
    for (order, item) in index().iter().enumerate() {
        let mut taken = vec![false; item.words.len()];
        let mut first_word = false;
        let all = wanted.iter().enumerate().all(|(n, word)| {
            let found = item
                .words
                .iter()
                .enumerate()
                .position(|(i, w)| !taken[i] && w.starts_with(word.as_str()));
            if let Some(i) = found {
                taken[i] = true;
                if n == 0 && i == 0 {
                    first_word = true;
                }
            }
            found.is_some()
        });
        if !all {
            continue;
        }
        let name = item.name.to_lowercase();
        let alias_exact = item.words[item.name_words..].contains(&joined);
        let tier = if name == joined || alias_exact {
            0
        } else if name.starts_with(&joined) {
            1
        } else if first_word {
            2
        } else {
            3
        };
        hits.push((
            tier,
            item.name_words,
            order,
            EmojiHit {
                emoji: item.emoji,
                name: item.name,
            },
        ));
    }
    hits.sort_by_key(|(tier, words, order, _)| (*tier, *words, *order));
    hits.into_iter().take(limit).map(|h| h.3).collect()
}

#[cfg(test)]
mod tests {
    use super::{
        EMOJI_LIMIT, RecentHashtags, SuggestKind, detect, emoji_query, extract_hashtags,
        hashtag_query, search_emoji,
    };

    fn at_end(text: &str) -> Option<(SuggestKind, std::ops::Range<usize>, String)> {
        detect(text, text.len(), true).map(|q| (q.kind, q.range, q.query))
    }

    #[test]
    fn hashtag_trigger_follows_the_caret_and_word_starts() {
        let q = hashtag_query("hi #ru", 6).unwrap();
        assert_eq!((q.range, q.query.as_str()), (3..6, "ru"));
        assert_eq!(hashtag_query("#", 1).unwrap().query, "");
        // Caret in the middle of a tag; text after it is ignored.
        let q = hashtag_query("x #rust y", 6).unwrap();
        assert_eq!((q.range, q.query.as_str()), (2..6, "rus"));
        // Caret past the tag's space, or before the `#`: nothing.
        assert!(hashtag_query("x #rust y", 8).is_none());
        assert!(hashtag_query("x #rust y", 2).is_none());
        // A tag character before `#` means it is not a hashtag.
        assert!(hashtag_query("foo#bar", 7).is_none());
        assert!(hashtag_query("ß#bar", 6).is_none());
        // Punctuation before is fine.
        assert!(hashtag_query("(#bar", 5).is_some());
    }

    #[test]
    fn hashtag_trigger_ignores_urls_and_bad_carets() {
        assert!(hashtag_query("see https://x.org/page#sec", 26).is_none());
        assert!(hashtag_query("a/b/#c", 6).is_none());
        // The URL ends at whitespace: a later hashtag works again.
        assert!(hashtag_query("https://x.org/ #c", 17).is_some());
        // Not a char boundary.
        assert!(hashtag_query("é#a", 1).is_none());
        // A tag cannot contain punctuation.
        assert!(hashtag_query("#a-b", 4).is_none());
    }

    #[test]
    fn emoji_trigger_follows_tdesktop_colon_rules() {
        let (kind, range, query) = at_end("hi :smi").unwrap();
        assert_eq!(
            (kind, range, query.as_str()),
            (SuggestKind::Emoji, 3..7, "smi")
        );
        // Letter/digit before the colon: `http:`, `12:30`, `note:x`.
        assert!(at_end("http:/").is_none());
        assert!(at_end("see http://ex").is_none());
        assert!(at_end("at 12:30").is_none());
        assert!(at_end("note:fi").is_none());
        // Too short, or followed by space.
        assert!(at_end(":s").is_none());
        assert!(at_end(": fire").is_none());
        assert!(at_end("x :").is_none());
        // Multi-word names, single spaces only.
        assert_eq!(at_end("a :thumbs up").unwrap().2, "thumbs up");
        assert!(at_end("a :thumbs  up").is_none());
        // Punctuation inside ends the query.
        assert!(at_end(":fire!").is_none());
        // Emoji can be switched off.
        assert!(detect(":fire", 5, false).is_none());
        // The caret decides, not the end of the text.
        assert_eq!(emoji_query(":fire now", 5).unwrap().query, "fire");
    }

    #[test]
    fn closer_trigger_wins() {
        // `#` inside an emoji query is not a tag character, so the colon
        // query is rejected and a later hashtag is found alone.
        assert_eq!(at_end("#tag :fi").unwrap().0, SuggestKind::Emoji);
        assert_eq!(at_end(":fire #ta").unwrap().0, SuggestKind::Hashtag);
    }

    #[test]
    fn hashtags_are_extracted_like_tdesktop() {
        assert_eq!(
            extract_hashtags("a #one, #Two_x b#no #123 #"),
            ["one", "Two_x"]
        );
        assert!(extract_hashtags("https://x.org/#frag").is_empty());
        assert_eq!(extract_hashtags("#日本語 ok"), ["日本語"]);
    }

    #[test]
    fn recent_hashtags_rank_by_use_then_recency() {
        let mut recent = RecentHashtags::default();
        recent.record_message("#a #b");
        // Equal counts: the newest goes first.
        assert_eq!(recent.matching(""), ["b", "a"]);
        recent.bump("a");
        assert_eq!(recent.matching(""), ["a", "b"]);
        recent.bump("b");
        assert_eq!(recent.matching(""), ["b", "a"]);
    }

    #[test]
    fn recent_hashtags_filter_and_cap() {
        let mut recent = RecentHashtags::default();
        recent.record_message("#Rust #rustacean #ruby #go");
        assert_eq!(recent.matching("ru"), ["ruby", "rustacean", "Rust"]);
        // The fully typed tag is not offered back (tdesktop).
        assert_eq!(recent.matching("rust"), ["rustacean"]);
        assert!(recent.matching("zz").is_empty());
        for n in 0..100 {
            recent.bump(&format!("t{n}"));
        }
        assert_eq!(recent.tags.len(), 64);
        assert_eq!(recent.tags[0].0, "t99");
        assert!(recent.remove("t99"));
        assert!(!recent.remove("t99"));
    }

    #[test]
    fn recent_hashtags_halve_huge_counts() {
        let mut recent = RecentHashtags {
            tags: vec![("a".into(), 0x4000), ("b".into(), 3), ("c".into(), 1)],
        };
        recent.bump("a");
        assert_eq!(
            recent.tags,
            [("a".into(), 0x2000), ("b".into(), 1), ("c".into(), 1)]
        );
    }

    #[test]
    fn recent_hashtags_round_trip_through_json() {
        let mut recent = RecentHashtags::default();
        recent.record_message("#x #y #y");
        let json = serde_json::to_string(&recent).unwrap();
        assert_eq!(
            serde_json::from_str::<RecentHashtags>(&json).unwrap(),
            recent
        );
        // A file from the future or a damaged one reads as empty.
        assert!(
            serde_json::from_str::<RecentHashtags>("{}")
                .unwrap()
                .tags
                .is_empty()
        );
    }

    #[test]
    fn emoji_search_ranks_exact_then_prefix() {
        let first = |q: &str| search_emoji(q, EMOJI_LIMIT).first().map(|h| h.emoji);
        assert_eq!(first("fire"), Some("🔥"));
        assert_eq!(first("thumbsup"), Some("👍"));
        assert_eq!(first("smile"), Some("😄"));
        assert_eq!(first("+1"), Some("👍"));
        assert_eq!(first("red heart"), Some("❤️"));
        let hits = search_emoji("thumbs up", 5);
        assert!(hits.iter().any(|h| h.emoji == "👍"));
        // Case-insensitive; words may come in any order.
        assert_eq!(first("FIRE"), Some("🔥"));
        assert!(
            search_emoji("face smiling", 5)
                .iter()
                .any(|h| h.name.contains("smiling face"))
        );
    }

    #[test]
    fn emoji_search_skips_skin_tones_and_junk() {
        assert!(
            search_emoji("waving hand", 20)
                .iter()
                .all(|h| !h.name.contains("skin tone"))
        );
        assert!(search_emoji("qqqqzz", 5).is_empty());
        assert!(search_emoji("  ", 5).is_empty());
        assert!(search_emoji("heart", 3).len() == 3);
    }
}
