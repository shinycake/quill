//! Connect-driver tests: speaker and listener invite links, and the
//! video chat invite request.
use super::*;
use std::sync::atomic::AtomicU64;

/// One envelope sequence for every reply in these tests: the session
/// ignores a sequence number it has already seen.
static SEQ: AtomicU64 = AtomicU64::new(1_000);

fn video_chat(can_be_managed: bool, rtmp: bool) -> crate::state::ActiveGroupCall {
    let mut call = tracked_group_call(false, can_be_managed, false);
    call.is_video_chat = true;
    call.is_rtmp_stream = rtmp;
    call
}

fn sent(recorder: &RecordingSender) -> Vec<Value> {
    recorder
        .snapshot()
        .iter()
        .map(|json| serde_json::from_str(json).unwrap())
        .collect()
}

fn answer(driver: &mut ConnectDriver<Arc<RecordingSender>>, request: &Value, url: &str) {
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let json = serde_json::json!({
        "@type": "httpUrl",
        "@extra": request["@extra"],
        "url": url,
    })
    .to_string();
    driver
        .ingest(copy_and_parse(&json, &SEQ, &sink).unwrap())
        .unwrap();
}

fn fail(driver: &mut ConnectDriver<Arc<RecordingSender>>, request: &Value) {
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let json = serde_json::json!({
        "@type": "error",
        "@extra": request["@extra"],
        "code": 400,
        "message": "GROUPCALL_FORBIDDEN",
    })
    .to_string();
    driver
        .ingest(copy_and_parse(&json, &SEQ, &sink).unwrap())
        .unwrap();
}

#[test]
fn admin_gets_speaker_and_listener_links() {
    let (_dir, recorder, mut driver, _seq) = group_call_test_driver();
    driver.session.calls.active_group_call = Some(video_chat(true, false));
    let before = recorder.snapshot().len();
    assert_eq!(driver.request_video_chat_share_links(), Ok(false));
    let requests: Vec<Value> = sent(&recorder).split_off(before);
    assert_eq!(requests.len(), 2);
    assert!(
        requests
            .iter()
            .all(|r| r["@type"] == "getVideoChatInviteLink")
    );
    assert!(requests.iter().all(|r| r["group_call_id"] == 77));
    assert_eq!(requests[0]["can_self_unmute"], false);
    assert_eq!(requests[1]["can_self_unmute"], true);

    // A second click while the answers are out sends nothing.
    assert_eq!(driver.request_video_chat_share_links(), Ok(false));
    assert_eq!(recorder.snapshot().len(), before + 2);

    answer(
        &mut driver,
        &requests[1],
        "https://t.me/quill?videochat=speak",
    );
    answer(&mut driver, &requests[0], "https://t.me/quill?videochat");
    let links = &driver
        .session
        .calls
        .active_group_call
        .as_ref()
        .unwrap()
        .share_links;
    assert!(links.ready(), "{links:?}");
    assert_eq!(
        links.listener.as_deref(),
        Some("https://t.me/quill?videochat")
    );
    assert_eq!(
        links.speaker_link(),
        Some("https://t.me/quill?videochat=speak")
    );

    // Ready: the box opens without new requests.
    assert_eq!(driver.request_video_chat_share_links(), Ok(true));
    assert_eq!(recorder.snapshot().len(), before + 2);
}

#[test]
fn members_and_rtmp_streams_ask_only_for_the_listener_link() {
    for (can_manage, rtmp) in [(false, false), (true, true)] {
        let (_dir, recorder, mut driver, _seq) = group_call_test_driver();
        driver.session.calls.active_group_call = Some(video_chat(can_manage, rtmp));
        let before = recorder.snapshot().len();
        driver.request_video_chat_share_links().unwrap();
        let requests: Vec<Value> = sent(&recorder).split_off(before);
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0]["can_self_unmute"], false);
        answer(&mut driver, &requests[0], "https://t.me/quill?livestream");
        let links = &driver
            .session
            .calls
            .active_group_call
            .as_ref()
            .unwrap()
            .share_links;
        assert!(links.ready());
        assert_eq!(links.speaker_link(), None);
    }
}

#[test]
fn failed_speaker_link_still_opens_with_the_listener_link() {
    let (_dir, recorder, mut driver, _seq) = group_call_test_driver();
    driver.session.calls.active_group_call = Some(video_chat(true, false));
    let before = recorder.snapshot().len();
    driver.request_video_chat_share_links().unwrap();
    let requests: Vec<Value> = sent(&recorder).split_off(before);
    fail(&mut driver, &requests[1]);
    answer(&mut driver, &requests[0], "https://t.me/quill?videochat");
    let call = driver.session.calls.active_group_call.as_ref().unwrap();
    assert!(call.share_links.ready());
    assert_eq!(call.share_links.speaker_link(), None);
    assert!(driver.session.calls.group_call_error.is_none());
}

#[test]
fn failed_listener_link_is_reported_and_retried() {
    let (_dir, recorder, mut driver, _seq) = group_call_test_driver();
    driver.session.calls.active_group_call = Some(video_chat(true, false));
    let before = recorder.snapshot().len();
    driver.request_video_chat_share_links().unwrap();
    let requests: Vec<Value> = sent(&recorder).split_off(before);
    answer(
        &mut driver,
        &requests[1],
        "https://t.me/quill?videochat=speak",
    );
    fail(&mut driver, &requests[0]);
    assert!(driver.session.calls.group_call_error.is_some());
    let after = recorder.snapshot().len();
    driver.request_video_chat_share_links().unwrap();
    let retry: Vec<Value> = sent(&recorder).split_off(after);
    assert_eq!(retry.len(), 1);
    assert_eq!(retry[0]["can_self_unmute"], false);
}

#[test]
fn share_links_need_a_video_chat() {
    let (_dir, _recorder, mut driver, _seq) = group_call_test_driver();
    assert_eq!(
        driver.request_video_chat_share_links(),
        Err(ConnectSendError::InvalidRequest)
    );
    driver.session.calls.active_group_call = Some(tracked_group_call(false, true, true));
    assert_eq!(
        driver.request_video_chat_share_links(),
        Err(ConnectSendError::InvalidRequest)
    );
}

#[test]
fn revoking_drops_both_cached_links() {
    let (_dir, recorder, mut driver, _seq) = group_call_test_driver();
    let mut call = video_chat(true, false);
    call.share_links.listener = Some("L".into());
    call.share_links.speaker = Some("S".into());
    driver.session.calls.active_group_call = Some(call);
    driver.revoke_video_chat_invite_link().unwrap();
    let request: Value = sent(&recorder).pop().unwrap();
    assert_eq!(request["@type"], "revokeGroupCallInviteLink");
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let ok = serde_json::json!({"@type": "ok", "@extra": request["@extra"]}).to_string();
    driver
        .ingest(copy_and_parse(&ok, &SEQ, &sink).unwrap())
        .unwrap();
    let links = &driver
        .session
        .calls
        .active_group_call
        .as_ref()
        .unwrap()
        .share_links;
    assert_eq!(
        links,
        &crate::calls::share_links::VideoChatShareLinks::default()
    );
}

#[test]
fn video_chat_invites_use_invite_video_chat_participants() {
    let (_dir, recorder, mut driver, _seq) = group_call_test_driver();
    driver.session.calls.active_group_call = Some(video_chat(false, false));
    driver.invite_group_call_participant(42).unwrap();
    let request: Value = sent(&recorder).pop().unwrap();
    assert_eq!(request["@type"], "inviteVideoChatParticipants");
    assert_eq!(request["group_call_id"], 77);
    assert_eq!(request["user_ids"], serde_json::json!([42]));
}
