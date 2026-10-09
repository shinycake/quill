//! Phone-number entry logic for the sign-in screen: the country list from
//! `getCountries`, calling-code lookup, as-you-type international
//! formatting and per-country length validation.
//!
//! tdesktop keeps a separate "country code" and "phone" field and groups
//! the digits with per-country patterns it downloads from the server
//! (`Countries::Groups`). TDLib's `countryInfo` carries no patterns, so a
//! small built-in table covers the common calling codes and everything
//! else falls back to groups of three. The server stays the authority:
//! a number that passes the local check can still be answered with
//! `PHONE_NUMBER_INVALID`.

use std::cmp::Ordering;

/// One row of `getCountries` (`countryInfo`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Country {
    /// ISO 3166-1 alpha-2 code.
    pub iso: String,
    pub name: String,
    pub english_name: String,
    /// Flag emoji, empty when TDLib has none.
    pub flag: String,
    /// Calling codes without the plus sign. May carry an area prefix
    /// (`1876`) for NANP territories.
    pub calling_codes: Vec<String>,
    /// TDLib hides a few entries from pickers (still valid for lookup).
    pub hidden: bool,
}

impl Country {
    /// The calling code shown on the country row ("+44").
    pub fn display_code(&self) -> String {
        self.calling_codes
            .first()
            .map(|code| format!("+{code}"))
            .unwrap_or_default()
    }
}

/// E.164 caps the whole number (country code included) at 15 digits.
pub const MAX_DIGITS: usize = 15;

/// Everything that is not an ASCII digit dropped; also folds the common
/// Unicode digit forms users paste (Arabic-Indic, fullwidth).
pub fn digits_only(input: &str) -> String {
    input
        .chars()
        .filter_map(|ch| match ch {
            '0'..='9' => Some(ch),
            '\u{0660}'..='\u{0669}' => char::from_digit(ch as u32 - 0x0660, 10),
            '\u{06F0}'..='\u{06F9}' => char::from_digit(ch as u32 - 0x06F0, 10),
            '\u{FF10}'..='\u{FF19}' => char::from_digit(ch as u32 - 0xFF10, 10),
            _ => None,
        })
        .collect()
}

/// Calling codes that win a shared code (`1`, `7`, ...) so the default
/// country is the one people expect.
const PREFERRED_ISO: &[(&str, &str)] = &[
    ("1", "US"),
    ("7", "RU"),
    ("44", "GB"),
    ("358", "FI"),
    ("590", "GP"),
    ("262", "RE"),
    ("212", "MA"),
];

/// Built-in codes for when `getCountries` has not answered yet (or failed):
/// enough to format a pasted number from any major region.
const FALLBACK_CODES: &[&str] = &[
    "1", "7", "20", "27", "30", "31", "32", "33", "34", "36", "39", "40", "41", "43", "44", "45",
    "46", "47", "48", "49", "51", "52", "53", "54", "55", "56", "57", "58", "60", "61", "62", "63",
    "64", "65", "66", "81", "82", "84", "86", "90", "91", "92", "93", "94", "95", "98", "212",
    "213", "216", "218", "234", "254", "351", "352", "353", "354", "355", "356", "357", "358",
    "359", "370", "371", "372", "373", "374", "375", "380", "381", "385", "386", "420", "421",
    "852", "853", "880", "886", "961", "962", "963", "964", "966", "971", "972", "973", "974",
    "977", "992", "993", "994", "995", "996", "998",
];

/// Longest known calling code that prefixes `digits`. Looks at the country
/// list first (so `1876` beats `1`), then the built-in table.
pub fn match_calling_code(digits: &str, countries: &[Country]) -> Option<String> {
    let mut best: Option<&str> = None;
    for code in countries
        .iter()
        .flat_map(|country| country.calling_codes.iter().map(String::as_str))
    {
        if !code.is_empty()
            && digits.starts_with(code)
            && best.is_none_or(|current| code.len() > current.len())
        {
            best = Some(code);
        }
    }
    if let Some(code) = best {
        return Some(code.to_string());
    }
    // ITU codes are prefix-free, so the first fallback hit is the only one.
    FALLBACK_CODES
        .iter()
        .find(|code| digits.starts_with(**code))
        .map(|code| (*code).to_string())
}

/// Country shown for a calling code (exact code, else the code's leading
/// digits, e.g. a `1876` number resolves through `1876` first).
pub fn country_for_code<'a>(code: &str, countries: &'a [Country]) -> Option<&'a Country> {
    let matching: Vec<&Country> = countries
        .iter()
        .filter(|country| country.calling_codes.iter().any(|c| c == code))
        .collect();
    if let Some(preferred) = PREFERRED_ISO
        .iter()
        .find(|(cc, _)| *cc == code)
        .and_then(|(_, iso)| matching.iter().find(|country| country.iso == *iso))
    {
        return Some(preferred);
    }
    matching
        .iter()
        .find(|country| !country.hidden)
        .or_else(|| matching.first())
        .copied()
}

/// Country for the digits typed so far: the longest calling code wins.
pub fn country_for_digits<'a>(digits: &str, countries: &'a [Country]) -> Option<&'a Country> {
    let code = match_calling_code(digits, countries)?;
    country_for_code(&code, countries)
}

/// Digit grouping of the national part for a calling code.
fn groups_for(code: &str) -> &'static [usize] {
    match code {
        "1" => &[3, 3, 4],
        "7" => &[3, 3, 2, 2],
        "20" => &[2, 4, 4],
        "33" => &[1, 2, 2, 2, 2],
        "34" | "351" | "380" => &[3, 3, 3],
        "39" => &[3, 3, 4],
        "44" => &[4, 6],
        "49" => &[4, 8],
        "52" => &[3, 3, 4],
        "55" => &[2, 5, 4],
        "61" => &[3, 3, 3],
        "81" | "82" => &[2, 4, 4],
        "86" => &[3, 4, 4],
        "90" => &[3, 3, 4],
        "91" => &[5, 5],
        "971" | "972" => &[2, 3, 4],
        _ => &[3, 3, 3, 3],
    }
}

/// Inclusive range of plausible national-number lengths for a calling code.
fn national_len_range(code: &str) -> (usize, usize) {
    match code {
        "1" | "7" | "52" | "90" | "91" => (10, 10),
        "33" | "34" | "61" | "380" => (9, 9),
        "44" => (9, 10),
        "49" => (7, 12),
        "55" => (10, 11),
        "81" => (9, 10),
        "86" => (11, 11),
        "972" | "971" => (8, 9),
        _ => (4, 12),
    }
}

fn group_digits(national: &str, groups: &[usize]) -> String {
    let mut out = String::new();
    let mut rest = national;
    for (index, size) in groups.iter().enumerate() {
        if rest.is_empty() {
            break;
        }
        if index > 0 {
            out.push(' ');
        }
        let take = if index + 1 == groups.len() {
            rest.len()
        } else {
            (*size).min(rest.len())
        };
        let (head, tail) = rest.split_at(take);
        out.push_str(head);
        rest = tail;
    }
    out
}

/// Result of formatting what the user typed or pasted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Formatted {
    /// Text for the field: `+44 7911 123456`.
    pub text: String,
    /// All digits, country code included.
    pub digits: String,
    /// Calling code recognised at the start of the digits.
    pub calling_code: Option<String>,
}

/// Format `input` as an international number. Always international: a
/// leading `00` is read as `+`, and bare digits are treated as the start of
/// `+<digits>` since TDLib requires the country code.
pub fn format_international(input: &str, countries: &[Country]) -> Formatted {
    let mut digits = digits_only(input);
    let trimmed = input.trim_start();
    if !trimmed.starts_with('+') && digits.starts_with("00") {
        digits.drain(..2);
    }
    digits.truncate(MAX_DIGITS);
    if digits.is_empty() {
        return Formatted {
            text: if input.trim_start().starts_with('+') {
                "+".into()
            } else {
                String::new()
            },
            digits,
            calling_code: None,
        };
    }
    let code = match_calling_code(&digits, countries);
    let text = match &code {
        Some(code) => {
            let national = &digits[code.len()..];
            let groups = groups_for(code);
            if national.is_empty() {
                format!("+{code}")
            } else {
                format!("+{code} {}", group_digits(national, groups))
            }
        }
        None => format!("+{digits}"),
    };
    Formatted {
        text,
        digits,
        calling_code: code,
    }
}

/// Reformat after an edit. Deleting only a separator (space or `+`) would
/// reproduce the same formatted text and trap the cursor, so a shorter
/// result with the same digits drops one more digit.
pub fn reformat_after_edit(previous: &str, edited: &str, countries: &[Country]) -> Formatted {
    let formatted = format_international(edited, countries);
    let prev_digits = digits_only(previous);
    if edited.chars().count() < previous.chars().count()
        && formatted.digits == prev_digits
        && !prev_digits.is_empty()
    {
        let mut shorter = prev_digits;
        shorter.pop();
        return format_international(&shorter, countries);
    }
    formatted
}

/// Why a number cannot be sent yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhoneCheck {
    Valid,
    Empty,
    /// Not enough digits for the detected country.
    TooShort,
    /// More digits than the country allows (or E.164's 15).
    TooLong,
    /// The digits do not start with any known calling code.
    UnknownCountry,
}

impl PhoneCheck {
    /// tdesktop's `lng_bad_phone`, shown only when the user tries to continue.
    pub fn message(self) -> Option<&'static str> {
        match self {
            Self::Valid | Self::Empty => None,
            Self::TooShort | Self::TooLong | Self::UnknownCountry => {
                Some("Invalid phone number. Please try again.")
            }
        }
    }
}

/// Validate formatted digits per detected country.
pub fn validate(formatted: &Formatted) -> PhoneCheck {
    if formatted.digits.is_empty() {
        return PhoneCheck::Empty;
    }
    let Some(code) = &formatted.calling_code else {
        return PhoneCheck::UnknownCountry;
    };
    let national = formatted.digits.len() - code.len();
    let (min, max) = national_len_range(code);
    match national.cmp(&min) {
        Ordering::Less => PhoneCheck::TooShort,
        _ if national > max || formatted.digits.len() > MAX_DIGITS => PhoneCheck::TooLong,
        _ => PhoneCheck::Valid,
    }
}

/// `+15551234567` for TDLib, or `None` while the number is not valid.
pub fn e164(formatted: &Formatted) -> Option<String> {
    (validate(formatted) == PhoneCheck::Valid).then(|| format!("+{}", formatted.digits))
}

/// Case-insensitive word-prefix search over name, English name and code
/// ("uni" finds "United Kingdom", "+44" or "44" finds the UK). Hidden
/// countries never appear. Sorted by name.
pub fn search_countries<'a>(countries: &'a [Country], query: &str) -> Vec<&'a Country> {
    let query = query.trim().to_lowercase();
    let code_query = query.trim_start_matches('+');
    let numeric = !code_query.is_empty() && code_query.chars().all(|c| c.is_ascii_digit());
    let mut out: Vec<&Country> = countries
        .iter()
        .filter(|country| !country.hidden)
        .filter(|country| {
            if query.is_empty() {
                return true;
            }
            if numeric {
                return country
                    .calling_codes
                    .iter()
                    .any(|code| code.starts_with(code_query));
            }
            [&country.name, &country.english_name].iter().any(|name| {
                let name = name.to_lowercase();
                name.split(|c: char| !c.is_alphanumeric())
                    .any(|word| !word.is_empty() && word.starts_with(&query))
                    || name.starts_with(&query)
            })
        })
        .collect();
    out.sort_by_key(|country| country.name.to_lowercase());
    out
}

/// Parse a `getCountries` answer's rows. Unknown or empty rows are skipped.
pub fn countries_from_json(value: &serde_json::Value) -> Vec<Country> {
    let text = |v: &serde_json::Value, key: &str| {
        v.get(key)
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .to_string()
    };
    value
        .get("countries")
        .and_then(serde_json::Value::as_array)
        .map(|rows| {
            rows.iter()
                .filter_map(|row| {
                    let iso = text(row, "country_code");
                    let calling_codes: Vec<String> = row
                        .get("calling_codes")
                        .and_then(serde_json::Value::as_array)
                        .map(|codes| {
                            codes
                                .iter()
                                .filter_map(serde_json::Value::as_str)
                                .filter(|code| {
                                    !code.is_empty() && code.chars().all(|c| c.is_ascii_digit())
                                })
                                .map(str::to_string)
                                .collect()
                        })
                        .unwrap_or_default();
                    if iso.is_empty() || calling_codes.is_empty() {
                        return None;
                    }
                    Some(Country {
                        iso,
                        name: text(row, "name"),
                        english_name: text(row, "english_name"),
                        flag: text(row, "flag_emoji"),
                        calling_codes,
                        hidden: row
                            .get("is_hidden")
                            .and_then(serde_json::Value::as_bool)
                            .unwrap_or(false),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::{
        Country, PhoneCheck, countries_from_json, country_for_digits, digits_only, e164,
        format_international, match_calling_code, reformat_after_edit, search_countries, validate,
    };

    fn country(iso: &str, name: &str, codes: &[&str]) -> Country {
        Country {
            iso: iso.into(),
            name: name.into(),
            english_name: name.into(),
            flag: String::new(),
            calling_codes: codes.iter().map(|c| (*c).into()).collect(),
            hidden: false,
        }
    }

    fn fixtures() -> Vec<Country> {
        vec![
            country("CA", "Canada", &["1"]),
            country("US", "United States", &["1"]),
            country("JM", "Jamaica", &["1876"]),
            country("GB", "United Kingdom", &["44"]),
            country("DE", "Germany", &["49"]),
            country("RU", "Russia", &["7"]),
            country("KZ", "Kazakhstan", &["7"]),
            country("IL", "Israel", &["972"]),
        ]
    }

    #[test]
    fn digits_fold_unicode_and_drop_punctuation() {
        assert_eq!(digits_only("+1 (555) 010-0199"), "15550100199");
        assert_eq!(digits_only("\u{0661}\u{0662}3"), "123");
        assert_eq!(digits_only("\u{FF11}\u{FF12}"), "12");
        assert_eq!(digits_only("abc"), "");
    }

    #[test]
    fn longest_calling_code_wins() {
        let list = fixtures();
        assert_eq!(
            match_calling_code("18765550100", &list).as_deref(),
            Some("1876")
        );
        assert_eq!(
            match_calling_code("15550100199", &list).as_deref(),
            Some("1")
        );
        assert_eq!(
            match_calling_code("972501234567", &list).as_deref(),
            Some("972")
        );
        assert_eq!(match_calling_code("0123", &list), None);
        // Fallback table when the list has not loaded.
        assert_eq!(
            match_calling_code("4915112345678", &[]).as_deref(),
            Some("49")
        );
    }

    #[test]
    fn shared_codes_prefer_the_expected_country() {
        let list = fixtures();
        assert_eq!(country_for_digits("15550100199", &list).unwrap().iso, "US");
        assert_eq!(country_for_digits("79990000000", &list).unwrap().iso, "RU");
        assert_eq!(country_for_digits("18765550100", &list).unwrap().iso, "JM");
    }

    #[test]
    fn formats_as_you_type() {
        let list = fixtures();
        let steps = [
            ("", ""),
            ("+", "+"),
            ("1", "+1"),
            ("15", "+1 5"),
            ("1555", "+1 555"),
            ("15550", "+1 555 0"),
            ("1555010", "+1 555 010"),
            ("15550100", "+1 555 010 0"),
            ("15550100199", "+1 555 010 0199"),
            ("447911123456", "+44 7911 123456"),
        ];
        for (typed, expected) in steps {
            assert_eq!(format_international(typed, &list).text, expected, "{typed}");
        }
    }

    #[test]
    fn pasted_international_forms_normalise() {
        let list = fixtures();
        for pasted in [
            "+1 (555) 010-0199",
            "001-555-010-0199",
            "+1.555.010.0199",
            "  +15550100199  ",
            "+1\u{a0}555\u{a0}010\u{a0}0199",
        ] {
            let formatted = format_international(pasted, &list);
            assert_eq!(formatted.text, "+1 555 010 0199", "{pasted}");
            assert_eq!(e164(&formatted).as_deref(), Some("+15550100199"));
        }
    }

    #[test]
    fn caps_at_fifteen_digits() {
        let list = fixtures();
        let formatted = format_international("+1 5550100199 99999", &list);
        assert_eq!(formatted.digits.len(), 15);
        assert_eq!(validate(&formatted), PhoneCheck::TooLong);
    }

    #[test]
    fn backspace_over_a_separator_removes_a_digit() {
        let list = fixtures();
        // "+1 555" with the space deleted -> "+1555" would re-format to the
        // same text; the digit before the space goes instead.
        let formatted = reformat_after_edit("+1 555", "+1555", &list);
        assert_eq!(formatted.text, "+1 55");
        // A normal edit that changes digits is untouched.
        assert_eq!(
            reformat_after_edit("+1 555", "+1 5555", &list).text,
            "+1 555 5"
        );
        // Deleting the plus of a bare "+1" removes the digit instead.
        assert_eq!(reformat_after_edit("+1", "1", &list).text, "");
    }

    #[test]
    fn validation_is_per_country() {
        let list = fixtures();
        let check = |input: &str| validate(&format_international(input, &list));
        assert_eq!(check(""), PhoneCheck::Empty);
        assert_eq!(check("+"), PhoneCheck::Empty);
        assert_eq!(check("+1 555 010"), PhoneCheck::TooShort);
        assert_eq!(check("+1 555 010 0199"), PhoneCheck::Valid);
        assert_eq!(check("+1 555 010 01999"), PhoneCheck::TooLong);
        assert_eq!(check("+44 7911 123456"), PhoneCheck::Valid);
        assert_eq!(check("+44 791 112345"), PhoneCheck::Valid);
        assert_eq!(check("+44 79"), PhoneCheck::TooShort);
        assert_eq!(check("+972 50 123 45678"), PhoneCheck::TooLong);
        assert_eq!(check("+972 50 123 456"), PhoneCheck::Valid);
        assert_eq!(check("+49 30 1234567"), PhoneCheck::Valid);
        assert_eq!(check("+0 555 010 0199"), PhoneCheck::UnknownCountry);
        assert!(PhoneCheck::TooShort.message().is_some());
        assert!(PhoneCheck::Valid.message().is_none());
        assert!(PhoneCheck::Empty.message().is_none());
    }

    #[test]
    fn search_matches_words_codes_and_skips_hidden() {
        let mut list = fixtures();
        list.push(Country {
            hidden: true,
            ..country("XX", "Hiddenland", &["999"])
        });
        let names = |q: &str| -> Vec<String> {
            search_countries(&list, q)
                .iter()
                .map(|c| c.iso.clone())
                .collect()
        };
        assert_eq!(names("").len(), 8);
        assert_eq!(names("uni"), ["GB", "US"]);
        assert_eq!(names("KINGDOM"), ["GB"]);
        assert_eq!(names("+44"), ["GB"]);
        assert_eq!(names("97"), ["IL"]);
        assert_eq!(names("hidden"), Vec::<String>::new());
        assert_eq!(names("zzz"), Vec::<String>::new());
    }

    #[test]
    fn parses_get_countries_rows() {
        let value = serde_json::json!({"@type":"countries","countries":[
            {"@type":"countryInfo","country_code":"GB","name":"United Kingdom","english_name":"United Kingdom","flag_emoji":"G","is_hidden":false,"calling_codes":["44"]},
            {"@type":"countryInfo","country_code":"","name":"Bad","calling_codes":["1"]},
            {"@type":"countryInfo","country_code":"ZZ","name":"NoCodes","calling_codes":[]},
            {"@type":"countryInfo","country_code":"JM","name":"Jamaica","calling_codes":["1876","x1"],"is_hidden":true}
        ]});
        let parsed = countries_from_json(&value);
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].display_code(), "+44");
        assert_eq!(parsed[1].calling_codes, ["1876"]);
        assert!(parsed[1].hidden);
        assert!(countries_from_json(&serde_json::json!({})).is_empty());
    }
}
