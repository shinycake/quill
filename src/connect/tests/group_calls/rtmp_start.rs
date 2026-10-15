//! Connect-driver tests: "Stream with..." for a chat with no video chat
//! (tdesktop `StartRtmpProcess`): the URL request, the answer feeding the
//! box, Revoke, Start Streaming as `createVideoChat` with `is_rtmp_stream`,
//! and the failure path.
use super::*;
use crate::calls::rtmp_start::RtmpInfo;
use crate::ids::ChatId;
use crate::telegram::envelope::ChannelMemberStatus;
use std::sync::atomic::AtomicU64;

static SEQ: AtomicU64 = AtomicU64::new(50_000);
const CHAT: i64 = 51;

fn driver_with_channel(
    manager: bool,
) -> (Arc<RecordingSender>, ConnectDriver<Arc<RecordingSender>>) {
    let (_dir, recorder, mut driver, seq) = group_call_test_driver();
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    ingest_call_json(
        &mut driver,
        &seq,
        &sink,
        r#"{"@type":"updateNewChat","chat":{"id":51,"title":"News","type":{"@type":"chatTypeSupergroup","supergroup_id":51,"is_channel":true},"unread_count":0}}"#,
    );
    if let Some(chat) = driver.session.chats.get_mut(&CHAT) {
        chat.set_member_status(
            if manager {
                ChannelMemberStatus::Creator
            } else {
                ChannelMemberStatus::Member
            },
            None,
        );
    }
    (recorder, driver)
}

fn last(recorder: &RecordingSender) -> Value {
    serde_json::from_str(&recorder.snapshot().pop().expect("a request")).unwrap()
}

fn reply(driver: &mut ConnectDriver<Arc<RecordingSender>>, json: Value) {
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    driver
        .ingest(copy_and_parse(&json.to_string(), &SEQ, &sink).unwrap())
        .unwrap();
}

fn url_answer(request: &Value, url: &str, key: &str) -> Value {
    serde_json::json!({
        "@type": "rtmpUrl",
        "@extra": request["@extra"],
        "url": url,
        "stream_key": key,
    })
}

#[test]
fn stream_with_asks_for_the_url_and_the_answer_opens_the_box() {
    let (recorder, mut driver) = driver_with_channel(true);
    driver.begin_rtmp_start(ChatId(CHAT)).expect("sends");
    let request = last(&recorder);
    assert_eq!(request["@type"], "getVideoChatRtmpUrl");
    assert_eq!(request["chat_id"], CHAT);
    let start = driver.session.calls.rtmp_start.as_ref().expect("process");
    assert!(!start.box_open(), "no box before the answer");

    reply(&mut driver, url_answer(&request, "rtmps://s/", "key-1"));
    let start = driver.session.calls.rtmp_start.as_ref().expect("process");
    assert_eq!(
        start.info,
        Some(RtmpInfo {
            url: "rtmps://s/".into(),
            key: "key-1".into()
        })
    );
    assert!(start.box_open());

    // A second start for the same chat sends nothing.
    let sent = recorder.snapshot().len();
    driver.begin_rtmp_start(ChatId(CHAT)).expect("no-op");
    assert_eq!(recorder.snapshot().len(), sent);
}

#[test]
fn revoke_sends_replace_and_swaps_the_key() {
    let (recorder, mut driver) = driver_with_channel(true);
    driver.begin_rtmp_start(ChatId(CHAT)).expect("sends");
    let request = last(&recorder);
    reply(&mut driver, url_answer(&request, "rtmps://s/", "old"));
    driver.revoke_rtmp_start_key().expect("sends");
    let request = last(&recorder);
    assert_eq!(request["@type"], "replaceVideoChatRtmpUrl");
    assert_eq!(request["chat_id"], CHAT);
    reply(&mut driver, url_answer(&request, "rtmps://s/", "new"));
    let key = driver
        .session
        .calls
        .rtmp_start
        .as_ref()
        .and_then(|start| start.info.as_ref())
        .map(|info| info.key.clone());
    assert_eq!(key.as_deref(), Some("new"));
}

#[test]
fn start_streaming_creates_an_rtmp_video_chat_and_ends_the_process() {
    let (recorder, mut driver) = driver_with_channel(true);
    driver.begin_rtmp_start(ChatId(CHAT)).expect("sends");
    let request = last(&recorder);
    reply(&mut driver, url_answer(&request, "rtmps://s/", "key"));
    driver.start_rtmp_stream().expect("sends");
    let create = last(&recorder);
    assert_eq!(create["@type"], "createVideoChat");
    assert_eq!(create["chat_id"], CHAT);
    assert_eq!(create["is_rtmp_stream"], true);
    assert_eq!(create["start_date"], 0);
    assert!(driver.session.calls.rtmp_start.is_none());
}

#[test]
fn closing_the_box_ends_the_process_without_a_request() {
    let (recorder, mut driver) = driver_with_channel(true);
    driver.begin_rtmp_start(ChatId(CHAT)).expect("sends");
    let sent = recorder.snapshot().len();
    driver.close_rtmp_start();
    assert!(driver.session.calls.rtmp_start.is_none());
    assert_eq!(recorder.snapshot().len(), sent);
    assert!(driver.start_rtmp_stream().is_err(), "nothing to start");
}

#[test]
fn only_a_manager_of_a_chat_without_a_call_can_start_the_process() {
    let (recorder, mut driver) = driver_with_channel(false);
    let sent = recorder.snapshot().len();
    assert!(driver.begin_rtmp_start(ChatId(CHAT)).is_err());
    assert_eq!(recorder.snapshot().len(), sent, "no request");

    let (_recorder, mut driver) = driver_with_channel(true);
    if let Some(chat) = driver.session.chats.get_mut(&CHAT) {
        chat.video_chat = Some(crate::state::VideoChatInfo {
            group_call_id: 9,
            has_participants: false,
            default_participant_id: None,
        });
    }
    assert!(driver.begin_rtmp_start(ChatId(CHAT)).is_err());
}

#[test]
fn a_failed_request_marks_the_process_and_leaves_the_call_error_alone() {
    let (recorder, mut driver) = driver_with_channel(true);
    driver.begin_rtmp_start(ChatId(CHAT)).expect("sends");
    let request = last(&recorder);
    reply(
        &mut driver,
        serde_json::json!({
            "@type": "error",
            "@extra": request["@extra"],
            "code": 500,
            "message": "INTERNAL",
        }),
    );
    let start = driver.session.calls.rtmp_start.as_ref().expect("process");
    assert!(start.failed && !start.box_open());
    assert!(driver.session.calls.group_call_error.is_none());
}

#[test]
fn administrator_rights_decide_who_manages_video_chats() {
    let (_recorder, mut driver) = driver_with_channel(false);
    let id = ChatId(CHAT);
    assert!(!driver.session.chat_can_manage_video_chats(id));
    if let Some(chat) = driver.session.chats.get_mut(&CHAT) {
        chat.set_member_status(ChannelMemberStatus::Administrator, None);
        chat.set_admin_can_manage_video_chats(Some(false));
    }
    assert!(!driver.session.chat_can_manage_video_chats(id));
    if let Some(chat) = driver.session.chats.get_mut(&CHAT) {
        chat.set_admin_can_manage_video_chats(Some(true));
    }
    assert!(driver.session.chat_can_manage_video_chats(id));
}
