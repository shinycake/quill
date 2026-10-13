//! Gram amounts in nano-units (10^9 per Gram), worded like the official
//! iOS app's `formatTonAmountText(…, maxDecimalPositions: 3)` with
//! `Currency.Grams` ("%@ Gram" / "%@ Grams").

const NANOS_PER_GRAM: u64 = 1_000_000_000;

/// Decimal places a message shows (`maxDecimalPositions: 3`).
const MESSAGE_DECIMALS: usize = 3;

/// The unsigned number: thousands grouped with commas, up to three
/// decimals with trailing zeros cut. An amount under 0.001 that is not
/// zero keeps all nine decimals instead of reading "0".
pub fn format_gram_amount(nanos: i64) -> String {
    format_gram_amount_with(nanos, MESSAGE_DECIMALS)
}

/// [`format_gram_amount`] with up to `decimals` places (iOS
/// `maxDecimalPositions`: 2 for a balance, 3 in lists, 5 for a key change).
pub fn format_gram_amount_with(nanos: i64, decimals: usize) -> String {
    let magnitude = nanos.unsigned_abs();
    let integer = magnitude / NANOS_PER_GRAM;
    let fraction = magnitude % NANOS_PER_GRAM;
    let full = format!("{fraction:09}");
    let mut shown: String = full.chars().take(decimals.min(9)).collect();
    if integer == 0 && magnitude != 0 && shown.chars().all(|ch| ch == '0') {
        shown = full;
    }
    let decimals = shown.trim_end_matches('0');
    let mut text = group_thousands(integer);
    if !decimals.is_empty() {
        text.push('.');
        text.push_str(decimals);
    }
    text
}

/// "1 Gram", "5 Grams", "0.5 Grams": the singular only for exactly one.
pub fn format_grams(nanos: i64) -> String {
    let number = format_gram_amount(nanos);
    if number == "1" {
        "1 Gram".to_string()
    } else {
        format!("{number} Grams")
    }
}

/// The card's signed amount: "+5 Grams" received, "\u{2212}5 Grams" sent
/// (a minus sign, not a hyphen, as the iOS card draws it).
pub fn signed_gram_amount(nanos: i64, received: bool) -> String {
    let sign = if received { '+' } else { '\u{2212}' };
    format!("{sign}{}", format_grams(nanos))
}

fn group_thousands(value: u64) -> String {
    let digits = value.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, ch) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    out
}
