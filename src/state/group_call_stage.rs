//! The media transport and the screen share of a tracked group call, one
//! value each. They replace `transport_ready` + `join_payload` +
//! `transport_error` and `screen_share_pending` + `screen_sharing` +
//! `screen_share_answer`, which were kept in step by hand and could say two
//! things at once (ready and failed, pending and sharing).

/// Where the call's native transport stands.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum GroupTransport {
    /// No join answer yet, or it was cleared for a rejoin.
    #[default]
    NoTransport,
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
            Self::NoTransport | Self::AnswerReady(_) => None,
        }
    }

    /// The stored join answer, empty unless one waits to be connected.
    pub fn answer(&self) -> &str {
        match self {
            Self::AnswerReady(answer) => answer,
            _ => "",
        }
    }

    /// The `joinVideoChat` answer arrived. A connected call keeps its
    /// transport (a late answer is not a new handshake), and an empty one
    /// says nothing.
    pub fn store_answer(&mut self, payload: String) {
        if payload.is_empty() || self.is_connected() {
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
            Self::NoTransport | Self::AnswerReady(_) => {}
        }
    }

    /// A fresh native context starts unconnected: the old connection, answer
    /// and error are gone.
    pub fn reset(&mut self) {
        *self = Self::NoTransport;
    }
}

/// The screen share of a tracked group call, one value: off, the last
/// share's failure, the start in flight, or the connected presentation.
/// Starting a share leaves a failure; every other exit is below.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum GroupScreenShare {
    #[default]
    Off,
    /// The last share failed (its offer, start, connect or stop); the call
    /// itself still works. A new start replaces it.
    Failed(String),
    /// `startGroupCallScreenSharing` is in flight as `request`; `answer` is
    /// its `Text` answer once it arrived, for the pump to connect. Another
    /// request's answer lands nowhere.
    Starting {
        request: crate::ids::RequestId,
        answer: Option<String>,
    },
    /// The presentation transport is connected. `paused`: your shared
    /// window is hidden (the driver sent
    /// `toggleGroupCallScreenSharingIsPaused`); your own tile shows it at
    /// once, before the participant update.
    Sharing { paused: bool },
}

impl GroupScreenShare {
    pub fn is_sharing(&self) -> bool {
        matches!(self, Self::Sharing { .. })
    }

    pub fn is_starting(&self) -> bool {
        matches!(self, Self::Starting { .. })
    }

    /// Sharing, or on the way to it.
    pub fn is_on(&self) -> bool {
        matches!(self, Self::Starting { .. } | Self::Sharing { .. })
    }

    /// The stored presentation answer, once one waits.
    pub fn answer(&self) -> Option<&str> {
        match self {
            Self::Starting { answer, .. } => answer.as_deref(),
            _ => None,
        }
    }

    /// The failure of the last share, until another starts.
    pub fn error(&self) -> Option<&str> {
        match self {
            Self::Failed(error) => Some(error),
            _ => None,
        }
    }

    /// Your shared window is hidden and the share is paused.
    pub fn paused(&self) -> bool {
        matches!(self, Self::Sharing { paused: true })
    }

    /// The start went out as `request`.
    pub fn start(&mut self, request: crate::ids::RequestId) {
        *self = Self::Starting {
            request,
            answer: None,
        };
    }

    /// The answer of the start `request`; any other (or none in flight) is
    /// stale.
    pub fn store_answer(&mut self, request: Option<crate::ids::RequestId>, payload: String) {
        if let Self::Starting {
            request: waiting,
            answer,
        } = self
            && request == Some(*waiting)
        {
            *answer = Some(payload);
        }
    }

    /// A start that did not go through ends; a connected share stays.
    pub fn cancel_start(&mut self) {
        if self.is_starting() {
            *self = Self::Off;
        }
    }

    pub fn connected(&mut self) {
        *self = Self::Sharing { paused: false };
    }

    /// The pause state of the connected share; nothing else has one.
    pub fn set_paused(&mut self, now_paused: bool) {
        if let Self::Sharing { paused } = self {
            *paused = now_paused;
        }
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

    #[test]
    fn a_late_answer_does_not_end_a_connection() {
        let mut transport = GroupTransport::default();
        transport.store_answer("a".into());
        assert_eq!(transport.answer(), "a");
        transport.connected();
        transport.store_answer("late".into());
        assert!(transport.is_connected());
        assert_eq!(transport.answer(), "");
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
        use crate::ids::RequestId;
        let mut share = GroupScreenShare::default();
        share.store_answer(Some(RequestId(1)), "stale".into());
        share.start(RequestId(2));
        assert_eq!(share.answer(), None);
        share.store_answer(Some(RequestId(1)), "earlier".into());
        assert_eq!(share.answer(), None, "another request's answer");
        share.store_answer(Some(RequestId(2)), "fresh".into());
        assert_eq!(share.answer(), Some("fresh"));
        share.connected();
        assert!(share.is_sharing() && share.is_on() && !share.is_starting());
        share.set_paused(true);
        assert!(share.paused());
        share.stop();
        assert!(!share.is_on() && !share.paused());
        share.fail("no".into());
        assert_eq!(share.error(), Some("no"));
        share.start(RequestId(3));
        assert_eq!(share.error(), None, "a new start leaves the failure");
    }
}
