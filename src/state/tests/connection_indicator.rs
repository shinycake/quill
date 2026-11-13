//! Slice parity:platform-reconnect-states — state → indicator mapping
//! (extends the parity:platform-offline-indicator visibility mapping
//! with per-state reconnect labels).
use super::common::*;
use super::*;

/// Every non-Ready connection state renders the indicator; Ready renders
/// nothing. WaitingForNetwork is the offline case (labelled banner);
/// the transitional states carry their per-state reconnect label.
#[test]
fn connection_state_maps_to_indicator_visibility() {
    assert_eq!(connection_indicator(ConnectionState::Ready), None);
    assert_eq!(
        connection_indicator(ConnectionState::WaitingForNetwork),
        Some(ConnectionIndicator::Offline)
    );
    for (state, label) in [
        (ConnectionState::ConnectingToProxy, "Connecting to proxy…"),
        (ConnectionState::Connecting, "Connecting…"),
        (ConnectionState::Updating, "Updating…"),
        // Unknown is not a TDLib state with its own label; it falls back
        // to the generic transitional label.
        (ConnectionState::Unknown, "Connecting…"),
    ] {
        assert_eq!(
            connection_indicator(state),
            Some(ConnectionIndicator::Transitioning(label)),
            "{state:?} must render the indicator with its label"
        );
    }
}

/// The indicator's label is what the connection strip renders.
#[test]
fn indicator_label_matches_state() {
    assert_eq!(ConnectionIndicator::Offline.label(), "Waiting for network…");
    assert_eq!(
        ConnectionIndicator::Transitioning("Updating…").label(),
        "Updating…"
    );
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
        Some(ConnectionIndicator::Transitioning("Updating…"))
    );
}

/// A fresh session has not heard from TDLib yet: no offline banner flash
/// at startup. The first `updateConnectionState` takes over.
#[test]
fn fresh_session_shows_no_indicator_until_first_update() {
    let (mut session, sink) = session();
    assert_eq!(session.connection, ConnectionState::Initial);
    assert_eq!(connection_indicator(session.connection), None);
    let seq = AtomicU64::new(0);
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
}

/// Translations that ran out of attempts offline are asked for again once
/// the connection is back (the chat stays open the whole time).
#[test]
fn reconnecting_gives_failed_translations_their_attempts_back() {
    use crate::state::{TRANSLATE_PUMP_ATTEMPTS, TranslateTarget, Translation};
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let (chat, message) = (ChatId(5), MessageId(9));
    for _ in 0..TRANSLATE_PUMP_ATTEMPTS {
        let job = session.begin_translation(
            TranslateTarget::Message {
                chat_id: chat.0,
                message_id: message.0,
            },
            "en",
            Some(chat),
        );
        session.finish_translation(job, Translation::failed("offline"));
    }
    assert!(!session.needs_translation(chat, message, "en"), "gave up");
    assert!(session.translation_unsettled(chat, message, "en"));
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateConnectionState","state":{"@type":"connectionStateWaitingForNetwork"}}"#,
    );
    assert!(!session.needs_translation(chat, message, "en"), "still off");
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateConnectionState","state":{"@type":"connectionStateReady"}}"#,
    );
    assert!(
        session.needs_translation(chat, message, "en"),
        "back online"
    );
}
