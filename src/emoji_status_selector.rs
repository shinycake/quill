//! The emoji status selector's rules that need no UI (tdesktop
//! `info/profile/info_profile_emoji_status_panel.cpp`, the status mode of
//! `chat_helpers/emoji_list_widget.cpp`, `Ui::TimePickerBox`).
//!
//! The selector lists a "remove" cell first, then the themed statuses (at
//! most [`FIRST_ROW_LIMIT`]), the recent and default ones, and the status
//! the account has now. A click sets the status with no end date; the
//! right-click menu offers [`MENU_DURATIONS`] and a custom time, which
//! opens the "Set Status for..." box with a drum picker over
//! [`time_picker_values`].
#![deny(clippy::indexing_slicing)]

use crate::lang::{keys, trn};

const HOUR: i32 = 3600;
const DAY: i32 = 86_400;

/// The themed statuses kept in front of the recent and default ones
/// (`kLimitFirstRow - 1`).
pub const FIRST_ROW_LIMIT: usize = 7;

/// The durations of the right-click menu (`fillEmojiStatusMenu`).
pub const MENU_DURATIONS: [i32; 4] = [HOUR, HOUR * 8, DAY, DAY * 7];

/// `Ui::DefaultTimePickerValues`.
pub fn time_picker_values() -> [i32; 16] {
    [
        60 * 15,
        60 * 30,
        HOUR,
        HOUR * 2,
        HOUR * 3,
        HOUR * 4,
        HOUR * 8,
        HOUR * 12,
        DAY,
        DAY * 2,
        DAY * 3,
        DAY * 7,
        DAY * 14,
        DAY * 31,
        DAY * 62,
        DAY * 93,
    ]
}

/// One cell of the leading status section.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatusSlot {
    /// The star cell: removes the status.
    Remove,
    Emoji(i64),
}

/// The leading section (`EmojiStatusPanel::show`): a remove cell, the
/// first themed statuses, then the recent and default ones that are not
/// listed yet, then `current` when it is still missing.
pub fn status_slots(
    themed: &[i64],
    recent: &[i64],
    default: &[i64],
    current: i64,
) -> Vec<StatusSlot> {
    let mut ids: Vec<i64> = Vec::new();
    for &id in themed.iter().filter(|id| **id > 0).take(FIRST_ROW_LIMIT) {
        if !ids.contains(&id) {
            ids.push(id);
        }
    }
    for &id in recent.iter().chain(default).filter(|id| **id > 0) {
        if !ids.contains(&id) {
            ids.push(id);
        }
    }
    if current > 0 && !ids.contains(&current) {
        ids.push(current);
    }
    std::iter::once(StatusSlot::Remove)
        .chain(ids.into_iter().map(StatusSlot::Emoji))
        .collect()
}

/// What choosing a cell does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatusPick {
    /// Set the status; `0` seconds keeps it until it is removed.
    Set {
        id: i64,
        secs: i32,
    },
    Remove,
    /// The account has no Premium: show the Premium preview instead.
    PremiumPreview,
}

/// `EmojiStatusPanel::filter`: removing is free, setting needs Premium.
pub fn pick(id: Option<i64>, secs: i32, premium: bool) -> StatusPick {
    match id {
        None => StatusPick::Remove,
        Some(_) if !premium => StatusPick::PremiumPreview,
        Some(id) => StatusPick::Set {
            id,
            secs: secs.max(0),
        },
    }
}

/// The row a time falls on (`TimePickerBox`): the nearest value, the
/// later one on a tie.
pub fn picker_start_index(values: &[i32], start: i32) -> usize {
    let after = values.partition_point(|v| *v < start);
    let (Some(before), Some(left), Some(right)) = (
        after.checked_sub(1),
        after.checked_sub(1).and_then(|i| values.get(i)),
        values.get(after),
    ) else {
        return after.min(values.len().saturating_sub(1));
    };
    if (start - left).abs() < (start - right).abs() {
        before
    } else {
        after
    }
}

/// `Ui::FormatTTL`.
pub fn format_ttl(secs: i32) -> String {
    if secs < DAY {
        trn(keys::HOURS, secs / HOUR)
    } else if secs < DAY * 7 {
        trn(keys::DAYS, secs / DAY)
    } else if secs < DAY * 31 {
        let days = secs / DAY;
        // tdesktop tests the seconds, not the days, for divisibility.
        if secs % 7 == 0 {
            trn(keys::WEEKS, days / 7)
        } else {
            format!(
                "{} {}",
                trn(keys::WEEKS, days / 7),
                trn(keys::DAYS, days % 7)
            )
        }
    } else if secs <= DAY * 31 * 11 {
        trn(keys::MONTHS, secs / (DAY * 31))
    } else {
        trn(
            keys::YEARS,
            (f64::from(secs) / f64::from(DAY * 365)).round() as i64,
        )
    }
}

/// `Ui::FormatMuteFor`.
pub fn format_mute_for(secs: i32) -> String {
    if secs <= 60 {
        trn(keys::SECONDS, secs)
    } else if secs <= 60 * 59 {
        trn(keys::MINUTES, secs / 60)
    } else {
        format_ttl(secs)
    }
}

/// The end of a status chosen "for `secs`" at `now` (Unix seconds), or
/// `None` when it does not fit the 32-bit field TDLib takes.
pub fn status_until(now_secs: u64, secs: i32) -> Option<i32> {
    crate::main_menu::emoji_status_expiration(now_secs, secs)
}

#[cfg(test)]
mod tests {
    use super::{StatusPick, StatusSlot, *};

    #[test]
    fn slots_start_with_remove_and_dedupe_like_tdesktop() {
        let themed: Vec<i64> = (1..=10).collect();
        let slots = status_slots(&themed, &[3, 20, 21], &[21, 30], 99);
        let ids: Vec<_> = slots
            .iter()
            .map(|s| match s {
                StatusSlot::Remove => 0,
                StatusSlot::Emoji(id) => *id,
            })
            .collect();
        assert_eq!(ids, vec![0, 1, 2, 3, 4, 5, 6, 7, 20, 21, 30, 99]);
    }

    #[test]
    fn current_status_already_listed_is_not_added_twice() {
        let slots = status_slots(&[], &[5], &[6], 5);
        assert_eq!(
            slots,
            vec![
                StatusSlot::Remove,
                StatusSlot::Emoji(5),
                StatusSlot::Emoji(6)
            ]
        );
    }

    #[test]
    fn invalid_ids_are_dropped() {
        let slots = status_slots(&[0, -4], &[0], &[], 0);
        assert_eq!(slots, vec![StatusSlot::Remove]);
    }

    #[test]
    fn picking_applies_the_premium_gate() {
        assert_eq!(pick(None, 0, false), StatusPick::Remove);
        assert_eq!(pick(Some(4), 0, false), StatusPick::PremiumPreview);
        assert_eq!(
            pick(Some(4), 3600, true),
            StatusPick::Set { id: 4, secs: 3600 }
        );
        assert_eq!(pick(Some(4), -5, true), StatusPick::Set { id: 4, secs: 0 });
    }

    #[test]
    fn picker_starts_on_the_nearest_value() {
        let values = time_picker_values();
        assert_eq!(picker_start_index(&values, 0), 0);
        assert_eq!(picker_start_index(&values, 3600), 2);
        assert_eq!(picker_start_index(&values, 3700), 2);
        assert_eq!(picker_start_index(&values, 5400), 3);
        assert_eq!(picker_start_index(&values, i32::MAX), values.len() - 1);
        assert_eq!(picker_start_index(&[], 10), 0);
    }

    #[test]
    fn picker_values_are_sorted_and_start_at_fifteen_minutes() {
        let values = time_picker_values();
        assert_eq!(values[0], 900);
        assert!(values.windows(2).all(|w| w[0] < w[1]));
    }

    #[test]
    fn durations_are_phrased_like_tdesktop() {
        assert_eq!(format_mute_for(900), "15 minutes");
        assert_eq!(format_mute_for(1800), "30 minutes");
        assert_eq!(format_mute_for(3600), "1 hour");
        assert_eq!(format_mute_for(3600 * 8), "8 hours");
        assert_eq!(format_mute_for(DAY), "1 day");
        assert_eq!(format_mute_for(DAY * 7), "1 week");
        assert_eq!(format_mute_for(DAY * 14), "2 weeks");
        assert_eq!(format_mute_for(DAY * 31), "1 month");
        assert_eq!(format_mute_for(DAY * 93), "3 months");
        assert_eq!(format_mute_for(45), "45 seconds");
    }

    #[test]
    fn until_overflow_is_refused() {
        assert_eq!(status_until(1_000, 3_600), Some(4_600));
        assert_eq!(status_until(i32::MAX as u64, 60), None);
    }
}
