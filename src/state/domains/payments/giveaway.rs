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
    /// The `getGiveawayInfo` this box waits for: another box's answer (one
    /// closed, then another giveaway opened) settles nothing here.
    pub request: RequestId,
}

impl GiveawayInfoLookup {
    /// `pending` is this box's own lookup: settle it.
    fn take_answer(&mut self, pending: Option<&PendingRequest>) -> bool {
        let ours = self.loading
            && pending.is_some_and(|p| {
                p.purpose == RequestPurpose::GetGiveawayInfo && p.id == self.request
            });
        if ours {
            self.loading = false;
        }
        ours
    }
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
    /// `launchPrepaidGiveaway` `request` is in flight for the boosted
    /// chat. Only its answer ends it: a form opened since starts `Idle`,
    /// and the old launch's answer does not land in it.
    Launching {
        chat_id: i64,
        request: u64,
    },
    Launched,
    /// A new giveaway for `chat_id` was paid for. Its form is done: Start
    /// must not buy the same giveaway again.
    Paid {
        chat_id: i64,
    },
    /// The payment of a new giveaway for `chat_id` waits on the provider's
    /// verification page (3-D Secure). The form stays done: Start must not
    /// buy a second one while that is pending. It ends with the form.
    Verifying {
        chat_id: i64,
    },
    Failed(String),
}

impl GiveawayLaunch {
    /// The giveaway of `chat_id` is created or paid for.
    pub fn is_done_for(&self, chat_id: i64) -> bool {
        match self {
            Self::Launched => true,
            Self::Paid { chat_id: paid } | Self::Verifying { chat_id: paid } => *paid == chat_id,
            _ => false,
        }
    }
}

impl GiveawayState {
    /// Whether `request` is the launch in flight.
    fn launching(&self, request: u64) -> bool {
        matches!(self.launch, GiveawayLaunch::Launching { request: r, .. } if r != 0 && r == request)
    }
}

/// The payments domain's giveaway state.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GiveawayState {
    pub info: Option<GiveawayInfoLookup>,
    pub options: GiveawayOptions,
    pub launch: GiveawayLaunch,
    /// Limits from TDLib's `giveaway_*` options.
    pub limits: crate::giveaway::GiveawayLimits,
    /// The "Choose members" list.
    pub award_members: crate::giveaway::AwardMembers,
}

impl Session {
    /// A page of the "Choose members" list: the people in it, not bots or
    /// yourself.
    pub(crate) fn apply_award_members(
        &mut self,
        generation: u64,
        members: &[crate::telegram::envelope::ParsedChatMember],
        total_count: i32,
    ) {
        use crate::telegram::envelope::MessageSender;
        let ids = members
            .iter()
            .filter_map(|m| match m.member_id {
                MessageSender::User { user_id } => Some(user_id),
                MessageSender::Chat { .. } => None,
            })
            .filter(|id| Some(*id) != self.my_user_id && !self.bots.bot_user_ids.contains(id))
            .collect();
        self.payments.giveaway.award_members.accept(
            generation,
            ids,
            i32::try_from(members.len()).unwrap_or(i32::MAX),
            total_count,
        );
    }

    /// `giveawayInfo*` for our own `getGiveawayInfo`.
    pub(crate) fn apply_giveaway_info(
        &mut self,
        info: GiveawayInfoData,
        pending: Option<&PendingRequest>,
    ) {
        if let Some(lookup) = self.payments.giveaway.info.as_mut()
            && lookup.take_answer(pending)
        {
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
        let Some(pending) = pending.filter(|p| p.purpose == RequestPurpose::LaunchPrepaidGiveaway)
        else {
            return;
        };
        if let Some(chat_id) = pending.chat_id {
            self.groups.chat_boost_status.remove(&chat_id.0);
            self.groups.chat_boost_details.remove(&chat_id.0);
        }
        if self.payments.giveaway.launching(pending.id.0) {
            self.payments.giveaway.launch = GiveawayLaunch::Launched;
        }
    }

    /// A failed lookup, option fetch or launch keeps its dialog open with
    /// the reason.
    pub(crate) fn apply_giveaway_error(&mut self, err: &TdError, pending: Option<&PendingRequest>) {
        let reason = error_reason(err);
        let state = &mut self.payments.giveaway;
        match pending.map(|p| p.purpose) {
            Some(RequestPurpose::GetGiveawayInfo) => {
                if let Some(lookup) = state.info.as_mut()
                    && lookup.take_answer(pending)
                {
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
            Some(RequestPurpose::LaunchPrepaidGiveaway)
                if state.launching(pending.map_or(0, |p| p.id.0)) =>
            {
                state.launch =
                    GiveawayLaunch::Failed(format!("Couldn't start the giveaway: {reason}"));
            }
            _ => {}
        }
    }
}
