//! Driver tests: voice chat, video chat and live stream links
//! (`parity:deeplink-voice-chat`).
use super::*;
use crate::call_link::CallLinkStep;
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
    let step = driver.offer_call_link(ChatId(51), "AbC").unwrap();
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
    driver.offer_call_link(ChatId(51), "AbC").unwrap();
    driver.join_video_chat(555).expect("join the offered call");
    assert_eq!(
        sent_request(&recorder, "joinVideoChat")["invite_hash"],
        "AbC"
    );
    // Leaving drops the hash: the next join of the call carries none.
    driver.session.leave_group_call_local();
    assert_eq!(driver.session.calls.call_link, None);
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
    driver.session.calls.call_link = Some(crate::call_link::PendingCallLink {
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
    let step = driver.offer_call_link(ChatId(52), "x").unwrap();
    assert_eq!(step, CallLinkStep::NoCall);
    assert_eq!(driver.session.calls.call_link, None);
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
    let step = driver.offer_call_link(ChatId(51), "AbC").unwrap();
    assert_eq!(step, CallLinkStep::Busy);
    assert_eq!(recorder.snapshot().len(), before);
    assert_eq!(driver.session.calls.call_link, None);
    let _ = std::fs::remove_dir_all(&dir);
}
