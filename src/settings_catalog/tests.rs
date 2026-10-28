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
        agent_access: true,
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
                agent_access: entry.shown(true),
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

#[test]
fn recent_searches_keep_the_newest_known_picks() {
    let mut recent = super::RecentSearches::default();
    for id in ["privacy", "account.log_out", "nope.gone", "privacy"] {
        recent.record(id);
    }
    assert_eq!(
        recent.ids().collect::<Vec<_>>(),
        ["privacy", "account.log_out"],
        "newest first, deduped, unknown ids hidden"
    );
    for n in 0..20 {
        recent.record(if n % 2 == 0 { "calls" } else { "data" });
    }
    assert!(recent.as_slice().len() <= super::MAX_RECENT);
    recent.remove("calls");
    assert!(recent.ids().all(|id| id != "calls"));
    assert_eq!(
        super::target("account.log_out").map(|t| t.page()),
        Some(Page::Account)
    );
}

#[test]
fn agent_access_off_leaves_only_its_switch_on_advanced() {
    let off = SearchContext {
        agent_access: false,
        ..ctx(&[])
    };
    for query in [
        "agent access",
        "mcp",
        "allow agents",
        "takeover",
        "activity",
    ] {
        let hits = search(query, &off);
        assert!(
            hits.iter()
                .all(|hit| hit.target.page() != Page::AgentAccess),
            "{query}: {:?}",
            hits.iter().map(|h| h.target.id()).collect::<Vec<_>>()
        );
    }
    let ids = |ctx: &SearchContext| -> Vec<&'static str> {
        search("agent access", ctx)
            .into_iter()
            .map(|hit| hit.target.id())
            .collect()
    };
    assert_eq!(ids(&off).first(), Some(&super::AGENT_SWITCH));
    let on = ids(&ctx(&[]));
    assert_eq!(on.first(), Some(&"agent_access"), "{on:?}");
    assert!(!on.contains(&super::AGENT_SWITCH), "one switch at a time");

    assert!(!Page::AgentAccess.shown(false));
    assert!(
        Page::ALL
            .into_iter()
            .filter(|p| *p != Page::AgentAccess)
            .all(|p| p.shown(false))
    );
    assert!(Page::ALL.into_iter().all(|p| p.shown(true)));
    let switch = find(super::AGENT_SWITCH).expect("the Advanced switch is an entry");
    assert_eq!(switch.page, Page::Advanced);
    assert!(switch.shown(false) && !switch.shown(true));
}

#[test]
fn highlight_keeps_a_run_inside_one_word() {
    let m = super::fuzzy::subsequence("notif", "Show notifications from").expect("match");
    assert_eq!(m.positions, vec![5, 6, 7, 8, 9]);
}
