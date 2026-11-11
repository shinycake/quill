//! Giveaways: the "About this giveaway" lookup, the payment options for a
//! new one and the progress of starting a prepaid one.
use crate::state::fetch::Fetch;
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
    /// The boosted chat the Premium prices were asked for: they depend on
    /// it, so another chat's prices are never shown for this one.
    pub for_chat: i64,
    pub premium: Fetch<Vec<PremiumGiveawayOption>>,
    pub stars: Fetch<Vec<StarGiveawayOption>>,
}

impl GiveawayOptions {
    pub fn loading(&self) -> bool {
        self.premium.is_loading() || self.stars.is_loading()
    }

    /// The Premium prices held, none while another chat's are gone.
    pub fn premium_options(&self) -> &[PremiumGiveawayOption] {
        self.premium.data().map_or(&[], Vec::as_slice)
    }

    pub fn stars_options(&self) -> &[StarGiveawayOption] {
        self.stars.data().map_or(&[], Vec::as_slice)
    }

    /// Both lists are asked for `boosted_chat_id`. The Premium prices of
    /// another chat are dropped at once, not kept under the spinner.
    pub fn begin(&mut self, boosted_chat_id: i64) {
        if self.for_chat != boosted_chat_id {
            self.for_chat = boosted_chat_id;
            self.premium = Fetch::Idle;
        }
        self.premium.begin();
        self.stars.begin();
    }
}

/// Starting a prepaid giveaway.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum GiveawayLaunch {
    #[default]
    Idle,
    /// `launchPrepaidGiveaway` `request` is in flight for the boosted
    /// chat `chat_id`. Only its answer ends it, so a form opened for
    /// another chat keeps it (see [`GiveawayLaunch::for_form`]): that form
    /// and the driver compare `chat_id`, and coming back to this chat's form
    /// still shows the launch in flight.
    Launching { chat_id: i64, request: u64 },
    /// The prepaid giveaway of `chat_id` started.
    Launched { chat_id: i64 },
    /// A new giveaway for `chat_id` was paid for. Its form is done: Start
    /// must not buy the same giveaway again.
    Paid { chat_id: i64 },
    /// The payment of a new giveaway for `chat_id` waits on the provider's
    /// verification page (3-D Secure). The form stays done: Start must not
    /// buy a second one while that is pending. It ends with the form.
    Verifying { chat_id: i64 },
    /// The launch for `chat_id` failed, with the line its form shows.
    Failed { chat_id: i64, reason: String },
}

impl GiveawayLaunch {
    /// The giveaway of `chat_id` is created or paid for.
    pub fn is_done_for(&self, chat_id: i64) -> bool {
        match self {
            Self::Launched { chat_id: done }
            | Self::Paid { chat_id: done }
            | Self::Verifying { chat_id: done } => *done == chat_id,
            _ => false,
        }
    }

    /// Whether a `launchPrepaidGiveaway` for `chat_id` is in flight.
    pub fn is_launching_for(&self, chat_id: i64) -> bool {
        matches!(self, Self::Launching { chat_id: c, .. } if *c == chat_id)
    }

    /// Why the launch for `chat_id` failed, if it did.
    pub fn failure_for(&self, chat_id: i64) -> Option<&str> {
        match self {
            Self::Failed { chat_id: c, reason } if *c == chat_id => Some(reason),
            _ => None,
        }
    }

    /// What a form starts from when it opens or closes. A launch still in
    /// flight is never forgotten: only its answer ends it, and the form of
    /// its chat must keep Start disabled. An answer already seen is.
    pub fn for_form(self) -> Self {
        match self {
            Self::Launching { .. } => self,
            _ => Self::Idle,
        }
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
        // Only the answer for the chat the form shows now.
        if pending.map(|p| p.purpose) == Some(RequestPurpose::GetPremiumGiveawayOptions)
            && pending.and_then(|p| p.chat_id).map(|c| c.0)
                == Some(self.payments.giveaway.options.for_chat)
        {
            self.payments.giveaway.options.premium.succeed(options);
        }
    }

    /// `starGiveawayPaymentOptions`.
    pub(crate) fn apply_star_giveaway_options(
        &mut self,
        options: Vec<StarGiveawayOption>,
        pending: Option<&PendingRequest>,
    ) {
        if pending.map(|p| p.purpose) == Some(RequestPurpose::GetStarGiveawayOptions) {
            self.payments.giveaway.options.stars.succeed(options);
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
        if let GiveawayLaunch::Launching { chat_id, request } = self.payments.giveaway.launch
            && request != 0
            && request == pending.id.0
        {
            self.payments.giveaway.launch = GiveawayLaunch::Launched { chat_id };
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
                if pending.and_then(|p| p.chat_id).map(|c| c.0) == Some(state.options.for_chat) {
                    state.options.premium.fail_dropping_data(reason);
                }
            }
            Some(RequestPurpose::GetStarGiveawayOptions) => {
                state.options.stars.fail_dropping_data(reason);
            }
            Some(RequestPurpose::LaunchPrepaidGiveaway) => {
                if let GiveawayLaunch::Launching { chat_id, request } = state.launch
                    && request != 0
                    && Some(request) == pending.map(|p| p.id.0)
                {
                    state.launch = GiveawayLaunch::Failed {
                        chat_id,
                        reason: format!("Couldn't start the giveaway: {reason}"),
                    };
                }
            }
            _ => {}
        }
    }
}
