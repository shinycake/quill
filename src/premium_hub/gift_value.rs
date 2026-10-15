//! The value of an upgraded gift (tdesktop `UniqueGiftValueBox` and
//! `AddUniqueGiftValueTable`, `settings/settings_credits_graphics.cpp`,
//! `boxes/gift_premium_box.cpp`).
//!
//! The unique gift box shows a "Value" row with the estimate TDLib puts on
//! `upgradedGift` (`value_currency`, `value_amount`). Its "learn more" asks
//! `getUpgradedGiftValueInfo` (a read) and the value box opens with the
//! answer: the price, why it is that price, the sale history table and how
//! many gifts of the kind are for sale on Telegram and on Fragment.
//!
//! Every price here is in the smallest units of `currency` (fiat, e.g. USD
//! cents), not TON.
#![deny(clippy::indexing_slicing)]

use super::{flag, group_digits, int, text_of};
use crate::ids::RequestId;
use crate::lang::{Key, keys, tr, tr_with, trn_with};
use crate::telegram::envelope::StickerContent;
use serde_json::{Value, json};

/// `upgradedGiftValueInfo`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GiftValueInfo {
    pub currency: String,
    pub value: i64,
    pub is_value_average: bool,
    pub initial_sale_date: i64,
    pub initial_sale_star_count: i64,
    pub initial_sale_price: i64,
    /// 0 when the gift was never resold.
    pub last_sale_date: i64,
    pub last_sale_price: i64,
    pub is_last_sale_on_fragment: bool,
    /// 0 when no gift of the kind is listed.
    pub minimum_price: i64,
    /// 0 when none was sold last month.
    pub average_sale_price: i64,
    pub telegram_listed_gift_count: i64,
    pub fragment_listed_gift_count: i64,
    /// Fragment's listings of the kind; may be empty.
    pub fragment_url: String,
}

/// Parse an `upgradedGiftValueInfo`; `None` for any other object.
/// Amounts and counts below zero read as zero.
pub fn parse_gift_value_info(value: &Value) -> Option<GiftValueInfo> {
    if value.get("@type").and_then(Value::as_str) != Some("upgradedGiftValueInfo") {
        return None;
    }
    let amount = |key: &str| int(value.get(key)).max(0);
    Some(GiftValueInfo {
        currency: text_of(value, "currency"),
        value: amount("value"),
        is_value_average: flag(value, "is_value_average"),
        initial_sale_date: amount("initial_sale_date"),
        initial_sale_star_count: amount("initial_sale_star_count"),
        initial_sale_price: amount("initial_sale_price"),
        last_sale_date: amount("last_sale_date"),
        last_sale_price: amount("last_sale_price"),
        is_last_sale_on_fragment: flag(value, "is_last_sale_on_fragment"),
        minimum_price: amount("minimum_price"),
        average_sale_price: amount("average_sale_price"),
        telegram_listed_gift_count: amount("telegram_listed_gift_count"),
        fragment_listed_gift_count: amount("fragment_listed_gift_count"),
        fragment_url: text_of(value, "fragment_url"),
    })
}

/// The answers this module parses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GiftValuePayload {
    Info(GiftValueInfo),
}

pub fn parse_payload(type_name: &str, value: &Value) -> Option<GiftValuePayload> {
    match type_name {
        "upgradedGiftValueInfo" => parse_gift_value_info(value).map(GiftValuePayload::Info),
        _ => None,
    }
}

/// `getUpgradedGiftValueInfo` for the gift with the unique `name`.
pub fn get_upgraded_gift_value_info(extra: RequestId, name: &str) -> String {
    json!({
        "@type": "getUpgradedGiftValueInfo",
        "@extra": extra.as_extra(),
        "name": name,
    })
    .to_string()
}

/// The gift whose value is asked for or shown, and how that went.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GiftValueLookup {
    /// The unique name (`t.me/nft/<name>`) the request carries.
    pub name: String,
    /// "Plush Pepe".
    pub title: String,
    /// "Plush Pepe #1,234".
    pub full_name: String,
    /// The regular gift, for the Telegram listings.
    pub regular_gift_id: i64,
    pub sticker: Option<StickerContent>,
    /// `@extra` of the request in flight; any other answer is stale.
    pub request: u64,
    pub loading: bool,
    pub info: Option<GiftValueInfo>,
    /// Why the last request failed, shown under the Value row.
    pub error: Option<String>,
    /// The value box is showing.
    pub open: bool,
}

impl GiftValueLookup {
    pub fn new(collectible: &super::upgraded::Collectible) -> Self {
        Self {
            name: collectible.name.clone(),
            title: collectible.title.clone(),
            full_name: collectible.full_name(),
            regular_gift_id: collectible.regular_gift_id,
            sticker: collectible.model.sticker.clone(),
            ..Self::default()
        }
    }

    /// An answer for request `id` arrived; `false` when it is stale (the
    /// user asked about another gift meanwhile) and was dropped.
    pub fn land(&mut self, id: u64, info: GiftValueInfo) -> bool {
        if !self.loading || id != self.request {
            return false;
        }
        self.loading = false;
        self.error = None;
        self.info = Some(info);
        self.open = true;
        true
    }

    /// Request `id` failed; stale failures are dropped like answers.
    pub fn fail(&mut self, id: u64, reason: String) -> bool {
        if !self.loading || id != self.request {
            return false;
        }
        self.loading = false;
        self.error = Some(reason);
        true
    }
}

/// The estimate on `upgradedGift`, when the gift has one: the Value row
/// is shown only then (tdesktop `unique->value`).
pub fn row_value(collectible: &super::upgraded::Collectible) -> Option<String> {
    (collectible.value_amount > 0 && !collectible.value_currency.is_empty())
        .then(|| format_value_price(collectible.value_amount, &collectible.value_currency, true))
}

/// tdesktop `FormatValuePrice`: "~" when approximate, then the amount
/// with the currency sign in front ("$1,234.50"; Quill's fiat formatter,
/// two decimals for most currencies).
pub fn format_value_price(amount: i64, currency: &str, approximately: bool) -> String {
    let exponent = crate::telegram::envelope::currency_exponent(currency);
    let divisor = 10_f64.powi(i32::try_from(exponent).unwrap_or(2));
    let units = amount.max(0) as f64 / divisor;
    let symbol = crate::ton_wallet::fiat::currency_symbol(currency);
    let text =
        crate::ton_wallet::fiat::format_fiat(units, symbol, usize::try_from(exponent).unwrap_or(2));
    if approximately {
        format!("~{text}")
    } else {
        text
    }
}

const MONTHS: [Key; 12] = [
    keys::MONTH_DAY1,
    keys::MONTH_DAY2,
    keys::MONTH_DAY3,
    keys::MONTH_DAY4,
    keys::MONTH_DAY5,
    keys::MONTH_DAY6,
    keys::MONTH_DAY7,
    keys::MONTH_DAY8,
    keys::MONTH_DAY9,
    keys::MONTH_DAY10,
    keys::MONTH_DAY11,
    keys::MONTH_DAY12,
];

/// tdesktop `FormatValueDate`: "January 5, 2025" in local time.
pub fn format_value_date(unix: i64) -> String {
    let time = crate::local_time::civil_local(unix);
    let month = MONTHS
        .get(usize::from(time.month.clamp(1, 12)) - 1)
        .map(|key| tr(*key))
        .unwrap_or_default();
    tr_with(
        keys::MONTH_DAY_YEAR,
        &[
            ("month", month),
            ("day", &time.day.to_string()),
            ("year", &time.year.to_string()),
        ],
    )
}

/// tdesktop `FormatExactCountDecimal`: whole numbers grouped, others with
/// their (at most two) decimals.
fn exact_decimal(number: f64) -> String {
    let rounded = number.round();
    if (number - rounded).abs() < 1e-9 {
        return group_digits(rounded as i64);
    }
    let text = format!("{number:.2}");
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// The change from the initial to the last price beside "Last Price"
/// (tdesktop `MakePriceWithChangePercentValue`): "+150%", "-3.25%"; whole
/// percents from 10% up, two decimals below. `None` without an initial
/// price.
pub fn change_percent(last: i64, initial: i64) -> Option<String> {
    if initial <= 0 {
        return None;
    }
    let diff = 100.0 * (last as f64 - initial as f64) / initial as f64;
    if !diff.is_finite() {
        return None;
    }
    let shown = if diff.abs() >= 10.0 {
        diff.round()
    } else {
        ((diff * 100.0).round() as i64) as f64 / 100.0
    };
    let prefix = if shown > 0.0 { "+" } else { "" };
    Some(format!("{prefix}{}%", exact_decimal(shown)))
}

/// Which explanation the box gives under the price.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AboutLine {
    /// "This is the price at which {gift} was last sold on {platform}."
    LastSale { on_fragment: bool },
    /// "This is the average sale price of {gift} gifts on Telegram and
    /// Fragment over the past month."
    Average,
}

/// tdesktop: the last sale explains the price when it is above the
/// average (`lastSalePrice > averagePrice`); otherwise the average does.
pub fn about_line(info: &GiftValueInfo) -> AboutLine {
    if info.last_sale_price > info.average_sale_price {
        AboutLine::LastSale {
            on_fragment: info.is_last_sale_on_fragment,
        }
    } else {
        AboutLine::Average
    }
}

/// The about text and the part of it shown in bold (the gift's name: with
/// the number for a last sale, the collection title for the average).
pub fn about_text(info: &GiftValueInfo, title: &str, full_name: &str) -> (String, String) {
    match about_line(info) {
        AboutLine::LastSale { on_fragment } => {
            let platform = if on_fragment {
                tr(keys::GIFT_VALUE_FRAGMENT)
            } else {
                tr(keys::GIFT_VALUE_TELEGRAM)
            };
            (
                tr_with(
                    keys::GIFT_VALUE_ABOUT_LAST,
                    &[("gift", full_name), ("platform", platform)],
                ),
                full_name.to_string(),
            )
        }
        AboutLine::Average => (
            tr_with(keys::GIFT_VALUE_ABOUT_AVERAGE, &[("gift", title)]),
            title.to_string(),
        ),
    }
}

/// What a row of the value table is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueRowKind {
    InitialSale,
    InitialPrice,
    LastSale,
    LastPrice,
    MinimumPrice,
    AveragePrice,
}

impl ValueRowKind {
    /// Stable id for the row's element.
    pub fn id(self) -> &'static str {
        match self {
            Self::InitialSale => "initial-sale",
            Self::InitialPrice => "initial-price",
            Self::LastSale => "last-sale",
            Self::LastPrice => "last-price",
            Self::MinimumPrice => "minimum-price",
            Self::AveragePrice => "average-price",
        }
    }
}

/// One row of the value table, ready to draw.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValueRow {
    pub kind: ValueRowKind,
    pub label: String,
    pub value: String,
    /// A Star sign goes before the value ("Initial Price").
    pub stars_icon: bool,
    /// The pill beside "Last Price" ("+150%").
    pub change: Option<String>,
    /// The explanation behind the value's info mark.
    pub tooltip: Option<String>,
}

impl ValueRow {
    fn new(kind: ValueRowKind, label: Key, value: String) -> Self {
        Self {
            kind,
            label: tr(label).to_string(),
            value,
            stars_icon: false,
            change: None,
            tooltip: None,
        }
    }
}

/// tdesktop `AddUniqueGiftValueTable`: dates only when known, the initial
/// price always, the other prices only when not zero.
pub fn table_rows(info: &GiftValueInfo, title: &str) -> Vec<ValueRow> {
    let price = |amount: i64| format_value_price(amount, &info.currency, false);
    let mut rows = Vec::new();
    if info.initial_sale_date > 0 {
        rows.push(ValueRow::new(
            ValueRowKind::InitialSale,
            keys::GIFT_VALUE_INITIAL_SALE,
            format_value_date(info.initial_sale_date),
        ));
    }
    let mut initial = ValueRow::new(
        ValueRowKind::InitialPrice,
        keys::GIFT_VALUE_INITIAL_PRICE,
        tr_with(
            keys::GIFT_VALUE_INITIAL_PRICE_VALUE,
            &[
                ("stars", &group_digits(info.initial_sale_star_count)),
                (
                    "amount",
                    &format_value_price(info.initial_sale_price, &info.currency, true),
                ),
            ],
        ),
    );
    initial.stars_icon = true;
    rows.push(initial);
    if info.last_sale_date > 0 {
        rows.push(ValueRow::new(
            ValueRowKind::LastSale,
            keys::GIFT_VALUE_LAST_SALE,
            format_value_date(info.last_sale_date),
        ));
    }
    if info.last_sale_price > 0 {
        let mut last = ValueRow::new(
            ValueRowKind::LastPrice,
            keys::GIFT_VALUE_LAST_PRICE,
            price(info.last_sale_price),
        );
        last.change = change_percent(info.last_sale_price, info.initial_sale_price);
        rows.push(last);
    }
    if info.minimum_price > 0 {
        let amount = price(info.minimum_price);
        let mut row = ValueRow::new(
            ValueRowKind::MinimumPrice,
            keys::GIFT_VALUE_MINIMUM_PRICE,
            amount.clone(),
        );
        row.tooltip = Some(tr_with(
            keys::GIFT_VALUE_MINIMUM_PRICE_TOOLTIP,
            &[("amount", &amount), ("gift", title)],
        ));
        rows.push(row);
    }
    if info.average_sale_price > 0 {
        let amount = price(info.average_sale_price);
        let mut row = ValueRow::new(
            ValueRowKind::AveragePrice,
            keys::GIFT_VLAUE_AVERAGE_PRICE,
            amount.clone(),
        );
        row.tooltip = Some(tr_with(
            keys::GIFT_VALUE_AVERAGE_PRICE_TOOLTIP,
            &[("amount", &amount), ("gift", title)],
        ));
        rows.push(row);
    }
    rows
}

/// Where gifts of the kind are for sale.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    /// Opens the resale list of the regular gift.
    Telegram,
    /// Opens `fragment_url`.
    Fragment,
}

/// One "{count} {emoji} for sale on {platform} {arrow}" link. `text`
/// keeps the `{emoji}` and `{arrow}` tags for the view to fill.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Availability {
    pub platform: Platform,
    pub count: i64,
    pub text: String,
}

/// The Fragment link, when it is safe to open (https only).
pub fn fragment_link(info: &GiftValueInfo) -> Option<&str> {
    let url = info.fragment_url.trim();
    url.get(..8)
        .is_some_and(|scheme| scheme.eq_ignore_ascii_case("https://"))
        .then_some(url)
        .filter(|url| url.len() > 8)
}

/// tdesktop: Telegram when gifts are listed there, Fragment when gifts
/// are listed there (and its link is usable), in that order.
pub fn availability(info: &GiftValueInfo) -> Vec<Availability> {
    let entry = |platform: Platform, count: i64| {
        let name = match platform {
            Platform::Telegram => tr(keys::GIFT_VALUE_TELEGRAM),
            Platform::Fragment => tr(keys::GIFT_VALUE_FRAGMENT),
        };
        Availability {
            platform,
            count,
            // tdesktop's `lt_count_decimal`: the count with separators.
            text: trn_with(
                keys::GIFT_VALUE_AVAILABILITY,
                count,
                &[("count", &group_digits(count)), ("platform", name)],
            ),
        }
    };
    let mut out = Vec::new();
    if info.telegram_listed_gift_count > 0 {
        out.push(entry(Platform::Telegram, info.telegram_listed_gift_count));
    }
    if info.fragment_listed_gift_count > 0 && fragment_link(info).is_some() {
        out.push(entry(Platform::Fragment, info.fragment_listed_gift_count));
    }
    out
}

#[cfg(test)]
#[path = "gift_value_tests.rs"]
mod tests;
