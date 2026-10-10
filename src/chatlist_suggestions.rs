//! The suggestions block above the chat list (pure, no UI).
//!
//! Telegram Desktop shows one suggestion at a time on top of the chat list
//! (`dialogs/dialogs_top_bar_suggestion.cpp`, `dialogs/suggestions/*`): a
//! contact's birthday, "Add your birthday", "Add your photo", "Is {phone}
//! still your number?", the password check, or a Premium offer, each with
//! a dismiss cross. TDLib reports the account-level ones as suggested
//! actions (`updateSuggestedActions`) and the birthdays as
//! `updateContactCloseBirthdays` (yesterday, today and tomorrow).

use std::collections::BTreeSet;

pub const ACTION_BIRTHDATE: &str = "suggestedActionSetBirthdate";
pub const ACTION_PHOTO: &str = "suggestedActionSetProfilePhoto";
pub const ACTION_PHONE: &str = "suggestedActionCheckPhoneNumber";
pub const ACTION_PASSWORD: &str = "suggestedActionCheckPassword";
pub const ACTION_PREMIUM: &str = "suggestedActionUpgradePremium";

/// One entry of `updateContactCloseBirthdays`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CloseBirthday {
    pub user_id: i64,
    pub day: u8,
    pub month: u8,
    pub year: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Suggestion {
    /// Contacts whose birthday is today (user ids).
    Birthdays(Vec<i64>),
    SetBirthdate,
    SetProfilePhoto,
    CheckPhone,
    CheckPassword,
    UpgradePremium,
}

/// What the account knows right now.
#[derive(Debug, Clone, Default)]
pub struct SuggestionFacts {
    pub actions: BTreeSet<String>,
    pub close_birthdays: Vec<CloseBirthday>,
    /// The user dismissed the birthday suggestion (`hideContactCloseBirthdays`).
    pub birthdays_hidden: bool,
}

/// Contacts whose birthday falls on `month`/`day`.
pub fn today_birthdays(list: &[CloseBirthday], month: u8, day: u8) -> Vec<i64> {
    list.iter()
        .filter(|b| b.month == month && b.day == day)
        .map(|b| b.user_id)
        .collect()
}

/// The suggestion to show, if any. The account-safety ones come first,
/// then birthdays, the profile nudges and the Premium offer.
pub fn pick(facts: &SuggestionFacts, month: u8, day: u8) -> Option<Suggestion> {
    let has = |name: &str| facts.actions.contains(name);
    if has(ACTION_PHONE) {
        return Some(Suggestion::CheckPhone);
    }
    if has(ACTION_PASSWORD) {
        return Some(Suggestion::CheckPassword);
    }
    if !facts.birthdays_hidden {
        let today = today_birthdays(&facts.close_birthdays, month, day);
        if !today.is_empty() {
            return Some(Suggestion::Birthdays(today));
        }
    }
    if has(ACTION_BIRTHDATE) {
        return Some(Suggestion::SetBirthdate);
    }
    if has(ACTION_PHOTO) {
        return Some(Suggestion::SetProfilePhoto);
    }
    has(ACTION_PREMIUM).then_some(Suggestion::UpgradePremium)
}

/// Title and subtitle (tdesktop `lng_dialogs_suggestions_*` and
/// `lng_settings_suggestion_*`). `names` are the birthday contacts' names,
/// `phone` is the account's number.
pub fn copy(suggestion: &Suggestion, names: &[String], phone: &str) -> (String, String) {
    match suggestion {
        Suggestion::Birthdays(ids) if ids.len() == 1 => (
            format!(
                "It’s {}'s birthday today! 🎂",
                names.first().map_or("a contact", String::as_str)
            ),
            "Send them a Gift.".into(),
        ),
        Suggestion::Birthdays(ids) => (
            format!("{} contacts have birthdays today! 🎂", ids.len()),
            "Send them a Gift.".into(),
        ),
        Suggestion::SetBirthdate => (
            "Add your birthday! 🎂".into(),
            "Let your contacts know when you’re celebrating.".into(),
        ),
        Suggestion::SetProfilePhoto => (
            "Add your photo! 📸".into(),
            "Help your friends spot you easily.".into(),
        ),
        Suggestion::CheckPhone => (
            format!("Is +{} still your number?", phone.trim_start_matches('+')),
            "Keep your number up to date to ensure you can always log into Telegram.".into(),
        ),
        Suggestion::CheckPassword => (
            "Your password".into(),
            "Your account is protected by 2-Step Verification. Do you still remember your password?"
                .into(),
        ),
        Suggestion::UpgradePremium => (
            "Upgrade to Telegram Premium".into(),
            "Switch from monthly to annual payments for Telegram Premium.".into(),
        ),
    }
}

/// The `SuggestedAction` constructor that dismisses the suggestion; `None`
/// for birthdays, which have their own request.
pub fn dismiss_action(suggestion: &Suggestion) -> Option<&'static str> {
    match suggestion {
        Suggestion::Birthdays(_) => None,
        Suggestion::SetBirthdate => Some(ACTION_BIRTHDATE),
        Suggestion::SetProfilePhoto => Some(ACTION_PHOTO),
        Suggestion::CheckPhone => Some(ACTION_PHONE),
        Suggestion::CheckPassword => Some(ACTION_PASSWORD),
        Suggestion::UpgradePremium => Some(ACTION_PREMIUM),
    }
}

/// Label of a birthday in the contacts list: Yesterday / Today / Tomorrow
/// relative to `month`/`day`, else the date.
pub fn birthday_label(b: &CloseBirthday, month: u8, day: u8) -> String {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    if b.month == month && b.day == day {
        return "Today".into();
    }
    // Month edges: the day after the 28th-31st is the 1st of the next
    // month, so compare by shape instead of arithmetic.
    let tomorrow = day_after(month, day);
    let yesterday = day_before(month, day);
    if (b.month, b.day) == tomorrow {
        "Tomorrow".into()
    } else if (b.month, b.day) == yesterday {
        "Yesterday".into()
    } else {
        format!(
            "{} {}",
            MONTHS[usize::from(b.month.clamp(1, 12)) - 1],
            b.day
        )
    }
}

const DAYS_IN_MONTH: [u8; 12] = [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];

fn day_after(month: u8, day: u8) -> (u8, u8) {
    let last = DAYS_IN_MONTH[usize::from(month.clamp(1, 12)) - 1];
    if day < last {
        (month, day + 1)
    } else {
        (month % 12 + 1, 1)
    }
}

fn day_before(month: u8, day: u8) -> (u8, u8) {
    if day > 1 {
        (month, day - 1)
    } else {
        let prev = if month == 1 { 12 } else { month - 1 };
        (prev, DAYS_IN_MONTH[usize::from(prev) - 1])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts(actions: &[&str], birthdays: &[(i64, u8, u8)], hidden: bool) -> SuggestionFacts {
        SuggestionFacts {
            actions: actions.iter().map(|s| s.to_string()).collect(),
            close_birthdays: birthdays
                .iter()
                .map(|&(user_id, month, day)| CloseBirthday {
                    user_id,
                    day,
                    month,
                    year: None,
                })
                .collect(),
            birthdays_hidden: hidden,
        }
    }

    #[test]
    fn nothing_to_suggest() {
        assert_eq!(pick(&facts(&[], &[], false), 3, 14), None);
        // Another action TDLib knows but the block does not show.
        assert_eq!(
            pick(&facts(&["suggestedActionSetPassword"], &[], false), 3, 14),
            None
        );
    }

    #[test]
    fn safety_checks_outrank_everything() {
        let f = facts(
            &[ACTION_PHOTO, ACTION_PASSWORD, ACTION_PHONE],
            &[(5, 3, 14)],
            false,
        );
        assert_eq!(pick(&f, 3, 14), Some(Suggestion::CheckPhone));
        let f = facts(&[ACTION_PHOTO, ACTION_PASSWORD], &[(5, 3, 14)], false);
        assert_eq!(pick(&f, 3, 14), Some(Suggestion::CheckPassword));
    }

    #[test]
    fn only_todays_birthdays_count_and_can_be_hidden() {
        let f = facts(
            &[ACTION_PHOTO],
            &[(5, 3, 13), (6, 3, 14), (7, 3, 14)],
            false,
        );
        assert_eq!(pick(&f, 3, 14), Some(Suggestion::Birthdays(vec![6, 7])));
        let hidden = facts(&[ACTION_PHOTO], &[(6, 3, 14)], true);
        assert_eq!(pick(&hidden, 3, 14), Some(Suggestion::SetProfilePhoto));
        let tomorrow_only = facts(&[], &[(6, 3, 15)], false);
        assert_eq!(pick(&tomorrow_only, 3, 14), None);
    }

    #[test]
    fn profile_nudges_then_premium() {
        let all = facts(
            &[ACTION_PREMIUM, ACTION_PHOTO, ACTION_BIRTHDATE],
            &[],
            false,
        );
        assert_eq!(pick(&all, 1, 1), Some(Suggestion::SetBirthdate));
        let rest = facts(&[ACTION_PREMIUM, ACTION_PHOTO], &[], false);
        assert_eq!(pick(&rest, 1, 1), Some(Suggestion::SetProfilePhoto));
        let premium = facts(&[ACTION_PREMIUM], &[], false);
        assert_eq!(pick(&premium, 1, 1), Some(Suggestion::UpgradePremium));
    }

    #[test]
    fn copy_names_the_contact_or_counts_them() {
        let one = copy(&Suggestion::Birthdays(vec![1]), &["Ada".into()], "");
        assert_eq!(one.0, "It’s Ada's birthday today! 🎂");
        let many = copy(&Suggestion::Birthdays(vec![1, 2, 3]), &[], "");
        assert_eq!(many.0, "3 contacts have birthdays today! 🎂");
        let phone = copy(&Suggestion::CheckPhone, &[], "15550100");
        assert_eq!(phone.0, "Is +15550100 still your number?");
        let plus = copy(&Suggestion::CheckPhone, &[], "+15550100");
        assert_eq!(plus.0, "Is +15550100 still your number?");
    }

    #[test]
    fn dismissals_name_the_action() {
        assert_eq!(dismiss_action(&Suggestion::Birthdays(vec![1])), None);
        assert_eq!(
            dismiss_action(&Suggestion::SetProfilePhoto),
            Some(ACTION_PHOTO)
        );
        assert_eq!(dismiss_action(&Suggestion::CheckPhone), Some(ACTION_PHONE));
    }

    #[test]
    fn birthday_labels_wrap_month_edges() {
        let b = |month, day| CloseBirthday {
            user_id: 1,
            day,
            month,
            year: None,
        };
        assert_eq!(birthday_label(&b(3, 14), 3, 14), "Today");
        assert_eq!(birthday_label(&b(3, 15), 3, 14), "Tomorrow");
        assert_eq!(birthday_label(&b(3, 13), 3, 14), "Yesterday");
        assert_eq!(birthday_label(&b(1, 1), 12, 31), "Tomorrow");
        assert_eq!(birthday_label(&b(12, 31), 1, 1), "Yesterday");
        assert_eq!(birthday_label(&b(3, 1), 2, 29), "Tomorrow");
        assert_eq!(birthday_label(&b(7, 4), 3, 14), "Jul 4");
    }
}
