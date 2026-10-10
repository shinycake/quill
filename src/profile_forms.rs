//! B10: pure helpers for the profile and contact panels — birthday form
//! validation and the copyable profile link.

/// Why a profile photo is reported (`reportChatPhoto`). tdesktop's report
/// box offers the same `ReportReason` constructors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhotoReportReason {
    Spam,
    Violence,
    Pornography,
    ChildAbuse,
    IllegalDrugs,
    PersonalDetails,
    Fake,
    Other,
}

impl PhotoReportReason {
    pub const ALL: [PhotoReportReason; 8] = [
        PhotoReportReason::Spam,
        PhotoReportReason::Violence,
        PhotoReportReason::Pornography,
        PhotoReportReason::ChildAbuse,
        PhotoReportReason::IllegalDrugs,
        PhotoReportReason::PersonalDetails,
        PhotoReportReason::Fake,
        PhotoReportReason::Other,
    ];

    /// The `ReportReason` constructor.
    pub fn td_type(self) -> &'static str {
        match self {
            PhotoReportReason::Spam => "reportReasonSpam",
            PhotoReportReason::Violence => "reportReasonViolence",
            PhotoReportReason::Pornography => "reportReasonPornography",
            PhotoReportReason::ChildAbuse => "reportReasonChildAbuse",
            PhotoReportReason::IllegalDrugs => "reportReasonIllegalDrugs",
            PhotoReportReason::PersonalDetails => "reportReasonPersonalDetails",
            PhotoReportReason::Fake => "reportReasonFake",
            PhotoReportReason::Other => "reportReasonCustom",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            PhotoReportReason::Spam => "Spam",
            PhotoReportReason::Violence => "Violence",
            PhotoReportReason::Pornography => "Pornography",
            PhotoReportReason::ChildAbuse => "Child abuse",
            PhotoReportReason::IllegalDrugs => "Illegal drugs",
            PhotoReportReason::PersonalDetails => "Personal details",
            PhotoReportReason::Fake => "Fake account",
            PhotoReportReason::Other => "Other",
        }
    }
}

/// What to do with a photo picked for a contact (tdesktop's userpic menu:
/// "Set Profile Photo", "Suggest Profile Photo", "Reset to Original").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PersonalPhotoMode {
    /// `setUserPersonalProfilePhoto`: only you see it.
    Set,
    /// `suggestUserProfilePhoto`: the contact may adopt it.
    Suggest,
    /// `setUserPersonalProfilePhoto` with no photo.
    Reset,
}

impl PersonalPhotoMode {
    pub fn title(self) -> &'static str {
        match self {
            PersonalPhotoMode::Set => "Set Profile Photo",
            PersonalPhotoMode::Suggest => "Suggest Profile Photo",
            PersonalPhotoMode::Reset => "Reset to Original",
        }
    }

    /// Confirmation text (`lng_profile_set_personal_sure`,
    /// `lng_profile_suggest_sure`, `lng_profile_photo_reset_sure`).
    pub fn confirm_text(self, name: &str) -> String {
        match self {
            PersonalPhotoMode::Set => format!(
                "Only you will see this photo and it will replace any photo {name} sets for themselves."
            ),
            PersonalPhotoMode::Suggest => {
                format!("You can suggest {name} to set this photo for their Telegram profile.")
            }
            PersonalPhotoMode::Reset => {
                format!("Are you sure you want to reset {name}'s photo to the original?")
            }
        }
    }

    pub fn button(self) -> &'static str {
        match self {
            PersonalPhotoMode::Set => "Set Photo",
            PersonalPhotoMode::Suggest => "Suggest",
            PersonalPhotoMode::Reset => "Reset",
        }
    }

    /// Whether a picked file is part of the flow.
    pub fn needs_file(self) -> bool {
        self != PersonalPhotoMode::Reset
    }

    pub fn done_note(self) -> &'static str {
        match self {
            PersonalPhotoMode::Set => "Photo updated",
            PersonalPhotoMode::Suggest => "Photo suggested",
            PersonalPhotoMode::Reset => "Photo reset",
        }
    }
}

/// A file TDLib can take as a profile photo (`inputChatPhotoStatic`).
pub fn is_profile_photo_file(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| {
            ["jpg", "jpeg", "png", "webp"]
                .iter()
                .any(|known| ext.eq_ignore_ascii_case(known))
        })
}

/// The unofficial-client notice on a profile (`lng_profile_unofficial_warning`).
pub fn unofficial_warning_text(first_name: &str) -> String {
    format!(
        "{first_name} uses an unofficial Telegram client. Messages to this user may be less secure."
    )
}

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
    let day: u8 = day
        .trim()
        .parse()
        .map_err(|_| "Enter the day as a number.")?;
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
    use super::{
        PersonalPhotoMode, PhotoReportReason, is_profile_photo_file, unofficial_warning_text,
    };
    use std::path::Path;

    #[test]
    fn personal_photo_modes_say_what_will_happen() {
        assert!(
            PersonalPhotoMode::Set
                .confirm_text("Ada")
                .contains("Only you")
        );
        assert!(
            PersonalPhotoMode::Suggest
                .confirm_text("Ada")
                .contains("suggest Ada")
        );
        assert!(
            PersonalPhotoMode::Reset
                .confirm_text("Ada")
                .contains("Ada's photo")
        );
        assert!(PersonalPhotoMode::Set.needs_file());
        assert!(PersonalPhotoMode::Suggest.needs_file());
        assert!(!PersonalPhotoMode::Reset.needs_file());
    }

    #[test]
    fn only_still_images_are_profile_photos() {
        assert!(is_profile_photo_file(Path::new("/a/b.JPG")));
        assert!(is_profile_photo_file(Path::new("c.webp")));
        assert!(!is_profile_photo_file(Path::new("c.gif")));
        assert!(!is_profile_photo_file(Path::new("noext")));
    }

    #[test]
    fn unofficial_warning_names_the_user() {
        let text = unofficial_warning_text("Ada");
        assert!(text.starts_with("Ada uses an unofficial Telegram client"));
    }

    #[test]
    fn report_reasons_map_to_distinct_constructors() {
        let mut seen = std::collections::HashSet::new();
        for reason in PhotoReportReason::ALL {
            assert!(reason.td_type().starts_with("reportReason"));
            assert!(seen.insert(reason.td_type()));
            assert!(!reason.label().is_empty());
        }
    }

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
