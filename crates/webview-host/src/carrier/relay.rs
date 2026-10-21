//! The carrier's relay state, owned by the event loop and free of any web
//! view so it can be tested: what the bridge said, the deliveries waiting
//! for the page, and the timers that decide a page failed.

use quill_webview_protocol::carrier::{
    CarrierFailure, MAX_PAGE_BINARY_BYTES, MAX_PAGE_TEXT_BYTES, MAX_PENDING_BYTES,
    MAX_PENDING_ITEMS, base64_len, is_base64,
};
use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// No word from a started page for this long means its renderer is gone
/// or hung (tdesktop `kHealthTimeout`).
pub(super) const PAGE_SILENCE: Duration = Duration::from_secs(10);
/// A delivery the page has not acknowledged in this long (tdesktop
/// `kWriteTimeout`).
pub(super) const WRITE_TIMEOUT: Duration = Duration::from_secs(10);
/// A page whose own script has not started this long after the load began
/// never loaded: a DNS, connection or TLS failure, or an engine error page.
pub(super) const LOAD_TIMEOUT: Duration = Duration::from_secs(30);
/// A page that finished loading but whose script has not started in this
/// long is not a relay page (an error page on the proxy's origin, say).
pub(super) const START_TIMEOUT: Duration = Duration::from_secs(10);

/// What the bridge script posts, after the token.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum PageMessage {
    /// `h`: the bridge is in place, or answers a probe.
    Hello,
    /// `o`: the page set a function as `onmessage`.
    Receiving,
    /// `a<seq>`: the page took a delivery.
    Ack(u64),
    /// `b<base64>`: a binary message.
    Binary(String),
    /// `t<text>`: a string message.
    Text(String),
    /// `f`: the bridge failed (no `onmessage`, the page's handler threw).
    Failed,
    /// Too long, not base64, or unknown: treated as a broken relay.
    Invalid,
}

/// Split one IPC body: `None` unless it starts with the carrier's token.
pub(super) fn page_message(body: &str, token: &str) -> Option<PageMessage> {
    let rest = body.strip_prefix(token)?;
    let mut chars = rest.chars();
    let kind = chars.next()?;
    let value = chars.as_str();
    Some(match kind {
        'h' if value.is_empty() => PageMessage::Hello,
        'o' if value.is_empty() => PageMessage::Receiving,
        'a' => value.parse().map_or(PageMessage::Invalid, PageMessage::Ack),
        'b' if value.len() <= base64_len(MAX_PAGE_BINARY_BYTES)
            && !value.is_empty()
            && is_base64(value) =>
        {
            PageMessage::Binary(value.to_string())
        }
        't' if value.len() <= MAX_PAGE_TEXT_BYTES => PageMessage::Text(value.to_string()),
        'f' => PageMessage::Failed,
        _ => PageMessage::Invalid,
    })
}

struct InFlight {
    sequence: u64,
    sent: Instant,
    data: String,
}

pub(super) struct Relay {
    pending: VecDeque<String>,
    pending_bytes: usize,
    in_flight: Option<InFlight>,
    sequence: u64,
    /// When the current document started loading.
    load_started: Instant,
    /// When it finished loading, if it did.
    load_finished: Option<Instant>,
    /// The bridge said hello in this document.
    bridge_seen: bool,
    /// The page's `onmessage` is a function: deliveries may go.
    receiving: bool,
    /// The page's own script did something in this document (set
    /// `onmessage` or posted); before that the bridge's own hello says
    /// nothing about the page.
    started: bool,
    /// The page posted at least once, in any document (it gets
    /// `{"t":"close"}` at the end).
    pub(super) posted: bool,
    last_heard: Instant,
    pub(super) closing_at: Option<Instant>,
}

impl Relay {
    pub(super) fn new(now: Instant) -> Self {
        Self {
            pending: VecDeque::new(),
            pending_bytes: 0,
            in_flight: None,
            sequence: 0,
            load_started: now,
            load_finished: None,
            bridge_seen: false,
            receiving: false,
            started: false,
            posted: false,
            last_heard: now,
            closing_at: None,
        }
    }

    /// A document started loading (the first, or a reload in scope): its
    /// bridge is new, and a delivery the old one never acknowledged goes
    /// again first.
    pub(super) fn load_started(&mut self, now: Instant) {
        self.load_started = now;
        self.load_finished = None;
        self.bridge_seen = false;
        self.receiving = false;
        self.started = false;
        if let Some(lost) = self.in_flight.take() {
            self.pending_bytes = self.pending_bytes.saturating_add(lost.data.len());
            self.pending.push_front(lost.data);
        }
    }

    pub(super) fn load_finished(&mut self, now: Instant) {
        self.load_finished = Some(now);
    }

    /// Whether the page's own script has started.
    pub(super) fn started(&self) -> bool {
        self.started
    }

    /// Note a bridge message. `Err` when it breaks the relay.
    pub(super) fn heard(&mut self, message: &PageMessage, now: Instant) -> Result<(), ()> {
        self.last_heard = now;
        match message {
            PageMessage::Hello => self.bridge_seen = true,
            PageMessage::Receiving => {
                self.receiving = true;
                self.started = true;
            }
            PageMessage::Ack(sequence) => match &self.in_flight {
                Some(flight) if flight.sequence == *sequence => self.in_flight = None,
                _ => return Err(()),
            },
            PageMessage::Binary(_) | PageMessage::Text(_) => {
                self.started = true;
                self.posted = true;
            }
            PageMessage::Failed | PageMessage::Invalid => return Err(()),
        }
        Ok(())
    }

    /// Queue a delivery; `false` when the page is too far behind.
    pub(super) fn queue(&mut self, data: String) -> bool {
        let bytes = self.pending_bytes.saturating_add(data.len());
        let items = self.pending.len() + usize::from(self.in_flight.is_some());
        if items >= MAX_PENDING_ITEMS || bytes > base64_len(MAX_PENDING_BYTES) {
            return false;
        }
        self.pending_bytes = bytes;
        self.pending.push_back(data);
        true
    }

    /// The next delivery to hand the page: none while one is in flight or
    /// before the page can take it.
    pub(super) fn next(&mut self, now: Instant) -> Option<(u64, String)> {
        if self.in_flight.is_some() || !self.bridge_seen || !self.receiving {
            return None;
        }
        let data = self.pending.pop_front()?;
        self.pending_bytes = self.pending_bytes.saturating_sub(data.len());
        self.sequence = self.sequence.saturating_add(1);
        self.in_flight = Some(InFlight {
            sequence: self.sequence,
            sent: now,
            data: data.clone(),
        });
        Some((self.sequence, data))
    }

    /// Why the carrier failed by `now`, if it did.
    pub(super) fn failure(&self, now: Instant) -> Option<CarrierFailure> {
        if !self.started {
            let load_failed = |message: &str| CarrierFailure::LoadFailed {
                message: message.into(),
            };
            if now.saturating_duration_since(self.load_started) > LOAD_TIMEOUT {
                return Some(load_failed("the relay page did not load"));
            }
            if self
                .load_finished
                .is_some_and(|at| now.saturating_duration_since(at) > START_TIMEOUT)
            {
                return Some(load_failed("the relay page did not start"));
            }
            return None;
        }
        let silent = now.saturating_duration_since(self.last_heard) > PAGE_SILENCE;
        let stuck = self
            .in_flight
            .as_ref()
            .is_some_and(|flight| now.saturating_duration_since(flight.sent) > WRITE_TIMEOUT);
        (silent || stuck).then_some(CarrierFailure::Terminated)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOKEN: &str = "00112233445566778899aabbccddeeff";

    #[test]
    fn bridge_messages_need_the_token() {
        let message = |body: &str| page_message(&format!("{TOKEN}{body}"), TOKEN);
        assert_eq!(message("h"), Some(PageMessage::Hello));
        assert_eq!(message("o"), Some(PageMessage::Receiving));
        assert_eq!(page_message("h", TOKEN), None);
        assert_eq!(
            page_message("ffffffffffffffffffffffffffffffffh", TOKEN),
            None
        );
        assert_eq!(page_message(TOKEN, TOKEN), None);
        assert_eq!(message("a17"), Some(PageMessage::Ack(17)));
        assert_eq!(message("ax"), Some(PageMessage::Invalid));
        assert_eq!(message("bAAEC"), Some(PageMessage::Binary("AAEC".into())));
        assert_eq!(message("b"), Some(PageMessage::Invalid));
        assert_eq!(message("b!!"), Some(PageMessage::Invalid));
        assert_eq!(
            message(r#"t{"t":"x"}"#),
            Some(PageMessage::Text(r#"{"t":"x"}"#.into()))
        );
        assert_eq!(message("f"), Some(PageMessage::Failed));
        assert_eq!(message("ox"), Some(PageMessage::Invalid));
        assert_eq!(message("z"), Some(PageMessage::Invalid));
    }

    #[test]
    fn oversized_page_messages_are_refused() {
        let text = "x".repeat(MAX_PAGE_TEXT_BYTES + 1);
        assert_eq!(
            page_message(&format!("{TOKEN}t{text}"), TOKEN),
            Some(PageMessage::Invalid)
        );
        let data = "A".repeat(base64_len(MAX_PAGE_BINARY_BYTES) + 4);
        assert_eq!(
            page_message(&format!("{TOKEN}b{data}"), TOKEN),
            Some(PageMessage::Invalid)
        );
    }

    fn ready(now: Instant) -> Relay {
        let mut relay = Relay::new(now);
        relay.heard(&PageMessage::Hello, now).unwrap();
        relay.heard(&PageMessage::Receiving, now).unwrap();
        relay
    }

    #[test]
    fn deliveries_wait_for_the_page_then_go_one_at_a_time() {
        let now = Instant::now();
        let mut relay = Relay::new(now);
        assert!(relay.queue("AAAA".into()));
        assert!(relay.queue("BBBB".into()));
        // Nothing goes before the bridge exists and the page listens.
        assert_eq!(relay.next(now), None);
        relay.heard(&PageMessage::Hello, now).unwrap();
        assert_eq!(relay.next(now), None);
        relay.heard(&PageMessage::Receiving, now).unwrap();
        assert_eq!(relay.next(now), Some((1, "AAAA".into())));
        assert_eq!(relay.next(now), None);
        assert!(relay.heard(&PageMessage::Ack(2), now).is_err());
        relay.heard(&PageMessage::Ack(1), now).unwrap();
        assert_eq!(relay.next(now), Some((2, "BBBB".into())));
        assert_eq!(relay.pending_bytes, 0);
    }

    #[test]
    fn a_reload_sends_the_lost_delivery_again() {
        let now = Instant::now();
        let mut relay = ready(now);
        assert!(relay.queue("AAAA".into()));
        assert!(relay.queue("BBBB".into()));
        assert_eq!(relay.next(now), Some((1, "AAAA".into())));
        relay.load_started(now);
        assert_eq!(relay.next(now), None);
        relay.heard(&PageMessage::Hello, now).unwrap();
        relay.heard(&PageMessage::Receiving, now).unwrap();
        assert_eq!(relay.next(now), Some((2, "AAAA".into())));
        relay.heard(&PageMessage::Ack(2), now).unwrap();
        assert_eq!(relay.next(now), Some((3, "BBBB".into())));
    }

    #[test]
    fn a_page_that_falls_behind_or_goes_quiet_fails() {
        let start = Instant::now();
        let mut relay = ready(start);
        for _ in 0..MAX_PENDING_ITEMS {
            assert!(relay.queue("AAAA".into()));
        }
        assert!(!relay.queue("AAAA".into()));
        let mut big = ready(start);
        assert!(!big.queue("A".repeat(base64_len(MAX_PENDING_BYTES) + 4)));

        let mut relay = ready(start);
        assert!(relay.queue("AAAA".into()));
        let _ = relay.next(start);
        assert_eq!(relay.failure(start + Duration::from_secs(1)), None);
        assert_eq!(
            relay.failure(start + WRITE_TIMEOUT + Duration::from_secs(1)),
            Some(CarrierFailure::Terminated)
        );

        let quiet = ready(start);
        assert_eq!(
            quiet.failure(start + PAGE_SILENCE + Duration::from_secs(1)),
            Some(CarrierFailure::Terminated)
        );
    }

    #[test]
    fn a_page_that_never_starts_failed_to_load() {
        let start = Instant::now();
        // Nothing at all (no route to the host, a TLS failure).
        let silent = Relay::new(start);
        assert_eq!(silent.failure(start + START_TIMEOUT * 2), None);
        assert!(matches!(
            silent.failure(start + LOAD_TIMEOUT + Duration::from_secs(1)),
            Some(CarrierFailure::LoadFailed { .. })
        ));
        // An error page on the proxy's origin: the bridge is injected and
        // answers, but no relay script ever takes `onmessage`.
        let mut error_page = Relay::new(start);
        error_page.heard(&PageMessage::Hello, start).unwrap();
        error_page.load_finished(start);
        assert_eq!(error_page.failure(start + Duration::from_secs(5)), None);
        assert!(matches!(
            error_page.failure(start + START_TIMEOUT + Duration::from_secs(1)),
            Some(CarrierFailure::LoadFailed { .. })
        ));
        // A page that posts has started, even before it listens.
        let mut talker = Relay::new(start);
        talker
            .heard(&PageMessage::Text("hi".into()), start)
            .unwrap();
        assert!(talker.started() && talker.posted);
        assert_eq!(talker.failure(start + Duration::from_secs(5)), None);
    }
}
