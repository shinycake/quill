//! Rate limits (TDLib 429 "Too Many Requests: retry after N") and the
//! pending-request sweep, both driven from the UI poll loop.
//!
//! tdesktop (`MTP::Instance` / `Api::...` flood handling) waits out
//! `FLOOD_WAIT_N` for reads and shows `lng_flood_error` for user actions.
//! Here an idempotent read that gets a 429 is re-sent after the wait
//! (bounded by [`FLOOD_RETRY_MAX_ATTEMPTS`] and [`FLOOD_RETRY_MAX_WAIT`]),
//! keeping its pending entry so per-chat dedupe guards stay closed and no
//! second copy can go out meanwhile. Everything else surfaces as before,
//! with the "Too many attempts" notice for user actions (reducer side).
use super::*;
use crate::ids::{ChatId, RequestId};
use crate::state::{PENDING_REQUEST_TTL, RequestPurpose};
use crate::telegram::envelope::{Envelope, EnvelopePayload};
use std::time::{Duration, Instant};

/// A read is re-sent at most this many times after a 429.
pub const FLOOD_RETRY_MAX_ATTEMPTS: u8 = 3;
/// Waits longer than this are not worth holding a spinner for; the error
/// surfaces instead.
pub const FLOOD_RETRY_MAX_WAIT: Duration = Duration::from_secs(60);
/// Used when TDLib sent a 429 without a number.
const FLOOD_RETRY_DEFAULT_WAIT: Duration = Duration::from_secs(5);
/// Extra second on top of the server's wait.
const FLOOD_RETRY_SLACK: Duration = Duration::from_secs(1);
/// How often the stale-request sweep scans the registry.
const SWEEP_EVERY: Duration = Duration::from_secs(15);

/// One request waiting out a rate limit.
pub(crate) struct FloodRetry {
    pub(crate) extra: RequestId,
    pub(crate) json: String,
    pub(crate) due: Instant,
}

impl<S: JsonSender> ConnectDriver<S> {
    /// Called for every inbound envelope before the reducer sees it.
    /// Returns `true` when the envelope was a rate-limit error for an
    /// idempotent read and a retry was scheduled: the reducer must not
    /// see it (the request stays pending). Any other answer for a request
    /// ends its retry bookkeeping.
    pub(crate) fn absorb_flood(&mut self, envelope: &Envelope, now: Instant) -> bool {
        let Some(extra) = envelope.extra else {
            return false;
        };
        if let EnvelopePayload::Error(err) = &envelope.payload
            && err.is_flood()
            && let Some(wait) = flood_retry_wait(err.flood_wait_secs)
            && self.session.requests.get(extra).is_some()
            && !self.flood_retries.iter().any(|r| r.extra == extra)
        {
            let attempts = self.flood_attempts.entry(extra.0).or_insert(0);
            if *attempts < FLOOD_RETRY_MAX_ATTEMPTS
                && let Some(json) = self.sender.take_retained(extra)
            {
                *attempts += 1;
                self.flood_retries.push(FloodRetry {
                    extra,
                    json,
                    due: now + wait + FLOOD_RETRY_SLACK,
                });
                return true;
            }
        }
        self.hold_custom_emoji_after_flood(envelope, extra, now);
        self.flood_attempts.remove(&extra.0);
        self.sender.forget_retained(extra);
        false
    }

    /// A custom emoji lookup's rate limit reaches the reducer (the wait is
    /// too long to retry, or the retries ran out). Its ids are freed there
    /// without counting a failure, and no lookup goes out until the wait is
    /// over, so the next pass does not hit the limit again at once.
    fn hold_custom_emoji_after_flood(
        &mut self,
        envelope: &Envelope,
        extra: RequestId,
        now: Instant,
    ) {
        let EnvelopePayload::Error(err) = &envelope.payload else {
            return;
        };
        let lookup = self
            .session
            .requests
            .get(extra)
            .is_some_and(|pending| pending.purpose == RequestPurpose::GetCustomEmojiStickers);
        if !err.is_flood() || !lookup {
            return;
        }
        let wait = err
            .flood_wait_secs
            .map_or(FLOOD_RETRY_DEFAULT_WAIT, Duration::from_secs);
        let until = now
            .checked_add(wait.saturating_add(FLOOD_RETRY_SLACK))
            .unwrap_or(now);
        self.custom_emoji_hold = Some(self.custom_emoji_hold.map_or(until, |held| held.max(until)));
    }

    /// Per-poll request housekeeping: re-send reads whose rate-limit wait
    /// is over, and (every [`SWEEP_EVERY`]) drop sweepable requests that
    /// never got an answer so their dedupe guards and spinners clear.
    /// Returns whether the UI should redraw.
    pub fn request_tick(&mut self, now: Instant) -> bool {
        // Stream-mode calls: answer what the engine queued since the last poll.
        let mut changed = self.pump_group_call_streams(now);
        // 1:1 calls: what the engine reported between Telegram updates.
        changed |= self.pump_call_engine_reports();
        changed |= self.pump_call_media(now);
        // Group calls: your own level and speaking state, and the
        // connected transports' camera and mute.
        changed |= self.pump_call_audio(now).unwrap_or(true);
        self.pump_connected_group_calls();
        // Group calls: the "You are on mute" hint.
        changed |= self.tick_muted_speech_hint(now);
        self.watch_call_sound(now);
        if !self.flood_retries.is_empty() {
            let (due, waiting): (Vec<_>, Vec<_>) = std::mem::take(&mut self.flood_retries)
                .into_iter()
                .partition(|retry| retry.due <= now);
            self.flood_retries = waiting;
            for retry in due {
                // The account may have been reset while waiting.
                if self.session.requests.get(retry.extra).is_none() {
                    self.flood_attempts.remove(&retry.extra.0);
                    continue;
                }
                if self.sender.send_json(&retry.json).is_ok() {
                    self.session.requests.touch(retry.extra, now);
                } else {
                    self.flood_attempts.remove(&retry.extra.0);
                    if let Some(pending) = self.session.requests.take(retry.extra) {
                        self.session.settle_lost_request(pending);
                    }
                    changed = true;
                }
            }
        }
        if self.custom_emoji_hold.is_some_and(|until| until <= now) {
            self.custom_emoji_hold = None;
            changed = true;
        }
        let sweep_due = self
            .last_request_sweep
            .is_none_or(|last| now.saturating_duration_since(last) >= SWEEP_EVERY);
        if sweep_due {
            self.last_request_sweep = Some(now);
            let waiting: Vec<RequestId> = self.flood_retries.iter().map(|r| r.extra).collect();
            changed |= self
                .session
                .sweep_stale_requests(now, PENDING_REQUEST_TTL, |id| waiting.contains(&id));
        }
        changed
    }

    /// How long until the earliest time-driven work here and in the session
    /// is due (a flood retry, a pending bot message expiring, a Star reaction
    /// toast, reopening a lost microphone), so the poll loop can cap its
    /// idle sleep. `None` when nothing is scheduled.
    pub fn next_deadline(&self, now: Instant, now_ms: u64) -> Option<Duration> {
        let retry = self
            .flood_retries
            .iter()
            .map(|r| r.due.saturating_duration_since(now))
            .min();
        let emoji_hold = self
            .custom_emoji_hold
            .map(|until| until.saturating_duration_since(now));
        let bot = self
            .session
            .next_pending_bot_expiry_ms()
            .map(|at| Duration::from_millis(at.saturating_sub(now_ms)));
        // A group call row whose sounding or speaking runs out.
        let levels = self.group_levels_deadline(now);
        // The muted-speech hint's next check while it listens.
        let hint = self.muted_hint_deadline(now);
        // Your own level's next read while the tap is open.
        let level = self.call_audio_deadline(now);
        let microphone = self.call_capture.until_reopen(now);
        let paid = self
            .session
            .payments
            .paid
            .next_change_ms(now_ms)
            .map(Duration::from_millis);
        // A live story's Stars commit when their Undo time is over.
        let live_stars = self
            .session
            .stories
            .live
            .as_ref()
            .and_then(|live| live.pending_reaction.as_ref())
            .map(|pending| Duration::from_millis(pending.commit_at_ms.saturating_sub(now_ms)));
        [
            retry, emoji_hold, bot, levels, hint, level, microphone, paid, live_stars,
        ]
        .into_iter()
        .flatten()
        .min()
    }

    /// The "Couldn't load messages · Retry" action: clear the failure and
    /// request the first page again.
    pub fn retry_history(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        let Some(chat_id) = self.session.open_chat else {
            return Err(ConnectSendError::InvalidRequest);
        };
        self.retry_history_for(chat_id)
    }

    /// [`Self::retry_history`] for a chat on screen (the open chat or one
    /// shown in its own window).
    pub fn retry_history_for(
        &mut self,
        chat_id: ChatId,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if let Some(history) = self.session.histories.get_mut(&chat_id.0) {
            history.load_failed = false;
        }
        self.fetch_history_for(chat_id)
    }
}

/// The wait before re-sending, or `None` when it is too long to hold the
/// request for.
pub(crate) fn flood_retry_wait(secs: Option<u64>) -> Option<Duration> {
    match secs {
        Some(secs) if Duration::from_secs(secs) > FLOOD_RETRY_MAX_WAIT => None,
        Some(secs) => Some(Duration::from_secs(secs)),
        None => Some(FLOOD_RETRY_DEFAULT_WAIT),
    }
}
