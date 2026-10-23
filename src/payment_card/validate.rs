//! Card number, expiry and CVC checks: a port of Telegram Desktop's
//! `stripe/stripe_card_validator.cpp`, which both native providers use.
#![deny(clippy::indexing_slicing)]

use std::fmt::Write as _;
use zeroize::Zeroizing;

/// The card networks the BIN table knows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardBrand {
    Unknown,
    Amex,
    DinersClub,
    Discover,
    Jcb,
    MasterCard,
    UnionPay,
    Visa,
}

impl CardBrand {
    /// The brand name Stripe's SDK prints (`CardBrandToString`), lowercased
    /// for a credential title the way the server stores saved ones.
    pub fn title_name(self) -> &'static str {
        match self {
            Self::Amex => "american express",
            Self::DinersClub => "diners club",
            Self::Discover => "discover",
            Self::Jcb => "jcb",
            Self::MasterCard => "mastercard",
            Self::Unknown | Self::UnionPay => "unknown",
            Self::Visa => "visa",
        }
    }

    /// `BrandFromString` over the `brand` of a Stripe card object.
    pub fn from_stripe_name(name: &str) -> Self {
        match name.to_lowercase().as_str() {
            "visa" => Self::Visa,
            "american express" => Self::Amex,
            "mastercard" => Self::MasterCard,
            "discover" => Self::Discover,
            "jcb" => Self::Jcb,
            "diners club" => Self::DinersClub,
            _ => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationState {
    Invalid,
    Incomplete,
    Valid,
}

/// A check's answer. `finished` means the field cannot take more input,
/// so focus moves on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Validation {
    pub state: ValidationState,
    pub finished: bool,
}

impl Validation {
    const INVALID: Self = Self::new(ValidationState::Invalid, false);
    const INCOMPLETE: Self = Self::new(ValidationState::Incomplete, false);

    const fn new(state: ValidationState, finished: bool) -> Self {
        Self { state, finished }
    }

    pub fn is_valid(self) -> bool {
        self.state == ValidationState::Valid
    }
}

/// The current month, against which an expiry date is checked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct YearMonth {
    pub year: u32,
    pub month: u32,
}

struct BinRange {
    low: &'static str,
    high: &'static str,
    length: usize,
    brand: CardBrand,
}

const fn range(low: &'static str, high: &'static str, length: usize, brand: CardBrand) -> BinRange {
    BinRange {
        low,
        high,
        length,
        brand,
    }
}

const MIN_CVC_LENGTH: usize = 3;

const UNKNOWN: BinRange = range("", "", 19, CardBrand::Unknown);

/// tdesktop's `AllRanges`, in its order: among equally specific ranges the
/// last one wins, as `std::sort` + `back()` picks there.
const RANGES: &[BinRange] = &[
    UNKNOWN,
    range("34", "34", 15, CardBrand::Amex),
    range("37", "37", 15, CardBrand::Amex),
    range("30", "30", 16, CardBrand::DinersClub),
    range("36", "36", 14, CardBrand::DinersClub),
    range("38", "39", 16, CardBrand::DinersClub),
    range("60", "60", 16, CardBrand::Discover),
    range("64", "65", 16, CardBrand::Discover),
    range("35", "35", 16, CardBrand::Jcb),
    range("50", "59", 16, CardBrand::MasterCard),
    range("22", "27", 16, CardBrand::MasterCard),
    range("67", "67", 16, CardBrand::MasterCard),
    range("62", "62", 16, CardBrand::UnionPay),
    range("81", "81", 16, CardBrand::UnionPay),
    range("40", "49", 16, CardBrand::Visa),
    range("413600", "413600", 13, CardBrand::Visa),
    range("444509", "444509", 13, CardBrand::Visa),
    range("444550", "444550", 13, CardBrand::Visa),
    range("450603", "450603", 13, CardBrand::Visa),
    range("450617", "450617", 13, CardBrand::Visa),
    range("450628", "450629", 13, CardBrand::Visa),
    range("450636", "450636", 13, CardBrand::Visa),
    range("450640", "450641", 13, CardBrand::Visa),
    range("450662", "450662", 13, CardBrand::Visa),
    range("463100", "463100", 13, CardBrand::Visa),
    range("476142", "476142", 13, CardBrand::Visa),
    range("476143", "476143", 13, CardBrand::Visa),
    range("492901", "492902", 13, CardBrand::Visa),
    range("492920", "492920", 13, CardBrand::Visa),
    range("492923", "492923", 13, CardBrand::Visa),
    range("492928", "492930", 13, CardBrand::Visa),
    range("492937", "492937", 13, CardBrand::Visa),
    range("492939", "492939", 13, CardBrand::Visa),
    range("492960", "492960", 13, CardBrand::Visa),
];

const MIN_LENGTH: usize = 13;
const MAX_LENGTH: usize = 19;

/// Qt's `toInt` on a short digit run; an empty run is 0.
fn prefix_number(digits: &str, len: usize) -> u64 {
    digits
        .chars()
        .take(len)
        .filter_map(|c| c.to_digit(10))
        .fold(0, |acc, d| {
            acc.saturating_mul(10).saturating_add(u64::from(d))
        })
}

fn matches(range: &BinRange, digits: &str) -> bool {
    let count = digits.chars().count();
    let low_len = count.min(range.low.len());
    if prefix_number(digits, low_len) < prefix_number(range.low, low_len) {
        return false;
    }
    let high_len = count.min(range.high.len());
    prefix_number(digits, high_len) <= prefix_number(range.high, high_len)
}

fn is_numeric(value: &str) -> bool {
    value.chars().all(|c| c.is_ascii_digit())
}

/// The text without spaces, wiped when dropped: it is a copy of the card.
fn remove_whitespace(value: &str) -> Zeroizing<String> {
    let kept = value.chars().filter(|c| !c.is_whitespace());
    let mut digits = Zeroizing::new(String::with_capacity(
        kept.clone().map(char::len_utf8).sum(),
    ));
    digits.extend(kept);
    digits
}

fn most_specific(digits: &str) -> &'static BinRange {
    static FALLBACK: BinRange = UNKNOWN;
    if digits.is_empty() {
        return &FALLBACK;
    }
    RANGES
        .iter()
        .filter(|range| matches(range, digits))
        .max_by_key(|range| range.low.len())
        .unwrap_or(&FALLBACK)
}

/// `BrandForNumber`: a brand only when exactly one network fits.
pub fn brand_for_number(number: &str) -> CardBrand {
    let digits = remove_whitespace(number);
    if !is_numeric(&digits) {
        return CardBrand::Unknown;
    }
    let mut found: Vec<CardBrand> = Vec::new();
    for range in RANGES.iter().filter(|range| matches(range, &digits)) {
        if range.brand != CardBrand::Unknown && !found.contains(&range.brand) {
            found.push(range.brand);
        }
    }
    match found.as_slice() {
        [brand] => *brand,
        _ => CardBrand::Unknown,
    }
}

/// The Luhn checksum over a digit string.
pub fn is_valid_luhn(digits: &str) -> bool {
    let mut sum = 0u32;
    for (index, c) in digits.chars().rev().enumerate() {
        let mut digit = c.to_digit(10).unwrap_or(0);
        if index % 2 == 1 {
            digit *= 2;
            if digit > 9 {
                digit -= 9;
            }
        }
        sum += digit;
    }
    sum.is_multiple_of(10)
}

/// `ValidateCard`. Finished only at the longest length any network uses.
pub fn validate_card(number: &str) -> Validation {
    let digits = remove_whitespace(number);
    if !is_numeric(&digits) {
        return Validation::INVALID;
    }
    if digits.is_empty() {
        return Validation::INCOMPLETE;
    }
    let len = digits.chars().count();
    if len > MAX_LENGTH || (len >= MIN_LENGTH && !is_valid_luhn(&digits)) {
        Validation::INVALID
    } else if len < MIN_LENGTH {
        Validation::INCOMPLETE
    } else {
        Validation::new(ValidationState::Valid, len == MAX_LENGTH)
    }
}

/// `ValidateExpireDate` over the field text ("MM/YY", any partial form).
pub fn validate_expiry(date: &str, today: YearMonth) -> Validation {
    let date = remove_whitespace(date);
    let typed = || date.chars().filter(|c| *c != '/');
    if !typed().all(|c| c.is_ascii_digit()) {
        return Validation::INVALID;
    }
    if typed().nth(1).is_none() {
        return Validation::INCOMPLETE;
    }
    // Months 2-9 typed as one digit take a leading zero.
    let lead = typed().next().is_some_and(|first| first > '1');
    let mut normalized = Zeroizing::new(String::with_capacity(date.len() + 1));
    normalized.extend(lead.then_some('0'));
    normalized.extend(typed());
    let month = prefix_number(&normalized, 2);
    if !(1..=12).contains(&month) {
        return Validation::INVALID;
    }
    match normalized.chars().count() {
        0..=3 => return Validation::INCOMPLETE,
        4 => {}
        _ => return Validation::INVALID,
    }
    let year = 2000 + prefix_number(normalized.get(2..).unwrap_or(""), 2);
    let (today_year, today_month) = (u64::from(today.year), u64::from(today.month));
    if year < today_year || (year == today_year && month < today_month) {
        return Validation::INVALID;
    }
    Validation::new(ValidationState::Valid, true)
}

/// `ValidateParsedExpireDate`: the month and four-digit year the form
/// sends.
pub fn validate_parsed_expiry(month: u32, year: u32, today: YearMonth) -> ValidationState {
    if year / 100 != 20 {
        return ValidationState::Invalid;
    }
    let mut date = Zeroizing::new(String::with_capacity(8));
    let _ = write!(date, "{month:02}{:02}", year % 100);
    validate_expiry(&date, today).state
}

fn max_cvc_length(brand: CardBrand) -> usize {
    match brand {
        CardBrand::Amex | CardBrand::Unknown => 4,
        _ => 3,
    }
}

/// `ValidateCvc`: three digits, four for Amex and unknown networks.
pub fn validate_cvc(number: &str, cvc: &str) -> Validation {
    if !is_numeric(cvc) {
        return Validation::INVALID;
    }
    let len = cvc.chars().count();
    if len < MIN_CVC_LENGTH {
        return Validation::INCOMPLETE;
    }
    let max = max_cvc_length(brand_for_number(number));
    if len > max {
        return Validation::INVALID;
    }
    Validation::new(ValidationState::Valid, len == max)
}

/// `CardNumberFormat`: digit group sizes for the number field.
pub fn card_number_groups(number: &str) -> &'static [usize] {
    const DEFAULT: &[usize] = &[4, 4, 4, 4];
    let digits = remove_whitespace(number);
    if !is_numeric(&digits) {
        return DEFAULT;
    }
    let range = most_specific(&digits);
    match range.brand {
        CardBrand::DinersClub if range.length == 14 => &[4, 6, 4],
        CardBrand::Amex => &[4, 6, 5],
        _ => DEFAULT,
    }
}
