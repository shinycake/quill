//! Driver tests: voice chat, video chat and live stream links
//! (`parity:deeplink-voice-chat`).
use super::*;
use crate::call_link::{CallLinkStage, CallLinkStep, LeaveKind};
use crate::ids::ChatId;

fn sent_types(recorder: &RecordingSender) -> Vec<String> {
    recorder
        .snapshot()
        .into_iter()
        .map(|json| {
            serde_json::from_str::<Value>(&json).unwrap()["@type"]
                .as_str()
                .unwrap_or("")
                .to_string()
        })
        .collect()
}

#[test]
fn the_link_offers_the_call_and_joins_nothing() {
    let (dir, mut driver, recorder, _handle, _sink, _seq) = ready_group_call_driver();
    let step = driver.offer_call_link(ChatId(51), "AbC", false).unwrap();
    assert_eq!(step, CallLinkStep::Offer { group_call_id: 555 });
    let fetch = sent_request(&recorder, "getGroupCall");
    assert_eq!(fetch["group_call_id"], 555);
    assert!(
        !sent_types(&recorder).iter().any(|t| t == "joinVideoChat"),
        "a link only offers the call"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_join_click_sends_the_links_hash() {
    let (dir, mut driver, recorder, _handle, _sink, _seq) = ready_group_call_driver();
    driver.offer_call_link(ChatId(51), "AbC", false).unwrap();
    driver.join_video_chat(555).expect("join the offered call");
    assert_eq!(
        sent_request(&recorder, "joinVideoChat")["invite_hash"],
        "AbC"
    );
    // Leaving drops the hash: the next join of the call carries none.
    driver.session.leave_group_call_local();
    assert_eq!(driver.session.calls.call_link_stage, CallLinkStage::Idle);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_plain_join_has_no_hash() {
    let (dir, mut driver, recorder, _handle, _sink, _seq) = ready_group_call_driver();
    driver.join_video_chat(555).expect("join tracked call");
    assert_eq!(sent_request(&recorder, "joinVideoChat")["invite_hash"], "");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_hash_for_another_call_is_not_sent() {
    let (dir, mut driver, recorder, _handle, _sink, _seq) = ready_group_call_driver();
    driver.session.calls.call_link_stage =
        CallLinkStage::Joinable(crate::call_link::PendingCallLink {
            group_call_id: 9,
            invite_hash: "other".into(),
        });
    driver.join_video_chat(555).expect("join tracked call");
    assert_eq!(sent_request(&recorder, "joinVideoChat")["invite_hash"], "");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_chat_without_a_call_is_a_bad_link_and_sends_nothing() {
    let (dir, mut driver, recorder, _handle, sink, seq) = ready_group_call_driver();
    ingest_call_json(
        &mut driver,
        &seq,
        &sink,
        r#"{"@type":"updateNewChat","chat":{"id":52,"title":"Quiet","type":{"@type":"chatTypeSupergroup","supergroup_id":52,"is_channel":true},"unread_count":0}}"#,
    );
    let before = recorder.snapshot().len();
    let step = driver.offer_call_link(ChatId(52), "x", false).unwrap();
    assert_eq!(step, CallLinkStep::NoCall);
    assert_eq!(driver.session.calls.call_link_stage, CallLinkStage::Idle);
    assert_eq!(recorder.snapshot().len(), before);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn another_joined_call_blocks_the_offer() {
    let (dir, mut driver, recorder, _handle, _sink, _seq) = ready_group_call_driver();
    let mut other = tracked_group_call(false, false, false);
    other.is_joined = true;
    driver.session.calls.active_group_call = Some(other);
    let before = recorder.snapshot().len();
    let step = driver.offer_call_link(ChatId(51), "AbC", false).unwrap();
    assert_eq!(step, CallLinkStep::OtherCallOn);
    assert_eq!(recorder.snapshot().len(), before);
    assert_eq!(driver.session.calls.call_link_stage.hash_for(555), "");
    let ask = driver
        .session
        .calls
        .call_link_stage
        .confirm()
        .cloned()
        .expect("asks");
    assert_eq!(ask.chat_id, 51);
    assert_eq!(
        ask.leave,
        LeaveKind::GroupCall {
            current_is_channel: false
        }
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn confirming_leaves_the_old_call_and_offers_the_new_one() {
    let (dir, mut driver, recorder, _handle, _sink, _seq) = ready_group_call_driver();
    let mut other = tracked_group_call(false, false, false);
    other.is_joined = true;
    driver.session.calls.active_group_call = Some(other);
    driver.offer_call_link(ChatId(51), "AbC", false).unwrap();
    assert!(!sent_types(&recorder).iter().any(|t| t == "leaveGroupCall"));
    driver.confirm_call_link_leave().unwrap();
    assert!(sent_types(&recorder).iter().any(|t| t == "leaveGroupCall"));
    assert_eq!(
        sent_request(&recorder, "getGroupCall")["group_call_id"],
        555
    );
    assert!(matches!(
        &driver.session.calls.call_link_stage,
        CallLinkStage::Offered { offered, .. } if *offered == crate::call_link::OfferedCallLink {
            group_call_id: 555,
            live_stream: false,
        }
    ));
    assert_eq!(driver.session.calls.call_link_stage.hash_for(555), "AbC");
    // The UI takes the note once; the hash stays for the join.
    assert!(
        driver
            .session
            .calls
            .call_link_stage
            .take_offered()
            .is_some()
    );
    assert!(
        driver
            .session
            .calls
            .call_link_stage
            .take_offered()
            .is_none()
    );
    assert_eq!(driver.session.calls.call_link_stage.hash_for(555), "AbC");
    assert!(
        !sent_types(&recorder).iter().any(|t| t == "joinVideoChat"),
        "the join screen still waits for its own click"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn declining_changes_nothing() {
    let (dir, mut driver, recorder, _handle, _sink, _seq) = ready_group_call_driver();
    let mut other = tracked_group_call(false, false, false);
    other.is_joined = true;
    driver.session.calls.active_group_call = Some(other);
    driver.offer_call_link(ChatId(51), "AbC", false).unwrap();
    let before = recorder.snapshot().len();
    driver.cancel_call_link_leave();
    assert_eq!(driver.session.calls.call_link_stage, CallLinkStage::Idle);
    assert_eq!(recorder.snapshot().len(), before);
    assert!(driver.session.calls.active_group_call.is_some());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_confirmed_stream_link_keeps_its_live_stream_into_the_offer() {
    let (dir, mut driver, _recorder, _handle, _sink, _seq) = ready_group_call_driver();
    let mut other = tracked_group_call(false, false, false);
    other.is_joined = true;
    driver.session.calls.active_group_call = Some(other);
    driver.offer_call_link(ChatId(51), "AbC", true).unwrap();
    driver.confirm_call_link_leave().unwrap();
    assert!(matches!(
        &driver.session.calls.call_link_stage,
        CallLinkStage::Offered { offered, .. } if offered.live_stream && offered.group_call_id == 555
    ));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn another_call_taking_the_place_drops_the_waiting_link() {
    let (dir, mut driver, recorder, _handle, _sink, _seq) = ready_group_call_driver();
    let mut other = tracked_group_call(false, false, false);
    other.is_joined = true;
    driver.session.calls.active_group_call = Some(other);
    driver.offer_call_link(ChatId(51), "AbC", false).unwrap();
    let confirm = driver
        .session
        .calls
        .call_link_stage
        .take_confirm()
        .expect("ask");
    // The call being left is call 999; a different one is joined instead.
    driver.session.calls.call_link_stage =
        CallLinkStage::Leaving(crate::call_link::CallLinkLeaving {
            link: confirm,
            leaving: crate::call_link::LeavingCall::Group(999),
        });
    let before = recorder.snapshot().len();
    driver.maybe_offer_pending_call_link().unwrap();
    assert_eq!(driver.session.calls.call_link_stage, CallLinkStage::Idle);
    assert_eq!(recorder.snapshot().len(), before, "nothing was offered");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_failed_fetch_of_the_offered_call_drops_its_hash() {
    let (dir, mut driver, recorder, _handle, sink, seq) = ready_group_call_driver();
    driver.offer_call_link(ChatId(51), "AbC", false).unwrap();
    assert_eq!(driver.session.calls.call_link_stage.hash_for(555), "AbC");
    let fetch = sent_request(&recorder, "getGroupCall");
    let json = serde_json::json!({
        "@type": "error",
        "@extra": fetch["@extra"],
        "code": 400,
        "message": "GROUPCALL_INVALID",
    })
    .to_string();
    driver
        .ingest(crate::telegram::client::copy_and_parse(&json, &seq, &sink).unwrap())
        .unwrap();
    assert_eq!(driver.session.calls.call_link_stage, CallLinkStage::Idle);
    let _ = std::fs::remove_dir_all(&dir);
}
