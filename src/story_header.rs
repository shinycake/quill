//! The story viewer's header: the posting time and the privacy badge, after
//! tdesktop's `media/stories/media_stories_header.cpp`.

use crate::local_time::{CivilTime, civil_at};
use serde_json::Value;

/// Who can see a story, when tdesktop marks it with a badge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrivacyBadge {
    CloseFriends,
    Contacts,
    SelectedContacts,
}

/// The badge for a story's raw `privacy_settings` (`StoryPrivacySettings`).
/// Everyone and unknown settings have none.
pub fn privacy_badge(settings: Option<&Value>) -> Option<PrivacyBadge> {
    match settings?.get("@type")?.as_str()? {
        "storyPrivacySettingsCloseFriends" => Some(PrivacyBadge::CloseFriends),
        "storyPrivacySettingsContacts" => Some(PrivacyBadge::Contacts),
        "storyPrivacySettingsSelectedUsers" => Some(PrivacyBadge::SelectedContacts),
        _ => None,
    }
}

/// How old a story is, in tdesktop's buckets (`ComposeTimestamp`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Age {
    JustNow,
    MinutesAgo(i64),
    HoursAgo(i64),
    Today(CivilTime),
    Yesterday(CivilTime),
    Date(CivilTime),
}

/// `when` and `now` are unix seconds; `offset_when` and `offset_now` are
/// the local UTC offsets at those instants.
pub fn age(when: i64, now: i64, offset_when: i32, offset_now: i32) -> Age {
    let elapsed = now.saturating_sub(when).max(0);
    let minutes = elapsed / 60;
    if minutes == 0 {
        return Age::JustNow;
    }
    if minutes < 60 {
        return Age::MinutesAgo(minutes);
    }
    let hours = elapsed / 3600;
    if hours < 12 {
        return Age::HoursAgo(hours);
    }
    let then = civil_at(when, offset_when);
    let today = civil_at(now, offset_now);
    match today.day_number() - then.day_number() {
        0 => Age::Today(then),
        1 => Age::Yesterday(then),
        _ => Age::Date(then),
    }
}

/// Seconds until `age` changes, so the label can be redrawn on time
/// (tdesktop's `Timestamp::changes`). `None` once it is a plain date.
pub fn seconds_until_change(when: i64, now: i64, offset_now: i32) -> Option<i64> {
    let elapsed = now.saturating_sub(when).max(0);
    let minutes = elapsed / 60;
    if minutes == 0 {
        Some(61 - elapsed)
    } else if minutes < 60 {
        Some(61 - elapsed % 60)
    } else if elapsed / 3600 < 12 {
        Some(3601 - elapsed % 3600)
    } else {
        let local = now + i64::from(offset_now);
        Some(86_400 - local.rem_euclid(86_400) + 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const NOW: i64 = 1_790_631_720; // 2026-09-28 21:42:00 UTC

    #[test]
    fn age_follows_tdesktop_buckets() {
        assert_eq!(age(NOW - 30, NOW, 0, 0), Age::JustNow);
        assert_eq!(age(NOW + 100, NOW, 0, 0), Age::JustNow);
        assert_eq!(age(NOW - 60, NOW, 0, 0), Age::MinutesAgo(1));
        assert_eq!(age(NOW - 59 * 60, NOW, 0, 0), Age::MinutesAgo(59));
        assert_eq!(age(NOW - 3600, NOW, 0, 0), Age::HoursAgo(1));
        assert_eq!(age(NOW - 11 * 3600, NOW, 0, 0), Age::HoursAgo(11));
        assert!(matches!(age(NOW - 12 * 3600, NOW, 0, 0), Age::Today(_)));
        assert!(matches!(age(NOW - 22 * 3600, NOW, 0, 0), Age::Yesterday(_)));
        assert!(matches!(age(NOW - 3 * 86_400, NOW, 0, 0), Age::Date(_)));
    }

    #[test]
    fn day_buckets_use_the_local_offset() {
        // 21:42 UTC is 00:42 the next day at +3: 12 hours back is yesterday.
        assert!(matches!(
            age(NOW - 12 * 3600, NOW, 3 * 3600, 3 * 3600),
            Age::Yesterday(_)
        ));
    }

    #[test]
    fn change_times_match_the_bucket() {
        assert_eq!(seconds_until_change(NOW - 10, NOW, 0), Some(51));
        assert_eq!(seconds_until_change(NOW - 90, NOW, 0), Some(31));
        assert_eq!(seconds_until_change(NOW - 3700, NOW, 0), Some(3501));
        // 21:42 + 2h18m = midnight, plus one second.
        assert_eq!(
            seconds_until_change(NOW - 13 * 3600, NOW, 0),
            Some(2 * 3600 + 18 * 60 + 1)
        );
    }

    #[test]
    fn badge_only_for_restricted_audiences() {
        let kind = |t: &str| json!({ "@type": t });
        assert_eq!(
            privacy_badge(Some(&kind("storyPrivacySettingsCloseFriends"))),
            Some(PrivacyBadge::CloseFriends)
        );
        assert_eq!(
            privacy_badge(Some(&kind("storyPrivacySettingsContacts"))),
            Some(PrivacyBadge::Contacts)
        );
        assert_eq!(
            privacy_badge(Some(&kind("storyPrivacySettingsSelectedUsers"))),
            Some(PrivacyBadge::SelectedContacts)
        );
        assert_eq!(
            privacy_badge(Some(&kind("storyPrivacySettingsEveryone"))),
            None
        );
        assert_eq!(privacy_badge(None), None);
    }
}
