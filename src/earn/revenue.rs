//! TON ad revenue and Stars revenue as TDLib reports them
//! (`chatRevenueStatistics`, `chatRevenueTransactions`,
//! `starRevenueStatistics`), the data behind tdesktop's
//! `info/channel_statistics/earn` and `info/bot/earn` screens.

use super::{int, text};
use crate::premium_hub::StarAmount;
use crate::telegram::envelope::StatisticalGraph;
use serde_json::Value;

fn graph(value: &Value, key: &str) -> Option<StatisticalGraph> {
    value
        .get(key)
        .filter(|v| !v.is_null())
        .and_then(|v| StatisticalGraph::parse(v).ok())
}

fn float(value: &Value, key: &str) -> f64 {
    value.get(key).and_then(Value::as_f64).unwrap_or(0.0)
}

/// `chatRevenueAmount`: TON amounts in nanotons.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TonAmounts {
    /// Total lifetime rewards.
    pub total: i64,
    /// Rewards since the last collection.
    pub balance: i64,
    /// Rewards available for collection.
    pub available: i64,
    pub withdrawal_enabled: bool,
}

impl TonAmounts {
    pub fn parse(value: Option<&Value>) -> Self {
        let Some(value) = value else {
            return Self::default();
        };
        Self {
            total: int(value.get("total_amount")),
            balance: int(value.get("balance_amount")),
            available: int(value.get("available_amount")),
            withdrawal_enabled: value
                .get("withdrawal_enabled")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        }
    }
}

/// `chatRevenueStatistics`.
#[derive(Debug, Clone, PartialEq)]
pub struct TonRevenueStats {
    pub by_hour: Option<StatisticalGraph>,
    pub revenue: Option<StatisticalGraph>,
    pub amounts: TonAmounts,
    /// US cents per nanoton.
    pub usd_rate: f64,
}

impl TonRevenueStats {
    pub fn parse(value: &Value) -> Self {
        Self {
            by_hour: graph(value, "revenue_by_hour_graph"),
            revenue: graph(value, "revenue_graph"),
            amounts: TonAmounts::parse(value.get("revenue_amount")),
            usd_rate: float(value, "usd_rate"),
        }
    }
}

/// What a TON transaction was (`ChatRevenueTransactionType`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TonTxKind {
    /// Rewards from ads shown between two dates.
    AdEarnings {
        start: i32,
        end: i32,
    },
    /// Rewards for a suggested post from `user_id`.
    PostEarnings {
        user_id: i64,
    },
    /// A collection through Fragment.
    Withdrawal {
        date: i32,
        state: WithdrawalState,
    },
    /// A failed collection's TON came back.
    Refund {
        date: i32,
    },
    Unsupported,
}

/// `RevenueWithdrawalState`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WithdrawalState {
    Pending,
    /// Done; `url` opens the transaction in a blockchain explorer.
    Succeeded {
        date: i32,
        url: String,
    },
    Failed,
}

/// `chatRevenueTransaction`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TonTx {
    /// Nanotons; positive for income.
    pub amount: i64,
    pub kind: TonTxKind,
}

impl TonTx {
    pub fn parse(value: &Value) -> Option<Self> {
        let ty = value.get("type")?;
        let kind = match ty.get("@type")?.as_str()? {
            "chatRevenueTransactionTypeSponsoredMessageEarnings" => TonTxKind::AdEarnings {
                start: int(ty.get("start_date")) as i32,
                end: int(ty.get("end_date")) as i32,
            },
            "chatRevenueTransactionTypeSuggestedPostEarnings" => TonTxKind::PostEarnings {
                user_id: int(ty.get("user_id")),
            },
            "chatRevenueTransactionTypeFragmentWithdrawal" => {
                let state = ty.get("state");
                let state = match state.and_then(|s| s.get("@type")).and_then(Value::as_str) {
                    Some("revenueWithdrawalStateSucceeded") => WithdrawalState::Succeeded {
                        date: state.map_or(0, |s| int(s.get("date")) as i32),
                        url: state.map(|s| text(s, "url")).unwrap_or_default(),
                    },
                    Some("revenueWithdrawalStateFailed") => WithdrawalState::Failed,
                    _ => WithdrawalState::Pending,
                };
                TonTxKind::Withdrawal {
                    date: int(ty.get("withdrawal_date")) as i32,
                    state,
                }
            }
            "chatRevenueTransactionTypeFragmentRefund" => TonTxKind::Refund {
                date: int(ty.get("refund_date")) as i32,
            },
            _ => TonTxKind::Unsupported,
        };
        Some(Self {
            amount: int(value.get("cryptocurrency_amount")),
            kind,
        })
    }

    /// Income (rewards and refunds) shows green with a plus.
    pub fn is_incoming(&self) -> bool {
        !matches!(self.kind, TonTxKind::Withdrawal { .. })
    }

    /// The row's headline (`lng_channel_earn_history_*`).
    pub fn title(&self) -> &'static str {
        match self.kind {
            TonTxKind::Refund { .. } => "Refund",
            TonTxKind::Withdrawal { .. } => "Collection",
            _ => "Rewards from ads",
        }
    }

    pub fn failed(&self) -> bool {
        matches!(
            self.kind,
            TonTxKind::Withdrawal {
                state: WithdrawalState::Failed,
                ..
            }
        )
    }

    pub fn pending(&self) -> bool {
        matches!(
            self.kind,
            TonTxKind::Withdrawal {
                state: WithdrawalState::Pending,
                ..
            }
        )
    }

    /// The explorer link of a finished collection.
    pub fn explorer_url(&self) -> Option<&str> {
        match &self.kind {
            TonTxKind::Withdrawal {
                state: WithdrawalState::Succeeded { url, .. },
                ..
            } if !url.is_empty() => Some(url),
            _ => None,
        }
    }

    /// tdesktop's date line: "start — end" for ad rewards, "date — Not
    /// Completed" for a failed collection, "Pending", or the date.
    pub fn date_line(&self) -> String {
        let date = |unix: i32| super::earn_date(i64::from(unix));
        match &self.kind {
            TonTxKind::AdEarnings { start, end } if *end != 0 => {
                format!("{} \u{2014} {}", date(*start), date(*end))
            }
            TonTxKind::AdEarnings { start, .. } => date(*start),
            TonTxKind::Withdrawal {
                date: when,
                state: WithdrawalState::Failed,
            } => format!("{} \u{2014} Not Completed", date(*when)),
            TonTxKind::Withdrawal {
                state: WithdrawalState::Pending,
                ..
            } => "Pending".into(),
            TonTxKind::Withdrawal { date: when, .. } | TonTxKind::Refund { date: when } => {
                date(*when)
            }
            TonTxKind::PostEarnings { .. } | TonTxKind::Unsupported => String::new(),
        }
    }
}

/// `chatRevenueTransactions`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TonTxPage {
    pub transactions: Vec<TonTx>,
    pub next_offset: String,
}

impl TonTxPage {
    pub fn parse(value: &Value) -> Self {
        Self {
            transactions: value
                .get("transactions")
                .and_then(Value::as_array)
                .map(|rows| rows.iter().filter_map(TonTx::parse).collect())
                .unwrap_or_default(),
            next_offset: text(value, "next_offset"),
        }
    }
}

/// `starRevenueStatus`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StarRevenueStatus {
    pub total: StarAmount,
    pub current: StarAmount,
    pub available: StarAmount,
    pub withdrawal_enabled: bool,
    /// Seconds until the next withdrawal is allowed; 0 = now.
    pub next_withdrawal_in: i32,
    /// When the next withdrawal is allowed (unix seconds), stamped by the
    /// reducer from `next_withdrawal_in`; 0 = now.
    pub next_withdrawal_at: i64,
}

impl StarRevenueStatus {
    pub fn parse(value: Option<&Value>) -> Self {
        let Some(value) = value else {
            return Self::default();
        };
        Self {
            total: StarAmount::parse(value.get("total_amount")),
            current: StarAmount::parse(value.get("current_amount")),
            available: StarAmount::parse(value.get("available_amount")),
            withdrawal_enabled: value
                .get("withdrawal_enabled")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            next_withdrawal_in: int(value.get("next_withdrawal_in")) as i32,
            next_withdrawal_at: 0,
        }
    }

    /// Stamp `next_withdrawal_at` against `now` (unix seconds).
    pub fn stamped(mut self, now: i64) -> Self {
        self.next_withdrawal_at = if self.next_withdrawal_in > 0 {
            now.saturating_add(i64::from(self.next_withdrawal_in))
        } else {
            0
        };
        self
    }

    pub fn has_any(&self) -> bool {
        [self.total, self.current, self.available]
            .iter()
            .any(|a| a.stars != 0 || a.nanos != 0)
    }
}

/// `starRevenueStatistics`.
#[derive(Debug, Clone, PartialEq)]
pub struct StarRevenueStats {
    pub by_day: Option<StatisticalGraph>,
    pub status: StarRevenueStatus,
    /// US cents per Star.
    pub usd_rate: f64,
}

impl StarRevenueStats {
    pub fn parse(value: &Value) -> Self {
        Self {
            by_day: graph(value, "revenue_by_day_graph"),
            status: StarRevenueStatus::parse(value.get("status")),
            usd_rate: float(value, "usd_rate"),
        }
    }
}
