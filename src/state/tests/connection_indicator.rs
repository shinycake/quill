//! Slice parity:platform-offline-indicator — state → indicator mapping.
use super::common::*;
use super::*;

/// Every non-Ready connection state renders the indicator; Ready renders
/// nothing. WaitingForNetwork is the offline case (labelled banner);
/// the transitional states render presence only (per-state labels are
/// the `platform-reconnect-states` slice).
#[test]
fn connection_state_maps_to_indicator_visibility() {
    assert_eq!(connection_indicator(ConnectionState::Ready), None);
    assert_eq!(
        connection_indicator(ConnectionState::WaitingForNetwork),
        Some(ConnectionIndicator::Offline)
    );
    for state in [
        ConnectionState::ConnectingToProxy,
        ConnectionState::Connecting,
        ConnectionState::Updating,
        ConnectionState::Unknown,
    ] {
        assert_eq!(
            connection_indicator(state),
            Some(ConnectionIndicator::Transitioning),
            "{state:?} must render the indicator"
        );
    }
}

/// `updateConnectionState` flows from TDLib through the reducer into the
/// indicator mapping — the UI re-reads `Session::connection` every frame
/// (the 40ms `poll_live` loop), so this is the live-update path.
#[test]
fn update_connection_state_drives_indicator() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateConnectionState","state":{"@type":"connectionStateReady"}}"#,
    );
    assert_eq!(connection_indicator(session.connection), None);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateConnectionState","state":{"@type":"connectionStateWaitingForNetwork"}}"#,
    );
    assert_eq!(
        connection_indicator(session.connection),
        Some(ConnectionIndicator::Offline)
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateConnectionState","state":{"@type":"connectionStateUpdating"}}"#,
    );
    assert_eq!(
        connection_indicator(session.connection),
        Some(ConnectionIndicator::Transitioning)
    );
}
