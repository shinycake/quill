use super::fuzzy::{best, subsequence, typo, words};
use super::search::{SearchContext, Target};
use super::{Page, Platforms, all, find, search};
use crate::lang::{english, tr};
use std::collections::HashSet;

fn ctx(recent: &[String]) -> SearchContext<'_> {
    SearchContext {
        text: &tr,
        english: &english,
        platform: Platforms::MACOS,
        recent,
    }
}

fn top(query: &str) -> Vec<&'static str> {
    search(query, &ctx(&[]))
        .into_iter()
        .take(3)
        .map(|hit| hit.target.id())
        .collect()
}

#[test]
fn folds_case_and_marks() {
    assert_eq!(words("Português (Brasil)"), ["portugues", "brasil"]);
    assert!(subsequence("PORTU", "Português").is_some());
}

#[test]
fn word_starts_and_runs_beat_scattered_letters() {
    let starts = subsequence("nm", "Night mode").map(|m| m.score);
    let scattered = subsequence("nm", "Animation").map(|m| m.score);
    assert!(starts > scattered, "{starts:?} {scattered:?}");
    let run = subsequence("font", "Font").map(|m| m.score);
    assert!(run > subsequence("font", "Notifications for private chats").map(|m| m.score));
}

#[test]
fn positions_point_at_original_chars() {
    let m = subsequence("ns", "Night mode settings").expect("match");
    assert_eq!(m.positions, [0, 11]);
    let m = subsequence("pt", "Português").expect("match");
    assert_eq!(m.positions.first(), Some(&0));
}

#[test]
fn rejects_letters_scattered_across_unrelated_words() {
    assert!(subsequence("font", "Notifications for private chats").is_none());
    assert!(subsequence("zzz", "Night mode").is_none());
}

#[test]
fn forgives_one_typo_per_long_word() {
    assert!(typo("notifcations", "Desktop notifications").is_some());
    assert!(typo("drk mode", "Dark mode").is_some());
    assert!(typo("prvacy setings", "Privacy settings").is_some());
    assert!(typo("dork", "Notifications").is_none());
    assert!(best("passcod", "Local passcode").is_some());
}

#[test]
fn ids_are_unique_and_prefixed_by_their_page() {
    let mut seen = HashSet::new();
    for entry in all() {
        assert!(seen.insert(entry.id), "duplicate id {}", entry.id);
        let prefix = format!("{}.", entry.page.id());
        assert!(
            entry.id.starts_with(&prefix),
            "{} not under {prefix}",
            entry.id
        );
        assert!(
            !english(entry.title).is_empty(),
            "{} has no title",
            entry.id
        );
    }
    for page in Page::ALL {
        assert!(!page.entries().is_empty(), "{page:?} has no entries");
        assert!(!english(page.title()).is_empty());
    }
}

#[test]
fn every_entry_finds_itself_by_its_title() {
    for entry in all() {
        let title = english(entry.title);
        let platform = if entry.platforms.contains(Platforms::MACOS) {
            Platforms::MACOS
        } else if entry.platforms.contains(Platforms::WINDOWS) {
            Platforms::WINDOWS
        } else {
            Platforms::LINUX
        };
        let hits = search(
            title,
            &SearchContext {
                text: &tr,
                english: &english,
                platform,
                recent: &[],
            },
        );
        assert!(
            hits.iter().any(|hit| hit.target == Target::Entry(entry)),
            "{} not found by {title:?}",
            entry.id
        );
    }
}

#[test]
fn common_queries_land_on_the_right_setting() {
    assert_eq!(top("sign out").first(), Some(&"account.log_out"));
    assert_eq!(top("logout").first(), Some(&"account.log_out"));
    assert!(top("dark mode").contains(&"appearance.night_mode"));
    assert!(top("2fa").contains(&"privacy.two_step"));
    assert!(top("ptt").contains(&"calls.push_to_talk"));
    assert!(
        top("proxy")
            .first()
            .is_some_and(|id| id.starts_with("connection"))
    );
    assert!(top("drk mode").contains(&"appearance.night_mode"));
}

#[test]
fn pages_rank_above_their_rows_on_a_tie() {
    let hits = search("Privacy and Security", &ctx(&[]));
    assert_eq!(
        hits.first().map(|h| h.target),
        Some(Target::Page(Page::Privacy))
    );
}

#[test]
fn recent_picks_break_ties() {
    let plain = top("notifications");
    let pick = plain.get(1).copied().expect("two results").to_string();
    let recent = [pick.clone()];
    let boosted = search("notifications", &ctx(&recent));
    let rank = boosted.iter().position(|h| h.target.id() == pick);
    assert!(rank.is_some_and(|r| r <= 1), "{plain:?} -> {rank:?}");
}

#[test]
fn other_platforms_settings_are_hidden() {
    let only_windows = all().find(|e| e.platforms == Platforms::WINDOWS);
    if let Some(entry) = only_windows {
        let hits = search(english(entry.title), &ctx(&[]));
        assert!(hits.iter().all(|h| h.target != Target::Entry(entry)));
    }
}

#[test]
fn empty_queries_match_nothing_and_find_resolves_ids() {
    assert!(search("  ", &ctx(&[])).is_empty());
    assert!(find("account.log_out").is_some());
    assert!(find("nope.nothing").is_none());
}
