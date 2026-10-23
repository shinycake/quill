//! Stripe's published test card numbers through tdesktop's checks.

use super::validate::*;

const TODAY: YearMonth = YearMonth {
    year: 2026,
    month: 10,
};

#[test]
fn luhn_accepts_test_cards_and_rejects_a_changed_digit() {
    for number in [
        "4242424242424242",
        "5555555555554444",
        "378282246310005",
        "6011111111111117",
        "36227206271667",
        "3566002020360505",
        "6200000000000005",
    ] {
        assert!(is_valid_luhn(number), "{number}");
    }
    assert!(!is_valid_luhn("4242424242424241"));
}

#[test]
fn brands_follow_the_bin_table() {
    assert_eq!(brand_for_number("4242 4242 4242 4242"), CardBrand::Visa);
    assert_eq!(brand_for_number("5555555555554444"), CardBrand::MasterCard);
    assert_eq!(brand_for_number("2223003122003222"), CardBrand::MasterCard);
    assert_eq!(brand_for_number("378282246310005"), CardBrand::Amex);
    assert_eq!(brand_for_number("6011111111111117"), CardBrand::Discover);
    assert_eq!(brand_for_number("36227206271667"), CardBrand::DinersClub);
    assert_eq!(brand_for_number("3566002020360505"), CardBrand::Jcb);
    assert_eq!(brand_for_number("6200000000000005"), CardBrand::UnionPay);
    // "3" could still be Amex, Diners or JCB.
    assert_eq!(brand_for_number("3"), CardBrand::Unknown);
    assert_eq!(brand_for_number("9999"), CardBrand::Unknown);
    assert_eq!(brand_for_number("42a"), CardBrand::Unknown);
}

#[test]
fn card_number_states() {
    let state = |number: &str| validate_card(number).state;
    assert_eq!(state(""), ValidationState::Incomplete);
    assert_eq!(state("4242"), ValidationState::Incomplete);
    assert_eq!(state("42x2"), ValidationState::Invalid);
    assert_eq!(state("4242424242424242"), ValidationState::Valid);
    assert_eq!(state("4242424242424241"), ValidationState::Invalid);
    assert_eq!(state("42424242424242424242"), ValidationState::Invalid);
    // Valid but not finished: only 19 digits end the field, as in tdesktop.
    assert!(!validate_card("4242424242424242").finished);
    assert!(validate_card("6205500000000000004").finished);
}

#[test]
fn expiry_states_against_this_month() {
    let state = |date: &str| validate_expiry(date, TODAY).state;
    assert_eq!(state(""), ValidationState::Incomplete);
    assert_eq!(state("1"), ValidationState::Incomplete);
    assert_eq!(state("13"), ValidationState::Invalid);
    assert_eq!(state("00"), ValidationState::Invalid);
    assert_eq!(state("12/2"), ValidationState::Incomplete);
    assert_eq!(state("09/26"), ValidationState::Invalid);
    assert_eq!(state("10/26"), ValidationState::Valid);
    assert_eq!(state("01/27"), ValidationState::Valid);
    assert_eq!(state("12/25"), ValidationState::Invalid);
    assert_eq!(state("1/2/3/4/5"), ValidationState::Invalid);
    // A leading 2–9 is a one-digit month.
    assert_eq!(state("530"), ValidationState::Valid);
    assert!(validate_expiry("10/30", TODAY).finished);
    assert_eq!(
        validate_parsed_expiry(4, 2030, TODAY),
        ValidationState::Valid
    );
    assert_eq!(
        validate_parsed_expiry(4, 1930, TODAY),
        ValidationState::Invalid
    );
    assert_eq!(
        validate_parsed_expiry(0, 2000, TODAY),
        ValidationState::Invalid
    );
}

#[test]
fn cvc_length_follows_the_network() {
    let visa = "4242424242424242";
    let amex = "378282246310005";
    assert_eq!(validate_cvc(visa, "12").state, ValidationState::Incomplete);
    assert_eq!(
        validate_cvc(visa, "123"),
        Validation {
            state: ValidationState::Valid,
            finished: true
        }
    );
    assert_eq!(validate_cvc(visa, "1234").state, ValidationState::Invalid);
    assert_eq!(validate_cvc(amex, "123").state, ValidationState::Valid);
    assert!(!validate_cvc(amex, "123").finished);
    assert!(validate_cvc(amex, "1234").finished);
    assert_eq!(validate_cvc(visa, "1a3").state, ValidationState::Invalid);
}

#[test]
fn digit_groups_by_network() {
    assert_eq!(card_number_groups("4242"), &[4, 4, 4, 4]);
    assert_eq!(card_number_groups("3782"), &[4, 6, 5]);
    assert_eq!(card_number_groups("3622"), &[4, 6, 4]);
    assert_eq!(card_number_groups("3056"), &[4, 4, 4, 4]);
}

#[test]
fn brand_names_for_titles() {
    assert_eq!(CardBrand::from_stripe_name("Visa"), CardBrand::Visa);
    assert_eq!(
        CardBrand::from_stripe_name("American Express").title_name(),
        "american express"
    );
    assert_eq!(
        CardBrand::from_stripe_name("MasterCard").title_name(),
        "mastercard"
    );
    assert_eq!(
        CardBrand::from_stripe_name("UnionPay").title_name(),
        "unknown"
    );
}
