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
    pub stage: GiveawayInfoStage,
}

/// Where one giveaway lookup stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GiveawayInfoStage {
    /// The `getGiveawayInfo` this box waits for: another box's answer (one
    /// closed, then another giveaway opened) settles nothing here.
    Loading(RequestId),
    Ready(GiveawayInfoData),
    /// The lookup failed, with the line the box shows.
    Failed(String),
}

impl GiveawayInfoLookup {
    /// A lookup waiting for `request`.
    pub fn new(
        chat_id: i64,
        message_id: i64,
        summary: GiveawaySummary,
        request: RequestId,
    ) -> Self {
        Self {
            chat_id,
            message_id,
            summary,
            stage: GiveawayInfoStage::Loading(request),
        }
    }

    pub fn is_loading(&self) -> bool {
        matches!(self.stage, GiveawayInfoStage::Loading(_))
    }

    /// The answer, once it came.
    pub fn info(&self) -> Option<&GiveawayInfoData> {
        match &self.stage {
            GiveawayInfoStage::Ready(info) => Some(info),
            _ => None,
        }
    }

    pub fn error(&self) -> Option<&str> {
        match &self.stage {
            GiveawayInfoStage::Failed(reason) => Some(reason),
            _ => None,
        }
    }

    /// `pending` is this box's own lookup: settle it with `next`.
    fn settle(&mut self, pending: Option<&PendingRequest>, next: GiveawayInfoStage) -> bool {
        let ours = matches!(self.stage, GiveawayInfoStage::Loading(request)
        if pending.is_some_and(|p| {
            p.purpose == RequestPurpose::GetGiveawayInfo && p.id == request
        }));
        if ours {
            self.stage = next;
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

    /// Both lists are about to be asked for `boosted_chat_id`. The Premium
    /// prices of another chat are dropped at once, not kept under the
    /// spinner. The caller begins each list and records its request.
    pub fn retarget(&mut self, boosted_chat_id: i64) {
        if self.for_chat != boosted_chat_id {
            self.for_chat = boosted_chat_id;
            self.premium = Fetch::Idle;
        }
    }
}

/// The stage of one chat's giveaway launch or payment.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum GiveawayLaunch {
    #[default]
    Idle,
    /// `launchPrepaidGiveaway` `request` is in flight. Only its answer ends
    /// it, so closing or reopening the form keeps it (see
    /// [`GiveawayLaunches::for_form`]) and the form shows the launch still
    /// in flight.
    Launching { request: crate::ids::RequestId },
    /// The prepaid giveaway started.
    Launched,
    /// A new giveaway was paid for. Its form is done: Start must not buy the
    /// same giveaway again.
    Paid,
    /// The payment of a new giveaway needs the provider's verification page
    /// (3-D Secure), which sends Quill no answer. The form stays done: Start
    /// must not buy a second one meanwhile, so closing and reopening the
    /// form keeps it. It ends when the user says the verification is over
    /// ([`GiveawayLaunches::end_verification`], "Start over" on the form) or
    /// with the session.
    NeedsVerification,
    /// The launch failed, with the line its form shows.
    Failed { reason: String },
}

/// The launch stage of every chat's giveaway form, one per boosted chat: a
/// launch for chat A is not overwritten by chat B's, and each answer lands
/// on the chat that asked.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GiveawayLaunches(std::collections::BTreeMap<i64, GiveawayLaunch>);

impl GiveawayLaunches {
    /// The stage of `chat_id`'s giveaway.
    pub fn stage(&self, chat_id: i64) -> &GiveawayLaunch {
        self.0.get(&chat_id).unwrap_or(&GiveawayLaunch::Idle)
    }

    /// Move `chat_id`'s giveaway to `stage`; `Idle` forgets it.
    pub fn set(&mut self, chat_id: i64, stage: GiveawayLaunch) {
        if stage == GiveawayLaunch::Idle {
            self.0.remove(&chat_id);
        } else {
            self.0.insert(chat_id, stage);
        }
    }

    /// The giveaway of `chat_id` is created or paid for.
    pub fn is_done_for(&self, chat_id: i64) -> bool {
        matches!(
            self.stage(chat_id),
            GiveawayLaunch::Launched | GiveawayLaunch::Paid | GiveawayLaunch::NeedsVerification
        )
    }

    /// Whether a `launchPrepaidGiveaway` for `chat_id` is in flight.
    pub fn is_launching_for(&self, chat_id: i64) -> bool {
        matches!(self.stage(chat_id), GiveawayLaunch::Launching { .. })
    }

    /// Why the launch for `chat_id` failed, if it did.
    pub fn failure_for(&self, chat_id: i64) -> Option<&str> {
        match self.stage(chat_id) {
            GiveawayLaunch::Failed { reason } => Some(reason),
            _ => None,
        }
    }

    /// The launch of `chat_id` started or paid, so its boost status is stale.
    pub fn is_started_for(&self, chat_id: i64) -> bool {
        matches!(
            self.stage(chat_id),
            GiveawayLaunch::Launched | GiveawayLaunch::Paid
        )
    }

    /// What `chat_id`'s form starts from when it opens or closes. A launch
    /// still in flight is never forgotten: only its answer ends it, and the
    /// form of its chat must keep Start disabled. An answer already seen is.
    /// A payment waiting on its verification is unresolved at the provider
    /// too, so it is kept the same way.
    pub fn for_form(&mut self, chat_id: i64) {
        if !matches!(
            self.stage(chat_id),
            GiveawayLaunch::Launching { .. } | GiveawayLaunch::NeedsVerification
        ) {
            self.0.remove(&chat_id);
        }
    }

    /// Whether `chat_id`'s payment waits on its verification page.
    pub fn is_verifying_for(&self, chat_id: i64) -> bool {
        matches!(self.stage(chat_id), GiveawayLaunch::NeedsVerification)
    }

    /// The user says `chat_id`'s verification is over (finished elsewhere or
    /// given up): the form takes a new giveaway again.
    pub fn end_verification(&mut self, chat_id: i64) {
        if self.is_verifying_for(chat_id) {
            self.0.remove(&chat_id);
        }
    }

    /// The failure of `chat_id`'s launch was dismissed (the form changed).
    pub fn dismiss_failure(&mut self, chat_id: i64) {
        if matches!(self.stage(chat_id), GiveawayLaunch::Failed { .. }) {
            self.0.remove(&chat_id);
        }
    }

    /// The launch whose `request` is `pending`, with its chat.
    pub fn launching(&self, request: crate::ids::RequestId) -> Option<i64> {
        self.0.iter().find_map(|(chat, stage)| {
            matches!(stage, GiveawayLaunch::Launching { request: r } if *r == request)
                .then_some(*chat)
        })
    }
}

/// The payments domain's giveaway state.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GiveawayState {
    pub info: Option<GiveawayInfoLookup>,
    pub options: GiveawayOptions,
    pub launches: GiveawayLaunches,
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
        request: crate::ids::RequestId,
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
            request,
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
        if let Some(lookup) = self.payments.giveaway.info.as_mut() {
            lookup.settle(pending, GiveawayInfoStage::Ready(info));
        }
    }

    /// `premiumGiveawayPaymentOptions`.
    pub(crate) fn apply_premium_giveaway_options(
        &mut self,
        options: Vec<PremiumGiveawayOption>,
        pending: Option<&PendingRequest>,
    ) {
        // Only the answer to the request the list waits for: a late answer
        // for another chat's form asked an older request.
        if pending.map(|p| p.purpose) == Some(RequestPurpose::GetPremiumGiveawayOptions) {
            self.payments
                .giveaway
                .options
                .premium
                .answered(pending.map(|p| p.id), options);
        }
    }

    /// `starGiveawayPaymentOptions`.
    pub(crate) fn apply_star_giveaway_options(
        &mut self,
        options: Vec<StarGiveawayOption>,
        pending: Option<&PendingRequest>,
    ) {
        if pending.map(|p| p.purpose) == Some(RequestPurpose::GetStarGiveawayOptions) {
            self.payments
                .giveaway
                .options
                .stars
                .answered(pending.map(|p| p.id), options);
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
            self.groups.outdate_boost_status(chat_id.0);
        }
        let launches = &mut self.payments.giveaway.launches;
        if let Some(chat_id) = launches.launching(pending.id) {
            launches.set(chat_id, GiveawayLaunch::Launched);
        }
    }

    /// A failed lookup, option fetch or launch keeps its dialog open with
    /// the reason.
    pub(crate) fn apply_giveaway_error(&mut self, err: &TdError, pending: Option<&PendingRequest>) {
        let reason = error_reason(err);
        let state = &mut self.payments.giveaway;
        match pending.map(|p| p.purpose) {
            Some(RequestPurpose::GetGiveawayInfo) => {
                if let Some(lookup) = state.info.as_mut() {
                    lookup.settle(
                        pending,
                        GiveawayInfoStage::Failed(
                            "This giveaway is not available any more.".into(),
                        ),
                    );
                }
            }
            Some(RequestPurpose::GetPremiumGiveawayOptions) => {
                state
                    .options
                    .premium
                    .failed(pending.map(|p| p.id), reason, true);
            }
            Some(RequestPurpose::GetStarGiveawayOptions) => {
                state
                    .options
                    .stars
                    .failed(pending.map(|p| p.id), reason, true);
            }
            Some(RequestPurpose::LaunchPrepaidGiveaway) => {
                if let Some(chat_id) = pending.and_then(|p| state.launches.launching(p.id)) {
                    state.launches.set(
                        chat_id,
                        GiveawayLaunch::Failed {
                            reason: format!("Couldn't start the giveaway: {reason}"),
                        },
                    );
                }
            }
            _ => {}
        }
    }
}
