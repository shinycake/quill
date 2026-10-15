//! Grams (TON) balance and history of the signed-in user: Settings → My
//! Grams, tdesktop's `Settings::Credits` with `CreditsType::Ton`.
//!
//! The requests here are `getTonTransactions` and
//! `getGramRevenueStatistics`, plus `getGramWithdrawalUrl` behind the
//! password box (`ton_withdraw.rs`); top-up and withdrawal open Fragment's
//! page in the browser. Nothing here moves Grams. Amounts are nano-Grams
//! (1e-9).
#![deny(clippy::indexing_slicing)]

use super::{GiftInfo, TxFilter, group_digits, parse_gift_info, parse_gift_sticker};
use crate::ids::RequestId;
use crate::lang::{keys, tr};
use crate::telegram::envelope::{MessageSender, ParsedFile, StickerContent};
use serde_json::{Value, json};
use std::collections::BTreeSet;

/// Rows per `getTonTransactions` page (the design's limit).
pub const PAGE: i32 = 50;

/// tdesktop `lng_suggest_low_ton_fragment_url`: where "Top-up via Fragment"
/// goes.
pub const TOPUP_URL: &str = crate::suggested_posts::LOW_TON_URL;

const NANOS: i64 = 1_000_000_000;

/// TDLib sends `int53` as a number and `int64` as a string; take both.
fn int(value: Option<&Value>) -> i64 {
    value
        .and_then(|v| v.as_i64().or_else(|| v.as_str()?.parse().ok()))
        .unwrap_or(0)
}

fn text(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn flag(value: &Value, key: &str) -> bool {
    value.get(key).and_then(Value::as_bool).unwrap_or(false)
}

/// `RevenueWithdrawalState` of a Fragment withdrawal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WithdrawalState {
    /// Null: a refund from Fragment.
    Unknown,
    Pending,
    Succeeded {
        date: i32,
        url: String,
    },
    Failed,
}

impl WithdrawalState {
    fn parse(value: Option<&Value>) -> Self {
        let Some(value) = value else {
            return Self::Unknown;
        };
        match value.get("@type").and_then(Value::as_str) {
            Some("revenueWithdrawalStatePending") => Self::Pending,
            Some("revenueWithdrawalStateSucceeded") => Self::Succeeded {
                date: i32::try_from(int(value.get("date"))).unwrap_or(0),
                url: text(value, "url"),
            },
            Some("revenueWithdrawalStateFailed") => Self::Failed,
            _ => Self::Unknown,
        }
    }
}

/// `TonTransactionType`; unknown constructors become `Unsupported`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TonTxKind {
    FragmentDeposit {
        is_gift: bool,
        sticker: Option<StickerContent>,
    },
    FragmentWithdrawal {
        state: WithdrawalState,
    },
    SuggestedPostPayment {
        chat_id: i64,
    },
    GiftPurchaseOffer {
        gift: Box<GiftInfo>,
    },
    UpgradedGiftPurchase {
        user_id: i64,
        gift: Box<GiftInfo>,
    },
    UpgradedGiftSale {
        user_id: i64,
        gift: Box<GiftInfo>,
        via_offer: bool,
    },
    StakeDiceStake,
    StakeDicePayout,
    Unsupported,
}

/// One `tonTransaction`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TonTx {
    pub id: String,
    /// Unique within the loaded list: the id, or `index-date` when the id
    /// is empty or repeats. Set by [`GramsHub::apply_page`].
    pub key: String,
    /// Negative when Grams left the account.
    pub gram_nano: i64,
    pub is_refund: bool,
    pub date: i32,
    pub kind: TonTxKind,
}

fn gift_of(ty: &Value) -> Box<GiftInfo> {
    Box::new(parse_gift_info(ty.get("gift").unwrap_or(&Value::Null)))
}

/// Parse a `TonTransactionType` object.
pub fn parse_kind(ty: &Value) -> TonTxKind {
    match ty.get("@type").and_then(Value::as_str).unwrap_or_default() {
        "tonTransactionTypeFragmentDeposit" => TonTxKind::FragmentDeposit {
            is_gift: flag(ty, "is_gift"),
            sticker: parse_gift_sticker(ty.get("sticker")).0,
        },
        "tonTransactionTypeFragmentWithdrawal" => TonTxKind::FragmentWithdrawal {
            state: WithdrawalState::parse(ty.get("withdrawal_state")),
        },
        "tonTransactionTypeSuggestedPostPayment" => TonTxKind::SuggestedPostPayment {
            chat_id: int(ty.get("chat_id")),
        },
        "tonTransactionTypeGiftPurchaseOffer" => TonTxKind::GiftPurchaseOffer { gift: gift_of(ty) },
        "tonTransactionTypeUpgradedGiftPurchase" => TonTxKind::UpgradedGiftPurchase {
            user_id: int(ty.get("user_id")),
            gift: gift_of(ty),
        },
        "tonTransactionTypeUpgradedGiftSale" => TonTxKind::UpgradedGiftSale {
            user_id: int(ty.get("user_id")),
            gift: gift_of(ty),
            via_offer: flag(ty, "via_offer"),
        },
        "tonTransactionTypeStakeDiceStake" => TonTxKind::StakeDiceStake,
        "tonTransactionTypeStakeDicePayout" => TonTxKind::StakeDicePayout,
        _ => TonTxKind::Unsupported,
    }
}

/// The files a row needs downloaded (deposit and gift stickers).
fn kind_files(ty: &Value) -> Vec<ParsedFile> {
    let mut files = parse_gift_sticker(ty.get("sticker")).1;
    if let Some(gift) = ty.get("gift") {
        files.extend(parse_gift_info(gift).files);
    }
    files
}

/// Parse a `tonTransaction`; `None` without a type. A row without an id
/// is kept (its key comes from its place in the list).
pub fn parse_ton_tx(value: &Value) -> Option<TonTx> {
    Some(TonTx {
        id: text(value, "id"),
        key: String::new(),
        gram_nano: int(value.get("gram_amount")),
        is_refund: flag(value, "is_refund"),
        date: i32::try_from(int(value.get("date"))).unwrap_or(0),
        kind: parse_kind(value.get("type")?),
    })
}

impl TonTx {
    pub fn is_incoming(&self) -> bool {
        self.gram_nano >= 0
    }

    /// The user or channel on the other side; the row shows its name.
    pub fn peer(&self) -> Option<MessageSender> {
        match &self.kind {
            TonTxKind::SuggestedPostPayment { chat_id } if *chat_id != 0 => {
                Some(MessageSender::Chat { chat_id: *chat_id })
            }
            TonTxKind::UpgradedGiftPurchase { user_id, .. }
            | TonTxKind::UpgradedGiftSale { user_id, .. }
                if *user_id != 0 =>
            {
                Some(MessageSender::User { user_id: *user_id })
            }
            _ => None,
        }
    }

    pub fn gift(&self) -> Option<&GiftInfo> {
        match &self.kind {
            TonTxKind::GiftPurchaseOffer { gift }
            | TonTxKind::UpgradedGiftPurchase { gift, .. }
            | TonTxKind::UpgradedGiftSale { gift, .. } => Some(gift),
            _ => None,
        }
    }

    /// The sticker drawn on the row and the receipt.
    pub fn sticker(&self) -> Option<&StickerContent> {
        match &self.kind {
            TonTxKind::FragmentDeposit { sticker, .. } => sticker.as_ref(),
            _ => self.gift().and_then(|gift| gift.sticker.as_ref()),
        }
    }

    /// The row headline. `None` = the peer's name (a suggested post
    /// shows the channel's title).
    pub fn title(&self) -> Option<String> {
        let title = match &self.kind {
            TonTxKind::FragmentDeposit { is_gift: true, .. } => {
                tr(keys::CREDITS_BOX_HISTORY_ENTRY_GIFT_NAME).to_string()
            }
            TonTxKind::FragmentDeposit { .. } => {
                tr(keys::CREDITS_BOX_HISTORY_ENTRY_CURRENCY_IN).to_string()
            }
            TonTxKind::FragmentWithdrawal { .. } => {
                tr(keys::CREDITS_BOX_HISTORY_ENTRY_FRAGMENT).to_string()
            }
            TonTxKind::SuggestedPostPayment { .. } => return None,
            TonTxKind::GiftPurchaseOffer { gift }
            | TonTxKind::UpgradedGiftPurchase { gift, .. }
            | TonTxKind::UpgradedGiftSale { gift, .. } => gift.title.clone(),
            TonTxKind::StakeDiceStake => tr(keys::Q_GRAMS_ENTRY_STAKE).to_string(),
            TonTxKind::StakeDicePayout => tr(keys::Q_GRAMS_ENTRY_PAYOUT).to_string(),
            TonTxKind::Unsupported => tr(keys::Q_GRAMS_ENTRY_UNKNOWN).to_string(),
        };
        Some(if title.is_empty() {
            tr(keys::Q_GRAMS_ENTRY_UNKNOWN).to_string()
        } else {
            title
        })
    }

    /// The row's second line, as tdesktop's `CreditsRow` picks it: refund,
    /// pending and failed first, then "Fragment" or what kind of gift
    /// deal it was. Empty when there is nothing to add.
    pub fn detail(&self) -> String {
        if self.is_refund {
            return tr(keys::CHANNEL_EARN_HISTORY_RETURN).to_string();
        }
        match &self.kind {
            TonTxKind::FragmentWithdrawal {
                state: WithdrawalState::Pending,
            } => tr(keys::CHANNEL_EARN_HISTORY_PENDING),
            TonTxKind::FragmentWithdrawal {
                state: WithdrawalState::Failed,
            } => tr(keys::CHANNEL_EARN_HISTORY_FAILED),
            TonTxKind::FragmentDeposit { .. } => tr(keys::CREDITS_BOX_HISTORY_ENTRY_FRAGMENT),
            TonTxKind::GiftPurchaseOffer { .. } => tr(keys::CREDITS_BOX_HISTORY_ENTRY_GIFT_OFFER),
            TonTxKind::UpgradedGiftPurchase { .. } => {
                tr(keys::CREDITS_BOX_HISTORY_ENTRY_GIFT_BOUGHT)
            }
            TonTxKind::UpgradedGiftSale { .. } => tr(keys::CREDITS_BOX_HISTORY_ENTRY_GIFT_SOLD),
            _ => "",
        }
        .to_string()
    }
}

/// `1,234.56`: whole Grams grouped, two decimals cut (not rounded), as
/// tdesktop's `MajorPart` + `MinorPart` for a history row.
pub fn gram_amount_2dp(nanos: i64) -> String {
    let magnitude = nanos.unsigned_abs();
    let whole = magnitude / NANOS.unsigned_abs();
    let cents = (magnitude % NANOS.unsigned_abs()) / 10_000_000;
    let whole = group_digits(i64::try_from(whole).unwrap_or(i64::MAX));
    format!("{whole}.{cents:02}")
}

/// `+1.25` / `−0.50` (a minus sign, as tdesktop's row draws it).
pub fn signed_gram_amount(nanos: i64) -> String {
    let sign = if nanos < 0 { '\u{2212}' } else { '+' };
    format!("{sign}{}", gram_amount_2dp(nanos))
}

/// The balance as tdesktop's `FormatCreditsAmountDecimal` words it: up to
/// six decimals, trailing zeros cut.
pub fn gram_balance(nanos: i64) -> String {
    crate::ton_wallet::format_gram_amount_with(nanos, 6)
}

/// The Settings row label (`FormatCreditsAmountToShort`): `12.5K` from
/// ten thousand Grams up, the exact balance below.
pub fn gram_balance_short(nanos: i64) -> String {
    let whole = nanos / NANOS;
    if whole.abs() < 10_000 {
        return gram_balance(nanos);
    }
    let (value, suffix) = if whole.abs() >= 1_000_000 {
        (whole as f64 / 1_000_000.0, "M")
    } else {
        (whole as f64 / 1_000.0, "K")
    };
    let tenths = (value * 10.0).trunc() / 10.0;
    let text = format!("{tenths:.1}");
    format!("{}{suffix}", text.trim_end_matches(".0"))
}

/// `≈$1.234`: the fiat line under a balance (tdesktop `ToUsd`). `None`
/// for a zero balance or without a rate.
pub fn usd_line(nanos: i64, usd_per_gram: Option<f64>, decimals: usize) -> Option<String> {
    let rate = usd_per_gram.filter(|rate| rate.is_finite() && *rate > 0.0)?;
    if nanos == 0 {
        return None;
    }
    let grams = nanos as f64 / NANOS as f64;
    Some(format!(
        "\u{2248}{}",
        crate::ton_wallet::fiat::format_fiat(grams * rate, "$", decimals)
    ))
}

/// `tonTransactions` — one page.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TonTxPage {
    pub balance_nano: i64,
    pub transactions: Vec<TonTx>,
    pub next_offset: String,
    pub files: Vec<ParsedFile>,
}

pub fn parse_ton_transactions(value: &Value) -> TonTxPage {
    let rows = value.get("transactions").and_then(Value::as_array);
    TonTxPage {
        balance_nano: int(value.get("gram_amount")),
        transactions: rows
            .map(|rows| rows.iter().filter_map(parse_ton_tx).collect())
            .unwrap_or_default(),
        next_offset: text(value, "next_offset"),
        files: rows
            .into_iter()
            .flatten()
            .filter_map(|row| row.get("type"))
            .flat_map(kind_files)
            .collect(),
    }
}

/// `gramRevenueStatus` + the statistics' `usd_rate`: what tdesktop's
/// withdrawal section shows.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct GramRevenue {
    pub available_nano: i64,
    pub balance_nano: i64,
    pub withdrawal_enabled: bool,
    /// USD cents per nano-Gram.
    pub usd_rate: f64,
}

impl GramRevenue {
    fn status(status: &Value) -> Self {
        Self {
            available_nano: int(status.get("available_amount")),
            balance_nano: int(status.get("balance_amount")),
            withdrawal_enabled: flag(status, "withdrawal_enabled"),
            usd_rate: 0.0,
        }
    }

    /// USD per Gram, for [`usd_line`].
    pub fn usd_per_gram(&self) -> Option<f64> {
        let rate = self.usd_rate * NANOS as f64 / 100.0;
        (rate.is_finite() && rate > 0.0).then_some(rate)
    }
}

/// Answers and updates for the Grams screen.
#[derive(Debug, Clone, PartialEq)]
pub enum GramsPayload {
    Transactions(TonTxPage),
    Revenue(GramRevenue),
    /// `updateGramRevenueStatus` (keeps the last known rate).
    RevenueStatus(GramRevenue),
}

pub fn parse_payload(type_name: &str, value: &Value) -> Option<GramsPayload> {
    match type_name {
        "tonTransactions" => Some(GramsPayload::Transactions(parse_ton_transactions(value))),
        "gramRevenueStatistics" => {
            let mut revenue = GramRevenue::status(value.get("status")?);
            revenue.usd_rate = value.get("usd_rate").and_then(Value::as_f64).unwrap_or(0.0);
            Some(GramsPayload::Revenue(revenue))
        }
        "updateGramRevenueStatus" => Some(GramsPayload::RevenueStatus(GramRevenue::status(
            value.get("status")?,
        ))),
        _ => None,
    }
}

/// In-flight Grams requests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GramsPurpose {
    /// `getTonTransactions` limit 1 for the Settings row's balance.
    Probe,
    /// A history page; `append` = a follow-up page.
    Page { append: bool },
    /// `getGramRevenueStatistics`.
    Revenue,
    /// `getUser` for a user the history names but the session does not
    /// know yet; the pending request's `chat_id` carries the user id.
    PeerUser,
    /// `getChat` for such a chat.
    PeerChat,
    /// `getGramWithdrawalUrl` from the password box of Collect. Response
    /// is `httpUrl`.
    WithdrawalUrl,
}

/// `getTonTransactions`; the direction is omitted (null) for All.
pub fn get_ton_transactions(
    extra: RequestId,
    filter: TxFilter,
    offset: &str,
    limit: i32,
) -> String {
    let mut request = json!({
        "@type": "getTonTransactions",
        "@extra": extra.as_extra(),
        "offset": offset,
        "limit": limit,
    });
    if let Some(direction) = filter.direction_type() {
        crate::json_access::set(&mut request, "direction", json!({ "@type": direction }));
    }
    request.to_string()
}

/// `getUser`.
pub fn get_user(extra: RequestId, user_id: i64) -> String {
    json!({
        "@type": "getUser",
        "@extra": extra.as_extra(),
        "user_id": user_id,
    })
    .to_string()
}

/// `getGramRevenueStatistics`.
pub fn get_gram_revenue_statistics(extra: RequestId, is_dark: bool) -> String {
    json!({
        "@type": "getGramRevenueStatistics",
        "@extra": extra.as_extra(),
        "is_dark": is_dark,
    })
    .to_string()
}

/// The Grams screen and the Settings row's balance.
#[derive(Debug, Clone, Default)]
pub struct GramsHub {
    pub open: bool,
    /// Known after the first answer (probe or page).
    pub balance_nano: Option<i64>,
    /// The Settings probe was sent this session.
    pub probed: bool,
    pub filter: TxFilter,
    pub entries: Vec<TonTx>,
    pub next_offset: String,
    pub loaded: bool,
    pub loading: bool,
    pub error: Option<String>,
    /// `@extra` of the newest page request; older answers are dropped.
    pub request: u64,
    /// The first All page had incoming / outgoing rows (tdesktop shows the
    /// tabs only when both directions exist).
    pub has_in: bool,
    pub has_out: bool,
    /// Key of the transaction whose receipt is showing.
    pub selected: Option<String>,
    pub revenue: Option<GramRevenue>,
    /// Users and chats already asked for (each once per session).
    pub asked_users: BTreeSet<i64>,
    pub asked_chats: BTreeSet<i64>,
    /// Asked for and refused: shown as a deleted account, not a
    /// placeholder that never fills.
    pub missing_users: BTreeSet<i64>,
    pub missing_chats: BTreeSet<i64>,
    /// A page landed; its peers still need checking.
    pub peers_dirty: bool,
    /// Collect's password box or security check, while open.
    pub withdraw: Option<GramWithdraw>,
    /// Fragment's page from `getGramWithdrawalUrl`, for the UI to open
    /// once (through the link policy).
    pub withdraw_url: Option<String>,
    /// `AMOUNT_TOO_SMALL` closed the box: the UI shows the toast once.
    pub withdraw_too_small: bool,
}

impl GramsHub {
    /// tdesktop shows "My Grams" in Settings while the balance is not zero.
    pub fn shows_settings_row(&self) -> bool {
        self.balance_nano.is_some_and(|nanos| nanos != 0)
    }

    /// Reset the list for a fresh first page of `filter`.
    pub fn restart(&mut self, filter: TxFilter) {
        self.filter = filter;
        self.entries.clear();
        self.next_offset.clear();
        self.loaded = false;
        self.loading = false;
        self.error = None;
        self.selected = None;
    }

    pub fn apply_page(&mut self, page: TonTxPage, append: bool) {
        self.loading = false;
        self.error = None;
        self.loaded = true;
        self.balance_nano = Some(page.balance_nano);
        self.next_offset = page.next_offset;
        if !append && self.filter == TxFilter::All {
            self.has_in = page.transactions.iter().any(TonTx::is_incoming);
            self.has_out = page.transactions.iter().any(|tx| !tx.is_incoming());
        }
        if !append {
            self.entries.clear();
        }
        let mut keys: BTreeSet<String> = self.entries.iter().map(|tx| tx.key.clone()).collect();
        for mut tx in page.transactions {
            let index = self.entries.len();
            tx.key = if tx.id.is_empty() || keys.contains(&tx.id) {
                format!("{index}-{}", tx.date)
            } else {
                tx.id.clone()
            };
            keys.insert(tx.key.clone());
            self.entries.push(tx);
        }
        self.peers_dirty = true;
    }

    /// Peers the loaded rows name, for the resolver.
    pub fn peers(&self) -> impl Iterator<Item = MessageSender> + '_ {
        self.entries.iter().filter_map(TonTx::peer)
    }

    /// Whether the All / Incoming / Outgoing tabs show.
    pub fn shows_tabs(&self) -> bool {
        self.filter != TxFilter::All || (self.has_in && self.has_out)
    }

    pub fn has_more(&self) -> bool {
        !self.next_offset.is_empty()
    }

    pub fn selected_tx(&self) -> Option<&TonTx> {
        let key = self.selected.as_deref()?;
        self.entries.iter().find(|tx| tx.key == key)
    }

    /// The withdrawal section shows while revenue is available
    /// (tdesktop `BuildCurrencyWithdrawalSection`).
    pub fn withdrawable(&self) -> Option<&GramRevenue> {
        self.revenue.as_ref().filter(|r| r.available_nano > 0)
    }
}

#[path = "ton_withdraw.rs"]
mod withdraw;
pub use withdraw::*;

#[cfg(test)]
#[path = "ton_tests.rs"]
mod tests;
