//! The wording of the suggested-post boxes (`lng_suggest_*` in Telegram
//! Desktop's `lang.strings`), kept apart from the core types.

use super::{SuggestMode, SuggestedPostLimits, SuggestedPrice};

/// The price box's main button: "Update Terms" when changing, "Offer for
/// Free" with no amount, else "Offer" before the amount (the icon and
/// number are drawn after it).
pub fn price_submit_label(mode: SuggestMode, price: Option<SuggestedPrice>) -> &'static str {
    match (mode, price) {
        (SuggestMode::Change, _) => "Update Terms",
        (_, None) => "Offer for Free",
        (_, Some(_)) => "Offer",
    }
}

/// The price box's tab labels: (Stars, Grams).
pub fn currency_tabs(admin: bool) -> (&'static str, &'static str) {
    if admin {
        ("Request Stars", "Request Grams")
    } else {
        ("Offer Stars", "Offer Grams")
    }
}

/// The section title above the price field.
pub fn price_section_title(ton: bool) -> &'static str {
    if ton {
        "Enter Price in Grams"
    } else {
        "Enter Price in Stars"
    }
}

/// The member's note under the field.
pub fn price_about_member(ton: bool) -> &'static str {
    if ton {
        "Choose how many Grams to pay to publish this message."
    } else {
        "Choose how many Stars to pay to publish this message."
    }
}

/// `lng_suggest_options_stars_warning` (bold parts are plain here).
pub const STARS_WARNING: &str = "Transactions in Stars may be reversed by the payment provider within 21 days. Only accept Stars from people you trust.";

/// `lng_suggest_options_date_about`.
pub const DATE_ABOUT: &str = "Select the date and time you want the message to be published. The post will remain available for at least 24 hours from this date.";

/// The accept box (`ConfirmApproval`): the question and, with a price,
/// what is paid or received. `channel` and `from` are display names;
/// `date` the formatted time, `None` for "right now".
pub fn accept_text(
    admin: bool,
    from: &str,
    channel: &str,
    price: Option<SuggestedPrice>,
    date: Option<&str>,
    limits: &SuggestedPostLimits,
) -> String {
    let mut text = if admin {
        format!("Do you want to publish this post from {from}?")
    } else {
        format!("Do you want to publish this post to {channel}?")
    };
    let Some(price) = price else {
        return text;
    };
    let count = if admin {
        limits.after_commission_text(price)
    } else {
        price.number_text()
    };
    let unit = match (price.is_ton(), count == "1") {
        (false, true) => "Star",
        (false, false) => "Stars",
        (true, true) => "Gram",
        (true, false) => "Grams",
    };
    let when = date.map_or_else(|| "right now".to_string(), str::to_string);
    text.push_str("\n\n");
    if admin {
        text.push_str(&format!(
            "{channel} will receive {count} {unit} ({}) for publishing {when}. It must remain visible for at least 24 hours after publication.",
            limits.percent_label(price)
        ));
        if !price.is_ton() {
            text.push_str("\n\n");
            text.push_str(STARS_WARNING);
        }
    } else {
        text.push_str(&format!(
            "You will pay {count} {unit} for publishing {when}."
        ));
    }
    text
}

/// The decline box's question (`lng_suggest_decline_text` / `_to`).
pub fn decline_text(admin: bool, from: &str, channel: &str) -> String {
    if admin {
        format!("Do you want to decline publishing this post from {from}?")
    } else {
        format!("Do you want to decline publishing this post to {channel}?")
    }
}

/// `declineSuggestedPost.comment` allows 0-128 characters.
pub const DECLINE_REASON_MAX: usize = 128;

/// "{amount} Grams Needed" (`lng_suggest_low_ton_title`).
pub fn low_ton_title(price: SuggestedPrice) -> String {
    format!("{} Grams Needed", price.number_text())
}

/// `lng_suggest_low_ton_text`.
pub const LOW_TON_TEXT: &str =
    "You can add funds to your balance via the third-party platform Fragment.";
/// `lng_suggest_low_ton_fragment`.
pub const LOW_TON_BUTTON: &str = "Add Funds via Fragment";
/// `lng_suggest_low_ton_fragment_url`.
pub const LOW_TON_URL: &str = "https://fragment.com/ads/topup";

/// The delete box over a paid post younger than the lifetime minimum
/// (`lng_suggest_warn_*`): (title, text).
pub fn delete_warning(ton: bool) -> (&'static str, &'static str) {
    if ton {
        (
            "Grams will be lost",
            "You won't receive Grams for the post if you delete it now. The post must remain visible for at least 24 hours after it was published.",
        )
    } else {
        (
            "Stars will be lost",
            "You won't receive Stars for the post if you delete it now. The post must remain visible for at least 24 hours after it was published.",
        )
    }
}

/// `lng_suggest_warn_delete_anyway`.
pub const DELETE_ANYWAY: &str = "Delete Anyway";
