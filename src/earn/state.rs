//! What the revenue and affiliate screens hold between TDLib answers.
//! One revenue page and one affiliate page are open at a time (the info
//! panel shows one section); balances are kept per peer for the profile
//! and bot edit rows.

use super::*;
use crate::premium_hub::{StarAmount, StarTx, StarTxPage, TxFilter};
use crate::state::fetch::Fetch;
use std::collections::HashMap;

/// A fetch the page waits on: a `Fetch` with a fifth outcome,
/// `Unavailable`. `Loading` holds the request that asked, so only its
/// answer or error settles it (not another page's of the same peer).
#[derive(Debug, Clone, Default, PartialEq)]
pub enum Load<T> {
    #[default]
    Idle,
    Loading(crate::ids::RequestId),
    Ready(T),
    /// The request failed; the text is shown on the page.
    Failed(String),
    /// The server has nothing of this kind for the peer (a megagroup
    /// without TON ads, `BROADCAST_REQUIRED`): the part is left out.
    Unavailable,
}

impl<T> Load<T> {
    pub fn ready(&self) -> Option<&T> {
        match self {
            Self::Ready(value) => Some(value),
            _ => None,
        }
    }

    pub fn is_loading(&self) -> bool {
        matches!(self, Self::Loading(_))
    }

    /// The request the fetch waits for.
    pub fn asked(&self) -> Option<crate::ids::RequestId> {
        match self {
            Self::Loading(request) => Some(*request),
            _ => None,
        }
    }

    /// The fetch waits for the answer of `request`.
    pub fn waits_for(&self, request: Option<crate::ids::RequestId>) -> bool {
        matches!(self, Self::Loading(asked) if Some(*asked) == request)
    }

    /// Settled: loaded, failed or never asked.
    pub fn is_settled(&self) -> bool {
        !self.is_loading()
    }
}

/// Limits and switches from TDLib options and the app config.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EarnLimits {
    /// `star_withdrawal_count_min`.
    pub star_withdrawal_min: Option<i64>,
    /// `star_withdrawal_count_max`.
    pub star_withdrawal_max: Option<i64>,
    /// `affiliate_program_commission_per_mille_min`.
    pub commission_min: Option<i32>,
    /// `affiliate_program_commission_per_mille_max`.
    pub commission_max: Option<i32>,
    /// `can_withdraw_chat_revenue`.
    pub can_withdraw_chat_revenue: bool,
    /// App config `starref_connect_allowed` (tdesktop `Join::Allowed`).
    pub starref_join_allowed: bool,
    /// App config `starref_program_allowed` (tdesktop `Setup::Allowed`).
    pub starref_setup_allowed: bool,
    /// The app config for these switches: asked once, again after it
    /// failed (the next info panel to open asks).
    pub app_config: Fetch<()>,
}

impl EarnLimits {
    /// Take one `updateOption`; `true` when it was one of ours.
    pub fn apply_option(&mut self, name: &str, value: Option<i64>, flag: Option<bool>) -> bool {
        match name {
            "star_withdrawal_count_min" => self.star_withdrawal_min = value,
            "star_withdrawal_count_max" => self.star_withdrawal_max = value,
            "affiliate_program_commission_per_mille_min" => {
                self.commission_min = value.and_then(|v| i32::try_from(v).ok())
            }
            "affiliate_program_commission_per_mille_max" => {
                self.commission_max = value.and_then(|v| i32::try_from(v).ok())
            }
            "can_withdraw_chat_revenue" => self.can_withdraw_chat_revenue = flag.unwrap_or(false),
            _ => return false,
        }
        true
    }

    /// The smallest withdrawal; TDLib's documented default until the
    /// option arrives.
    pub fn withdrawal_min(&self) -> i64 {
        self.star_withdrawal_min.unwrap_or(1000).max(1)
    }

    /// The commission range for the setup slider; TDLib's documented
    /// defaults until the options arrive.
    pub fn commission_range(&self) -> (i32, i32) {
        (
            self.commission_min.unwrap_or(1),
            self.commission_max.unwrap_or(800),
        )
    }
}

/// Where a paged list stands: one value, so a failed page is not taken
/// for "ready, ask the next one" (a request every frame, round 16).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PageLoad {
    /// Nothing asked yet.
    #[default]
    Unasked,
    /// A page is on its way under `request`; `more` when it follows pages
    /// already in. Only that request's answer or error settles it.
    Loading {
        more: bool,
        request: crate::ids::RequestId,
    },
    /// Every page asked for is in.
    Ready,
    /// The last page failed (its error arm, or the send). Nothing asks
    /// again by itself: the list shows a Retry.
    Failed { more: bool },
}

impl PageLoad {
    pub fn is_loading(self) -> bool {
        matches!(self, Self::Loading { .. })
    }

    /// A first page answered (or failed): what the list shows is its own.
    pub fn has_first_page(self) -> bool {
        matches!(
            self,
            Self::Loading { more: true, .. } | Self::Ready | Self::Failed { .. }
        )
    }

    pub fn is_failed(self) -> bool {
        matches!(self, Self::Failed { .. })
    }

    /// A page was asked for under `request`; `append` when it is the next
    /// page.
    pub fn start(&mut self, append: bool, request: crate::ids::RequestId) {
        *self = Self::Loading {
            more: append && self.has_first_page(),
            request,
        };
    }

    /// The page asked for did not come.
    pub fn fail(&mut self) {
        *self = Self::Failed {
            more: matches!(
                self,
                Self::Loading { more: true, .. } | Self::Failed { more: true }
            ),
        };
    }

    /// `request` is the page this list waits for: an older page's answer
    /// (before a sort change or a restart) is not this one's.
    pub fn waits_for(self, request: Option<crate::ids::RequestId>) -> bool {
        matches!(self, Self::Loading { request: waiting, .. } if Some(waiting) == request)
    }

    /// The error of `request`: fails the list only when it is the page the
    /// list waits for.
    pub fn fail_request(&mut self, request: Option<crate::ids::RequestId>) {
        if self.waits_for(request) {
            self.fail();
        }
    }

    /// Whether a scroll near the end may ask the next page: only once the
    /// pages asked for are in, never after a failure.
    pub fn may_page(self, next_offset: &str) -> bool {
        self == Self::Ready && !next_offset.is_empty()
    }
}

/// The TON history of a revenue page.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TonHistory {
    pub rows: Vec<TonTx>,
    pub next_offset: String,
    pub load: PageLoad,
}

/// One Stars history list (All, Incoming or Outgoing).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StarHistory {
    pub rows: Vec<StarTx>,
    pub next_offset: String,
    pub load: PageLoad,
}

impl StarHistory {
    pub fn apply(&mut self, page: StarTxPage, append: bool) {
        if !append {
            self.rows.clear();
        }
        self.rows.extend(page.transactions);
        self.next_offset = page.next_offset;
        self.load = PageLoad::Ready;
    }
}

/// Which revenue screen a page is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EarnScreen {
    /// tdesktop `ChannelEarn`: TON ads and, for channels, Stars
    /// ("Monetization"; a bot's "Toncoin" balance opens it too).
    Monetization,
    /// tdesktop `BotEarn`: Stars only ("Stars Balance"; a bot or the user).
    Stars,
}

/// The open revenue page.
#[derive(Debug, Clone, PartialEq)]
pub struct EarnPage {
    pub peer: EarnPeer,
    pub screen: EarnScreen,
    pub ton: Load<TonRevenueStats>,
    pub stars: Load<StarRevenueStats>,
    pub ton_history: TonHistory,
    /// All, Incoming, Outgoing (only All is used on a channel page).
    pub star_history: [StarHistory; 3],
    /// The channel page's history tab: false = TON, true = Stars.
    pub stars_tab: bool,
    /// The Stars page's history tab.
    pub star_filter: TxFilter,
    /// `getStarAdAccountUrl` (tdesktop's "Buy Ads").
    pub ad_account_url: Option<String>,
}

impl EarnPage {
    pub fn new(peer: EarnPeer, screen: EarnScreen) -> Self {
        Self {
            peer,
            screen,
            ton: Load::Idle,
            stars: Load::Idle,
            ton_history: TonHistory::default(),
            star_history: Default::default(),
            stars_tab: false,
            star_filter: TxFilter::All,
            ad_account_url: None,
        }
    }

    pub fn history_mut(&mut self, filter: TxFilter) -> &mut StarHistory {
        let [all, incoming, outgoing] = &mut self.star_history;
        match filter {
            TxFilter::All => all,
            TxFilter::Incoming => incoming,
            TxFilter::Outgoing => outgoing,
        }
    }

    pub fn history(&self, filter: TxFilter) -> &StarHistory {
        let [all, incoming, outgoing] = &self.star_history;
        match filter {
            TxFilter::All => all,
            TxFilter::Incoming => incoming,
            TxFilter::Outgoing => outgoing,
        }
    }

    /// Both statistics settled: the page leaves its loading state.
    pub fn loaded(&self) -> bool {
        match self.screen {
            EarnScreen::Monetization => self.ton.is_settled() && self.stars.is_settled(),
            EarnScreen::Stars => self.stars.is_settled(),
        }
    }
}

/// Which page of the revenue and affiliate screens the info panel shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EarnPanel {
    /// tdesktop `ChannelEarn` (Monetization) or `BotEarn` (Stars Balance).
    Revenue { peer: EarnPeer, screen: EarnScreen },
    /// tdesktop `BotStarRef::Setup` for an own bot.
    Setup { bot_user_id: i64 },
    /// tdesktop `BotStarRef::Join` for the user, a bot or a channel.
    Join { peer: EarnPeer },
}

impl EarnPanel {
    /// The panel title (tdesktop `Widget::title`).
    pub fn title(self) -> &'static str {
        match self {
            Self::Revenue {
                screen: EarnScreen::Monetization,
                ..
            } => "Monetization",
            Self::Revenue { .. } => "Stars Balance",
            Self::Setup { .. } => "Affiliate Program",
            Self::Join { .. } => "Affiliate Programs",
        }
    }
}

/// What a withdrawal is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WithdrawTarget {
    /// Collect the TON ad rewards of a chat.
    Ton { chat_id: i64 },
    /// Withdraw `count` Stars of `owner`.
    Stars { owner: EarnPeer, count: i64 },
}

/// Where a withdrawal stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WithdrawStep {
    /// The first request, without a password, is out as this request.
    /// Only its answer or error moves the step.
    Checking(crate::ids::RequestId),
    /// tdesktop's password box: "Two-step verification".
    Password { error: Option<String> },
    /// The password went out as this request. Its answer ends the
    /// withdrawal; its error returns to `Password` with the line; an
    /// answer of an earlier attempt matches neither.
    PasswordSending(crate::ids::RequestId),
    /// tdesktop `TransferPasswordError`: no password (`later` false) or a
    /// password or session that is too fresh (`later` true).
    SecurityCheck { later: bool },
}

/// A withdrawal in progress.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Withdraw {
    pub target: WithdrawTarget,
    pub step: WithdrawStep,
}

impl Withdraw {
    /// Whether `request` is the answer the current step waits for: the
    /// password-less try while `Checking`, the password's own request
    /// while `PasswordSending`; no other step takes an answer. An answer
    /// to another one (sent before a Cancel) neither opens Fragment nor
    /// touches this attempt.
    pub fn takes_answer(&self, request: Option<crate::ids::RequestId>) -> bool {
        match self.step {
            WithdrawStep::Checking(sent) | WithdrawStep::PasswordSending(sent) => {
                request == Some(sent)
            }
            WithdrawStep::Password { .. } | WithdrawStep::SecurityCheck { .. } => false,
        }
    }
}

/// Where the setup page's save stands, with the confirmation box that
/// goes with it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetupStage {
    /// The sliders; no box.
    Editing,
    /// A confirmation box is open.
    Confirming(SetupConfirm),
    /// The save went out as this request and its `ConfirmUpdate` stays
    /// open until the answer (tdesktop). Only its answer closes the page;
    /// its error returns to `Confirming(Save)`.
    SavingInBox(crate::ids::RequestId),
    /// A save or end is out with no box open (the end's box closes as it
    /// goes; a save's box was closed meanwhile). Its error returns to
    /// `Editing`.
    Saving(crate::ids::RequestId),
}

/// The bot setup page (tdesktop `BotStarRef::Setup`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetupPage {
    pub bot_user_id: i64,
    /// The program when the page opened.
    pub program: Option<ProgramInfo>,
    /// The sliders' values.
    pub params: ProgramParams,
    stage: SetupStage,
}

impl SetupPage {
    pub fn new(bot_user_id: i64, program: Option<ProgramInfo>) -> Self {
        Self {
            bot_user_id,
            program,
            params: setup_start(program),
            stage: SetupStage::Editing,
        }
    }

    pub fn stage(&self) -> SetupStage {
        self.stage
    }

    /// The confirmation box that is open.
    pub fn confirm(&self) -> Option<SetupConfirm> {
        match self.stage {
            SetupStage::Confirming(confirm) => Some(confirm),
            SetupStage::SavingInBox(_) => Some(SetupConfirm::Save),
            SetupStage::Editing | SetupStage::Saving(_) => None,
        }
    }

    /// A save or end is in flight.
    pub fn is_saving(&self) -> bool {
        self.saving_request().is_some()
    }

    /// The save or end in flight.
    pub fn saving_request(&self) -> Option<crate::ids::RequestId> {
        match self.stage {
            SetupStage::SavingInBox(request) | SetupStage::Saving(request) => Some(request),
            SetupStage::Editing | SetupStage::Confirming(_) => None,
        }
    }

    /// `request` is the save this page waits for.
    pub(crate) fn waits_for(&self, request: Option<crate::ids::RequestId>) -> bool {
        self.saving_request().is_some() && self.saving_request() == request
    }

    /// Open a confirmation box (not while a save is out).
    pub fn ask_confirm(&mut self, confirm: SetupConfirm) {
        if !self.is_saving() {
            self.stage = SetupStage::Confirming(confirm);
        }
    }

    /// The box was closed: a save in flight carries on without it.
    pub fn close_confirm(&mut self) {
        self.stage = match self.stage {
            SetupStage::Confirming(_) => SetupStage::Editing,
            SetupStage::SavingInBox(request) => SetupStage::Saving(request),
            other => other,
        };
    }

    /// The save went out as `request`: its box stays open while it runs.
    pub(crate) fn save_sent(&mut self, request: crate::ids::RequestId) {
        self.stage = match self.stage {
            SetupStage::Confirming(SetupConfirm::Save) => SetupStage::SavingInBox(request),
            _ => SetupStage::Saving(request),
        };
    }

    /// The end went out as `request`: its box closes as it goes.
    pub(crate) fn end_sent(&mut self, request: crate::ids::RequestId) {
        self.stage = SetupStage::Saving(request);
    }

    /// The save or end under `request` was refused: back to the box that
    /// was open, or to the sliders.
    pub(crate) fn save_refused(&mut self, request: Option<crate::ids::RequestId>) {
        if !self.waits_for(request) {
            return;
        }
        self.stage = match self.stage {
            SetupStage::SavingInBox(_) => SetupStage::Confirming(SetupConfirm::Save),
            _ => SetupStage::Editing,
        };
    }

    /// A running program (Update instead of Start, End shown, sliders
    /// cannot go lower).
    pub fn exists(&self) -> bool {
        self.program.is_some_and(|p| p.is_running())
    }
}

/// The setup page's confirmation boxes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetupConfirm {
    /// `ConfirmUpdate`: start or update with the shown values.
    Save,
    /// `ConfirmEndBox`.
    End,
}

/// A program list that pages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProgramList<T> {
    pub programs: Vec<T>,
    pub total_count: i32,
    pub next_offset: String,
    pub load: PageLoad,
}

impl<T> Default for ProgramList<T> {
    fn default() -> Self {
        Self {
            programs: Vec::new(),
            total_count: 0,
            next_offset: String::new(),
            load: PageLoad::Unasked,
        }
    }
}

impl<T> ProgramList<T> {
    /// A scroll near the end asks the next page (`join_upkeep`).
    pub fn may_page(&self) -> bool {
        self.load.may_page(&self.next_offset)
    }
}

/// The join page (tdesktop `BotStarRef::Join`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JoinPage {
    pub peer: EarnPeer,
    pub mine: ProgramList<ConnectedProgram>,
    pub found: ProgramList<FoundProgram>,
    pub sort: ProgramSort,
}

impl JoinPage {
    pub fn new(peer: EarnPeer) -> Self {
        Self {
            peer,
            mine: ProgramList::default(),
            found: ProgramList::default(),
            sort: ProgramSort::default(),
        }
    }
}

/// A confirm box's request: none, or the one it waits for. An answer for
/// another request (a closed box's, or an earlier box's) is not this box's.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum BoxSend {
    #[default]
    Idle,
    Sending(crate::ids::RequestId),
}

impl BoxSend {
    pub fn is_sending(self) -> bool {
        matches!(self, Self::Sending(_))
    }

    /// `request` is the one this box waits for.
    pub fn waits_for(self, request: Option<crate::ids::RequestId>) -> bool {
        matches!(self, Self::Sending(waiting) if Some(waiting) == request)
    }
}

/// tdesktop `JoinStarRefBox`: join one bot's program.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JoinBox {
    pub bot_user_id: i64,
    pub info: ProgramInfo,
    /// Who the commissions go to.
    pub recipient: EarnPeer,
    /// The user picks the recipient among their own peers (tdesktop
    /// `requireOthers`: the user's own page or "View existing programs").
    pub choose_recipient: bool,
    pub stage: JoinStage,
}

/// Where the join box stands: only the answer of `Joining`'s own request
/// closes it (the error returns it to `Open`), so a late answer of a box
/// the user left acts on nothing.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum JoinStage {
    #[default]
    Open,
    Joining(crate::ids::RequestId),
}

impl JoinStage {
    pub fn is_joining(self) -> bool {
        matches!(self, Self::Joining(_))
    }

    pub fn waits_for(self, request: Option<crate::ids::RequestId>) -> bool {
        matches!(self, Self::Joining(waiting) if Some(waiting) == request)
    }
}

/// A click on a program asks whether it is joined
/// (`getConnectedAffiliateProgram`). Only that request's answer opens a
/// box; leaving the page or a newer click returns it to `Idle`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Resolve {
    #[default]
    Idle,
    Asking(crate::ids::RequestId),
}

impl Resolve {
    pub fn waits_for(self, request: Option<crate::ids::RequestId>) -> bool {
        matches!(self, Self::Asking(waiting) if Some(waiting) == request)
    }
}

/// tdesktop `StarRefLinkBox`: a joined program's link.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkBox {
    pub program: ConnectedProgram,
    pub recipient: EarnPeer,
    /// Whether the box's icon animation has started. It lives with the
    /// box, so a box that goes away (a layer drop, another account) takes
    /// it along and the next one plays from the start.
    pub icon: LinkIcon,
}

/// The link icon's one-time play: waiting for the first draw, then
/// running from the instant that draw happened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkIcon {
    Unseen,
    Playing { since: std::time::Instant },
}

impl LinkBox {
    pub fn new(program: ConnectedProgram, recipient: EarnPeer) -> Self {
        Self {
            program,
            recipient,
            icon: LinkIcon::Unseen,
        }
    }

    /// The first draw starts the play; later draws keep its start.
    pub fn note_drawn(&mut self, now: std::time::Instant) {
        if self.icon == LinkIcon::Unseen {
            self.icon = LinkIcon::Playing { since: now };
        }
    }
}

/// The "Revoke Link" confirmation of a joined program.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RevokeBox {
    pub program: ConnectedProgram,
    pub recipient: EarnPeer,
    pub send: BoxSend,
}

/// A move the UI makes once, taken from `EarnState::effect`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EarnEffect {
    /// The setup page saved or ended the program: go back from it.
    SetupDone,
}

/// Who may receive commissions, as the two list requests settle it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Recipients {
    #[default]
    Unasked,
    /// The channel and bot lists are out; `found` is what answered so far.
    /// `join` is a join box that opens once the recipients arrive; it goes
    /// with the stage (a failure, the profile it came from closing).
    Loading {
        waiting: Vec<crate::ids::RequestId>,
        found: Vec<EarnPeer>,
        join: Option<JoinBox>,
    },
    Ready(Vec<EarnPeer>),
    /// A send failed. Answers of the abandoned attempt are dropped and the
    /// next ask starts over.
    Failed,
}

impl EarnState {
    /// Open `join` now, or park it on the recipients when they are still
    /// coming (a join box that picks a recipient needs the list).
    pub fn open_or_park_join(&mut self, join: JoinBox) {
        if !join.choose_recipient {
            self.join_box = Some(join);
        } else if let Some(open) = self.recipients.park(join) {
            self.join_box = Some(open);
        }
    }
}

impl Recipients {
    pub fn is_loading(&self) -> bool {
        matches!(self, Self::Loading { .. })
    }

    /// Park `join` until the lists answer. A join box needs no parking
    /// once they have, so it comes back to be opened at once.
    pub fn park(&mut self, join: JoinBox) -> Option<JoinBox> {
        match self {
            Self::Loading { join: slot, .. } => {
                *slot = Some(join);
                None
            }
            _ => Some(join),
        }
    }

    /// Forget the parked join box.
    pub fn drop_parked(&mut self) {
        if let Self::Loading { join, .. } = self {
            *join = None;
        }
    }

    /// The parked join box, if any.
    pub fn parked(&self) -> Option<&JoinBox> {
        match self {
            Self::Loading { join, .. } => join.as_ref(),
            _ => None,
        }
    }

    /// The list, once both requests answered.
    pub fn list(&self) -> Option<&[EarnPeer]> {
        match self {
            Self::Ready(list) => Some(list),
            _ => None,
        }
    }
}

/// Everything the revenue and affiliate screens keep.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EarnState {
    pub limits: EarnLimits,
    pub page: Option<EarnPage>,
    pub withdraw: Option<Withdraw>,
    /// TON balances (nanotons) per peer: `updateChatRevenueAmount` and the
    /// statistics answers.
    pub ton_balances: HashMap<EarnPeer, TonAmounts>,
    /// Stars balances per peer: `updateStarRevenueStatus` and answers.
    pub star_balances: HashMap<EarnPeer, StarAmount>,
    /// Where each peer's profile balances stand (asked once per session
    /// unless they failed).
    pub balance_asks: HashMap<EarnPeer, crate::state::fetch::BatchAsk>,
    pub setup: Option<SetupPage>,
    pub join: Option<JoinPage>,
    pub join_box: Option<JoinBox>,
    pub link_box: Option<LinkBox>,
    /// The click on a program that is being resolved.
    pub resolve: Resolve,
    pub revoke: Option<RevokeBox>,
    /// "View existing programs" of the setup page (tdesktop
    /// `ProgramsListBox`).
    pub programs_box: Option<ProgramList<FoundProgram>>,
    /// Who may receive commissions: the user, owned public channels the
    /// user can post in, and owned bots (tdesktop `ResolveRecipients`).
    pub recipients: Recipients,
    /// Moves the UI makes once, oldest first; it drains them (the setup
    /// page saved or ended the program: go back, tdesktop
    /// `showBackFromStack`).
    pub effects: Vec<EarnEffect>,
    /// One-shot notices the UI shows as toasts.
    pub notes: Vec<String>,
    /// A URL the UI opens in the browser (a Fragment withdrawal page).
    pub open_url: Option<String>,
    /// `getStarRevenueStatistics` for the user worked: the Stars dialog
    /// shows "View Statistics" (tdesktop `Credits::statsEnabled`).
    pub self_stats: Option<bool>,
}

impl EarnState {
    /// A Collect or Withdraw whose first request has not answered.
    pub fn is_checking_withdrawal(&self) -> bool {
        self.withdraw
            .as_ref()
            .is_some_and(|w| matches!(w.step, WithdrawStep::Checking(_)))
    }

    /// A balance answer of `request`.
    pub fn settle_balance(&mut self, peer: EarnPeer, request: Option<crate::ids::RequestId>) {
        if let Some(ask) = self.balance_asks.get_mut(&peer) {
            ask.settle(request);
        }
    }

    /// A balance request of `request` failed.
    pub fn fail_balance(&mut self, peer: EarnPeer, request: Option<crate::ids::RequestId>) {
        if let Some(ask) = self.balance_asks.get_mut(&peer) {
            ask.fail(request);
        }
    }

    /// A failed balance ask is forgotten, so the next profile open asks
    /// again (a failure never asks again by itself).
    pub fn forget_failed_balances(&mut self) {
        self.balance_asks
            .retain(|_, ask| !matches!(ask, crate::state::fetch::BatchAsk::Failed));
    }

    /// A failed app-config ask is forgotten, so the next profile open asks
    /// again (the per-frame upkeep only asks a config never asked).
    pub fn forget_failed_app_config(&mut self) {
        if self.limits.app_config.error().is_some() {
            self.limits.app_config = Fetch::Idle;
        }
    }

    pub fn note(&mut self, text: impl Into<String>) {
        self.notes.push(text.into());
    }

    /// The Stars balance of `peer`, when known.
    pub fn star_balance(&self, peer: EarnPeer) -> Option<StarAmount> {
        self.star_balances.get(&peer).copied()
    }

    /// The available TON of `peer` in nanotons, when known.
    pub fn ton_balance(&self, peer: EarnPeer) -> Option<i64> {
        self.ton_balances.get(&peer).map(|a| a.balance)
    }
}
