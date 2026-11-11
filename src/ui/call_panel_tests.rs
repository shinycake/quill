//! The call window's status lines (`call_panel.rs`).
use super::{CallStatus, call_status, duration_text};
use quill::calls::engine::TransportState;
use quill::telegram::envelope::CallState;

#[test]
fn statuses_follow_tdesktop() {
    let pending = |is_created, is_received| CallState::Pending {
        is_created,
        is_received,
    };
    assert_eq!(
        call_status(&pending(false, false), true, None, 0).line(),
        "requesting…"
    );
    assert_eq!(
        call_status(&pending(true, false), true, None, 0).line(),
        "waiting…"
    );
    assert_eq!(
        call_status(&pending(true, true), true, None, 0).line(),
        "ringing…"
    );
    assert_eq!(
        call_status(&pending(true, true), false, None, 0).line(),
        "is calling you…"
    );
    assert_eq!(
        call_status(&CallState::ExchangingKeys, true, None, 0).line(),
        "exchanging encryption keys…"
    );
    let ready = |t| call_status(&CallState::Ready, true, t, 75);
    assert_eq!(
        ready(Some(TransportState::Connected)),
        CallStatus::Steady("1:15".to_string())
    );
    assert_eq!(
        ready(None),
        CallStatus::Connecting("connecting…".to_string())
    );
    assert_eq!(
        ready(Some(TransportState::Failed)).line(),
        "failed to connect"
    );
    assert!(!ready(Some(TransportState::Failed)).connecting());
}

#[test]
fn durations_read_like_tdesktop() {
    assert_eq!(duration_text(0), "0:00");
    assert_eq!(duration_text(605), "10:05");
    assert_eq!(duration_text(3725), "1:02:05");
}
