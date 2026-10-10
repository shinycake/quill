//! Connection indicator: how the TDLib connection state renders.
use super::*;

/// Slice parity:platform-offline-indicator — what the UI renders for a
/// TDLib connection state. Only `Ready` is "connected" (no indicator);
/// every other state renders the offline/connection indicator.
/// `Unknown` is treated as transitional (falls back to "Connecting…"),
/// never as connected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionIndicator {
    /// Fully offline — banner with the "Waiting for network…" label.
    Offline,
    /// Connecting / updating / proxy — presence dot plus the per-state
    /// label ("Connecting…", "Updating…", "Connecting to proxy…").
    /// (Slice parity:platform-reconnect-states.)
    Transitioning(&'static str),
}

impl ConnectionIndicator {
    /// The label the connection strip renders for this indicator.
    pub fn label(&self) -> &'static str {
        match self {
            ConnectionIndicator::Offline => "Waiting for network…",
            ConnectionIndicator::Transitioning(label) => label,
        }
    }
}

/// Slice parity:platform-offline-indicator — `Session::connection` →
/// indicator visibility. `None` = `Ready` = connected, nothing renders.
pub fn connection_indicator(state: ConnectionState) -> Option<ConnectionIndicator> {
    match state {
        ConnectionState::Ready | ConnectionState::Initial => None,
        ConnectionState::WaitingForNetwork => Some(ConnectionIndicator::Offline),
        ConnectionState::ConnectingToProxy => {
            Some(ConnectionIndicator::Transitioning("Connecting to proxy…"))
        }
        ConnectionState::Connecting => Some(ConnectionIndicator::Transitioning("Connecting…")),
        ConnectionState::Updating => Some(ConnectionIndicator::Transitioning("Updating…")),
        ConnectionState::Unknown => Some(ConnectionIndicator::Transitioning("Connecting…")),
    }
}

impl Session {
    /// Slice parity:platform-offline-errors — whether the client is
    /// currently offline for send/call purposes. `Ready` and `Updating`
    /// are online (`Updating` is live sync; sends/calls still work —
    /// see #218 mapping it to Transitioning). Other connection states
    /// mean TDLib has no usable live connection.
    pub fn is_offline(&self) -> bool {
        !matches!(
            self.connection,
            ConnectionState::Ready | ConnectionState::Updating
        )
    }
}
