//! The card fields' live formatting, as typed.

use super::field_edit::*;
use super::validate::YearMonth;

const TODAY: YearMonth = YearMonth {
    year: 2026,
    month: 10,
};

fn typed(value: &str) -> FieldText {
    FieldText::new(value, value.chars().count())
}

fn shown(result: &FieldResult) -> (&str, usize) {
    (result.value(), result.cursor())
}

#[test]
fn the_number_groups_as_it_is_typed() {
    let result = edit_card_number(None, &typed("42424"));
    assert_eq!(shown(&result), ("4242 4", 6));
    let result = edit_card_number(None, &typed("4242 42424242 4242"));
    assert_eq!(shown(&result), ("4242 4242 4242 4242", 19));
    assert!(!result.invalid());
    let amex = edit_card_number(None, &typed("378282246310005"));
    assert_eq!(amex.value(), "3782 822463 10005");
    let wrong = edit_card_number(None, &typed("4242424242424241"));
    assert!(wrong.invalid());
    let letters = edit_card_number(None, &typed("4a2"));
    assert_eq!(letters.value(), "42");
    assert!(!letters.invalid());
}

#[test]
fn backspace_after_a_space_removes_the_digit_before_it() {
    // "4242 |4" -> Backspace removed the space.
    let was = EditSnapshot::new("4242 4", 5, 5);
    let result = edit_card_number(Some(&was), &FieldText::new("42424", 4));
    assert_eq!(shown(&result), ("4244", 3));
    // Delete before a space removes the digit after it.
    let was = EditSnapshot::new("4242 42", 4, 4);
    let result = edit_card_number(Some(&was), &FieldText::new("424242", 4));
    assert_eq!(shown(&result), ("4242 2", 5));
    // A plain Backspace of a digit is unchanged.
    let was = EditSnapshot::new("4242 42", 7, 7);
    let result = edit_card_number(Some(&was), &FieldText::new("4242 4", 6));
    assert_eq!(shown(&result), ("4242 4", 6));
}

#[test]
fn the_expiry_becomes_month_slash_year() {
    assert_eq!(shown(&edit_expiry(None, &typed("1"), TODAY)), ("1", 1));
    assert_eq!(shown(&edit_expiry(None, &typed("12"), TODAY)), ("12/", 3));
    assert_eq!(shown(&edit_expiry(None, &typed("4"), TODAY)), ("04/", 3));
    assert_eq!(
        shown(&edit_expiry(None, &typed("0430"), TODAY)),
        ("04/30", 5)
    );
    let done = edit_expiry(None, &typed("04/30"), TODAY);
    assert!(done.finished());
    assert!(!done.invalid());
    let past = edit_expiry(None, &typed("01/20"), TODAY);
    assert!(past.invalid());
    // An impossible month keeps two digits, without the slash.
    let bad = edit_expiry(None, &typed("135"), TODAY);
    assert_eq!(shown(&bad), ("13", 2));
    assert!(bad.invalid());
    // Backspace right after the slash removes the month's last digit.
    let was = EditSnapshot::new("12/", 3, 3);
    let result = edit_expiry(Some(&was), &FieldText::new("12", 2), TODAY);
    assert_eq!(shown(&result), ("1", 1));
}

#[test]
fn cvc_name_and_zip() {
    let cvc = edit_cvc("4242424242424242", &typed("12a3"));
    assert_eq!(cvc.value(), "123");
    assert!(cvc.finished());
    let name = edit_holder_name(&FieldText::new("jane doe", 3));
    assert_eq!(shown(&name), ("JANE DOE", 3));
    assert!(!name.invalid());
    assert!(edit_holder_name(&typed("")).invalid());
    assert!(edit_required(&typed("")).invalid());
    assert!(!edit_required(&typed("10001")).invalid());
}

#[test]
fn values_handed_on() {
    assert_eq!(digits_of("4242 4242"), "42424242");
    assert_eq!(parse_expiry("04/30"), (4, 2030));
    assert_eq!(parse_expiry("04"), (4, 2000));
    assert_eq!(parse_expiry(""), (0, 2000));
}
