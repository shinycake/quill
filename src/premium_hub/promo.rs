//! The Premium promo page: plans with prices, the "Doubled limits" page and
//! the limit lookup behind the limit boxes.
//!
//! tdesktop reference: `settings/settings_premium.cpp` (header, feature
//! list, "Doubled limits" row, the subscribe button with the price per
//! month) and `boxes/premium_limits_box.cpp` (`lng_premium_subscribe_*`).
//! Nothing here buys anything: the subscribe button opens @PremiumBot.

use super::{PremiumInfo, flag, int, text_of};
use crate::premium_limits::{LimitPair, PremiumLimitKind};
use crate::telegram::envelope::format_payment_price;
use serde_json::Value;

/// Which page of the Premium dialog is showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PromoPage {
    /// Header, feature list and the subscribe button.
    #[default]
    Overview,
    /// "Doubled limits": every limit with its free and Premium value.
    Limits,
    /// The preview of the feature at this index of the feature list, with
    /// its promotion clip (tdesktop `premium_preview_box.cpp`).
    Preview(usize),
}

/// One `premiumStatePaymentOption`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PremiumPlan {
    pub months: i64,
    pub currency: String,
    /// Smallest currency units for the whole plan.
    pub amount: i64,
    pub discount_percentage: i64,
    pub is_current: bool,
    pub is_upgrade: bool,
}

/// The plans in `premiumState.payment_options`; rows without a price or a
/// month count are dropped.
pub fn parse_plans(state: &Value) -> Vec<PremiumPlan> {
    state
        .get("payment_options")
        .and_then(Value::as_array)
        .map(|rows| {
            rows.iter()
                .filter_map(|row| {
                    let option = row.get("payment_option")?;
                    let months = int(option.get("month_count"));
                    let amount = int(option.get("amount"));
                    let currency = text_of(option, "currency");
                    if months <= 0 || amount <= 0 || currency.is_empty() {
                        return None;
                    }
                    Some(PremiumPlan {
                        months,
                        currency,
                        amount,
                        discount_percentage: int(option.get("discount_percentage")),
                        is_current: flag(row, "is_current"),
                        is_upgrade: flag(row, "is_upgrade"),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// One promotion clip: which feature (by its row title) and the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PremiumAnimation {
    pub title: &'static str,
    pub file_id: i32,
    pub width: i32,
    pub height: i32,
}

/// `premiumState.animations`; unknown features and clips without a file
/// are dropped.
pub fn parse_animations(state: &Value) -> Vec<PremiumAnimation> {
    state
        .get("animations")
        .and_then(Value::as_array)
        .map(|rows| {
            rows.iter()
                .filter_map(|row| {
                    let kind = row.get("feature")?.get("@type")?.as_str()?;
                    let title = super::feature_row(kind)?.title;
                    let clip = row.get("animation")?;
                    let file_id = i32::try_from(int(clip.get("animation")?.get("id"))).ok()?;
                    if file_id <= 0 {
                        return None;
                    }
                    Some(PremiumAnimation {
                        title,
                        file_id,
                        width: int(clip.get("width")) as i32,
                        height: int(clip.get("height")) as i32,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The clip of the feature with this row title.
pub fn animation_for<'a>(
    animations: &'a [PremiumAnimation],
    title: &str,
) -> Option<&'a PremiumAnimation> {
    animations.iter().find(|a| a.title == title)
}

/// The preview page one step from `index` in a list of `count` features,
/// wrapping around (`delta` is 1 or -1).
pub fn step_preview(index: usize, count: usize, delta: i32) -> usize {
    if count == 0 {
        return 0;
    }
    (index as i64 + i64::from(delta)).rem_euclid(count as i64) as usize
}

impl PremiumPlan {
    /// Name of the plan (tdesktop `lng_premium_subscribe_months_*`).
    pub fn name(&self) -> String {
        match self.months {
            1 => "Monthly".into(),
            6 => "Semiannual".into(),
            12 => "Annual".into(),
            24 => "2-Year".into(),
            n => format!("{n} months"),
        }
    }

    /// Price per month, rounded to the nearest unit.
    pub fn per_month_text(&self) -> String {
        let per_month = (self.amount + self.months / 2) / self.months;
        format_payment_price(&self.currency, per_month)
    }

    /// Price for the whole plan, `lng_premium_subscribe_total`.
    pub fn total_text(&self) -> String {
        let total = format_payment_price(&self.currency, self.amount);
        match self.months {
            1 => format!("{total} per month"),
            12 => format!("{total} per year"),
            n => format!("{total} per {n} months"),
        }
    }
}

/// The plan the subscribe button names: the monthly one, else the first.
pub fn button_plan(plans: &[PremiumPlan]) -> Option<&PremiumPlan> {
    plans
        .iter()
        .find(|plan| plan.months == 1)
        .or_else(|| plans.first())
}

/// "Subscribe for {price} per month" (`lng_premium_subscribe_button`).
pub fn subscribe_label(plans: &[PremiumPlan]) -> String {
    match button_plan(plans) {
        Some(plan) => format!("Subscribe for {} per month", plan.per_month_text()),
        None => "Subscribe".into(),
    }
}

/// One line of the "Doubled limits" page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LimitLine {
    pub label: String,
    pub default_value: i64,
    pub premium_value: i64,
    pub unit: &'static str,
}

impl PremiumInfo {
    /// The free and Premium value TDLib reported for `kind`.
    pub fn limit_pair(&self, kind: PremiumLimitKind) -> Option<LimitPair> {
        self.limit_pair_for_type(kind.td_limit_type()?)
    }

    /// The free and Premium value TDLib reported for the
    /// `premiumLimitType*` constructor `type_name`.
    pub fn limit_pair_for_type(&self, type_name: &str) -> Option<LimitPair> {
        self.limits
            .iter()
            .find(|row| row.type_name == type_name)
            .map(|row| LimitPair {
                default_value: row.default_value,
                premium_value: row.premium_value,
            })
    }
}

/// The limit for `kind` from the loaded Premium info, else the published
/// fallback.
pub fn limit_pair(info: Option<&PremiumInfo>, kind: PremiumLimitKind) -> LimitPair {
    info.and_then(|info| info.limit_pair(kind))
        .unwrap_or_else(|| kind.fallback())
}

/// Lines of the "Doubled limits" page: whatever the server reported, in its
/// order, then the file size (a feature, not a limit type). Without
/// `getPremiumFeatures` yet it falls back to the known set.
pub fn limit_lines(info: Option<&PremiumInfo>) -> Vec<LimitLine> {
    let mut lines: Vec<LimitLine> = match info {
        Some(info) if !info.limits.is_empty() => info
            .limits
            .iter()
            .map(|row| LimitLine {
                label: row.title.to_string(),
                default_value: row.default_value,
                premium_value: row.premium_value,
                unit: PremiumLimitKind::from_td_limit_type(&row.type_name)
                    .map_or("", PremiumLimitKind::unit),
            })
            .collect(),
        _ => PremiumLimitKind::ALL
            .into_iter()
            .filter(|kind| *kind != PremiumLimitKind::FileSize)
            .map(|kind| line_of(kind, kind.fallback()))
            .collect(),
    };
    let size = PremiumLimitKind::FileSize;
    lines.push(line_of(size, size.fallback()));
    lines
}

fn line_of(kind: PremiumLimitKind, pair: LimitPair) -> LimitLine {
    LimitLine {
        label: kind.label().to_string(),
        default_value: pair.default_value,
        premium_value: pair.premium_value,
        unit: kind.unit(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::premium_hub::{parse_premium_features, parse_premium_state};
    use serde_json::json;

    fn state() -> Value {
        json!({
            "state": {"text": "", "entities": []},
            "payment_options": [
                {"payment_option": {"currency": "USD", "amount": 499, "discount_percentage": 0,
                    "month_count": 1}, "is_current": false, "is_upgrade": false},
                {"payment_option": {"currency": "USD", "amount": 2999, "discount_percentage": 50,
                    "month_count": 12}, "is_current": false, "is_upgrade": false},
                {"payment_option": {"currency": "", "amount": 1, "month_count": 3}},
                {"payment_option": {"currency": "USD", "amount": 100, "month_count": 0}}
            ]
        })
    }

    #[test]
    fn plans_keep_priced_options_and_name_them() {
        let plans = parse_plans(&state());
        assert_eq!(plans.len(), 2);
        assert_eq!(plans[0].name(), "Monthly");
        assert_eq!(plans[1].name(), "Annual");
        assert_eq!(plans[1].discount_percentage, 50);
        assert_eq!(plans[1].per_month_text(), "USD 2.50");
        assert_eq!(plans[1].total_text(), "USD 29.99 per year");
        assert_eq!(plans[0].total_text(), "USD 4.99 per month");
        // The state parser carries the same list.
        assert_eq!(parse_premium_state(&state()).options, plans);
    }

    #[test]
    fn animations_map_to_feature_rows() {
        let state = json!({"animations": [
            {"feature": {"@type": "premiumFeatureVoiceRecognition"},
             "animation": {"width": 640, "height": 480, "animation": {"id": 77}}},
            {"feature": {"@type": "premiumFeatureNewerThanThis"},
             "animation": {"animation": {"id": 5}}},
            {"feature": {"@type": "premiumFeatureDisabledAds"},
             "animation": {"animation": {"id": 0}}}
        ]});
        let clips = parse_animations(&state);
        assert_eq!(clips.len(), 1);
        let clip = animation_for(&clips, "Voice-to-text conversion").unwrap();
        assert_eq!((clip.file_id, clip.width, clip.height), (77, 640, 480));
        assert!(animation_for(&clips, "No ads").is_none());
        assert_eq!(parse_premium_state(&state).animations, clips);
    }

    #[test]
    fn preview_paging_wraps() {
        assert_eq!(step_preview(0, 5, -1), 4);
        assert_eq!(step_preview(4, 5, 1), 0);
        assert_eq!(step_preview(2, 5, 1), 3);
        assert_eq!(step_preview(0, 0, 1), 0);
    }

    #[test]
    fn subscribe_button_names_the_monthly_price() {
        let plans = parse_plans(&state());
        assert_eq!(subscribe_label(&plans), "Subscribe for USD 4.99 per month");
        assert_eq!(
            subscribe_label(&plans[1..]),
            "Subscribe for USD 2.50 per month"
        );
        assert_eq!(subscribe_label(&[]), "Subscribe");
    }

    fn info() -> PremiumInfo {
        parse_premium_features(&json!({
            "features": [],
            "limits": [
                {"type": {"@type": "premiumLimitTypePinnedChatCount"}, "default_value": 5, "premium_value": 10},
                {"type": {"@type": "premiumLimitTypeChatFolderCount"}, "default_value": 10, "premium_value": 20},
                {"type": {"@type": "premiumLimitTypeCaptionLength"}, "default_value": 1024, "premium_value": 4096}
            ]
        }))
    }

    #[test]
    fn limit_lookup_prefers_the_server_then_the_fallback() {
        let info = info();
        let pins = limit_pair(Some(&info), PremiumLimitKind::Pins);
        assert_eq!((pins.default_value, pins.premium_value), (5, 10));
        // Not reported: published fallback.
        let gifs = limit_pair(Some(&info), PremiumLimitKind::SavedGifs);
        assert_eq!(gifs, PremiumLimitKind::SavedGifs.fallback());
        assert_eq!(
            limit_pair(None, PremiumLimitKind::Channels),
            PremiumLimitKind::Channels.fallback()
        );
    }

    #[test]
    fn limit_page_lists_server_rows_then_file_size() {
        let lines = limit_lines(Some(&info()));
        let labels: Vec<_> = lines.iter().map(|l| l.label.as_str()).collect();
        assert_eq!(
            labels,
            ["Pinned chats", "Folders", "Caption length", "File size"]
        );
        assert_eq!(lines[1].unit, "");
        assert_eq!(lines[3].premium_value, 4);
        let known = limit_lines(None);
        assert_eq!(known.len(), PremiumLimitKind::ALL.len());
        assert_eq!(known.last().map(|l| l.label.as_str()), Some("File size"));
    }
}
