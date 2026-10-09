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
use crate::ids::RequestId;
use crate::state::PENDING_REQUEST_TTL;
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
        self.flood_attempts.remove(&extra.0);
        self.sender.forget_retained(extra);
        false
    }

    /// Per-poll request housekeeping: re-send reads whose rate-limit wait
    /// is over, and (every [`SWEEP_EVERY`]) drop sweepable requests that
    /// never got an answer so their dedupe guards and spinners clear.
    /// Returns whether the UI should redraw.
    pub fn request_tick(&mut self, now: Instant) -> bool {
        let mut changed = false;
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
                    self.session.requests.take(retry.extra);
                    changed = true;
                }
            }
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

    /// The "Couldn't load messages · Retry" action: clear the failure and
    /// request the first page again.
    pub fn retry_history(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        if let Some(chat_id) = self.session.open_chat
            && let Some(history) = self.session.histories.get_mut(&chat_id.0)
        {
            history.load_failed = false;
        }
        self.fetch_history()
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
