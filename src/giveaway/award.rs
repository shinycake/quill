//! Premium gift codes for chosen members and the country list of the form
//! (tdesktop `create_giveaway_box.cpp`: "Award specific users" and
//! `SelectCountriesBox`).
//!
//! Choosing members swaps the random giveaway for one gift code per member.
//! tdesktop prices it as the one-person price times the number of members
//! (`PremiumGiftCodeOptions::optionsForGiveaway`); the server checks the
//! amount.
use super::{ADDITIONAL_CHATS_MAX, COUNTRIES_MAX, GiveawayDraft, GiveawayKind};
use crate::telegram::envelope::PremiumGiveawayOption;

impl GiveawayDraft {
    /// The giveaway hands a Premium gift code to chosen members.
    pub fn is_specific_users(&self) -> bool {
        self.prepaid.is_none()
            && self.kind == GiveawayKind::Premium
            && !self.award_user_ids.is_empty()
    }

    /// Add or remove a member to award; refuses one past the limit.
    pub fn toggle_award_user(&mut self, user_id: i64) -> Result<(), super::GiveawayFormError> {
        if let Some(index) = self.award_user_ids.iter().position(|id| *id == user_id) {
            self.award_user_ids.remove(index);
            return Ok(());
        }
        if self.award_user_ids.len() >= ADDITIONAL_CHATS_MAX {
            return Err(super::GiveawayFormError::TooManyUsers);
        }
        self.award_user_ids.push(user_id);
        Ok(())
    }

    /// Add or remove a country; refuses one past the limit.
    pub fn toggle_country(&mut self, code: &str) -> Result<(), super::GiveawayFormError> {
        if let Some(index) = self.country_codes.iter().position(|c| c == code) {
            self.country_codes.remove(index);
            return Ok(());
        }
        if self.country_codes.len() >= COUNTRIES_MAX {
            return Err(super::GiveawayFormError::TooManyCountries);
        }
        self.country_codes.push(code.to_string());
        Ok(())
    }
}

/// Months a gift code can last, ascending.
pub fn gift_code_months(options: &[PremiumGiveawayOption]) -> Vec<i32> {
    let mut months: Vec<i32> = options.iter().map(|o| o.month_count).collect();
    months.sort_unstable();
    months.dedup();
    months
}

/// The price of `months` of Premium for `users` members: the currency and
/// the one-person price times `users`. The one-person price comes from the
/// option with the fewest prizes, divided by its prize count.
pub fn gift_code_price(
    options: &[PremiumGiveawayOption],
    months: i32,
    users: usize,
) -> Option<(String, i64)> {
    let base = options
        .iter()
        .filter(|o| o.month_count == months && o.winner_count > 0)
        .min_by_key(|o| o.winner_count)?;
    let users = i64::try_from(users).ok().filter(|u| *u > 0)?;
    let each = base.amount / i64::from(base.winner_count);
    Some((base.currency.clone(), each.checked_mul(users)?))
}

/// Keeps the draft's months when a gift code offers them, else takes the
/// shortest.
pub fn settle_gift_code_months(draft: &mut GiveawayDraft, options: &[PremiumGiveawayOption]) {
    let months = gift_code_months(options);
    if !months.contains(&draft.premium_months) {
        draft.premium_months = months.first().copied().unwrap_or(0);
    }
}
