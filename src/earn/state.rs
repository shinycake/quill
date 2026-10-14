//! What the revenue and affiliate screens hold between TDLib answers.
//! One revenue page and one affiliate page are open at a time (the info
//! panel shows one section); balances are kept per peer for the profile
//! and bot edit rows.

use super::*;
use crate::premium_hub::{StarAmount, StarTx, StarTxPage, TxFilter};
use std::collections::HashMap;

/// A fetch the page waits on.
#[derive(Debug, Clone, Default, PartialEq)]
pub enum Load<T> {
    #[default]
    Idle,
    Loading,
    Ready(T),
    /// The request failed; the text is shown on the page.
    Failed(String),
}

impl<T> Load<T> {
    pub fn ready(&self) -> Option<&T> {
        match self {
            Self::Ready(value) => Some(value),
            _ => None,
        }
    }

    pub fn is_loading(&self) -> bool {
        matches!(self, Self::Loading)
    }

    /// Settled: loaded, failed or never asked.
    pub fn is_settled(&self) -> bool {
        !self.is_loading()
    }
}

/// Limits and switches from TDLib options and the app config.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
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
    /// The app config was read at least once.
    pub app_config_loaded: bool,
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

/// The TON history of a revenue page.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TonHistory {
    pub rows: Vec<TonTx>,
    pub next_offset: String,
    pub loading: bool,
    pub loaded: bool,
}

/// One Stars history list (All, Incoming or Outgoing).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StarHistory {
    pub rows: Vec<StarTx>,
    pub next_offset: String,
    pub loading: bool,
    pub loaded: bool,
}

impl StarHistory {
    pub fn apply(&mut self, page: StarTxPage, append: bool) {
        if !append {
            self.rows.clear();
        }
        self.rows.extend(page.transactions);
        self.next_offset = page.next_offset;
        self.loading = false;
        self.loaded = true;
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
    /// A megagroup without TON ads (`BROADCAST_REQUIRED`).
    pub ton_unavailable: bool,
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
            ton_unavailable: false,
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
    /// The first request, without a password, is out.
    Checking,
    /// tdesktop's password box: "Two-step verification".
    Password {
        sending: bool,
        error: Option<String>,
    },
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

/// The bot setup page (tdesktop `BotStarRef::Setup`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetupPage {
    pub bot_user_id: i64,
    /// The program when the page opened.
    pub program: Option<ProgramInfo>,
    /// The sliders' values.
    pub params: ProgramParams,
    pub confirm: Option<SetupConfirm>,
    pub saving: bool,
}

impl SetupPage {
    pub fn new(bot_user_id: i64, program: Option<ProgramInfo>) -> Self {
        Self {
            bot_user_id,
            program,
            params: setup_start(program),
            confirm: None,
            saving: false,
        }
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
    pub loading: bool,
    pub loaded: bool,
}

impl<T> Default for ProgramList<T> {
    fn default() -> Self {
        Self {
            programs: Vec::new(),
            total_count: 0,
            next_offset: String::new(),
            loading: false,
            loaded: false,
        }
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
    pub joining: bool,
}

/// tdesktop `StarRefLinkBox`: a joined program's link.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkBox {
    pub program: ConnectedProgram,
    pub recipient: EarnPeer,
}

/// The "Revoke Link" confirmation of a joined program.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RevokeBox {
    pub program: ConnectedProgram,
    pub recipient: EarnPeer,
    pub sending: bool,
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
    /// Peers whose balances were asked for (once per session).
    pub balances_asked: Vec<EarnPeer>,
    pub setup: Option<SetupPage>,
    pub join: Option<JoinPage>,
    pub join_box: Option<JoinBox>,
    pub link_box: Option<LinkBox>,
    pub revoke: Option<RevokeBox>,
    /// "View existing programs" of the setup page (tdesktop
    /// `ProgramsListBox`).
    pub programs_box: Option<ProgramList<FoundProgram>>,
    /// Who may receive commissions: the user, owned public channels the
    /// user can post in, and owned bots (tdesktop `ResolveRecipients`).
    pub recipients: Option<Vec<EarnPeer>>,
    pub recipients_loading: u8,
    /// A join box that opens once the recipients arrive.
    pub pending_join: Option<JoinBox>,
    /// Set when the setup page saved or ended the program: the UI goes
    /// back from it (tdesktop `showBackFromStack`).
    pub setup_closed: bool,
    /// One-shot notices the UI shows as toasts.
    pub notes: Vec<String>,
    /// A URL the UI opens in the browser (a Fragment withdrawal page).
    pub open_url: Option<String>,
    /// `getStarRevenueStatistics` for the user worked: the Stars dialog
    /// shows "View Statistics" (tdesktop `Credits::statsEnabled`).
    pub self_stats: Option<bool>,
}

impl EarnState {
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
