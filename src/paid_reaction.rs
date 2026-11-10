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

use crate::ids::RequestId;
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

/// One tap's Stars, with the `addPendingPaidMessageReaction` that holds
/// them (`None` for the demo, which sends nothing).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PaidTap {
    pub request: Option<RequestId>,
    pub stars: i64,
}

/// Where a pending Star reaction stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaidStage {
    /// The Undo window is open until `commit_at_ms`. `attempts` counts the
    /// commits that could not be sent (the window restarts after each).
    Waiting { attempts: u32 },
    /// `commitPendingPaidMessageReactions` is out; the entry stays, so the
    /// Stars are not forgotten before TDLib answers. Its answer removes it.
    Committing(RequestId),
}

/// Stars that were tapped on one message and not yet committed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingPaid {
    pub chat_id: i64,
    pub message_id: i64,
    /// The total of `taps`.
    pub stars: i64,
    /// Each tap, so a refused one is rolled back alone.
    pub taps: Vec<PaidTap>,
    pub stage: PaidStage,
    /// The sender choice of the latest tap; `None` keeps the account's
    /// current default (TDLib: "type: pass null").
    pub sender: Option<PaidSender>,
    /// When the Stars are committed (unix milliseconds).
    pub commit_at_ms: u64,
}

impl PendingPaid {
    /// Whole seconds left on the toast's countdown, never below 1 while
    /// the Stars are still waiting.
    pub fn is_waiting(&self) -> bool {
        matches!(self.stage, PaidStage::Waiting { .. })
    }

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
        self.pending
            .iter()
            .filter(|p| p.chat_id == chat_id && p.message_id == message_id)
            .map(|p| p.stars)
            .sum()
    }

    /// Stars waiting across all messages: they are reserved against the
    /// balance (tdesktop `credits().lock`).
    pub fn total_pending(&self) -> i64 {
        self.pending.iter().map(|p| p.stars).sum()
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

    /// Records a tap (`request`: its `addPendingPaidMessageReaction`). The
    /// timer restarts and the message moves to the top of the toast stack.
    /// A reaction already committing is not added to: the tap waits in its
    /// own entry. Returns the new waiting total, or `None` when the cap
    /// would be exceeded.
    pub fn add(
        &mut self,
        chat_id: i64,
        message_id: i64,
        stars: i64,
        sender: Option<PaidSender>,
        now_ms: u64,
        request: Option<RequestId>,
    ) -> Option<i64> {
        if stars <= 0 || stars > self.room(chat_id, message_id) {
            return None;
        }
        let mut entry = match self
            .pending
            .iter()
            .position(|p| p.chat_id == chat_id && p.message_id == message_id && p.is_waiting())
        {
            Some(ix) => self.pending.remove(ix),
            None => PendingPaid {
                chat_id,
                message_id,
                stars: 0,
                taps: Vec::new(),
                stage: PaidStage::Waiting { attempts: 0 },
                sender: None,
                commit_at_ms: 0,
            },
        };
        entry.stars += stars;
        entry.taps.push(PaidTap { request, stars });
        if sender.is_some() {
            entry.sender = sender;
        }
        entry.commit_at_ms = now_ms + ACCUMULATE_MS;
        let total = entry.stars;
        self.pending.push(entry);
        Some(total)
    }

    /// Takes back the Stars of the tap `request` after TDLib refused it. A
    /// tap of an entry since committed or undone is not there any more.
    pub fn rollback(&mut self, request: RequestId) {
        let Some(ix) = self
            .pending
            .iter()
            .position(|p| p.taps.iter().any(|tap| tap.request == Some(request)))
        else {
            return;
        };
        let spent = self.pending.get_mut(ix).is_some_and(|p| {
            if let Some(at) = p.taps.iter().position(|tap| tap.request == Some(request)) {
                let tap = p.taps.remove(at);
                p.stars -= tap.stars;
            }
            p.stars <= 0
        });
        if spent {
            self.pending.remove(ix);
        }
    }

    /// Undo: forgets the waiting Stars of a message (one already committing
    /// cannot be undone). Returns whether any were waiting (the caller then
    /// sends `removePending...`).
    pub fn undo(&mut self, chat_id: i64, message_id: i64) -> bool {
        let before = self.pending.len();
        self.pending
            .retain(|p| !(p.chat_id == chat_id && p.message_id == message_id && p.is_waiting()));
        self.pending.len() != before
    }

    /// The waiting reactions whose time is up, as (chat, message). They
    /// stay until [`Self::committing`] or [`Self::commit_unsent`] moves them.
    pub fn due(&self, now_ms: u64) -> Vec<(i64, i64)> {
        self.pending
            .iter()
            .filter(|p| p.is_waiting() && p.commit_at_ms <= now_ms)
            .map(|p| (p.chat_id, p.message_id))
            .collect()
    }

    /// The commit of a due reaction went out as `request`.
    pub fn committing(&mut self, chat_id: i64, message_id: i64, request: RequestId) {
        if let Some(entry) = self.waiting_mut(chat_id, message_id) {
            entry.stage = PaidStage::Committing(request);
        }
    }

    /// The commit could not be sent: the entry stays and its Undo window
    /// starts again. Returns whether this was the first failure (the user
    /// is told once).
    pub fn commit_unsent(&mut self, chat_id: i64, message_id: i64, now_ms: u64) -> bool {
        let Some(entry) = self.waiting_mut(chat_id, message_id) else {
            return false;
        };
        let PaidStage::Waiting { attempts } = entry.stage else {
            return false;
        };
        entry.stage = PaidStage::Waiting {
            attempts: attempts.saturating_add(1),
        };
        entry.commit_at_ms = now_ms + ACCUMULATE_MS;
        attempts == 0
    }

    fn waiting_mut(&mut self, chat_id: i64, message_id: i64) -> Option<&mut PendingPaid> {
        self.pending
            .iter_mut()
            .find(|p| p.chat_id == chat_id && p.message_id == message_id && p.is_waiting())
    }

    /// TDLib answered the commit `request`: the entry is done. Returns
    /// whether one was waiting for it.
    pub fn commit_settled(&mut self, request: RequestId) -> bool {
        let before = self.pending.len();
        self.pending
            .retain(|p| p.stage != PaidStage::Committing(request));
        self.pending.len() != before
    }

    /// The reaction the toast shows: the most recently tapped one.
    pub fn toast(&self) -> Option<&PendingPaid> {
        self.pending.iter().rev().find(|p| p.is_waiting())
    }

    /// A reaction is waiting out its Undo window (a toast and a countdown
    /// to redraw).
    pub fn has_pending(&self) -> bool {
        self.pending.iter().any(PendingPaid::is_waiting)
    }

    /// How long until a countdown shows another second or a reaction is
    /// due. `None` when none is waiting.
    pub fn next_change_ms(&self, now_ms: u64) -> Option<u64> {
        self.pending
            .iter()
            .filter(|paid| paid.is_waiting())
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
