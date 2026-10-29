//! The call window's status lines (`call_panel.rs`).
use super::{call_status, duration_text};
use quill::calls::engine::TransportState;
use quill::telegram::envelope::CallState;

#[test]
fn statuses_follow_tdesktop() {
    let pending = |is_created, is_received| CallState::Pending {
        is_created,
        is_received,
    };
    assert_eq!(
        call_status(&pending(false, false), true, None, 0).0,
        "requesting…"
    );
    assert_eq!(
        call_status(&pending(true, false), true, None, 0).0,
        "waiting…"
    );
    assert_eq!(
        call_status(&pending(true, true), true, None, 0).0,
        "ringing…"
    );
    assert_eq!(
        call_status(&pending(true, true), false, None, 0).0,
        "is calling you…"
    );
    assert_eq!(
        call_status(&CallState::ExchangingKeys, true, None, 0).0,
        "exchanging encryption keys…"
    );
    let ready = |t| call_status(&CallState::Ready, true, t, 75);
    assert_eq!(
        ready(Some(TransportState::Connected)),
        ("1:15".to_string(), false)
    );
    assert_eq!(ready(None), ("connecting…".to_string(), true));
    assert_eq!(ready(Some(TransportState::Failed)).0, "failed to connect");
}

#[test]
fn durations_read_like_tdesktop() {
    assert_eq!(duration_text(0), "0:00");
    assert_eq!(duration_text(605), "10:05");
    assert_eq!(duration_text(3725), "1:02:05");
}
