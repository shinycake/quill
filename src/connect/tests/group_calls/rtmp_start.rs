//! Connect-driver tests: "Stream with..." for a chat with no video chat
//! (tdesktop `StartRtmpProcess`): the URL request, the answer feeding the
//! box, Revoke, Start Streaming as `createVideoChat` with `is_rtmp_stream`,
//! the failure path, and who may manage video chats (`canManageGroupCall`)
//! as `updateSupergroup` reports it.
use super::*;
use crate::calls::rtmp_start::RtmpInfo;
use crate::ids::ChatId;
use std::sync::atomic::AtomicU64;

static SEQ: AtomicU64 = AtomicU64::new(50_000);
const CHAT: i64 = 51;

const CREATOR: &str =
    r#"{"@type":"chatMemberStatusCreator","is_anonymous":false,"is_member":true}"#;
const MEMBER: &str = r#"{"@type":"chatMemberStatusMember"}"#;

/// `updateSupergroup` for the channel with the viewer's own `status`.
fn supergroup_update(status: &str) -> String {
    format!(
        r#"{{"@type":"updateSupergroup","supergroup":{{"@type":"supergroup","id":51,"member_count":9,"is_channel":true,"status":{status}}}}}"#
    )
}

fn admin(can_manage_video_chats: bool) -> String {
    format!(
        r#"{{"@type":"chatMemberStatusAdministrator","can_be_edited":false,"rights":{{"@type":"chatAdministratorRights","can_manage_chat":true,"can_manage_video_chats":{can_manage_video_chats}}}}}"#
    )
}

fn ingest(driver: &mut ConnectDriver<Arc<RecordingSender>>, json: &str) {
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    driver
        .ingest(copy_and_parse(json, &SEQ, &sink).unwrap())
        .unwrap();
}

/// The channel as TDLib introduces it: `updateSupergroup` with the
/// viewer's status first, then `updateNewChat`.
fn driver_with_status(status: &str) -> (Arc<RecordingSender>, ConnectDriver<Arc<RecordingSender>>) {
    let (_dir, recorder, mut driver, _seq) = group_call_test_driver();
    ingest(&mut driver, &supergroup_update(status));
    ingest(
        &mut driver,
        r#"{"@type":"updateNewChat","chat":{"id":51,"title":"News","type":{"@type":"chatTypeSupergroup","supergroup_id":51,"is_channel":true},"unread_count":0}}"#,
    );
    (recorder, driver)
}

fn driver_with_channel(
    manager: bool,
) -> (Arc<RecordingSender>, ConnectDriver<Arc<RecordingSender>>) {
    driver_with_status(if manager { CREATOR } else { MEMBER })
}

fn last(recorder: &RecordingSender) -> Value {
    serde_json::from_str(&recorder.snapshot().pop().expect("a request")).unwrap()
}

fn reply(driver: &mut ConnectDriver<Arc<RecordingSender>>, json: Value) {
    ingest(driver, &json.to_string());
}

fn url_answer(request: &Value, url: &str, key: &str) -> Value {
    serde_json::json!({
        "@type": "rtmpUrl",
        "@extra": request["@extra"],
        "url": url,
        "stream_key": key,
    })
}

fn error_answer(request: &Value) -> Value {
    serde_json::json!({
        "@type": "error",
        "@extra": request["@extra"],
        "code": 500,
        "message": "INTERNAL",
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
    reply(&mut driver, error_answer(&request));
    let start = driver.session.calls.rtmp_start.as_ref().expect("process");
    assert!(start.failed && !start.box_open());
    assert!(driver.session.calls.group_call_error.is_none());
}

#[test]
fn a_late_failure_of_an_earlier_process_is_ignored() {
    let (recorder, mut driver) = driver_with_channel(true);
    driver.begin_rtmp_start(ChatId(CHAT)).expect("sends");
    let first = last(&recorder);
    driver.close_rtmp_start();
    driver.begin_rtmp_start(ChatId(CHAT)).expect("sends again");
    reply(&mut driver, error_answer(&first));
    let start = driver.session.calls.rtmp_start.as_ref().expect("process");
    assert!(!start.failed, "the new process did not fail");
}

#[test]
fn administrator_rights_decide_who_manages_video_chats() {
    let id = ChatId(CHAT);
    let (_recorder, driver) = driver_with_channel(false);
    assert!(!driver.session.chat_can_manage_video_chats(id), "member");
    let (_recorder, driver) = driver_with_channel(true);
    assert!(driver.session.chat_can_manage_video_chats(id), "creator");
    let (_recorder, driver) = driver_with_status(&admin(false));
    assert!(
        !driver.session.chat_can_manage_video_chats(id),
        "an administrator without the right"
    );
}

#[test]
fn a_channel_administrator_with_the_right_gets_it_from_update_supergroup() {
    // `updateSupergroup` precedes `updateNewChat`: the right is in place
    // when the chat appears, with no own-member probe.
    let (recorder, mut driver) = driver_with_status(&admin(true));
    let id = ChatId(CHAT);
    assert!(driver.session.chat_can_manage_video_chats(id));
    driver.begin_rtmp_start(id).expect("a manager may stream");
    assert_eq!(last(&recorder)["@type"], "getVideoChatRtmpUrl");
}

#[test]
fn promotion_and_demotion_update_the_right() {
    let id = ChatId(CHAT);
    let (_recorder, mut driver) = driver_with_channel(false);
    assert!(!driver.session.chat_can_manage_video_chats(id));
    ingest(&mut driver, &supergroup_update(&admin(true)));
    assert!(driver.session.chat_can_manage_video_chats(id), "promoted");
    ingest(&mut driver, &supergroup_update(&admin(false)));
    assert!(
        !driver.session.chat_can_manage_video_chats(id),
        "right taken"
    );
    ingest(&mut driver, &supergroup_update(&admin(true)));
    ingest(&mut driver, &supergroup_update(MEMBER));
    assert!(!driver.session.chat_can_manage_video_chats(id), "demoted");
}

#[test]
fn a_direct_messages_chat_never_manages_video_chats() {
    let (_recorder, mut driver) = driver_with_channel(true);
    ingest(
        &mut driver,
        r#"{"@type":"updateSupergroup","supergroup":{"@type":"supergroup","id":51,"member_count":9,"is_channel":false,"is_direct_messages_group":true,"is_administered_direct_messages_group":true,"status":{"@type":"chatMemberStatusCreator","is_anonymous":false,"is_member":true}}}"#,
    );
    assert!(!driver.session.chat_can_manage_video_chats(ChatId(CHAT)));
}

#[test]
fn creating_while_a_one_to_one_call_hangs_up_is_allowed() {
    let (recorder, mut driver) = driver_with_channel(true);
    driver.session.calls.active_call = Some(ActiveCall {
        id: 5,
        user_id: 11,
        is_outgoing: true,
        is_video: false,
        state: CallState::Ready,
        started_at: std::time::Instant::now(),
        ready_at: None,
        ready: None,
        transport: None,
        transport_error: None,
        signaling_queue: Vec::new(),
        signaling_dropped: 0,
        muted: false,
        camera_on: false,
        screen_sharing: false,
        remote_video: RemoteVideoState::Inactive,
        remote_screen: RemoteVideoState::Inactive,
        remote_audio_muted: false,
        remote_battery_low: false,
    });
    assert!(
        driver.start_video_chat(CHAT, String::new(), 0).is_err(),
        "not while the call is on"
    );
    if let Some(call) = driver.session.calls.active_call.as_mut() {
        call.state = CallState::HangingUp;
    }
    driver
        .start_video_chat(CHAT, String::new(), 0)
        .expect("the ending call is out of the way");
    assert_eq!(last(&recorder)["@type"], "createVideoChat");
}

#[test]
fn a_join_in_flight_counts_as_being_in_the_call_and_leaving_sends_leave() {
    use crate::calls::group_call_create::CurrentCall;
    let (recorder, mut driver) = driver_with_channel(true);
    driver.set_call_engine(Box::new(crate::calls::engine::MockEngine::new()));
    // You asked to join another chat's call (77); its answer has not come.
    driver.session.calls.active_group_call = Some(crate::state::ActiveGroupCall::fresh(77));
    assert_eq!(
        driver.session.current_call_for_create(None),
        CurrentCall::None,
        "only looked at"
    );
    driver.join_video_chat(77).expect("the join goes out");
    assert_eq!(last(&recorder)["@type"], "joinVideoChat");
    assert!(matches!(
        driver.session.current_call_for_create(None),
        CurrentCall::Group {
            scheduled: false,
            ..
        }
    ));
    // The UI's own join flag counts too.
    driver.session.requests.pending.clear();
    assert!(driver.session.group_call_joined_or_joining(Some(77)));
    assert!(!driver.session.group_call_joined_or_joining(None));
    // Leaving it before creating sends `leaveGroupCall` for that call.
    driver.leave_group_call().expect("leaves");
    let leave = last(&recorder);
    assert_eq!(leave["@type"], "leaveGroupCall");
    assert_eq!(leave["group_call_id"], 77);
}

#[test]
fn a_refused_create_is_kept_for_the_ui_by_request() {
    let (recorder, mut driver) = driver_with_channel(true);
    let sent = driver
        .start_video_chat(CHAT, String::new(), 0)
        .expect("sends");
    let request = last(&recorder);
    assert_eq!(request["@type"], "createVideoChat");
    reply(&mut driver, error_answer(&request));
    assert_eq!(driver.session.calls.failed_creates, vec![sent]);
    assert!(
        driver.session.calls.group_call_error.is_none(),
        "no error waits for the next call"
    );
}

#[test]
fn the_join_as_list_is_asked_for_before_creating() {
    let (recorder, mut driver) = driver_with_channel(true);
    let sent = driver.request_create_join_as(ChatId(CHAT)).expect("sends");
    let request = last(&recorder);
    assert_eq!(request["@type"], "getVideoChatAvailableParticipants");
    assert_eq!(request["chat_id"], CHAT);
    reply(
        &mut driver,
        serde_json::json!({
            "@type": "messageSenders",
            "@extra": request["@extra"],
            "total_count": 2,
            "senders": [
                {"@type": "messageSenderUser", "user_id": 1},
                {"@type": "messageSenderChat", "chat_id": CHAT},
            ],
        }),
    );
    let join_as = driver.session.calls.create_join_as.as_ref().expect("kept");
    assert_eq!(join_as.request, sent);
    assert_eq!(
        join_as.answer,
        Some(Ok(vec![
            MessageSender::User { user_id: 1 },
            MessageSender::Chat { chat_id: CHAT },
        ]))
    );
    // A failed list: the UI creates as yourself.
    driver.request_create_join_as(ChatId(CHAT)).expect("sends");
    let request = last(&recorder);
    reply(&mut driver, error_answer(&request));
    let join_as = driver.session.calls.create_join_as.as_ref().expect("kept");
    assert_eq!(join_as.answer, Some(Err(())));
}
