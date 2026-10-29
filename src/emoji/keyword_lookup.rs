//! Emoji keyword lookups for the composer (tdesktop `EmojiKeywords`).
//!
//! tdesktop downloads each input language's keyword pack and searches it
//! on the device; TDLib does the same behind `searchEmojis` (the packs
//! come from `messages.getEmojiKeywords` by language code, the typed text
//! never leaves the device). This keeps the answers by query so the
//! `:emoji` popup and the word highlights read them without waiting, and
//! ranks them the way `EmojiKeywords::LangPack::query` does.
#![deny(clippy::indexing_slicing)]

use std::collections::HashMap;

use crate::ids::RequestId;
use crate::telegram::envelope_emoji::EmojiKeyword;

/// Lookups kept before the oldest half is forgotten.
const ANSWER_CAP: usize = 2048;
/// Lookups in flight at once; the rest are asked for on a later sync.
pub const IN_FLIGHT_CAP: usize = 8;

/// `searchEmojis` answers by lowercased query.
#[derive(Debug, Clone, Default)]
pub struct KeywordLookups {
    answers: HashMap<String, (u64, Vec<EmojiKeyword>)>,
    in_flight: HashMap<RequestId, String>,
    /// Bumped on every answer, so readers can tell when to look again.
    pub generation: u64,
}

impl KeywordLookups {
    /// The answer for `query` (normalized), if it arrived.
    pub fn answer(&self, query: &str) -> Option<&[EmojiKeyword]> {
        self.answers.get(query).map(|(_, list)| list.as_slice())
    }

    /// Whether `query` still needs a request: no answer, not in flight
    /// and room for one more.
    pub fn wants(&self, query: &str) -> bool {
        !self.answers.contains_key(query)
            && self.in_flight.len() < IN_FLIGHT_CAP
            && !self.in_flight.values().any(|q| q == query)
    }

    pub fn sent(&mut self, id: RequestId, query: String) {
        self.in_flight.insert(id, query);
    }

    /// An answer (or, with an empty list, a failure) for request `id`.
    pub fn answered(&mut self, id: RequestId, keywords: Vec<EmojiKeyword>) {
        let Some(query) = self.in_flight.remove(&id) else {
            return;
        };
        if self.answers.len() >= ANSWER_CAP {
            let mut ages: Vec<u64> = self.answers.values().map(|(age, _)| *age).collect();
            ages.sort_unstable();
            let cutoff = ages.get(ages.len() / 2).copied().unwrap_or(u64::MAX);
            self.answers.retain(|_, (age, _)| *age > cutoff);
        }
        self.generation += 1;
        self.answers.insert(query, (self.generation, keywords));
    }

    /// Forget requests that will never answer (swept, or the account
    /// went away), so they do not hold a flight slot.
    pub fn retain_in_flight(&mut self, live: impl Fn(RequestId) -> bool) {
        self.in_flight.retain(|id, _| live(*id));
    }
}

/// tdesktop `NormalizeQuery`.
pub fn normalize_query(query: &str) -> String {
    query.to_lowercase()
}

/// tdesktop `SkipExactKeyword`: words too short or too generic to offer
/// emoji for when typed alone. The English rules apply to ASCII words,
/// since an answer does not say which pack a keyword came from.
pub fn skip_exact_keyword(word: &str) -> bool {
    let mut chars = word.chars();
    let (first, second, third) = (chars.next(), chars.next(), chars.next());
    match (first, second, third) {
        (Some(c), None, _) if !c.is_alphabetic() => true,
        _ if word == "10" => true,
        _ if !word.is_ascii() => false,
        (Some(_), None, _) => true,
        (Some(_), Some(_), None) => !matches!(word, "us" | "uk" | "hi" | "ok"),
        _ => false,
    }
}

/// The emoji for `normalized` from a `searchEmojis` answer: keywords that
/// equal it (`exact`) or start with it, in keyword order, each emoji
/// once (`LangPack::query` walks its sorted map; `AppendFoundEmoji`
/// skips repeats). TDLib's order is unspecified, hence the sort.
pub fn keyword_emoji(normalized: &str, exact: bool, answer: &[EmojiKeyword]) -> Vec<String> {
    if normalized.is_empty() || (exact && skip_exact_keyword(normalized)) {
        return Vec::new();
    }
    let mut found: Vec<(String, &str)> = answer
        .iter()
        .filter_map(|hit| {
            let key = hit.keyword.to_lowercase().trim().to_string();
            let fits = if exact {
                key == normalized
            } else {
                key.starts_with(normalized)
            };
            fits.then_some((key, hit.emoji.as_str()))
        })
        .collect();
    found.sort_by(|a, b| a.0.cmp(&b.0));
    let mut out: Vec<String> = Vec::new();
    for (_, emoji) in found {
        push_unique(&mut out, emoji);
    }
    out
}

/// Append `emoji` unless it is already there (variation selectors aside).
pub fn push_unique(list: &mut Vec<String>, emoji: &str) {
    let bare = emoji.replace('\u{fe0f}', "");
    if !list.iter().any(|e| e.replace('\u{fe0f}', "") == bare) {
        list.push(emoji.to_string());
    }
}

/// tdesktop `PrioritizeRecent`: recently used emoji move to the front, in
/// the recent list's order.
pub fn prioritize_recent(mut list: Vec<String>, recent: &[String]) -> Vec<String> {
    let mut front = 0;
    for used in recent {
        let bare = used.replace('\u{fe0f}', "");
        let Some(at) = list.iter().position(|e| e.replace('\u{fe0f}', "") == bare) else {
            continue;
        };
        if at >= front {
            if let Some(slice) = list.get_mut(front..=at) {
                slice.rotate_right(1);
            }
            front += 1;
        }
    }
    list
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kw(emoji: &str, keyword: &str) -> EmojiKeyword {
        EmojiKeyword {
            emoji: emoji.into(),
            keyword: keyword.into(),
        }
    }

    #[test]
    fn prefix_and_exact_matches_follow_keyword_order() {
        let answer = [
            kw("😂", "happy tears"),
            kw("😊", "happy"),
            kw("😁", "happy"),
            kw("😊", "happiness"),
            kw("🎂", "birthday"),
        ];
        assert_eq!(keyword_emoji("happ", false, &answer), ["😊", "😁", "😂"]);
        assert_eq!(keyword_emoji("happy", true, &answer), ["😊", "😁"]);
        assert!(keyword_emoji("happ", true, &answer).is_empty());
        // Keywords are compared lowercased.
        assert_eq!(keyword_emoji("bi", false, &[kw("🎂", "Birthday")]), ["🎂"]);
    }

    #[test]
    fn short_and_generic_words_are_not_exact_matches() {
        let answer = [kw("👋", "hi"), kw("🅰️", "a"), kw("🔟", "10"), kw("💲", "$")];
        assert_eq!(keyword_emoji("hi", true, &answer), ["👋"]);
        assert!(keyword_emoji("a", true, &answer).is_empty());
        assert!(keyword_emoji("10", true, &answer).is_empty());
        // tdesktop's `$` exception sits behind its non-letter rule.
        assert!(keyword_emoji("$", true, &answer).is_empty());
        assert!(skip_exact_keyword("be"));
        assert!(!skip_exact_keyword("ok"));
        assert!(!skip_exact_keyword("да"));
        assert!(skip_exact_keyword("!"));
        // Prefix queries keep short words.
        assert_eq!(keyword_emoji("a", false, &answer), ["🅰️"]);
    }

    #[test]
    fn recent_emoji_move_to_the_front_in_recent_order() {
        let list: Vec<String> = ["😊", "😁", "😂", "🙂"].map(String::from).into();
        let recent: Vec<String> = ["🙂", "💩", "😁"].map(String::from).into();
        assert_eq!(prioritize_recent(list, &recent), ["🙂", "😁", "😊", "😂"]);
        // Already in front stays there.
        let list: Vec<String> = ["😊", "😁"].map(String::from).into();
        let recent = vec!["😊".to_string()];
        assert_eq!(prioritize_recent(list, &recent), ["😊", "😁"]);
    }

    #[test]
    fn lookups_track_flight_and_answers() {
        let mut lookups = KeywordLookups::default();
        assert!(lookups.wants("fire"));
        lookups.sent(RequestId(1), "fire".into());
        assert!(!lookups.wants("fire"));
        lookups.answered(RequestId(1), vec![kw("🔥", "fire")]);
        assert_eq!(lookups.answer("fire").map(<[_]>::len), Some(1));
        assert_eq!(lookups.generation, 1);
        // A stray answer changes nothing.
        lookups.answered(RequestId(9), Vec::new());
        assert_eq!(lookups.generation, 1);
        for n in 0..IN_FLIGHT_CAP {
            lookups.sent(RequestId(100 + n as u64), format!("q{n}"));
        }
        assert!(!lookups.wants("other"));
        lookups.retain_in_flight(|id| id.0 < 100);
        assert!(lookups.wants("other"));
    }

    #[test]
    fn old_answers_are_forgotten_past_the_cap() {
        let mut lookups = KeywordLookups::default();
        for n in 0..=ANSWER_CAP as u64 {
            lookups.sent(RequestId(n), format!("q{n}"));
            lookups.answered(RequestId(n), Vec::new());
        }
        assert!(lookups.answer("q0").is_none());
        assert!(lookups.answer(&format!("q{ANSWER_CAP}")).is_some());
        assert!(lookups.answers.len() <= ANSWER_CAP / 2 + 2);
    }
}
