//! Driver tests: the rejoin after a network loss (`need_rejoin`) is one
//! explicit state (`GroupCallRejoin`). The banner stays for the whole
//! rejoin, an update that arrives while it is in flight does not start a
//! second one, and a failed rejoin is tried again up to the cap.
use super::*;
use crate::state::{ActiveGroupCall, GroupCallRejoin, MAX_GROUP_CALL_REJOIN_ATTEMPTS};

const CALL: &str = r#"{"@type":"updateGroupCall","group_call":{"@type":"groupCall","id":555,"unique_id":"999","title":"Design voice","invite_link":"","paid_message_star_count":0,"scheduled_start_date":0,"enabled_start_notification":false,"is_active":true,"is_video_chat":true,"is_live_story":false,"is_rtmp_stream":false,"is_joined":JOINED,"need_rejoin":NEED,"is_owned":false,"can_be_managed":true,"participant_count":1,"has_hidden_listeners":false,"loaded_all_participants":false,"message_sender_id":null,"recent_speakers":[],"is_my_video_enabled":false,"is_my_video_paused":false,"can_enable_video":true,"mute_new_participants":false,"can_toggle_mute_new_participants":true,"can_send_messages":true,"are_messages_allowed":true,"can_toggle_are_messages_allowed":false,"can_delete_messages":false,"record_duration":0,"is_video_recorded":false,"duration":0}}"#;

fn update(joined: bool, need_rejoin: bool) -> String {
    CALL.replace("JOINED", &joined.to_string())
        .replace("NEED", &need_rejoin.to_string())
}

fn joins(recorder: &RecordingSender) -> usize {
    recorder
        .snapshot()
        .iter()
        .filter(|json| json.contains(r#""@type":"joinVideoChat""#))
        .count()
}

fn tracked(driver: &ConnectDriver<Arc<RecordingSender>>) -> &ActiveGroupCall {
    driver
        .session
        .calls
        .active_group_call
        .as_ref()
        .expect("tracked call")
}

#[test]
fn the_banner_stays_from_the_drop_until_the_rejoined_update() {
    let (dir, mut driver, recorder, _handle, sink, seq) = ready_group_call_driver();
    ingest_call_json(&mut driver, &seq, &sink, &update(true, false));
    assert!(!tracked(&driver).reconnecting());

    // The drop: the ingest tick that applies it also starts the rejoin.
    ingest_call_json(&mut driver, &seq, &sink, &update(false, true));
    assert_eq!(joins(&recorder), 1);
    assert!(tracked(&driver).rejoin.in_flight().is_some());
    assert!(tracked(&driver).reconnecting(), "the join is on its way");

    // TDLib while the join is processed: neither joined nor dropped.
    ingest_call_json(&mut driver, &seq, &sink, &update(false, false));
    assert!(tracked(&driver).reconnecting());

    ingest_call_json(&mut driver, &seq, &sink, &update(true, false));
    assert_eq!(tracked(&driver).rejoin, GroupCallRejoin::Idle);
    assert!(!tracked(&driver).reconnecting());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_need_rejoin_update_after_the_join_went_out_sends_no_second_join() {
    let (dir, mut driver, recorder, handle, sink, seq) = ready_group_call_driver();
    ingest_call_json(&mut driver, &seq, &sink, &update(false, true));
    assert_eq!(joins(&recorder), 1);
    let request = tracked(&driver).rejoin.in_flight();
    assert!(request.is_some());
    let leaves = handle.group_leaves().len();

    // An update older than the join answer still says `need_rejoin`.
    ingest_call_json(&mut driver, &seq, &sink, &update(false, true));
    driver.maybe_auto_rejoin_group_call().expect("a no-op");
    assert_eq!(joins(&recorder), 1);
    assert_eq!(tracked(&driver).rejoin.in_flight(), request);
    assert_eq!(
        handle.group_leaves().len(),
        leaves,
        "the native context of the join in flight is kept"
    );
    assert!(matches!(
        tracked(&driver).rejoin,
        GroupCallRejoin::InFlight { attempt: 1, .. }
    ));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_failed_rejoin_is_due_again_until_the_cap() {
    let (dir, mut driver, recorder, _handle, sink, seq) = ready_group_call_driver();
    // Each failure makes the next attempt due, and the same ingest tick
    // sends it.
    ingest_call_json(&mut driver, &seq, &sink, &update(false, true));
    for attempt in 1..=MAX_GROUP_CALL_REJOIN_ATTEMPTS {
        assert_eq!(joins(&recorder), attempt);
        assert!(matches!(
            tracked(&driver).rejoin,
            GroupCallRejoin::InFlight { attempt: n, .. } if n == attempt
        ));
        let request = tracked(&driver).rejoin.in_flight().expect("in flight");
        ingest_call_json(
            &mut driver,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"error","code":500,"message":"BOOM","@extra":{}}}"#,
                request.0
            ),
        );
        assert!(tracked(&driver).reconnecting());
    }
    assert_eq!(tracked(&driver).rejoin, GroupCallRejoin::GaveUp);
    assert!(tracked(&driver).join_failed());
    assert_eq!(
        driver.session.calls.group_call_error.as_deref(),
        Some("Reconnect attempts exhausted.")
    );

    // Spent: neither a tick nor another `need_rejoin` sends more.
    ingest_call_json(&mut driver, &seq, &sink, &update(false, true));
    driver.maybe_auto_rejoin_group_call().expect("a no-op");
    assert_eq!(joins(&recorder), MAX_GROUP_CALL_REJOIN_ATTEMPTS);
    assert!(tracked(&driver).reconnecting(), "the banner is the way out");

    // The manual Rejoin starts the attempts over.
    driver.rejoin_group_call(true).expect("manual rejoin");
    assert_eq!(joins(&recorder), MAX_GROUP_CALL_REJOIN_ATTEMPTS + 1);
    assert!(matches!(
        tracked(&driver).rejoin,
        GroupCallRejoin::InFlight { attempt: 1, .. }
    ));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn only_the_failure_of_the_rejoin_in_flight_counts() {
    let mine = crate::ids::RequestId(7);
    let mut rejoin = GroupCallRejoin::Idle;
    rejoin.want();
    assert_eq!(rejoin.next_attempt(), Some(1));
    rejoin = GroupCallRejoin::InFlight {
        request: mine,
        attempt: 1,
    };
    rejoin.want();
    assert_eq!(rejoin.next_attempt(), None, "a drop does not re-arm it");
    assert!(!rejoin.attempt_failed(crate::ids::RequestId(8)));
    assert!(rejoin.attempt_failed(mine));
    assert_eq!(rejoin, GroupCallRejoin::Wanted { attempts: 1 });
    rejoin = GroupCallRejoin::InFlight {
        request: mine,
        attempt: 2,
    };
    rejoin.attempt_unsent(mine);
    assert_eq!(
        rejoin,
        GroupCallRejoin::Wanted { attempts: 1 },
        "unsent: not counted"
    );
    rejoin = GroupCallRejoin::InFlight {
        request: mine,
        attempt: MAX_GROUP_CALL_REJOIN_ATTEMPTS,
    };
    assert!(rejoin.attempt_failed(mine));
    assert_eq!(rejoin, GroupCallRejoin::GaveUp);
    rejoin.want();
    assert_eq!(rejoin, GroupCallRejoin::GaveUp);
}
