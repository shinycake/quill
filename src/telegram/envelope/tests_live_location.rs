use super::LiveLocationState;

fn live_state(expires_in: i32, received_at: i64) -> LiveLocationState {
    LiveLocationState {
        live_period: 900,
        expires_in,
        heading: 0,
        proximity_alert_radius: 0,
        received_at,
    }
}

#[test]
fn remaining_time_counts_down_from_receipt() {
    let live = live_state(600, 1_000);
    assert_eq!(live.remaining_at(1_000), 600);
    assert_eq!(live.remaining_at(1_090), 510);
    assert_eq!(live.remaining_at(1_600), 0);
    assert_eq!(live.remaining_at(9_000), 0);
    // A clock that stepped backwards never adds time.
    assert_eq!(live.remaining_at(500), 600);
    assert_eq!(live.status_label_at(1_090), "Live · expires in 8:30");
    assert_eq!(live.status_label_at(2_000), "Live location ended");
}

#[test]
fn only_the_sender_can_stop_a_running_live_location() {
    let live = live_state(600, 1_000);
    assert!(live.can_stop_at(true, 1_100));
    assert!(!live.can_stop_at(false, 1_100));
    assert!(!live.can_stop_at(true, 1_700));
    // Stopped by `editMessageLiveLocation`: TDLib reports expires_in 0.
    assert!(!live_state(0, 1_000).can_stop_at(true, 1_000));
}
