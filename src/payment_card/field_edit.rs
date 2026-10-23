//! What a card field shows after each edit: the validators of Telegram
//! Desktop's `payments/ui/payments_edit_card.cpp`. The number groups its
//! digits by network, the expiry becomes "MM/YY", and Backspace or Delete
//! next to a separator removes the digit beside it instead.
#![deny(clippy::indexing_slicing)]

use super::validate::{
    Validation, ValidationState, YearMonth, card_number_groups, validate_card, validate_cvc,
    validate_expiry,
};
use zeroize::{Zeroize, ZeroizeOnDrop};

/// A field's text and caret, in characters.
#[derive(Clone, PartialEq, Eq, Zeroize, ZeroizeOnDrop)]
pub struct FieldText {
    value: String,
    cursor: usize,
}

impl FieldText {
    pub fn new(value: impl Into<String>, cursor: usize) -> Self {
        Self {
            value: value.into(),
            cursor,
        }
    }

    pub fn value(&self) -> &str {
        &self.value
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    fn chars(&self) -> Vec<char> {
        self.value.chars().collect()
    }
}

/// The text, caret and selection anchor just before a Backspace or
/// Delete, so the edit can be told apart from typing.
#[derive(Clone, PartialEq, Eq, Zeroize, ZeroizeOnDrop)]
pub struct EditSnapshot {
    text: FieldText,
    anchor: usize,
}

impl EditSnapshot {
    pub fn new(value: impl Into<String>, cursor: usize, anchor: usize) -> Self {
        Self {
            text: FieldText::new(value, cursor),
            anchor,
        }
    }
}

/// What the field shows after the edit, and whether it is invalid (shown
/// as an error) or finished (focus moves to the next field).
#[derive(Clone, PartialEq, Eq, Zeroize, ZeroizeOnDrop)]
pub struct FieldResult {
    text: FieldText,
    invalid: bool,
    finished: bool,
}

impl FieldResult {
    fn from_validation(text: FieldText, validation: Validation) -> Self {
        Self {
            text,
            invalid: validation.state == ValidationState::Invalid,
            finished: validation.finished,
        }
    }

    pub fn value(&self) -> &str {
        self.text.value()
    }

    pub fn cursor(&self) -> usize {
        self.text.cursor()
    }

    pub fn invalid(&self) -> bool {
        self.invalid
    }

    pub fn finished(&self) -> bool {
        self.finished
    }
}

fn slice(chars: &[char], from: usize, to: usize) -> String {
    chars
        .get(from.min(chars.len())..to.min(chars.len()))
        .map(|run| run.iter().collect())
        .unwrap_or_default()
}

fn from(chars: &[char], at: usize) -> String {
    slice(chars, at, chars.len())
}

/// `IsBackspace`: one character before the caret went, nothing selected.
fn is_backspace(was: &EditSnapshot, now: &FieldText) -> bool {
    let (was_chars, now_chars) = (was.text.chars(), now.chars());
    let pos = was.text.cursor;
    was.anchor == pos
        && pos == now.cursor + 1
        && slice(&was_chars, 0, pos - 1) == slice(&now_chars, 0, now.cursor)
        && from(&was_chars, pos) == from(&now_chars, now.cursor)
}

/// `IsDelete`: one character after the caret went, nothing selected.
fn is_delete(was: &EditSnapshot, now: &FieldText) -> bool {
    let (was_chars, now_chars) = (was.text.chars(), now.chars());
    let pos = was.text.cursor;
    was.anchor == pos
        && pos == now.cursor
        && slice(&was_chars, 0, pos) == slice(&now_chars, 0, now.cursor)
        && from(&was_chars, pos + 1) == from(&now_chars, now.cursor)
}

/// `NumbersOnlyState`: the digits, and the caret among them.
fn numbers_only(text: &FieldText) -> FieldText {
    let chars = text.chars();
    let digits = |run: &[char]| {
        run.iter()
            .filter(|c| c.is_ascii_digit())
            .collect::<String>()
    };
    let before = chars.get(..text.cursor.min(chars.len())).unwrap_or(&[]);
    FieldText::new(digits(&chars), digits(before).chars().count())
}

/// The digits after the edit: typing keeps what is there; Backspace or
/// Delete removes a digit from the text as it was, so stepping over a
/// separator still deletes something.
fn edited_digits(was: Option<&EditSnapshot>, now: &FieldText) -> FieldText {
    let Some(was) = was else {
        return numbers_only(now);
    };
    let deleted = is_delete(was, now);
    if !deleted && !is_backspace(was, now) {
        return numbers_only(now);
    }
    let real = numbers_only(&was.text);
    let chars = real.chars();
    let pos = real.cursor;
    if deleted {
        FieldText::new(slice(&chars, 0, pos) + &from(&chars, pos + 1), pos)
    } else if pos > 0 {
        FieldText::new(slice(&chars, 0, pos - 1) + &from(&chars, pos), pos - 1)
    } else {
        FieldText::new(real.value.clone(), 0)
    }
}

/// `PostprocessCardValidateResult`: a space after each digit group.
fn group_card_number(text: FieldText) -> FieldText {
    let mut chars = text.chars();
    let mut cursor = text.cursor;
    let mut position = 0;
    for length in card_number_groups(&text.value) {
        position += length;
        if position >= chars.len() {
            break;
        }
        chars.insert(position, ' ');
        if cursor >= position {
            cursor += 1;
        }
        position += 1;
    }
    FieldText::new(chars.into_iter().collect::<String>(), cursor)
}

/// `PostprocessExpireDateValidateResult`: "MM/YY", a leading zero for
/// months 2–9, and no slash after an impossible month like "13".
fn format_expiry(text: FieldText) -> FieldText {
    let mut chars = text.chars();
    let mut cursor = text.cursor;
    match (chars.first().copied(), chars.get(1).copied()) {
        (None, _) => return text,
        (Some('1'), Some(second)) if second > '2' => {
            chars.truncate(2);
            let len = chars.len();
            return FieldText::new(chars.into_iter().collect::<String>(), cursor.min(len));
        }
        (Some(first), _) if first > '1' => {
            chars.insert(0, '0');
            cursor += 1;
        }
        _ => {}
    }
    if chars.len() > 1 {
        chars.truncate(4);
        chars.insert(2, '/');
        if cursor >= 2 {
            cursor += 1;
        }
    }
    let len = chars.len();
    FieldText::new(chars.into_iter().collect::<String>(), cursor.min(len))
}

/// The number field after an edit.
pub fn edit_card_number(was: Option<&EditSnapshot>, now: &FieldText) -> FieldResult {
    let digits = edited_digits(was, now);
    let validation = validate_card(&digits.value);
    FieldResult::from_validation(group_card_number(digits), validation)
}

/// The expiry field after an edit, checked against this month.
pub fn edit_expiry(was: Option<&EditSnapshot>, now: &FieldText, today: YearMonth) -> FieldResult {
    let digits = edited_digits(was, now);
    let validation = validate_expiry(&digits.value, today);
    FieldResult::from_validation(format_expiry(digits), validation)
}

/// The CVC field after an edit; its length depends on the number's
/// network.
pub fn edit_cvc(number: &str, now: &FieldText) -> FieldResult {
    let digits = numbers_only(now);
    let validation = validate_cvc(number, &digits.value);
    FieldResult::from_validation(digits, validation)
}

/// The cardholder field: upper case, and empty is an error.
pub fn edit_holder_name(now: &FieldText) -> FieldResult {
    let upper = now.value.to_uppercase();
    let len = upper.chars().count();
    FieldResult {
        invalid: upper.is_empty(),
        text: FieldText::new(upper, now.cursor.min(len)),
        finished: false,
    }
}

/// A required text field (the ZIP code): empty is an error.
pub fn edit_required(now: &FieldText) -> FieldResult {
    FieldResult {
        invalid: now.value.is_empty(),
        text: now.clone(),
        finished: false,
    }
}

/// Digits only, as the number and CVC fields hand their value on.
pub fn digits_of(value: &str) -> String {
    value.chars().filter(char::is_ascii_digit).collect()
}

/// The month and four-digit year of an "MM/YY" field (tdesktop's
/// `ExtractMonth` / `ExtractYear`; a missing part is 0, a missing year
/// 2000).
pub fn parse_expiry(value: &str) -> (u32, u32) {
    let mut parts = value.split('/');
    let number = |part: Option<&str>| part.and_then(|p| p.trim().parse::<u32>().ok()).unwrap_or(0);
    let month = number(parts.next());
    let year = number(parts.next()) + 2000;
    (month, year)
}
