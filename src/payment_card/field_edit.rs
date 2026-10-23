//! What a card field shows after each edit: the validators of Telegram
//! Desktop's `payments/ui/payments_edit_card.cpp`. The number groups its
//! digits by network, the expiry becomes "MM/YY", and Backspace or Delete
//! next to a separator removes the digit beside it instead.
//!
//! Every copy of the text made here is wiped when dropped: [`FieldText`]
//! zeroizes itself, and each new value is built at its final capacity so
//! no growing `String` leaves an unwiped buffer behind.
#![deny(clippy::indexing_slicing)]

use super::validate::{
    Validation, ValidationState, YearMonth, card_number_groups, validate_card, validate_cvc,
    validate_expiry,
};
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

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

    fn chars(&self) -> Zeroizing<Vec<char>> {
        let mut chars = Zeroizing::new(Vec::with_capacity(self.value.len()));
        chars.extend(self.value.chars());
        chars
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

/// The characters from `from` to `to`, clamped to the text.
fn run(chars: &[char], from: usize, to: usize) -> &[char] {
    let to = to.min(chars.len());
    chars.get(from.min(to)..to).unwrap_or(&[])
}

/// `IsBackspace`: one character before the caret went, nothing selected.
fn is_backspace(was: &EditSnapshot, now: &FieldText) -> bool {
    let (was_chars, now_chars) = (was.text.chars(), now.chars());
    let pos = was.text.cursor;
    was.anchor == pos
        && now.cursor.checked_add(1) == Some(pos)
        && run(&was_chars, 0, now.cursor) == run(&now_chars, 0, now.cursor)
        && run(&was_chars, pos, usize::MAX) == run(&now_chars, now.cursor, usize::MAX)
}

/// `IsDelete`: one character after the caret went, nothing selected.
fn is_delete(was: &EditSnapshot, now: &FieldText) -> bool {
    let (was_chars, now_chars) = (was.text.chars(), now.chars());
    let pos = was.text.cursor;
    was.anchor == pos
        && pos == now.cursor
        && run(&was_chars, 0, pos) == run(&now_chars, 0, now.cursor)
        && run(&was_chars, pos.saturating_add(1), usize::MAX)
            == run(&now_chars, now.cursor, usize::MAX)
}

/// The characters `chars` yields, in a `String` allocated once at their
/// exact length.
fn collect_exact<I: Iterator<Item = char> + Clone>(chars: I) -> String {
    let mut value = String::with_capacity(chars.clone().map(char::len_utf8).sum());
    value.extend(chars);
    value
}

/// `NumbersOnlyState`: the digits, and the caret among them.
fn numbers_only(text: &FieldText) -> FieldText {
    let digits = text.value.chars().filter(char::is_ascii_digit);
    let cursor = text
        .value
        .chars()
        .take(text.cursor)
        .filter(char::is_ascii_digit)
        .count();
    FieldText::new(collect_exact(digits), cursor)
}

/// `text` without the character at `index`, the caret at `cursor`.
fn without(text: &FieldText, index: usize, cursor: usize) -> FieldText {
    let kept = text
        .value
        .chars()
        .enumerate()
        .filter(move |(at, _)| *at != index)
        .map(|(_, c)| c);
    FieldText::new(collect_exact(kept), cursor)
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
    let pos = real.cursor;
    if deleted {
        without(&real, pos, pos)
    } else if let Some(before) = pos.checked_sub(1) {
        without(&real, before, before)
    } else {
        real
    }
}

/// `PostprocessCardValidateResult`: a space after each digit group, for
/// every group the digits run past.
fn group_card_number(text: FieldText) -> FieldText {
    let len = text.value.chars().count();
    let breaks = || {
        card_number_groups(&text.value)
            .iter()
            .scan(0usize, |sum, group| {
                *sum = sum.saturating_add(*group);
                Some(*sum)
            })
            .take_while(move |at| *at < len)
    };
    let cursor = text
        .cursor
        .saturating_add(breaks().filter(|at| *at <= text.cursor).count());
    let spaced = text.value.chars().enumerate().flat_map(|(at, c)| {
        let space = breaks().any(|point| point == at).then_some(' ');
        space.into_iter().chain(std::iter::once(c))
    });
    FieldText::new(collect_exact(spaced), cursor)
}

/// `PostprocessExpireDateValidateResult`: "MM/YY", a leading zero for
/// months 2–9, and no slash after an impossible month like "13".
fn format_expiry(text: FieldText) -> FieldText {
    let mut chars = text.value.chars();
    let (Some(first), second) = (chars.next(), chars.next()) else {
        return text;
    };
    if first == '1' && second.is_some_and(|second| second > '2') {
        let month = collect_exact(text.value.chars().take(2));
        return FieldText::new(month, text.cursor.min(2));
    }
    let lead = first > '1';
    let digits = lead.then_some('0').into_iter().chain(text.value.chars());
    let mut cursor = text.cursor.saturating_add(usize::from(lead));
    if digits.clone().nth(1).is_some() && cursor >= 2 {
        cursor = cursor.saturating_add(1);
    }
    // The slash follows the month, once there are two month digits.
    let shown = digits
        .take(4)
        .enumerate()
        .flat_map(|(at, c)| std::iter::once(c).chain((at == 1).then_some('/')));
    let value = collect_exact(shown);
    let len = value.chars().count();
    FieldText::new(value, cursor.min(len))
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

/// The cardholder field: upper case, and empty is an error. Upper case can
/// lengthen the text ("ß" becomes "SS"), so the caret moves by what the
/// text before it grew.
pub fn edit_holder_name(now: &FieldText) -> FieldResult {
    let upper = collect_exact(now.value.chars().flat_map(char::to_uppercase));
    let cursor = now
        .value
        .chars()
        .take(now.cursor)
        .flat_map(char::to_uppercase)
        .count();
    let len = upper.chars().count();
    FieldResult {
        invalid: upper.is_empty(),
        text: FieldText::new(upper, cursor.min(len)),
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

/// Digits only, as the number and CVC fields hand their value on. The
/// result moves into [`super::CardDetails`], which wipes it.
pub fn digits_of(value: &str) -> String {
    collect_exact(value.chars().filter(char::is_ascii_digit))
}

/// The month and four-digit year of an "MM/YY" field (tdesktop's
/// `ExtractMonth` / `ExtractYear`; a missing part is 0, a missing year
/// 2000).
pub fn parse_expiry(value: &str) -> (u32, u32) {
    let mut parts = value.split('/');
    let number = |part: Option<&str>| part.and_then(|p| p.trim().parse::<u32>().ok()).unwrap_or(0);
    let month = number(parts.next());
    let year = number(parts.next()).saturating_add(2000);
    (month, year)
}
