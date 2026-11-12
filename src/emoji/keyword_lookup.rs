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
/// Lookups in flight at once; the rest are asked for again when one
/// answers.
pub const IN_FLIGHT_CAP: usize = 8;
/// Failed requests for one query before it is left alone for the session
/// (the keyword pack would not load).
const ATTEMPTS: u8 = 2;

/// Where one query's lookup stands.
#[derive(Debug, Clone)]
enum Lookup {
    /// `request` is out; `failures` earlier attempts failed.
    Asking { request: RequestId, failures: u8 },
    /// Answered; `age` orders the entries when the oldest are forgotten.
    Answered {
        age: u64,
        keywords: Vec<EmojiKeyword>,
    },
    /// Failed with the connection up, `failures` times in all; asked again
    /// until [`ATTEMPTS`] have failed.
    Failed { failures: u8 },
    /// Failed while the connection was not ready: nothing is asked until
    /// it is back ([`KeywordLookups::forgive_failures`]).
    Offline,
}

/// `searchEmojis` answers by lowercased query, each query one stage.
#[derive(Debug, Clone, Default)]
pub struct KeywordLookups {
    entries: HashMap<String, Lookup>,
    /// Bumped on every answer or failure, so readers can tell when to
    /// look again (and ask again for what the cap turned away).
    pub generation: u64,
}

impl KeywordLookups {
    fn asking(&self) -> usize {
        self.entries
            .values()
            .filter(|lookup| matches!(lookup, Lookup::Asking { .. }))
            .count()
    }

    /// The answer for `query` (normalized), if it arrived.
    pub fn answer(&self, query: &str) -> Option<&[EmojiKeyword]> {
        match self.entries.get(query) {
            Some(Lookup::Answered { keywords, .. }) => Some(keywords.as_slice()),
            _ => None,
        }
    }

    /// Whether `query` still needs a request: no answer, not in flight,
    /// attempts left, and room for one more.
    pub fn wants(&self, query: &str) -> bool {
        match self.entries.get(query) {
            None => self.asking() < IN_FLIGHT_CAP,
            Some(Lookup::Failed { failures }) => {
                *failures < ATTEMPTS && self.asking() < IN_FLIGHT_CAP
            }
            Some(Lookup::Asking { .. } | Lookup::Answered { .. } | Lookup::Offline) => false,
        }
    }

    /// `query` has no answer and will get none for now: every attempt
    /// failed, or it failed while offline, and nothing is in flight for it.
    pub fn gave_up(&self, query: &str) -> bool {
        match self.entries.get(query) {
            Some(Lookup::Failed { failures }) => *failures >= ATTEMPTS,
            Some(Lookup::Offline) => true,
            _ => false,
        }
    }

    /// The request in flight for `query`, if one is.
    pub fn request_for(&self, query: &str) -> Option<RequestId> {
        match self.entries.get(query) {
            Some(Lookup::Asking { request, .. }) => Some(*request),
            _ => None,
        }
    }

    /// Whether request `id` is still in flight.
    pub fn waits_for(&self, id: RequestId) -> bool {
        self.query_of(id).is_some()
    }

    fn query_of(&self, id: RequestId) -> Option<&String> {
        self.entries.iter().find_map(|(query, lookup)| {
            matches!(lookup, Lookup::Asking { request, .. } if *request == id).then_some(query)
        })
    }

    /// `id` went out for `query`; failures it had before carry over.
    pub fn sent(&mut self, id: RequestId, query: String) {
        let failures = match self.entries.get(&query) {
            Some(Lookup::Failed { failures }) => *failures,
            _ => 0,
        };
        self.entries.insert(
            query,
            Lookup::Asking {
                request: id,
                failures,
            },
        );
    }

    /// Request `id` failed: its slot frees and the query may be asked
    /// again, up to [`ATTEMPTS`] times. `connected` is whether the
    /// connection was ready: a failure while it was not waits for it.
    pub fn failed(&mut self, id: RequestId, connected: bool) {
        let Some(query) = self.query_of(id).cloned() else {
            return;
        };
        let failures = match self.entries.get(&query) {
            Some(Lookup::Asking { failures, .. }) => failures.saturating_add(1),
            _ => return,
        };
        let next = if connected {
            Lookup::Failed { failures }
        } else {
            Lookup::Offline
        };
        self.entries.insert(query, next);
        self.generation += 1;
    }

    /// The answer for request `id`.
    pub fn answered(&mut self, id: RequestId, keywords: Vec<EmojiKeyword>) {
        let Some(query) = self.query_of(id).cloned() else {
            return;
        };
        if self.entries.len() >= ANSWER_CAP {
            let mut ages: Vec<u64> = self
                .entries
                .values()
                .filter_map(|lookup| match lookup {
                    Lookup::Answered { age, .. } => Some(*age),
                    _ => None,
                })
                .collect();
            ages.sort_unstable();
            let cutoff = ages.get(ages.len() / 2).copied().unwrap_or(u64::MAX);
            self.entries.retain(|_, lookup| match lookup {
                Lookup::Answered { age, .. } => *age > cutoff,
                Lookup::Asking { .. } => true,
                Lookup::Failed { .. } | Lookup::Offline => false,
            });
        }
        self.generation += 1;
        self.entries.insert(
            query,
            Lookup::Answered {
                age: self.generation,
                keywords,
            },
        );
    }

    /// The connection is back: queries that failed while it was down are
    /// asked again, and readers look again.
    pub fn forgive_failures(&mut self) {
        let before = self.entries.len();
        self.entries
            .retain(|_, lookup| !matches!(lookup, Lookup::Offline));
        if self.entries.len() != before {
            self.generation += 1;
        }
    }

    /// Forget requests that will never answer (swept, or the account
    /// went away), so they do not hold a flight slot. Each counts as a
    /// failed attempt, so a query that is never answered is given up on
    /// after [`ATTEMPTS`] instead of being asked after every sweep.
    pub fn retain_in_flight(&mut self, live: impl Fn(RequestId) -> bool, connected: bool) {
        let gone: Vec<RequestId> = self
            .entries
            .values()
            .filter_map(|lookup| match lookup {
                Lookup::Asking { request, .. } if !live(*request) => Some(*request),
                _ => None,
            })
            .collect();
        for id in gone {
            self.failed(id, connected);
        }
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
        lookups.retain_in_flight(|id| id.0 < 100, true);
        assert!(lookups.wants("other"));
    }

    #[test]
    fn a_query_that_failed_its_attempts_stays_given_up_when_the_connection_returns() {
        let mut lookups = KeywordLookups::default();
        for round in 1..=u64::from(ATTEMPTS) {
            lookups.sent(RequestId(round), "fire".into());
            lookups.failed(RequestId(round), true);
        }
        assert!(lookups.gave_up("fire") && !lookups.wants("fire"));
        // Online failures stay given up: the pack itself would not load.
        lookups.forgive_failures();
        assert!(lookups.gave_up("fire"));
    }

    #[test]
    fn a_query_that_failed_offline_is_asked_again_when_the_connection_returns() {
        let mut lookups = KeywordLookups::default();
        lookups.sent(RequestId(1), "fire".into());
        lookups.failed(RequestId(1), false);
        assert!(lookups.gave_up("fire") && !lookups.wants("fire"));
        lookups.forgive_failures();
        assert!(!lookups.gave_up("fire") && lookups.wants("fire"));
    }

    #[test]
    fn a_success_leaves_no_failures_behind() {
        let mut lookups = KeywordLookups::default();
        lookups.sent(RequestId(1), "fire".into());
        lookups.failed(RequestId(1), true);
        lookups.sent(RequestId(2), "fire".into());
        lookups.answered(RequestId(2), Vec::new());
        assert!(lookups.answer("fire").is_some() && !lookups.gave_up("fire"));
    }

    #[test]
    fn a_failed_query_is_asked_again_a_bounded_number_of_times() {
        let mut lookups = KeywordLookups::default();
        lookups.sent(RequestId(1), "fire".into());
        lookups.failed(RequestId(1), true);
        assert!(lookups.answer("fire").is_none());
        assert!(lookups.wants("fire"));
        assert_eq!(lookups.generation, 1);
        lookups.sent(RequestId(2), "fire".into());
        lookups.failed(RequestId(2), true);
        assert!(!lookups.wants("fire"));
    }

    #[test]
    fn a_swept_request_counts_as_a_failed_attempt() {
        let mut lookups = KeywordLookups::default();
        lookups.sent(RequestId(1), "fire".into());
        lookups.retain_in_flight(|_| false, true);
        assert!(lookups.wants("fire"), "one attempt left");
        lookups.sent(RequestId(2), "fire".into());
        lookups.retain_in_flight(|_| false, true);
        assert!(lookups.gave_up("fire"));
        assert!(!lookups.wants("fire"));
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
        assert!(lookups.entries.len() <= ANSWER_CAP / 2 + 2);
    }

    #[test]
    fn a_query_gives_up_after_its_attempts_fail() {
        let mut lookups = KeywordLookups::default();
        lookups.sent(RequestId(1), "zz".into());
        assert!(!lookups.gave_up("zz"), "still in flight");
        lookups.failed(RequestId(1), true);
        assert!(!lookups.gave_up("zz"), "one attempt left");
        lookups.sent(RequestId(2), "zz".into());
        assert!(!lookups.gave_up("zz"), "in flight again");
        lookups.failed(RequestId(2), true);
        assert!(lookups.gave_up("zz"));
        assert!(!lookups.wants("zz"));
        lookups.sent(RequestId(3), "ok".into());
        lookups.answered(RequestId(3), Vec::new());
        assert!(!lookups.gave_up("ok"), "an answered query did not fail");
    }
}
