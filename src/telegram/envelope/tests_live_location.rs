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
    assert_eq!(live.status_label_at(1_090), "Live · expires in 9 min");
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

#[test]
fn countdown_label_has_minute_resolution_until_the_last_minute() {
    let live = live_state(7_300, 1_000);
    assert_eq!(live.status_label_at(1_000), "Live · expires in 2h 02m");
    let live = live_state(600, 1_000);
    assert_eq!(live.status_label_at(1_000), "Live · expires in 10 min");
    assert_eq!(live.status_label_at(1_059), "Live · expires in 10 min");
    assert_eq!(live.status_label_at(1_060), "Live · expires in 9 min");
    assert_eq!(live.status_label_at(1_540), "Live · expires in 1 min");
    assert_eq!(live.status_label_at(1_541), "Live · expires in 59 s");
    assert_eq!(live.status_label_at(1_599), "Live · expires in 1 s");
}

#[test]
fn refresh_waits_for_the_label_to_change_and_stops_at_the_end() {
    let live = live_state(600, 1_000);
    // 10 min until the first minute turns over.
    assert_eq!(live.refresh_in_at(1_000), Some(60));
    assert_eq!(live.refresh_in_at(1_001), Some(59));
    assert_eq!(live.refresh_in_at(1_059), Some(1));
    assert_eq!(live.refresh_in_at(1_060), Some(60));
    // The last minute counts seconds.
    assert_eq!(live.refresh_in_at(1_540), Some(1));
    assert_eq!(live.refresh_in_at(1_599), Some(1));
    assert_eq!(live.refresh_in_at(1_600), None);
    assert_eq!(live_state(0, 1_000).refresh_in_at(1_000), None);
}
