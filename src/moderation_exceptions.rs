//! Restricted members as an exceptions list (tdesktop
//! `ParticipantsBoxController` with `Role::Restricted`, and
//! `EditRestrictedBox`): how a member's current end date shows in the
//! list, and where the Duration choice starts when an exception is edited.

use crate::moderation::{MAX_RESTRICT_DAYS, RestrictUntil};

/// True when the end date means "forever": none, or further ahead than
/// the longest limited restriction (tdesktop `IsRestrictedForever`).
pub fn is_forever(until_date: i32, now: i64) -> bool {
    until_date == 0 || i64::from(until_date) > now + MAX_RESTRICT_DAYS * 86_400
}

/// The Duration an edit of an existing exception starts on: Forever when
/// the old date is forever or already over, else that date as a custom
/// time (tdesktop adds the old date as its own choice).
pub fn initial_until(until_date: i32, now: i64) -> RestrictUntil {
    if is_forever(until_date, now) || i64::from(until_date) <= now {
        RestrictUntil::Forever
    } else {
        RestrictUntil::Custom(i64::from(until_date))
    }
}

/// The status text of a restricted row: "restricted until {date}", or
/// "restricted" when it never ends. `format` writes the date and time.
pub fn restricted_status(until_date: i32, now: i64, format: impl Fn(i32) -> String) -> String {
    if is_forever(until_date, now) {
        "restricted".into()
    } else {
        format!("restricted until {}", format(until_date))
    }
}

#[cfg(test)]
mod tests {
    use super::{initial_until, is_forever, restricted_status};
    use crate::moderation::RestrictUntil;

    const NOW: i64 = 1_800_000_000;

    #[test]
    fn zero_and_far_dates_are_forever() {
        assert!(is_forever(0, NOW));
        assert!(is_forever((NOW + 400 * 86_400) as i32, NOW));
        assert!(!is_forever((NOW + 86_400) as i32, NOW));
    }

    #[test]
    fn editing_starts_on_the_old_date_when_it_is_limited() {
        let soon = (NOW + 3 * 86_400) as i32;
        assert_eq!(
            initial_until(soon, NOW),
            RestrictUntil::Custom(i64::from(soon))
        );
        assert_eq!(initial_until(0, NOW), RestrictUntil::Forever);
        assert_eq!(
            initial_until((NOW - 10) as i32, NOW),
            RestrictUntil::Forever
        );
    }

    #[test]
    fn status_names_the_end_date() {
        let soon = (NOW + 86_400) as i32;
        assert_eq!(restricted_status(0, NOW, |_| "x".into()), "restricted");
        assert_eq!(
            restricted_status(soon, NOW, |d| format!("d{d}")),
            format!("restricted until d{soon}")
        );
    }
}
