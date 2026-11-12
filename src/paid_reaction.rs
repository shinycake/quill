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
use crate::state::fetch::Fetch;
use crate::telegram::envelope::MessageSender;
use serde_json::Value;
use std::collections::HashMap;

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

/// Where a pending Star reaction is. It leaves the list on TDLib's answer
/// to the request it is waiting on (the request id matches the answer to
/// what asked): `ok`, or the error of a commit. TDLib takes the pending
/// Stars out of the message when it sends a commit and gives them back to
/// the balance when the server refuses it
/// (`MessageReactions::send_paid_message_reaction`,
/// `SendPaidReactionQuery::on_error`), so a refused commit holds nothing.
/// A commit that could not be sent and a refused undo never reached that
/// point: TDLib still holds the Stars, so the entry is `Stuck`, a note is
/// shown, and the send is tried again on a growing delay
/// ([`MAX_TRIES`] times, then the entry is let go). The next tap on the
/// message takes its Stars back into a waiting entry, and the toast shows
/// what TDLib would commit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaidStage {
    /// Held for the Undo window, until `commit_at_ms` (unix milliseconds).
    Waiting { commit_at_ms: u64 },
    /// `commitPendingPaidMessageReactions` is out.
    Committing(RequestId),
    /// `removePendingPaidMessageReactions` is out.
    /// `tries` earlier undo sends of this entry were refused.
    Undoing { request: RequestId, tries: u8 },
    /// A commit could not be sent, or an undo was refused: TDLib still
    /// holds the Stars. Nothing shows them; they stay reserved against the
    /// balance and the cap until a new tap on the message joins them, a
    /// later commit or undo of it is answered, or the `tries` run out. The
    /// `settle` that failed is sent again at `retry_at_ms`.
    Stuck {
        settle: Settle,
        retry_at_ms: u64,
        tries: u8,
    },
}

/// What a stuck entry sends again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Settle {
    Commit,
    Undo,
}

/// How many times a stuck entry is sent again before it is let go (TDLib
/// then still holds its Stars until the next tap on the message or the end
/// of the session, as before).
pub const MAX_TRIES: u8 = 5;
/// The wait before the first resend; it doubles each try, up to
/// [`MAX_RETRY_MS`].
const FIRST_RETRY_MS: u64 = 2_000;
const MAX_RETRY_MS: u64 = 60_000;

/// The wait after `tries` failed sends.
fn retry_after(tries: u8) -> u64 {
    FIRST_RETRY_MS
        .saturating_mul(1 << tries.saturating_sub(1).min(5))
        .min(MAX_RETRY_MS)
}

/// One tap's Stars, with the `addPendingPaidMessageReaction` that holds
/// them (`None` for the demo, which sends nothing).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PaidTap {
    pub request: Option<RequestId>,
    pub stars: i64,
}

/// Stars that were tapped on one message and not yet settled with TDLib.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingPaid {
    pub stage: PaidStage,
    pub chat_id: i64,
    pub message_id: i64,
    /// Each tap, so a refused one is taken back alone.
    pub taps: Vec<PaidTap>,
    /// The sender choice of the latest tap; `None` keeps the account's
    /// current default (TDLib: "type: pass null").
    pub sender: Option<PaidSender>,
}

impl PendingPaid {
    /// The Stars of every tap of this entry.
    pub fn stars(&self) -> i64 {
        self.taps
            .iter()
            .fold(0i64, |total, tap| total.saturating_add(tap.stars))
    }

    /// The Stars wait out the Undo window.
    pub fn is_waiting(&self) -> bool {
        matches!(self.stage, PaidStage::Waiting { .. })
    }

    /// A send failed and TDLib still holds the Stars.
    pub fn is_stuck(&self) -> bool {
        matches!(self.stage, PaidStage::Stuck { .. })
    }

    /// When the Stars are committed (unix milliseconds); 0 once they are
    /// not waiting.
    pub fn commit_at_ms(&self) -> u64 {
        match self.stage {
            PaidStage::Waiting { commit_at_ms } => commit_at_ms,
            _ => 0,
        }
    }

    /// Whole seconds left on the toast's countdown, never below 1 while
    /// the Stars are still waiting.
    pub fn seconds_left(&self, now_ms: u64) -> u64 {
        self.commit_at_ms()
            .saturating_sub(now_ms)
            .div_ceil(1000)
            .max(1)
    }
}

/// The Star reactions waiting to be committed, newest last.
#[derive(Debug, Clone, Default)]
pub struct PaidReactions {
    pending: Vec<PendingPaid>,
    /// `paid_reaction_star_count_max`, once known.
    pub max_stars: Option<i64>,
    /// Channels the account may send Stars as in each chat
    /// (`getChatAvailablePaidMessageReactionSenders`), by chat.
    senders: HashMap<i64, Fetch<Vec<i64>>>,
}

impl PaidReactions {
    /// The per-message cap.
    pub fn max(&self) -> i64 {
        self.max_stars.unwrap_or(FALLBACK_MAX).max(2)
    }

    /// Keeps the channel senders of `chat_id` (the account itself is the
    /// regular choice and is not listed). Taken only by the ask that waits
    /// for `request`.
    pub fn set_senders(
        &mut self,
        chat_id: i64,
        request: Option<RequestId>,
        senders: &[MessageSender],
    ) {
        let channels = senders
            .iter()
            .filter_map(|sender| match sender {
                MessageSender::Chat { chat_id } => Some(*chat_id),
                MessageSender::User { .. } => None,
            })
            .collect();
        self.senders
            .entry(chat_id)
            .or_default()
            .answered(request, channels);
    }

    /// Whether the senders of `chat_id` may be asked for: never asked, or
    /// the last ask failed. One in flight or answered is not asked again.
    pub fn can_ask_senders(&self, chat_id: i64) -> bool {
        self.senders.get(&chat_id).is_none_or(Fetch::can_ask)
    }

    /// The senders of `chat_id` were asked for under `request`.
    pub fn senders_asked(&mut self, chat_id: i64, request: RequestId) {
        let fetch = self.senders.entry(chat_id).or_default();
        fetch.begin();
        fetch.sent(request);
    }

    /// The ask `request` for the senders of `chat_id` failed: it may be
    /// tried again. Another ask's late error changes nothing.
    pub fn senders_failed(&mut self, chat_id: i64, request: Option<RequestId>) {
        self.senders
            .entry(chat_id)
            .or_default()
            .failed(request, "senders unavailable", false);
    }

    /// Channels to send as in `chat_id`.
    pub fn channels_for(&self, chat_id: i64) -> &[i64] {
        self.senders
            .get(&chat_id)
            .and_then(Fetch::data)
            .map_or(&[], Vec::as_slice)
    }

    /// Stars still held for Undo on a message.
    pub fn pending_stars(&self, chat_id: i64, message_id: i64) -> i64 {
        self.find(chat_id, message_id).map_or(0, PendingPaid::stars)
    }

    /// Stars reserved against the balance and the cap: waiting ones and
    /// those whose commit or undo TDLib has not answered yet.
    fn reserved_on(&self, chat_id: i64, message_id: i64) -> i64 {
        self.pending
            .iter()
            .filter(|p| p.chat_id == chat_id && p.message_id == message_id)
            .fold(0i64, |total, p| total.saturating_add(p.stars()))
    }

    /// Stars reserved across all messages (tdesktop `credits().lock`).
    pub fn total_pending(&self) -> i64 {
        self.pending
            .iter()
            .fold(0i64, |total, p| total.saturating_add(p.stars()))
    }

    /// The entry still held for Undo.
    fn find(&self, chat_id: i64, message_id: i64) -> Option<&PendingPaid> {
        self.pending
            .iter()
            .find(|p| p.is_waiting() && p.chat_id == chat_id && p.message_id == message_id)
    }

    /// How many more Stars `message` may take before the cap.
    pub fn room(&self, chat_id: i64, message_id: i64) -> i64 {
        self.max()
            .saturating_sub(self.reserved_on(chat_id, message_id))
            .max(0)
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
            Some(have) if have.saturating_sub(self.total_pending()) < stars => {
                Some(Refusal::NotEnough)
            }
            _ => None,
        }
    }

    /// Records a tap (`request`: its `addPendingPaidMessageReaction`). The
    /// timer restarts and the message moves to the top of the toast stack.
    /// Returns the new waiting total, or `None` when the cap would be
    /// exceeded.
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
        let mut entry = PendingPaid {
            stage: PaidStage::Waiting {
                commit_at_ms: now_ms.saturating_add(ACCUMULATE_MS),
            },
            chat_id,
            message_id,
            taps: Vec::new(),
            sender: None,
        };
        // The waiting entry and Stars TDLib still holds from a failed commit
        // or undo of this message become one: TDLib commits them together.
        let mut ix = 0;
        while let Some(p) = self.pending.get(ix) {
            if (p.is_waiting() || p.is_stuck())
                && p.chat_id == chat_id
                && p.message_id == message_id
            {
                let mut joined = self.pending.remove(ix);
                entry.taps.append(&mut joined.taps);
                entry.sender = joined.sender.or(entry.sender);
            } else {
                ix += 1;
            }
        }
        entry.taps.push(PaidTap { request, stars });
        if sender.is_some() {
            entry.sender = sender;
        }
        let total = entry.stars();
        self.pending.push(entry);
        Some(total)
    }

    /// Takes back the Stars of the tap `request` after TDLib refused it. A
    /// tap of an entry since settled is not there any more, and a newer
    /// entry of the same message never loses another tap's Stars.
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
                p.taps.remove(at);
            }
            p.stars() <= 0
        });
        if spent {
            self.pending.remove(ix);
        }
    }

    /// Forgets the waiting Stars of a message without asking TDLib (the
    /// offline demo, which has none). Returns whether any were waiting.
    pub fn undo(&mut self, chat_id: i64, message_id: i64) -> bool {
        let before = self.pending.len();
        self.pending
            .retain(|p| !(p.is_waiting() && p.chat_id == chat_id && p.message_id == message_id));
        self.pending.len() != before
    }

    /// Moves the entry of the message that is waiting, or stuck and sent
    /// again, to `stage`.
    fn set_stage(&mut self, chat_id: i64, message_id: i64, stage: PaidStage) {
        if let Some(entry) = self.pending.iter_mut().find(|p| {
            (p.is_waiting() || p.is_stuck()) && p.chat_id == chat_id && p.message_id == message_id
        }) {
            entry.stage = stage;
        }
    }

    /// The Undo request `request` went out: the Stars wait for TDLib's answer.
    pub fn begin_undo(&mut self, chat_id: i64, message_id: i64, request: RequestId) {
        let tries = self
            .pending
            .iter()
            .find_map(|p| match p.stage {
                PaidStage::Stuck { tries, .. }
                    if p.chat_id == chat_id && p.message_id == message_id =>
                {
                    Some(tries)
                }
                _ => None,
            })
            .unwrap_or(0);
        self.set_stage(chat_id, message_id, PaidStage::Undoing { request, tries });
    }

    /// The commit request `request` went out: the Stars wait for TDLib's answer.
    pub fn begin_commit(&mut self, chat_id: i64, message_id: i64, request: RequestId) {
        self.set_stage(chat_id, message_id, PaidStage::Committing(request));
    }

    /// The commit, or the undo of a stuck entry, could not be sent: TDLib
    /// still holds the Stars, so the entry is `Stuck` and sent again after a
    /// growing wait; past [`MAX_TRIES`] it is let go. Returns whether an
    /// entry was waiting (the user is told once, not on each resend).
    pub fn commit_unsent(&mut self, chat_id: i64, message_id: i64, now_ms: u64) -> bool {
        let was_waiting = self.find(chat_id, message_id).is_some();
        let failed = self.pending.iter().find_map(|p| match p.stage {
            PaidStage::Stuck { settle, tries, .. }
                if p.chat_id == chat_id && p.message_id == message_id =>
            {
                Some((settle, tries))
            }
            _ => None,
        });
        let (settle, tries) = failed.unwrap_or((Settle::Commit, 0));
        self.stick(chat_id, message_id, settle, tries.saturating_add(1), now_ms);
        was_waiting
    }

    /// Marks the entry of the message stuck after `tries` failed sends of
    /// `settle`, or lets it go when the tries are used up.
    fn stick(&mut self, chat_id: i64, message_id: i64, settle: Settle, tries: u8, now_ms: u64) {
        if tries > MAX_TRIES {
            self.pending
                .retain(|p| !(p.chat_id == chat_id && p.message_id == message_id && p.is_stuck()));
            return;
        }
        self.set_stage(
            chat_id,
            message_id,
            PaidStage::Stuck {
                settle,
                retry_at_ms: now_ms.saturating_add(retry_after(tries)),
                tries,
            },
        );
        self.join_stuck(chat_id, message_id);
    }

    /// Every `Stuck` entry of the message becomes one: part of the waiting
    /// entry when there is one (TDLib commits them together), else a single
    /// `Stuck` entry.
    fn join_stuck(&mut self, chat_id: i64, message_id: i64) {
        let mut taps = Vec::new();
        let mut sender = None;
        let mut stage = None;
        let mut ix = 0;
        while let Some(p) = self.pending.get(ix) {
            if let PaidStage::Stuck {
                settle: kept,
                retry_at_ms: at,
                tries: spent,
            } = p.stage
                && p.chat_id == chat_id
                && p.message_id == message_id
            {
                let mut stuck = self.pending.remove(ix);
                taps.append(&mut stuck.taps);
                sender = stuck.sender.or(sender);
                stage = Some(PaidStage::Stuck {
                    settle: kept,
                    retry_at_ms: at,
                    tries: spent,
                });
            } else {
                ix += 1;
            }
        }
        if taps.is_empty() {
            return;
        }
        let waiting = self
            .pending
            .iter_mut()
            .find(|p| p.is_waiting() && p.chat_id == chat_id && p.message_id == message_id);
        match waiting {
            Some(waiting) => {
                taps.append(&mut waiting.taps);
                waiting.taps = taps;
                waiting.sender = waiting.sender.or(sender);
            }
            None => {
                if let Some(stage) = stage {
                    self.pending.push(PendingPaid {
                        stage,
                        chat_id,
                        message_id,
                        taps,
                        sender,
                    });
                }
            }
        }
    }

    /// TDLib answered `request` with ok: its Stars are settled, and so are
    /// the Stars of that message an earlier failure left `Stuck` (a commit
    /// or an undo covers everything TDLib holds for the message).
    pub fn settled(&mut self, request: RequestId) {
        let answered = |p: &PendingPaid| matches!(p.stage, PaidStage::Committing(asked) | PaidStage::Undoing { request: asked, .. } if asked == request);
        let messages: Vec<(i64, i64)> = self
            .pending
            .iter()
            .filter(|p| answered(p))
            .map(|p| (p.chat_id, p.message_id))
            .collect();
        self.pending.retain(|p| {
            !answered(p) && !(p.is_stuck() && messages.contains(&(p.chat_id, p.message_id)))
        });
    }

    /// TDLib refused `request`. A refused commit releases its Stars, as in
    /// Telegram Desktop: TDLib took them out of the message with the commit
    /// and unlocked them on the error, so nothing is held and nothing is
    /// sent again (a permanent error, a low balance, would otherwise loop).
    /// A refused undo removed nothing: TDLib still holds those Stars, so the
    /// entry is `Stuck` and the undo is sent again after a wait.
    pub fn refused(&mut self, request: RequestId, now_ms: u64) {
        self.pending
            .retain(|p| p.stage != PaidStage::Committing(request));
        let mut messages = Vec::new();
        for p in &mut self.pending {
            if let PaidStage::Undoing {
                request: asked,
                tries,
            } = p.stage
                && asked == request
            {
                let tries = tries.saturating_add(1);
                p.stage = PaidStage::Stuck {
                    settle: Settle::Undo,
                    retry_at_ms: now_ms.saturating_add(retry_after(tries)),
                    tries,
                };
                messages.push((p.chat_id, p.message_id));
            }
        }
        for (chat_id, message_id) in messages {
            self.join_stuck(chat_id, message_id);
        }
        // A refused undo sent again too often is let go.
        self.pending
            .retain(|p| !matches!(p.stage, PaidStage::Stuck { tries, .. } if tries > MAX_TRIES));
    }

    /// Every waiting reaction whose time is up, and every stuck one whose
    /// wait is over, still as they were: the driver calls `begin_commit`
    /// (or `begin_undo`) once the request went out, else `commit_unsent`.
    pub fn due(&self, now_ms: u64) -> Vec<PendingPaid> {
        self.pending
            .iter()
            .filter(|p| match p.stage {
                PaidStage::Waiting { commit_at_ms } => commit_at_ms <= now_ms,
                PaidStage::Stuck { retry_at_ms, .. } => retry_at_ms <= now_ms,
                PaidStage::Committing(_) | PaidStage::Undoing { .. } => false,
            })
            .cloned()
            .collect()
    }

    /// The reaction the toast shows: the most recently tapped one.
    pub fn toast(&self) -> Option<&PendingPaid> {
        self.pending.iter().rev().find(|p| p.is_waiting())
    }

    /// Whether any Stars are held for Undo (the toast ticks).
    pub fn has_pending(&self) -> bool {
        self.toast().is_some()
    }

    /// Whether a commit or undo still waits for TDLib.
    pub fn in_flight(&self) -> bool {
        self.pending.iter().any(|p| {
            matches!(
                p.stage,
                PaidStage::Committing(_) | PaidStage::Undoing { .. }
            )
        })
    }

    /// How long until a countdown shows another second or a reaction is
    /// due, or a stuck one is sent again. `None` when none is waiting.
    pub fn next_change_ms(&self, now_ms: u64) -> Option<u64> {
        self.pending
            .iter()
            .filter_map(|paid| match paid.stage {
                PaidStage::Waiting { commit_at_ms } => {
                    let left = commit_at_ms.saturating_sub(now_ms);
                    Some(match left % 1000 {
                        0 if left > 0 => 1000,
                        into_second => into_second,
                    })
                }
                PaidStage::Stuck { retry_at_ms, .. } => Some(retry_at_ms.saturating_sub(now_ms)),
                PaidStage::Committing(_) | PaidStage::Undoing { .. } => None,
            })
            .min()
    }
}

#[cfg(test)]
mod tests;
