//! Suggested posts in a channel's direct messages, after Telegram Desktop's
//! `api/api_suggest_post.cpp`, `history_view_suggest_options.cpp` and
//! `history_view_suggest_decision.cpp`.
//!
//! A member of a channel's direct messages chat can offer a post with a
//! price (Stars or Grams) and a publication time; the channel accepts,
//! declines or counters it. This module is the pure core: the TDLib types
//! (`suggestedPostInfo`, `inputSuggestedPostInfo`), the wording, the limits
//! from TDLib options, the decision buttons, the change detection of a
//! counter-offer, and the session state (the composer's suggest bar per
//! chat, the Grams balance, one-shot errors). The requests are in
//! `telegram::requests::suggested_posts`, the driver in
//! `connect::suggested_posts`, the screens in `ui::suggested_posts_ui`.

use crate::ids::{ChatId, MessageId};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};

/// Nano-Grams in one Gram cent (TDLib prices Grams in 1/100 Gram).
const NANOS_PER_GRAM_CENT: i64 = 10_000_000;

/// A suggested post's price (`SuggestedPostPrice`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SuggestedPrice {
    /// `suggestedPostPriceStar.star_count`.
    Stars(i64),
    /// `suggestedPostPriceGram.gram_cent_count`: 1/100 of a Gram.
    TonCents(i64),
}

impl SuggestedPrice {
    /// A `SuggestedPostPrice`; null, unknown or non-positive is no price.
    pub fn parse(value: Option<&Value>) -> Option<Self> {
        let value = value.filter(|v| !v.is_null())?;
        let price = match value.get("@type").and_then(Value::as_str)? {
            "suggestedPostPriceStar" => Self::Stars(int(value.get("star_count"))),
            "suggestedPostPriceGram" => Self::TonCents(int(value.get("gram_cent_count"))),
            _ => return None,
        };
        (price.amount() > 0).then_some(price)
    }

    pub fn to_json(self) -> Value {
        match self {
            Self::Stars(count) => json!({
                "@type": "suggestedPostPriceStar",
                "star_count": count,
            }),
            Self::TonCents(cents) => json!({
                "@type": "suggestedPostPriceGram",
                "gram_cent_count": cents,
            }),
        }
    }

    pub fn is_ton(self) -> bool {
        matches!(self, Self::TonCents(_))
    }

    /// Stars, or Gram cents.
    pub fn amount(self) -> i64 {
        match self {
            Self::Stars(count) | Self::TonCents(count) => count,
        }
    }

    /// The amount in nano-Grams (Grams only; Stars give 0).
    pub fn nanos(self) -> i64 {
        match self {
            Self::Stars(_) => 0,
            Self::TonCents(cents) => cents.saturating_mul(NANOS_PER_GRAM_CENT),
        }
    }

    /// The number without a unit: "1,250" Stars, "12.5" Grams
    /// (tdesktop `Lang::FormatCreditsAmountDecimal`).
    pub fn number_text(self) -> String {
        match self {
            Self::Stars(count) => crate::premium_hub::group_digits(count),
            Self::TonCents(_) => crate::ton_wallet::format_gram_amount(self.nanos()),
        }
    }

    /// "5 Stars", "1 Star", "12.5 Grams", "1 Gram"
    /// (`lng_suggest_stars_amount`, `lng_suggest_ton_amount`).
    pub fn label(self) -> String {
        let number = self.number_text();
        match (self, number == "1") {
            (Self::Stars(_), true) => "1 Star".into(),
            (Self::Stars(_), false) => format!("{number} Stars"),
            (Self::TonCents(_), true) => "1 Gram".into(),
            (Self::TonCents(_), false) => format!("{number} Grams"),
        }
    }
}

/// "Free" for no price (`lng_suggest_action_price_free`), else the label.
pub fn price_label(price: Option<SuggestedPrice>) -> String {
    price.map_or_else(|| "Free".into(), SuggestedPrice::label)
}

/// `SuggestedPostState`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SuggestedPostState {
    #[default]
    Pending,
    Approved,
    Declined,
}

/// `message.suggested_post_info` (`suggestedPostInfo`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SuggestedPostInfo {
    pub price: Option<SuggestedPrice>,
    /// Unix time the post is expected to be published; 0 for any time.
    pub send_date: i32,
    pub state: SuggestedPostState,
    /// The current user may approve it (`approveSuggestedPost`).
    pub can_be_approved: bool,
    /// The current user may decline it (`declineSuggestedPost`).
    pub can_be_declined: bool,
}

impl SuggestedPostInfo {
    /// A `suggestedPostInfo`; null or another type is none.
    pub fn parse(value: Option<&Value>) -> Option<Self> {
        let value = value.filter(|v| !v.is_null())?;
        if value.get("@type").and_then(Value::as_str) != Some("suggestedPostInfo") {
            return None;
        }
        let state = match value
            .get("state")
            .and_then(|state| state.get("@type"))
            .and_then(Value::as_str)
        {
            Some("suggestedPostStateApproved") => SuggestedPostState::Approved,
            Some("suggestedPostStateDeclined") => SuggestedPostState::Declined,
            _ => SuggestedPostState::Pending,
        };
        let flag = |name: &str| value.get(name).and_then(Value::as_bool).unwrap_or(false);
        Some(Self {
            price: SuggestedPrice::parse(value.get("price")),
            send_date: date(value.get("send_date")),
            state,
            can_be_approved: flag("can_be_approved"),
            can_be_declined: flag("can_be_declined"),
        })
    }

    /// The price and time, as a counter-offer starts from them.
    pub fn terms(&self) -> SuggestTerms {
        SuggestTerms {
            price: self.price,
            send_date: self.send_date,
        }
    }
}

/// The terms of an offer: what the suggest bar holds and what a send,
/// draft or counter-offer carries (`inputSuggestedPostInfo`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SuggestTerms {
    pub price: Option<SuggestedPrice>,
    /// 0 for any time.
    pub send_date: i32,
}

impl SuggestTerms {
    /// An `inputSuggestedPostInfo` (a draft's `suggested_post_info`).
    pub fn parse_input(value: Option<&Value>) -> Option<Self> {
        let value = value.filter(|v| !v.is_null())?;
        if value.get("@type").and_then(Value::as_str) != Some("inputSuggestedPostInfo") {
            return None;
        }
        Some(Self {
            price: SuggestedPrice::parse(value.get("price")),
            send_date: date(value.get("send_date")),
        })
    }

    /// `inputSuggestedPostInfo` for `messageSendOptions` and drafts.
    pub fn to_input_json(&self) -> Value {
        json!({
            "@type": "inputSuggestedPostInfo",
            "price": self.price.map_or(Value::Null, SuggestedPrice::to_json),
            "send_date": self.send_date.max(0),
        })
    }

    /// What the bar's second line says.
    pub fn bar_line(&self) -> BarLine {
        match (self.price, self.send_date) {
            (None, 0) => BarLine::Prompt,
            (Some(price), 0) => BarLine::Priced(price),
            (None, date) => BarLine::Dated(date),
            (Some(price), date) => BarLine::PricedDated(price, date),
        }
    }
}

/// The suggest bar's title (`lng_suggest_bar_title`).
pub const BAR_TITLE: &str = "Suggest a Post Below";
/// The bar's and the price box's title when changing an offer
/// (`lng_suggest_options_change`).
pub const CHANGE_TITLE: &str = "Suggest Changes";

/// The suggest bar's second line (`SuggestOptionsBar::composeText`). The
/// amount is drawn as the currency icon followed by the number, so the
/// line comes in parts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BarLine {
    /// "Click to offer a price for publishing."
    Prompt,
    /// "{amount} for publishing anytime."
    Priced(SuggestedPrice),
    /// "Publish on {date}"
    Dated(i32),
    /// "{amount}   📆 {date}"
    PricedDated(SuggestedPrice, i32),
}

/// `lng_suggest_bar_text`.
pub const BAR_PROMPT: &str = "Click to offer a price for publishing.";
/// What follows the amount in `lng_suggest_bar_priced`.
pub const BAR_PRICED_TAIL: &str = " for publishing anytime.";

/// "Publish on {date}" (`lng_suggest_bar_dated`).
pub fn bar_dated(date: &str) -> String {
    format!("Publish on {date}")
}

/// What follows the amount when both are set: three spaces, the calendar
/// emoji and the date.
pub fn bar_both_tail(date: &str) -> String {
    format!("   \u{1F4C6} {date}")
}

/// Which box a price or time choice belongs to (tdesktop `SuggestMode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SuggestMode {
    /// A new offer (the composer bar, "Add Offer").
    New,
    /// A counter-offer ("Suggest Changes").
    Change,
    /// Picking a time while accepting a post that has none.
    Publish,
}

impl SuggestMode {
    /// The price box title.
    pub fn price_title(self) -> &'static str {
        match self {
            Self::New | Self::Publish => "Suggest a Message",
            Self::Change => CHANGE_TITLE,
        }
    }

    /// The time box title ("Time" / "Edit Time").
    pub fn time_title(self) -> &'static str {
        match self {
            Self::New | Self::Publish => "Time",
            Self::Change => "Edit Time",
        }
    }

    /// The time box's confirm button.
    pub fn time_submit(self) -> &'static str {
        match self {
            Self::Publish => "Publish",
            Self::New => "Save",
            Self::Change => "Update Time",
        }
    }

    /// The time box's left button, which picks no time.
    pub fn time_none(self) -> &'static str {
        match self {
            Self::Publish => "Publish Now",
            Self::New | Self::Change => "Anytime",
        }
    }
}

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

/// The decision buttons under a suggestion
/// (`HistoryItem::computeSuggestionActions` + `updateSuggestControls`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SuggestionButtons {
    pub accept: bool,
    pub decline: bool,
    /// "Suggest Changes": only with Accept (`AcceptAndDecline`).
    pub change: bool,
}

impl SuggestionButtons {
    /// TDLib already applied the side and rights checks tdesktop does
    /// (`can_be_approved` / `can_be_declined`); only a pending offer has
    /// buttons. `AcceptAndDecline` shows [Decline | Accept] and [Suggest
    /// Changes]; `Decline` alone shows [Decline].
    pub fn for_info(info: &SuggestedPostInfo) -> Self {
        if info.state != SuggestedPostState::Pending {
            return Self::default();
        }
        let accept = info.can_be_approved;
        Self {
            accept,
            decline: accept || info.can_be_declined,
            change: accept,
        }
    }

    pub fn any(&self) -> bool {
        self.accept || self.decline || self.change
    }
}

/// What a counter-offer changed against the offer it replies to
/// (tdesktop `ResolveChanges`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Changes {
    pub price: bool,
    pub date: bool,
    pub message: bool,
}

impl Changes {
    /// `was` is the replied offer's terms (none: this is no counter-offer);
    /// `content_same` whether the text and media are unchanged. Neither
    /// price nor time changed means the message did.
    pub fn resolve(
        was: Option<SuggestTerms>,
        now: SuggestTerms,
        content_same: bool,
    ) -> Option<Self> {
        let was = was?;
        let price = was.price != now.price;
        let date = was.send_date != now.send_date;
        let message = if !price && !date { true } else { !content_same };
        Some(Self {
            price,
            date,
            message,
        })
    }
}

/// The first line of the block above a suggestion.
pub fn block_title(from_self: bool, from: &str, changes: Option<Changes>) -> String {
    match changes {
        None if from_self => "You suggest to post this message.".into(),
        None => format!("{from} suggests to post this message."),
        Some(c) if c.message => format!("{from} suggests changes for the message."),
        Some(c) if c.price && c.date => {
            format!("{from} suggests a new price and time for the message.")
        }
        Some(c) if c.price => format!("{from} suggests a new price for the message."),
        Some(_) => format!("{from} suggests a new time for the message."),
    }
}

/// The table's labels: (price, time).
pub fn block_labels(changes: Option<Changes>) -> (&'static str, &'static str) {
    let price = if changes.is_some_and(|c| c.price) {
        "New Price"
    } else {
        "Price"
    };
    let time = if changes.is_some_and(|c| c.date) {
        "New Time"
    } else {
        "Time"
    };
    (price, time)
}

/// The faded line under the table when the message changed
/// (`lng_suggest_change_text_label`).
pub const BLOCK_CHECK_BELOW: &str = "Check the suggested message below";

/// What a price check found (`AddStarsTonPriceInput::computeResult`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PriceCheck {
    Ok(Option<SuggestedPrice>),
    /// Under the minimum: the field shows its error and the box toasts
    /// "Minimum price is …".
    TooSmall,
    /// Over the maximum: the field shows its error only.
    TooLarge,
    /// Not a number.
    Invalid,
}

/// Limits and shares from TDLib options. The defaults are TDLib's own
/// (`OptionManager.cpp`), used until the options arrive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SuggestedPostLimits {
    pub star_min: i64,
    pub star_max: i64,
    pub ton_cent_min: i64,
    pub ton_cent_max: i64,
    pub star_per_mille: i64,
    pub ton_per_mille: i64,
    pub send_delay_min: i64,
    pub send_delay_max: i64,
    pub lifetime_min: i64,
}

impl Default for SuggestedPostLimits {
    fn default() -> Self {
        Self {
            star_min: 5,
            star_max: 100_000,
            ton_cent_min: 1,
            ton_cent_max: 1_000_000,
            star_per_mille: 850,
            ton_per_mille: 850,
            send_delay_min: 300,
            send_delay_max: 2_678_400,
            lifetime_min: 86_400,
        }
    }
}

impl SuggestedPostLimits {
    /// One `updateOption`; other names are ignored.
    pub fn apply_option(&mut self, name: &str, value: i64) {
        let slot = match name {
            "suggested_post_star_count_min" => &mut self.star_min,
            "suggested_post_star_count_max" => &mut self.star_max,
            "suggested_post_gram_cent_count_min" => &mut self.ton_cent_min,
            "suggested_post_gram_cent_count_max" => &mut self.ton_cent_max,
            "suggested_post_star_earnings_per_mille" => &mut self.star_per_mille,
            "suggested_post_gram_earnings_per_mille" => &mut self.ton_per_mille,
            "suggested_post_send_delay_min" => &mut self.send_delay_min,
            "suggested_post_send_delay_max" => &mut self.send_delay_max,
            "suggested_post_lifetime_min" => &mut self.lifetime_min,
            _ => return,
        };
        *slot = value.max(0);
    }

    /// The earliest and latest time a post may be set for
    /// (`ChooseSuggestTimeBox`: the minimum delay plus a minute).
    pub fn time_range(&self, now: i64) -> (i64, i64) {
        let min = now.saturating_add(self.send_delay_min).saturating_add(60);
        let max = now.saturating_add(self.send_delay_max).max(min);
        (min, max)
    }

    /// Where the time box opens: a day from now for no time, else `value`
    /// clamped into the range.
    pub fn initial_time(&self, now: i64, value: i32) -> i64 {
        if value <= 0 {
            return now.saturating_add(86_400);
        }
        let (min, max) = self.time_range(now);
        i64::from(value).clamp(min, max)
    }

    /// `time` clamped into the range (a picked time that went stale).
    pub fn clamp_time(&self, now: i64, time: i64) -> i64 {
        let (min, max) = self.time_range(now);
        time.clamp(min, max)
    }

    /// Checks the price field's text in the chosen currency. An empty
    /// field is a free post.
    pub fn check_price(&self, text: &str, ton: bool) -> PriceCheck {
        let text = text.trim();
        if text.is_empty() {
            return PriceCheck::Ok(None);
        }
        let amount = if ton {
            parse_gram_cents(text)
        } else {
            text.parse::<i64>().ok().filter(|v| *v >= 0)
        };
        let Some(amount) = amount else {
            return PriceCheck::Invalid;
        };
        if amount == 0 {
            return PriceCheck::Ok(None);
        }
        let (min, max) = if ton {
            (self.ton_cent_min, self.ton_cent_max)
        } else {
            (self.star_min, self.star_max)
        };
        if amount < min {
            PriceCheck::TooSmall
        } else if amount > max {
            PriceCheck::TooLarge
        } else if ton {
            PriceCheck::Ok(Some(SuggestedPrice::TonCents(amount)))
        } else {
            PriceCheck::Ok(Some(SuggestedPrice::Stars(amount)))
        }
    }

    /// "Minimum price is {count} Stars." (`lng_gift_sell_min_price`).
    pub fn min_price_toast(&self, ton: bool) -> String {
        let price = if ton {
            SuggestedPrice::TonCents(self.ton_cent_min)
        } else {
            SuggestedPrice::Stars(self.star_min)
        };
        format!("Minimum price is {}.", price.label())
    }

    fn per_mille(&self, price: SuggestedPrice) -> i64 {
        match price {
            SuggestedPrice::Stars(_) => self.star_per_mille,
            SuggestedPrice::TonCents(_) => self.ton_per_mille,
        }
    }

    /// The channel's share as a number (`PriceAfterCommission`): whole
    /// Stars, Grams with their fraction.
    pub fn after_commission_text(&self, price: SuggestedPrice) -> String {
        let per_mille = self.per_mille(price);
        match price {
            SuggestedPrice::Stars(count) => crate::premium_hub::group_digits(
                count
                    .saturating_mul(per_mille)
                    .checked_div(1000)
                    .unwrap_or(0),
            ),
            SuggestedPrice::TonCents(_) => crate::ton_wallet::format_gram_amount(
                price
                    .nanos()
                    .saturating_mul(per_mille)
                    .checked_div(1000)
                    .unwrap_or(0),
            ),
        }
    }

    /// "85%" (`FormatAfterCommissionPercent`).
    pub fn percent_label(&self, price: SuggestedPrice) -> String {
        crate::paid_messages::earnings_percent(self.per_mille(price))
    }

    /// The admin's note under the price field
    /// (`lng_suggest_options_you_get_stars` / `_ton`).
    pub fn you_get(&self, price: SuggestedPrice) -> String {
        let number = self.after_commission_text(price);
        let unit = match (price.is_ton(), number == "1") {
            (false, true) => "Star",
            (false, false) => "Stars",
            (true, true) => "Gram",
            (true, false) => "Grams",
        };
        format!(
            "You will receive {number} {unit} ({}) for publishing this post.",
            self.percent_label(price)
        )
    }

    /// Whether deleting a paid post published at `date` loses the payment
    /// ("Stars will be lost").
    pub fn delete_loses_payment(&self, now: i64, date: i32) -> bool {
        now.saturating_sub(i64::from(date)) < self.lifetime_min
    }
}

/// "12.5" or "12,5" Grams as Gram cents; more than two decimals is not a
/// valid price.
pub fn parse_gram_cents(text: &str) -> Option<i64> {
    let text = text.trim().replace(',', ".");
    let (whole, fraction) = text.split_once('.').unwrap_or((text.as_str(), ""));
    if fraction.len() > 2
        || !whole.chars().all(|c| c.is_ascii_digit())
        || !fraction.chars().all(|c| c.is_ascii_digit())
        || (whole.is_empty() && fraction.is_empty())
    {
        return None;
    }
    let whole: i64 = if whole.is_empty() {
        0
    } else {
        whole.parse().ok()?
    };
    let cents: i64 = match fraction.len() {
        0 => 0,
        1 => fraction.parse::<i64>().ok()?.saturating_mul(10),
        _ => fraction.parse().ok()?,
    };
    whole.checked_mul(100)?.checked_add(cents)
}

/// The text a price field shows for `price` in its own currency.
pub fn field_text(price: Option<SuggestedPrice>) -> String {
    match price {
        Some(SuggestedPrice::Stars(count)) => count.to_string(),
        Some(price @ SuggestedPrice::TonCents(_)) => {
            crate::ton_wallet::format_gram_amount(price.nanos()).replace(',', "")
        }
        None => String::new(),
    }
}

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

/// The viewer's side in a channel's direct messages chat.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirectMessagesRole {
    /// A member writing to the channel: may suggest posts.
    Member,
    /// One of the channel's administrators.
    Admin,
}

impl DirectMessagesRole {
    /// From `supergroup.is_direct_messages_group` and
    /// `is_administered_direct_messages_group`.
    pub fn from_flags(is_direct_messages: bool, administered: bool) -> Option<Self> {
        match (is_direct_messages, administered) {
            (false, _) => None,
            (true, true) => Some(Self::Admin),
            (true, false) => Some(Self::Member),
        }
    }
}

/// The composer's suggest bar in one chat.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SuggestBar {
    pub terms: SuggestTerms,
    pub mode: SuggestMode,
    /// "Edit Message" of this suggestion: saving the edit sends a new
    /// message that replies to it, with these terms.
    pub edit_of: Option<MessageId>,
}

impl SuggestBar {
    pub fn new(terms: SuggestTerms) -> Self {
        Self {
            terms,
            mode: SuggestMode::New,
            edit_of: None,
        }
    }

    pub fn title(&self) -> &'static str {
        match self.mode {
            SuggestMode::Change => CHANGE_TITLE,
            SuggestMode::New | SuggestMode::Publish => BAR_TITLE,
        }
    }
}

/// The session's suggested-post state (in `PaymentsState`).
#[derive(Debug, Clone, Default)]
pub struct SuggestedPostsState {
    pub limits: SuggestedPostLimits,
    /// The suggest bar of each chat that has one up, by chat id.
    pub bars: HashMap<i64, SuggestBar>,
    /// `updateOwnedGramCount`: the account's Grams in nano-Grams.
    pub gram_balance: Option<i64>,
    /// Suggestions with an approve or decline in flight.
    pub busy: HashSet<(i64, i64)>,
    /// One-shot errors the UI shows as toasts.
    pub notes: Vec<String>,
}

impl SuggestedPostsState {
    pub fn bar(&self, chat_id: ChatId) -> Option<&SuggestBar> {
        self.bars.get(&chat_id.0)
    }

    /// A draft from TDLib for the chat itself: its terms put the bar up or
    /// take it down. An edit in progress keeps its bar.
    pub fn sync_draft(&mut self, chat_id: ChatId, terms: Option<SuggestTerms>) {
        if self
            .bars
            .get(&chat_id.0)
            .is_some_and(|bar| bar.edit_of.is_some())
        {
            return;
        }
        match terms {
            Some(terms) => {
                let bar = self
                    .bars
                    .entry(chat_id.0)
                    .or_insert_with(|| SuggestBar::new(terms));
                bar.terms = terms;
            }
            None => {
                self.bars.remove(&chat_id.0);
            }
        }
    }

    /// The terms a draft of `chat_id` carries (a bar up for a new offer).
    pub fn draft_terms(&self, chat_id: ChatId) -> Option<SuggestTerms> {
        self.bars
            .get(&chat_id.0)
            .filter(|bar| bar.edit_of.is_none())
            .map(|bar| bar.terms)
    }
}

/// Updates and answers of this feature (in `PaymentsPayload`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SuggestedPostPayload {
    /// `updateMessageSuggestedPostInfo`.
    InfoUpdated {
        chat_id: ChatId,
        message_id: MessageId,
        info: Option<SuggestedPostInfo>,
    },
    /// `updateOwnedGramCount`, in nano-Grams.
    GramBalance(i64),
}

/// Parses this feature's TDLib types.
pub fn parse_payload(type_name: &str, value: &Value) -> Option<SuggestedPostPayload> {
    match type_name {
        "updateMessageSuggestedPostInfo" => Some(SuggestedPostPayload::InfoUpdated {
            chat_id: ChatId(int(value.get("chat_id"))),
            message_id: MessageId(int(value.get("message_id"))),
            info: SuggestedPostInfo::parse(value.get("suggested_post_info")),
        }),
        "updateOwnedGramCount" => Some(SuggestedPostPayload::GramBalance(
            int(value.get("gram_amount")).max(0),
        )),
        _ => None,
    }
}

/// Requests of this feature (in `PaymentsPurpose`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SuggestedPostPurpose {
    /// `approveSuggestedPost`. Answer `ok`.
    Approve { chat_id: i64, message_id: i64 },
    /// `declineSuggestedPost`. Answer `ok`.
    Decline { chat_id: i64, message_id: i64 },
    /// `addOffer`: new terms for a suggestion, or an offer for a plain
    /// message. Answer `message` (applied as a new message).
    AddOffer { chat_id: i64, message_id: i64 },
    /// `sendMessage` replying to a suggestion with changed content.
    /// Answer `message`.
    SendChange { chat_id: i64, message_id: i64 },
}

impl SuggestedPostPurpose {
    /// The suggestion the request is about.
    pub fn key(self) -> (i64, i64) {
        match self {
            Self::Approve {
                chat_id,
                message_id,
            }
            | Self::Decline {
                chat_id,
                message_id,
            }
            | Self::AddOffer {
                chat_id,
                message_id,
            }
            | Self::SendChange {
                chat_id,
                message_id,
            } => (chat_id, message_id),
        }
    }
}

/// The toast for a refused request. Quill keeps only the error's class,
/// never TDLib's text (it can carry private data), so the toast names
/// what failed and the class.
pub fn error_note(purpose: SuggestedPostPurpose, reason: &str) -> String {
    let what = match purpose {
        SuggestedPostPurpose::Approve { .. } => "publish the post",
        SuggestedPostPurpose::Decline { .. } => "decline the post",
        SuggestedPostPurpose::AddOffer { .. } | SuggestedPostPurpose::SendChange { .. } => {
            "send the offer"
        }
    };
    format!("Couldn't {what}: {reason}.")
}

/// Whether "Edit Message" is offered for the suggestion's content
/// (`CanEditSuggestedMessage`: no rich page, and media that takes a
/// caption or none).
pub fn can_edit_message(content: &crate::telegram::envelope::MessageContent) -> bool {
    use crate::telegram::envelope::MessageContent as C;
    matches!(
        content,
        C::Text(_)
            | C::Photo(_)
            | C::Video(_)
            | C::Document(_)
            | C::Audio(_)
            | C::Animation(_)
            | C::VoiceNote(_)
    )
}

fn int(value: Option<&Value>) -> i64 {
    value
        .and_then(|v| v.as_i64().or_else(|| v.as_str()?.parse().ok()))
        .unwrap_or(0)
}

fn date(value: Option<&Value>) -> i32 {
    i32::try_from(int(value).max(0)).unwrap_or(i32::MAX)
}

#[cfg(test)]
#[path = "suggested_posts_tests.rs"]
mod tests;
