//! The media transport and the screen share of a tracked group call, one
//! value each. They replace `transport_ready` + `join_payload` +
//! `transport_error` and `screen_share_pending` + `screen_sharing` +
//! `screen_share_answer`, which were kept in step by hand and could say two
//! things at once (ready and failed, pending and sharing).

use crate::calls::engine::{GroupCallMode, StreamState};
use crate::ids::RequestId;

/// Where the call's native transport stands.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum GroupTransport {
    /// No join answer yet, or it was cleared for a rejoin.
    #[default]
    NoTransport,
    /// The Join press was taken: `request` is its `joinVideoChat`. Only the
    /// answer or the error of that request ends it (a request that never
    /// left, or an offer nobody wants, puts it back with `join_unsent`).
    Joining { request: RequestId },
    /// The `joinVideoChat` answer is stored; the pump connects it.
    AnswerReady(String),
    /// The native handshake completed. `error` is the last engine failure
    /// (mute, camera, subscribe) of a call that otherwise works.
    Connected { error: Option<String> },
    /// The join or its connect failed; only a rejoin or the Join button
    /// brings a new answer.
    Failed(String),
}

impl GroupTransport {
    pub fn is_connected(&self) -> bool {
        matches!(self, Self::Connected { .. })
    }

    /// The last transport error, connected or not.
    pub fn error(&self) -> Option<&str> {
        match self {
            Self::Connected { error } => error.as_deref(),
            Self::Failed(error) => Some(error),
            Self::NoTransport | Self::Joining { .. } | Self::AnswerReady(_) => None,
        }
    }

    /// A join press was taken and its request is out.
    pub fn joining(&self) -> bool {
        matches!(self, Self::Joining { .. })
    }

    /// The join request that waits for its answer.
    pub fn join_request(&self) -> Option<RequestId> {
        match self {
            Self::Joining { request } => Some(*request),
            _ => None,
        }
    }

    /// The join `request` was made; a connected call keeps its transport.
    pub fn begin_join(&mut self, request: RequestId) {
        if !self.is_connected() {
            *self = Self::Joining { request };
        }
    }

    /// The join `request` never left (or its offer was dropped): back to
    /// no transport, so the Join button asks again.
    pub fn join_unsent(&mut self, request: RequestId) {
        if *self == (Self::Joining { request }) {
            *self = Self::NoTransport;
        }
    }

    /// The error of join `request`: it ends only its own `Joining`.
    pub fn join_failed(&mut self, request: RequestId) {
        self.join_unsent(request);
    }

    /// The stored join answer, empty unless one waits to be connected.
    pub fn answer(&self) -> &str {
        match self {
            Self::AnswerReady(answer) => answer,
            _ => "",
        }
    }

    /// The `joinVideoChat` answer of `request` arrived. A connected call
    /// keeps its transport (a late answer is not a new handshake); another
    /// request's answer does not end a `Joining` that waits for its own; an
    /// empty one says nothing, and ends only the `Joining` it answers.
    pub fn store_answer(&mut self, request: Option<RequestId>, payload: String) {
        if self.is_connected() {
            return;
        }
        if let Self::Joining { request: mine } = *self
            && request.is_some_and(|answered| answered != mine)
        {
            return;
        }
        if payload.is_empty() {
            if self.joining() {
                *self = Self::NoTransport;
            }
            return;
        }
        *self = Self::AnswerReady(payload);
    }

    /// The handshake with the stored answer succeeded.
    pub fn connected(&mut self) {
        *self = Self::Connected { error: None };
    }

    /// The handshake failed: the answer is spent.
    pub fn connect_failed(&mut self, error: impl Into<String>) {
        *self = Self::Failed(error.into());
    }

    /// A failure outside the handshake: a connected call keeps working with
    /// the error noted, any other stage fails.
    pub fn fail(&mut self, error: impl Into<String>) {
        match self {
            Self::Connected { error: held } => *held = Some(error.into()),
            _ => *self = Self::Failed(error.into()),
        }
    }

    /// Note an engine failure of a connected call once: a failing engine
    /// does not rewrite it every tick.
    pub fn note_error_once(&mut self, error: impl Into<String>) {
        if let Self::Connected { error: held } = self
            && held.is_none()
        {
            *held = Some(error.into());
        }
    }

    /// Forget the last error. A failed join has no transport left to keep.
    pub fn clear_error(&mut self) {
        match self {
            Self::Connected { error } => *error = None,
            Self::Failed(_) => *self = Self::NoTransport,
            Self::NoTransport | Self::Joining { .. } | Self::AnswerReady(_) => {}
        }
    }

    /// A fresh native context starts unconnected: the old connection, answer
    /// and error are gone.
    pub fn reset(&mut self) {
        *self = Self::NoTransport;
    }
}

/// How a joined group call carries its media, with what the call window
/// shows for a stream: the state exists only for a stream mode, so a WebRTC
/// call has none to read stale.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GroupMedia {
    /// WebRTC, or not connected yet.
    #[default]
    Rtc,
    /// Segments come from TDLib: an MTProto stream or an RTMP feed (`mode`
    /// is never `Rtc`), and what the window shows for it.
    Stream {
        mode: GroupCallMode,
        state: StreamState,
    },
}

impl GroupMedia {
    /// The engine's mode after the native connect; a stream mode starts
    /// connecting.
    pub fn connected(mode: GroupCallMode) -> Self {
        if mode.is_stream() {
            Self::Stream {
                mode,
                state: StreamState::Joined,
            }
        } else {
            Self::Rtc
        }
    }

    pub fn mode(&self) -> GroupCallMode {
        match self {
            Self::Rtc => GroupCallMode::Rtc,
            Self::Stream { mode, .. } => *mode,
        }
    }

    pub fn is_stream(&self) -> bool {
        matches!(self, Self::Stream { .. })
    }

    /// The stream's state; a call without a stream reads `Joined`.
    pub fn stream_state(&self) -> StreamState {
        match self {
            Self::Rtc => StreamState::Joined,
            Self::Stream { state, .. } => *state,
        }
    }

    /// The feeder's state; a call without a stream has none to take it.
    pub fn set_stream_state(&mut self, next: StreamState) {
        if let Self::Stream { state, .. } = self {
            *state = next;
        }
    }
}

/// The screen share of a tracked group call, one value: off, the last
/// share's failure, the start in flight, or the connected presentation.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum GroupScreenShare {
    #[default]
    Off,
    /// The last share failed (its offer, start, connect or stop); the call
    /// itself still works. A new start or a stop replaces it.
    Failed(String),
    /// `startGroupCallScreenSharing` is in flight as `request` (only its
    /// answer or error counts); `answer` is its `Text` answer once it
    /// arrived, for the pump to connect. Leaves by the connect, the
    /// request's error, a cancel or a stop.
    Starting { request: RequestId, answer: String },
    /// The presentation transport is connected.
    Sharing,
    /// Connected, and the capture is held (the system picker's pause): the
    /// own tile shows paused. A start, a stop or a connect leaves it.
    Paused,
}

impl GroupScreenShare {
    /// Connected, paused or not (the capture runs while it is).
    pub fn is_sharing(&self) -> bool {
        matches!(self, Self::Sharing | Self::Paused)
    }

    pub fn is_paused(&self) -> bool {
        matches!(self, Self::Paused)
    }

    pub fn is_starting(&self) -> bool {
        matches!(self, Self::Starting { .. })
    }

    /// Sharing, or on the way to it.
    pub fn is_on(&self) -> bool {
        matches!(self, Self::Starting { .. } | Self::Sharing | Self::Paused)
    }

    /// The failure of the last share, until another starts.
    pub fn error(&self) -> Option<&str> {
        match self {
            Self::Failed(error) => Some(error),
            _ => None,
        }
    }

    /// The stored presentation answer, empty unless one waits.
    pub fn answer(&self) -> &str {
        match self {
            Self::Starting { answer, .. } => answer,
            _ => "",
        }
    }

    /// A start went out as `request`; a start already in flight is replaced
    /// and its late answer is then dropped.
    pub fn start(&mut self, request: RequestId) {
        *self = Self::Starting {
            request,
            answer: String::new(),
        };
    }

    /// The answer of `request`, the start in flight; any other is stale.
    pub fn store_answer(&mut self, request: Option<RequestId>, payload: String) {
        if let Self::Starting {
            request: waiting,
            answer,
        } = self
            && request == Some(*waiting)
        {
            *answer = payload;
        }
    }

    /// `request` came back with an error: only the start it asked ends.
    pub fn fail_start(&mut self, request: Option<RequestId>) {
        if let Self::Starting {
            request: waiting, ..
        } = self
            && request.is_none_or(|request| request == *waiting)
        {
            *self = Self::Off;
        }
    }

    /// A start that did not go through ends; a connected share stays.
    pub fn cancel_start(&mut self) {
        if self.is_starting() {
            *self = Self::Off;
        }
    }

    pub fn connected(&mut self) {
        *self = Self::Sharing;
    }

    /// The capture was held or let go: only a connected share has one.
    /// Returns whether it was held before.
    pub fn set_paused(&mut self, paused: bool) -> bool {
        let was = self.is_paused();
        match (&*self, paused) {
            (Self::Sharing, true) => *self = Self::Paused,
            (Self::Paused, false) => *self = Self::Sharing,
            _ => {}
        }
        was
    }

    /// Stopped or reset.
    pub fn stop(&mut self) {
        *self = Self::Off;
    }

    /// The share failed (or could not start): off, with the reason.
    pub fn fail(&mut self, error: String) {
        *self = Self::Failed(error);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::RequestId;

    #[test]
    fn a_late_answer_does_not_end_a_connection() {
        let mut transport = GroupTransport::default();
        transport.store_answer(None, "a".into());
        assert_eq!(transport.answer(), "a");
        transport.connected();
        transport.store_answer(None, "late".into());
        assert!(transport.is_connected());
        assert_eq!(transport.answer(), "");
    }

    #[test]
    fn a_join_ends_on_its_own_request_only() {
        let (mine, other) = (RequestId(7), RequestId(8));
        let mut transport = GroupTransport::default();
        transport.begin_join(mine);
        assert!(transport.joining());
        transport.store_answer(Some(other), "stale".into());
        assert!(transport.joining());
        transport.join_failed(other);
        assert!(transport.joining());
        transport.join_failed(mine);
        assert_eq!(transport, GroupTransport::NoTransport);
        transport.begin_join(mine);
        transport.store_answer(Some(mine), String::new());
        assert_eq!(transport, GroupTransport::NoTransport);
        transport.begin_join(mine);
        transport.join_unsent(mine);
        assert!(!transport.joining());
        transport.begin_join(mine);
        transport.store_answer(Some(mine), "a".into());
        assert_eq!(transport.answer(), "a");
        transport.connected();
        transport.begin_join(other);
        assert!(transport.is_connected());
    }

    #[test]
    fn errors_ride_a_connection_and_fail_a_handshake() {
        let mut transport = GroupTransport::Connected { error: None };
        transport.note_error_once("first");
        transport.note_error_once("second");
        assert_eq!(transport.error(), Some("first"));
        transport.clear_error();
        assert_eq!(transport.error(), None);
        let mut failed = GroupTransport::AnswerReady("a".into());
        failed.connect_failed("no");
        assert!(!failed.is_connected());
        assert_eq!(failed.answer(), "");
        failed.clear_error();
        assert_eq!(failed, GroupTransport::NoTransport);
    }

    #[test]
    fn a_stale_screen_answer_is_not_kept() {
        let mut share = GroupScreenShare::default();
        share.store_answer(Some(RequestId(1)), "stale".into());
        share.start(RequestId(2));
        assert_eq!(share.answer(), "");
        // The first start's answer lands during the second.
        share.store_answer(Some(RequestId(1)), "first".into());
        share.store_answer(None, "unmatched".into());
        assert_eq!(share.answer(), "");
        share.store_answer(Some(RequestId(2)), "fresh".into());
        assert_eq!(share.answer(), "fresh");
        share.connected();
        assert!(share.is_sharing() && share.is_on() && !share.is_starting());
        share.set_paused(true);
        assert!(share.is_paused());
        share.stop();
        assert!(!share.is_on() && !share.is_paused());
        share.fail("no".into());
        assert_eq!(share.error(), Some("no"));
        share.start(RequestId(3));
        assert_eq!(share.error(), None, "a new start leaves the failure");
    }

    #[test]
    fn only_the_start_in_flight_fails() {
        let mut share = GroupScreenShare::default();
        share.start(RequestId(1));
        share.start(RequestId(2));
        share.fail_start(Some(RequestId(1)));
        assert!(share.is_starting(), "the older start's error is dropped");
        share.fail_start(Some(RequestId(2)));
        assert!(!share.is_on());
        // A connected share is not ended by a start's error.
        share.connected();
        share.fail_start(Some(RequestId(2)));
        assert!(share.is_sharing());
    }

    #[test]
    fn pausing_needs_a_connected_share_and_ends_with_it() {
        let mut share = GroupScreenShare::default();
        share.set_paused(true);
        assert!(!share.is_paused(), "off cannot be paused");
        share.start(RequestId(1));
        share.set_paused(true);
        assert!(!share.is_paused(), "starting cannot be paused");
        share.connected();
        share.set_paused(true);
        assert!(share.is_paused() && share.is_sharing());
        share.set_paused(false);
        assert!(!share.is_paused() && share.is_sharing());
        share.set_paused(true);
        share.stop();
        share.connected();
        assert!(!share.is_paused(), "a new share starts unpaused");
    }
}
