//! B10: pure helpers for the profile and contact panels — birthday form
//! validation and the copyable profile link.

/// First year Telegram accepts in a birthday (tdesktop `Ui::BirthdayYears`
/// starts the picker at 1900).
pub const FIRST_BIRTHDAY_YEAR: i32 = 1900;

/// A validated birthday: `(day, month, optional year)`.
pub type BirthdayParts = (u8, u8, Option<i32>);

/// Parse the birthday form's three fields. The year may stay empty (the
/// birthday is then shown without an age); day and month are required.
/// The day must exist in the month (29 February is accepted without a
/// year, or in a leap year). `current_year` bounds the year from above.
pub fn parse_birthday(
    day: &str,
    month: &str,
    year: &str,
    current_year: i32,
) -> Result<BirthdayParts, &'static str> {
    let day: u8 = day.trim().parse().map_err(|_| "Enter the day as a number.")?;
    let month: u8 = month
        .trim()
        .parse()
        .map_err(|_| "Enter the month as a number from 1 to 12.")?;
    if !(1..=12).contains(&month) {
        return Err("The month must be from 1 to 12.");
    }
    let year = match year.trim() {
        "" => None,
        text => {
            let year: i32 = text.parse().map_err(|_| "Enter the year as a number.")?;
            if !(FIRST_BIRTHDAY_YEAR..=current_year).contains(&year) {
                return Err("That year is out of range.");
            }
            Some(year)
        }
    };
    let longest = match month {
        2 => match year {
            Some(year) if !is_leap_year(year) => 28,
            _ => 29,
        },
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    };
    if day == 0 || day > longest {
        return Err("That day does not exist in the month.");
    }
    Ok((day, month, year))
}

fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

/// The `t.me` link copied from the profile's link row.
pub fn profile_link(username: &str) -> String {
    format!("https://t.me/{}", username.trim_start_matches('@'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn birthday_accepts_a_missing_year() {
        assert_eq!(parse_birthday("9", "10", "", 2026), Ok((9, 10, None)));
        assert_eq!(
            parse_birthday(" 9 ", "10", "1990", 2026),
            Ok((9, 10, Some(1990)))
        );
    }

    #[test]
    fn birthday_rejects_impossible_dates() {
        assert!(parse_birthday("31", "4", "", 2026).is_err());
        assert!(parse_birthday("0", "4", "", 2026).is_err());
        assert!(parse_birthday("1", "13", "", 2026).is_err());
        assert!(parse_birthday("x", "1", "", 2026).is_err());
        assert!(parse_birthday("1", "1", "1899", 2026).is_err());
        assert!(parse_birthday("1", "1", "2027", 2026).is_err());
    }

    #[test]
    fn leap_day_needs_a_leap_year_when_the_year_is_given() {
        assert!(parse_birthday("29", "2", "", 2026).is_ok());
        assert!(parse_birthday("29", "2", "2000", 2026).is_ok());
        assert!(parse_birthday("29", "2", "1900", 2026).is_err());
        assert!(parse_birthday("29", "2", "2023", 2026).is_err());
    }

    #[test]
    fn profile_link_strips_the_at_sign() {
        assert_eq!(profile_link("@ada"), "https://t.me/ada");
        assert_eq!(profile_link("ada"), "https://t.me/ada");
    }
}
