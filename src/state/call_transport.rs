//! The 1:1 call's media transport stage.
use super::*;
use crate::calls::engine::ConnectParams;
use std::time::Instant;

/// The 1:1 call's media transport. One value, so a phase can never sit
/// next to a stale error and the retry budget lives with the call it
/// belongs to. Quill connects once per Ready call, retries the retained
/// parameters up to `MAX_CALL_RECONNECT_ATTEMPTS` times on a reported
/// failure, and a `Connected` report starts the budget over.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallTransport {
    /// Not connected yet. `start_error`: the engine could not start the
    /// call; the call stays signaling-only and still connects on Ready.
    NotStarted {
        start_error: Option<String>,
    },
    /// A connect attempt is under way: issued, or the engine reports it is
    /// connecting. The engine's next state report ends it.
    Attempt(CallLink),
    Connected(CallLink),
    /// A retry of the retained parameters is in flight. It ends with the
    /// engine's next report, or at `deadline` (see
    /// [`CallTransport::reconnect_overdue`]), when the driver counts it as
    /// a failed report: the next retry, or the end of the budget.
    Reconnecting {
        link: CallLink,
        deadline: Instant,
    },
    /// The engine closed the transport.
    Closed(CallLink),
    /// No transport. `link` is kept when one was connected, so a later
    /// engine failure report can still retry (or say the budget is spent).
    Failed {
        reason: String,
        link: Option<CallLink>,
    },
}

/// What a connected transport keeps for its reconnects.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CallLink {
    pub params: ConnectParams,
    /// Reconnects since the last `Connected` report.
    pub reconnect_attempts: usize,
}

/// Reconnects Quill tries on its own after the engine reports a failure.
pub const MAX_CALL_RECONNECT_ATTEMPTS: usize = 3;

/// How long a retry may stay silent before it counts as failed.
pub const CALL_RECONNECT_WINDOW: std::time::Duration = std::time::Duration::from_secs(20);

impl Default for CallTransport {
    fn default() -> Self {
        Self::NotStarted { start_error: None }
    }
}

impl CallTransport {
    /// The phase the engine API and the UI speak; `None` before connect.
    pub fn state(&self) -> Option<TransportState> {
        match self {
            Self::NotStarted { .. } => None,
            Self::Attempt(_) => Some(TransportState::Connecting),
            Self::Connected(_) => Some(TransportState::Connected),
            Self::Reconnecting { .. } => Some(TransportState::Reconnecting),
            Self::Closed(_) => Some(TransportState::Closed),
            Self::Failed { .. } => Some(TransportState::Failed),
        }
    }

    /// The error to show: a failed start before connect, or the failure.
    pub fn error(&self) -> Option<&str> {
        match self {
            Self::NotStarted { start_error } => start_error.as_deref(),
            Self::Failed { reason, .. } => Some(reason),
            _ => None,
        }
    }

    /// A connect was attempted (the engine owns a transport, or failed to).
    pub fn started(&self) -> bool {
        !matches!(self, Self::NotStarted { .. })
    }

    pub fn is_connected(&self) -> bool {
        matches!(self, Self::Connected(_))
    }

    pub fn link(&self) -> Option<&CallLink> {
        match self {
            Self::NotStarted { .. } => None,
            Self::Attempt(link)
            | Self::Connected(link)
            | Self::Reconnecting { link, .. }
            | Self::Closed(link) => Some(link),
            Self::Failed { link, .. } => link.as_ref(),
        }
    }

    pub fn reconnect_attempts(&self) -> usize {
        self.link().map_or(0, |link| link.reconnect_attempts)
    }

    /// The engine could not start the call. Before connect it is the
    /// start error; a failed transport takes it as its reason; a live
    /// transport has outgrown it.
    pub fn record_start_error(&mut self, error: String) {
        match self {
            Self::NotStarted { start_error } => *start_error = Some(error),
            Self::Failed { reason, .. } => *reason = error,
            _ => {}
        }
    }

    /// A retry of `link` started at `now`.
    pub fn reconnecting(link: CallLink, now: Instant) -> Self {
        Self::Reconnecting {
            link,
            deadline: now + CALL_RECONNECT_WINDOW,
        }
    }

    /// A retry that the engine has not reported on by `now`.
    pub fn reconnect_overdue(&self, now: Instant) -> bool {
        matches!(self, Self::Reconnecting { deadline, .. } if *deadline <= now)
    }

    /// When the running retry is overdue, if one runs.
    pub fn reconnect_deadline(&self) -> Option<Instant> {
        match self {
            Self::Reconnecting { deadline, .. } => Some(*deadline),
            _ => None,
        }
    }

    /// A live phase the engine reported, carrying `link`. `Failed` is not
    /// one: a reported failure goes through the reconnect decision.
    pub fn reported(state: TransportState, link: CallLink, now: Instant) -> Option<Self> {
        match state {
            TransportState::Connecting => Some(Self::Attempt(link)),
            TransportState::Connected => Some(Self::Connected(CallLink {
                reconnect_attempts: 0,
                ..link
            })),
            TransportState::Reconnecting => Some(Self::reconnecting(link, now)),
            TransportState::Closed => Some(Self::Closed(link)),
            TransportState::Failed => None,
        }
    }
}
