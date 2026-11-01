//! Paid (Star) reactions: the pending-send state machine behind the toast
//! with Undo, the reactor list of a message and the sender choice.
//!
//! Telegram Desktop (`data_message_reactions.cpp`, `payments_reaction_process.cpp`,
//! `history_view_paid_reaction_toast.cpp`) holds a Star reaction for 5.5
//! seconds after each tap so quick taps add up, shows a toast with Undo and
//! sends the total when the timer runs out. TDLib keeps that pending total
//! itself (`addPendingPaidMessageReaction`); Quill only tracks the deadline
//! and sends `commitPendingPaidMessageReactions` or
//! `removePendingPaidMessageReactions`.
#![deny(clippy::indexing_slicing)]

use crate::telegram::envelope::MessageSender;
use serde_json::Value;

/// How long a tap waits before the Stars are committed (tdesktop
/// `kPaidAccumulatePeriod`).
pub const ACCUMULATE_MS: u64 = 5_500;
/// The amount the slider box starts on (tdesktop `kDefaultPerReaction`).
pub const DEFAULT_AMOUNT: i64 = 50;
/// The cap when the `paid_reaction_star_count_max` option is unknown
/// (tdesktop `kMaxPerReactionFallback`).
pub const FALLBACK_MAX: i64 = 10_000;
/// A tap on the chip adds this many Stars.
pub const QUICK_AMOUNT: i64 = 1;

/// Positions on the amount slider (0 = one Star, `SLIDER_STEPS` = the cap).
pub const SLIDER_STEPS: i64 = 100;

/// The amount at slider position `step`: cubic, so the low end (1 to 100
/// Stars) keeps fine control while the cap stays reachable.
pub fn amount_for_step(step: i64, max: i64) -> i64 {
    let max = max.max(2);
    let t = step.clamp(0, SLIDER_STEPS) as f64 / SLIDER_STEPS as f64;
    (1.0 + (max - 1) as f64 * t * t * t).round() as i64
}

/// The slider position closest to `amount`.
pub fn step_for_amount(amount: i64, max: i64) -> i64 {
    let max = max.max(2);
    let amount = amount.clamp(1, max);
    let t = ((amount - 1) as f64 / (max - 1) as f64).cbrt();
    (t * SLIDER_STEPS as f64).round() as i64
}

/// Who the Stars are shown as in the Top Senders list
/// (`PaidReactionType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PaidSender {
    /// Shown with the account's name.
    Regular,
    /// Shown as Anonymous.
    Anonymous,
    /// Shown as a channel the account manages.
    Chat(i64),
}

impl PaidSender {
    /// JSON for the `type` field of `addPendingPaidMessageReaction`.
    pub fn to_tdlib_json(self) -> Value {
        match self {
            Self::Regular => serde_json::json!({"@type": "paidReactionTypeRegular"}),
            Self::Anonymous => serde_json::json!({"@type": "paidReactionTypeAnonymous"}),
            Self::Chat(chat_id) => {
                serde_json::json!({"@type": "paidReactionTypeChat", "chat_id": chat_id})
            }
        }
    }
}

/// One `paidReactor`: a sender of Stars on a message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaidReactor {
    /// `None` for an anonymous reactor.
    pub sender: Option<MessageSender>,
    pub star_count: i64,
    pub is_top: bool,
    pub is_me: bool,
    pub is_anonymous: bool,
}

/// Why a Star reaction was not started.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// The balance (minus Stars already waiting) is lower than the amount.
    NotEnough,
    /// The per-message cap would be exceeded.
    TooMany,
}

impl Refusal {
    pub fn message(self) -> &'static str {
        match self {
            Self::NotEnough => "Not enough Stars. Buy more in the Telegram app or @PremiumBot.",
            Self::TooMany => "That is more Stars than one message can take.",
        }
    }
}

/// Stars that were tapped on one message and not yet committed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingPaid {
    pub chat_id: i64,
    pub message_id: i64,
    pub stars: i64,
    /// The sender choice of the latest tap; `None` keeps the account's
    /// current default (TDLib: "type: pass null").
    pub sender: Option<PaidSender>,
    /// When the Stars are committed (unix milliseconds).
    pub commit_at_ms: u64,
}

impl PendingPaid {
    /// Whole seconds left on the toast's countdown, never below 1 while
    /// the Stars are still waiting.
    pub fn seconds_left(&self, now_ms: u64) -> u64 {
        self.commit_at_ms
            .saturating_sub(now_ms)
            .div_ceil(1000)
            .max(1)
    }
}

/// The Star reactions waiting to be committed, newest last.
#[derive(Debug, Clone, Default)]
pub struct PaidReactions {
    pending: Vec<PendingPaid>,
    /// The message whose reaction picker offered the Star reaction (from
    /// `getMessageAvailableReactions`).
    pub offered: Option<(i64, i64)>,
    /// `paid_reaction_star_count_max`, once known.
    pub max_stars: Option<i64>,
    /// Channels the account may send Stars as in a chat
    /// (`getChatAvailablePaidMessageReactionSenders`), for one chat.
    pub senders: Option<(i64, Vec<i64>)>,
    /// The last refusal, shown in the status line.
    pub note: Option<String>,
}

impl PaidReactions {
    /// The per-message cap.
    pub fn max(&self) -> i64 {
        self.max_stars.unwrap_or(FALLBACK_MAX).max(2)
    }

    /// Keeps the channel senders of `chat_id` (the account itself is the
    /// regular choice and is not listed).
    pub fn set_senders(&mut self, chat_id: i64, senders: &[MessageSender]) {
        let channels = senders
            .iter()
            .filter_map(|sender| match sender {
                MessageSender::Chat { chat_id } => Some(*chat_id),
                MessageSender::User { .. } => None,
            })
            .collect();
        self.senders = Some((chat_id, channels));
    }

    /// Channels to send as in `chat_id`.
    pub fn channels_for(&self, chat_id: i64) -> &[i64] {
        match &self.senders {
            Some((chat, channels)) if *chat == chat_id => channels,
            _ => &[],
        }
    }

    /// Whether the picker for this message offers the Star reaction.
    pub fn offered_for(&self, chat_id: i64, message_id: i64) -> bool {
        self.offered == Some((chat_id, message_id))
    }

    /// Stars waiting on a message.
    pub fn pending_stars(&self, chat_id: i64, message_id: i64) -> i64 {
        self.find(chat_id, message_id).map_or(0, |p| p.stars)
    }

    /// Stars waiting across all messages: they are reserved against the
    /// balance (tdesktop `credits().lock`).
    pub fn total_pending(&self) -> i64 {
        self.pending.iter().map(|p| p.stars).sum()
    }

    fn find(&self, chat_id: i64, message_id: i64) -> Option<&PendingPaid> {
        self.pending
            .iter()
            .find(|p| p.chat_id == chat_id && p.message_id == message_id)
    }

    /// How many more Stars `message` may take before the cap.
    pub fn room(&self, chat_id: i64, message_id: i64) -> i64 {
        (self.max() - self.pending_stars(chat_id, message_id)).max(0)
    }

    /// Why `stars` cannot be added right now, if they cannot. `balance` is
    /// the account's Stars when known; Stars waiting on other messages
    /// count against it (tdesktop locks them).
    pub fn refusal(
        &self,
        chat_id: i64,
        message_id: i64,
        stars: i64,
        balance: Option<i64>,
    ) -> Option<Refusal> {
        if stars <= 0 || stars > self.room(chat_id, message_id) {
            return Some(Refusal::TooMany);
        }
        match balance {
            Some(have) if have - self.total_pending() < stars => Some(Refusal::NotEnough),
            _ => None,
        }
    }

    /// Records a tap. The timer restarts and the message moves to the top
    /// of the toast stack. Returns the new waiting total, or `None` when
    /// the cap would be exceeded.
    pub fn add(
        &mut self,
        chat_id: i64,
        message_id: i64,
        stars: i64,
        sender: Option<PaidSender>,
        now_ms: u64,
    ) -> Option<i64> {
        if stars <= 0 || stars > self.room(chat_id, message_id) {
            return None;
        }
        let mut entry = match self
            .pending
            .iter()
            .position(|p| p.chat_id == chat_id && p.message_id == message_id)
        {
            Some(ix) => self.pending.remove(ix),
            None => PendingPaid {
                chat_id,
                message_id,
                stars: 0,
                sender: None,
                commit_at_ms: 0,
            },
        };
        entry.stars += stars;
        if sender.is_some() {
            entry.sender = sender;
        }
        entry.commit_at_ms = now_ms + ACCUMULATE_MS;
        let total = entry.stars;
        self.pending.push(entry);
        Some(total)
    }

    /// Takes back `stars` after TDLib refused a tap.
    pub fn rollback(&mut self, chat_id: i64, message_id: i64, stars: i64) {
        if let Some(ix) = self
            .pending
            .iter()
            .position(|p| p.chat_id == chat_id && p.message_id == message_id)
        {
            let spent = self.pending.get_mut(ix).is_some_and(|p| {
                p.stars -= stars;
                p.stars <= 0
            });
            if spent {
                self.pending.remove(ix);
            }
        }
    }

    /// Undo: forgets the waiting Stars of a message. Returns whether any
    /// were waiting (the caller then sends `removePending...`).
    pub fn undo(&mut self, chat_id: i64, message_id: i64) -> bool {
        let before = self.pending.len();
        self.pending
            .retain(|p| !(p.chat_id == chat_id && p.message_id == message_id));
        self.pending.len() != before
    }

    /// Removes and returns every reaction whose time is up.
    pub fn take_due(&mut self, now_ms: u64) -> Vec<PendingPaid> {
        let mut due = Vec::new();
        let mut ix = 0;
        while let Some(next) = self.pending.get(ix) {
            if next.commit_at_ms <= now_ms {
                due.push(self.pending.remove(ix));
            } else {
                ix += 1;
            }
        }
        due
    }

    /// The reaction the toast shows: the most recently tapped one.
    pub fn toast(&self) -> Option<&PendingPaid> {
        self.pending.last()
    }

    pub fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }

    /// How long until a countdown shows another second or a reaction is
    /// due. `None` when none is waiting.
    pub fn next_change_ms(&self, now_ms: u64) -> Option<u64> {
        self.pending
            .iter()
            .map(|paid| {
                let left = paid.commit_at_ms.saturating_sub(now_ms);
                match left % 1000 {
                    0 if left > 0 => 1000,
                    into_second => into_second,
                }
            })
            .min()
    }
}

#[cfg(test)]
mod tests;
