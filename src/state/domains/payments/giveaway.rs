//! Giveaways: the "About this giveaway" lookup, the payment options for a
//! new one and the progress of starting a prepaid one.
use crate::state::*;
use crate::telegram::envelope::{
    GiveawayInfoData, GiveawaySummary, PremiumGiveawayOption, StarGiveawayOption,
};

/// A giveaway message the user asked about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GiveawayInfoLookup {
    /// The chat and message `getGiveawayInfo` was asked about.
    pub chat_id: i64,
    pub message_id: i64,
    pub summary: GiveawaySummary,
    /// `None` while loading or after a failed lookup.
    pub info: Option<GiveawayInfoData>,
    pub loading: bool,
    pub error: Option<String>,
}

/// What a new giveaway can be bought with.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GiveawayOptions {
    pub premium: Vec<PremiumGiveawayOption>,
    pub stars: Vec<StarGiveawayOption>,
    pub premium_loading: bool,
    pub stars_loading: bool,
    pub premium_error: Option<String>,
    pub stars_error: Option<String>,
}

impl GiveawayOptions {
    pub fn loading(&self) -> bool {
        self.premium_loading || self.stars_loading
    }
}

/// Starting a prepaid giveaway.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum GiveawayLaunch {
    #[default]
    Idle,
    /// `launchPrepaidGiveaway` is in flight for the boosted chat.
    Launching {
        chat_id: i64,
    },
    Launched,
    Failed(String),
}

/// The payments domain's giveaway state.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GiveawayState {
    pub info: Option<GiveawayInfoLookup>,
    pub options: GiveawayOptions,
    pub launch: GiveawayLaunch,
}

impl Session {
    /// `giveawayInfo*` for our own `getGiveawayInfo`.
    pub(crate) fn apply_giveaway_info(
        &mut self,
        info: GiveawayInfoData,
        pending: Option<&PendingRequest>,
    ) {
        if pending.map(|p| p.purpose) == Some(RequestPurpose::GetGiveawayInfo)
            && let Some(lookup) = self.payments.giveaway.info.as_mut()
        {
            lookup.loading = false;
            lookup.error = None;
            lookup.info = Some(info);
        }
    }

    /// `premiumGiveawayPaymentOptions`.
    pub(crate) fn apply_premium_giveaway_options(
        &mut self,
        options: Vec<PremiumGiveawayOption>,
        pending: Option<&PendingRequest>,
    ) {
        if pending.map(|p| p.purpose) == Some(RequestPurpose::GetPremiumGiveawayOptions) {
            let state = &mut self.payments.giveaway.options;
            state.premium_loading = false;
            state.premium_error = None;
            state.premium = options;
        }
    }

    /// `starGiveawayPaymentOptions`.
    pub(crate) fn apply_star_giveaway_options(
        &mut self,
        options: Vec<StarGiveawayOption>,
        pending: Option<&PendingRequest>,
    ) {
        if pending.map(|p| p.purpose) == Some(RequestPurpose::GetStarGiveawayOptions) {
            let state = &mut self.payments.giveaway.options;
            state.stars_loading = false;
            state.stars_error = None;
            state.stars = options;
        }
    }

    /// `ok` for `launchPrepaidGiveaway`. The boost numbers are stale now,
    /// so the cached status goes and the boosts page asks again.
    pub(crate) fn apply_giveaway_ok(&mut self, pending: Option<&PendingRequest>) {
        if pending.map(|p| p.purpose) != Some(RequestPurpose::LaunchPrepaidGiveaway) {
            return;
        }
        if let GiveawayLaunch::Launching { chat_id } = self.payments.giveaway.launch {
            self.groups.chat_boost_status.remove(&chat_id);
            self.groups.chat_boost_details.remove(&chat_id);
        }
        self.payments.giveaway.launch = GiveawayLaunch::Launched;
    }

    /// A failed lookup, option fetch or launch keeps its dialog open with
    /// the reason.
    pub(crate) fn apply_giveaway_error(&mut self, err: &TdError, pending: Option<&PendingRequest>) {
        let reason = error_reason(err);
        let state = &mut self.payments.giveaway;
        match pending.map(|p| p.purpose) {
            Some(RequestPurpose::GetGiveawayInfo) => {
                if let Some(lookup) = state.info.as_mut() {
                    lookup.loading = false;
                    lookup.info = None;
                    lookup.error = Some("This giveaway is not available any more.".into());
                }
            }
            Some(RequestPurpose::GetPremiumGiveawayOptions) => {
                state.options.premium_loading = false;
                state.options.premium_error = Some(reason);
            }
            Some(RequestPurpose::GetStarGiveawayOptions) => {
                state.options.stars_loading = false;
                state.options.stars_error = Some(reason);
            }
            Some(RequestPurpose::LaunchPrepaidGiveaway) => {
                state.launch =
                    GiveawayLaunch::Failed(format!("Couldn't start the giveaway: {reason}"));
            }
            _ => {}
        }
    }
}
