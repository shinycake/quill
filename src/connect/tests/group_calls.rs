//! Connect-driver tests: group calls and video chats.
use super::super::*;
use super::*;
use crate::calls::engine::CallEngine;
use crate::calls::engine::{
    MediaDevice, MediaDeviceKind, MockEngine, RemoteVideoState, VideoFrame,
};
use crate::diagnostics::DiagnosticSink;
use crate::diagnostics::MemorySink;
use crate::state::ActiveCall;
use crate::telegram::client::copy_and_parse;
use crate::telegram::envelope::{CallState, MessageSender};
use serde_json::Value;
use std::sync::Arc;

/// Phase C2f: participant-management request shapes and gates
/// (schema 1.8.67 — inviteGroupCallParticipant :14375,
/// banGroupCallParticipants :14385,
/// setGroupCallParticipantVolumeLevel :14438,
/// declineGroupCallInvitation :14380, joinGroupCall :14285).
#[test]
fn driver_group_call_participant_management_shapes_and_gates() {
    let (dir, recorder, mut driver, _seq) = group_call_test_driver();
    let last_sent = || serde_json::from_str::<Value>(recorder.snapshot().last().unwrap()).unwrap();

    // Gates with no active group call.
    assert_eq!(
        driver.invite_group_call_participant(7),
        Err(ConnectSendError::InvalidRequest)
    );
    assert_eq!(
        driver.ban_group_call_participant(7),
        Err(ConnectSendError::InvalidRequest)
    );
    assert_eq!(
        driver.set_group_call_participant_volume(MessageSender::User { user_id: 7 }, 10000),
        Err(ConnectSendError::InvalidRequest)
    );
    // Decline needs no active call.
    driver
        .decline_group_call_invitation(3, 42)
        .expect("decline sends without an active call");
    let sent = last_sent();
    assert_eq!(sent["@type"], "declineGroupCallInvitation");
    assert_eq!(sent["chat_id"], 3);
    assert_eq!(sent["message_id"], 42);

    driver.session.active_group_call = Some(tracked_group_call(false, true, false));

    // Invite: shape follows schema 1.8.67 :14375; `is_video`
    // follows the tracked call.
    driver
        .invite_group_call_participant(7)
        .expect("invite sends");
    let sent = last_sent();
    assert_eq!(sent["@type"], "inviteGroupCallParticipant");
    assert_eq!(sent["group_call_id"], 77);
    assert_eq!(sent["user_id"], 7);
    assert_eq!(sent["is_video"], false);

    // Ban: schema :14385 takes `user_ids:vector<int64>` (the
    // plural constructor), owner-gated on `groupCall.is_owned` —
    // `can_be_managed` is "for video chats and live stories only"
    // and does NOT grant ban rights in a voice chat.
    driver.session.active_group_call = Some(tracked_group_call(false, true, false));
    assert_eq!(
        driver.ban_group_call_participant(9),
        Err(ConnectSendError::InvalidRequest),
        "can_be_managed=true but is_owned=false must refuse"
    );
    driver.session.active_group_call = Some(tracked_group_call(false, false, true));
    driver
        .ban_group_call_participant(9)
        .expect("ban sends for owner");
    let sent = last_sent();
    assert_eq!(sent["@type"], "banGroupCallParticipants");
    assert_eq!(sent["group_call_id"], 77);
    assert_eq!(sent["user_ids"], serde_json::json!([9]));
    driver.session.active_group_call = Some(tracked_group_call(false, true, true));

    // Volume: schema :14438, 1-20000 (hundreds of percents).
    driver
        .set_group_call_participant_volume(MessageSender::User { user_id: 7 }, 5000)
        .expect("volume sends");
    let sent = last_sent();
    assert_eq!(sent["@type"], "setGroupCallParticipantVolumeLevel");
    assert_eq!(sent["group_call_id"], 77);
    assert_eq!(sent["volume_level"], 5000);
    assert_eq!(sent["participant_id"]["@type"], "messageSenderUser");
    assert_eq!(sent["participant_id"]["user_id"], 7);
    // Out-of-range levels clamp to the schema's 1-20000.
    driver
        .set_group_call_participant_volume(MessageSender::User { user_id: 7 }, 0)
        .expect("volume 0 clamps");
    assert_eq!(last_sent()["volume_level"], 1);
    driver
        .set_group_call_participant_volume(MessageSender::User { user_id: 7 }, 30_000)
        .expect("volume 30000 clamps");
    assert_eq!(last_sent()["volume_level"], 20000);

    // Accept: `joinGroupCall` with `inputGroupCallMessage`
    // (schema line 5288: accept via joinGroupCall). Refuses while
    // a 1:1 call is active.
    driver
        .accept_group_call_invitation(3, 42)
        .expect("accept sends without an active call");
    let sent = last_sent();
    assert_eq!(sent["@type"], "joinGroupCall");
    assert_eq!(sent["input_group_call"]["@type"], "inputGroupCallMessage");
    assert_eq!(sent["input_group_call"]["chat_id"], 3);
    assert_eq!(sent["input_group_call"]["message_id"], 42);
    driver.session.active_call = Some(ActiveCall {
        id: 5,
        user_id: 11,
        is_outgoing: true,
        is_video: false,
        state: CallState::Pending {
            is_created: true,
            is_received: false,
        },
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
    });
    assert_eq!(
        driver.accept_group_call_invitation(3, 42),
        Err(ConnectSendError::InvalidRequest)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// Phase C2h: management drivers — recording, invite revocation,
/// RTMP, in-call chat, and scheduled starts. Shapes follow the
/// pinned TDLib 1.8.67 schema; gates follow the tracked `groupCall`
/// flags.
#[test]
fn driver_group_call_management_shapes_and_gates() {
    let (dir, recorder, mut driver, seq) = group_call_test_driver();
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let last_sent = || serde_json::from_str::<Value>(recorder.snapshot().last().unwrap()).unwrap();

    driver.session.active_group_call = Some(tracked_group_call(false, true, false));
    driver
        .session
        .active_group_call
        .as_mut()
        .unwrap()
        .is_video_chat = true;

    // Recording: `startGroupCallRecording` / `endGroupCallRecording`
    // (schema :14405 / :14408), gated on `groupCall.can_be_managed`
    // for video chats.
    driver
        .start_group_call_recording("Team voice".to_string(), true)
        .expect("recording starts");
    let sent = last_sent();
    assert_eq!(sent["@type"], "startGroupCallRecording");
    assert_eq!(sent["group_call_id"], 77);
    assert_eq!(sent["title"], "Team voice");
    assert_eq!(sent["record_video"], true);
    driver.stop_group_call_recording().expect("recording stops");
    assert_eq!(last_sent()["@type"], "endGroupCallRecording");
    assert_eq!(last_sent()["group_call_id"], 77);
    driver
        .session
        .active_group_call
        .as_mut()
        .unwrap()
        .can_be_managed = false;
    assert_eq!(
        driver.start_group_call_recording("Team voice".to_string(), true),
        Err(ConnectSendError::InvalidRequest),
        "non-manageable call must refuse recording"
    );
    driver
        .session
        .active_group_call
        .as_mut()
        .unwrap()
        .can_be_managed = true;

    // Start now: `startScheduledVideoChat` (:14277), gated on
    // `can_be_managed` for a still-scheduled call.
    driver
        .session
        .active_group_call
        .as_mut()
        .unwrap()
        .scheduled_start_date = 1_788_000_000;
    driver
        .start_scheduled_video_chat()
        .expect("start-now sends");
    assert_eq!(last_sent()["@type"], "startScheduledVideoChat");
    assert_eq!(last_sent()["group_call_id"], 77);
    driver
        .session
        .active_group_call
        .as_mut()
        .unwrap()
        .can_be_managed = false;
    assert_eq!(
        driver.start_scheduled_video_chat(),
        Err(ConnectSendError::InvalidRequest),
        "non-manageable call must refuse start-now"
    );
    driver
        .session
        .active_group_call
        .as_mut()
        .unwrap()
        .can_be_managed = true;
    driver
        .session
        .active_group_call
        .as_mut()
        .unwrap()
        .scheduled_start_date = 0;
    assert_eq!(
        driver.start_scheduled_video_chat(),
        Err(ConnectSendError::InvalidRequest),
        "already-active call must refuse start-now"
    );

    // Start-notification toggle:
    // `toggleVideoChatEnabledStartNotification` (:14282), gated on
    // a still-scheduled call; the driver flips the tracked
    // `enabled_start_notification` flag.
    driver
        .session
        .active_group_call
        .as_mut()
        .unwrap()
        .scheduled_start_date = 1_788_000_000;
    driver
        .session
        .active_group_call
        .as_mut()
        .unwrap()
        .enabled_start_notification = false;
    driver
        .toggle_video_chat_start_notification()
        .expect("notify-me sends");
    assert_eq!(
        last_sent()["@type"],
        "toggleVideoChatEnabledStartNotification"
    );
    assert_eq!(last_sent()["group_call_id"], 77);
    assert_eq!(last_sent()["enabled_start_notification"], true);
    // Same shape with the flag on: the driver turns it off.
    driver
        .session
        .active_group_call
        .as_mut()
        .unwrap()
        .enabled_start_notification = true;
    driver
        .toggle_video_chat_start_notification()
        .expect("un-notify-me sends");
    assert_eq!(last_sent()["enabled_start_notification"], false);
    driver
        .session
        .active_group_call
        .as_mut()
        .unwrap()
        .scheduled_start_date = 0;
    assert_eq!(
        driver.toggle_video_chat_start_notification(),
        Err(ConnectSendError::InvalidRequest),
        "already-active call must refuse the notify toggle"
    );

    // Invite revocation: `revokeGroupCallInviteLink` (:14398),
    // gated on `can_be_managed` for video chats.
    driver
        .revoke_video_chat_invite_link()
        .expect("revoke sends");
    assert_eq!(last_sent()["@type"], "revokeGroupCallInviteLink");
    assert_eq!(last_sent()["group_call_id"], 77);

    // In-call chat: `sendGroupCallMessage` (:14341), gated on
    // `can_send_messages && are_messages_allowed`.
    {
        let call = driver.session.active_group_call.as_mut().unwrap();
        call.can_send_messages = true;
        call.are_messages_allowed = true;
    }
    driver
        .send_group_call_message("hello".to_string())
        .expect("message sends");
    let sent = last_sent();
    assert_eq!(sent["@type"], "sendGroupCallMessage");
    assert_eq!(sent["group_call_id"], 77);
    assert_eq!(sent["text"]["text"], "hello");
    assert_eq!(sent["paid_message_star_count"], 0);
    driver
        .session
        .active_group_call
        .as_mut()
        .unwrap()
        .are_messages_allowed = false;
    assert_eq!(
        driver.send_group_call_message("hello".to_string()),
        Err(ConnectSendError::InvalidRequest),
        "disabled chat must refuse send"
    );
    driver
        .session
        .active_group_call
        .as_mut()
        .unwrap()
        .are_messages_allowed = true;
    // Chat toggle: `toggleGroupCallAreMessagesAllowed` (:14322),
    // gated on `can_toggle_are_messages_allowed`; flips the flag.
    driver
        .session
        .active_group_call
        .as_mut()
        .unwrap()
        .can_toggle_are_messages_allowed = true;
    driver
        .toggle_group_call_are_messages_allowed()
        .expect("toggle sends");
    assert_eq!(last_sent()["@type"], "toggleGroupCallAreMessagesAllowed");
    assert_eq!(last_sent()["are_messages_allowed"], false);

    // RTMP: `getVideoChatRtmpUrl` / `replaceVideoChatRtmpUrl`
    // (:14261 / :14264) resolve the chat from the tracked call.
    ingest_call_json(
        &mut driver,
        &seq,
        &dyn_sink,
        r#"{"@type":"updateNewChat","chat":{"id":51,"title":"Design voice","type":{"@type":"chatTypeSupergroup","supergroup_id":51,"is_channel":false},"unread_count":0,"video_chat":{"@type":"videoChat","group_call_id":77,"has_participants":true}}}"#,
    );
    driver.fetch_video_chat_rtmp_url().expect("rtmp url sends");
    let sent = last_sent();
    assert_eq!(sent["@type"], "getVideoChatRtmpUrl");
    assert_eq!(sent["chat_id"], 51);
    // Regenerate is owner-gated (`replaceVideoChatRtmpUrl` mints a
    // new stream key).
    driver.session.active_group_call.as_mut().unwrap().is_owned = true;
    driver
        .replace_video_chat_rtmp_url()
        .expect("rtmp replace sends");
    let sent = last_sent();
    assert_eq!(sent["@type"], "replaceVideoChatRtmpUrl");
    assert_eq!(sent["chat_id"], 51);

    // Scheduling: `createVideoChat` start_date (schema :14256) —
    // 0 starts immediately; scheduled dates must be ≥10s and ≤8d
    // ahead. Requires no tracked call (a start/join target).
    driver.session.active_group_call = None;
    driver
        .start_video_chat(51, "Planning".to_string(), 0)
        .expect("immediate start sends");
    let sent = last_sent();
    assert_eq!(sent["@type"], "createVideoChat");
    assert_eq!(sent["start_date"], 0);
    assert_eq!(sent["is_rtmp_stream"], false);
    let future = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
        + 3600;
    driver
        .start_video_chat(51, "Planning".to_string(), future)
        .expect("scheduled start sends");
    assert_eq!(last_sent()["start_date"], future);
    assert!(driver.start_video_chat(51, "x".to_string(), 1).is_err());
    // Empty title is valid per schema :14256 ("if empty, chat title
    // will be used") — it sends through.
    driver
        .start_video_chat(51, String::new(), 0)
        .expect("empty title falls back to chat title");
    assert_eq!(last_sent()["title"], "");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Phase C2f: `joinGroupCall` success (`groupCallInfo`, schema
/// 1.8.67 :7190) triggers a `getGroupCall` fetch so the accepted
/// call gets tracked; the join payload is stored like the
/// `joinVideoChat` Text arm.
#[test]
fn driver_group_call_invitation_accept_starts_tracking() {
    let (dir, recorder, mut driver, seq) = group_call_test_driver();
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    driver
        .accept_group_call_invitation(3, 42)
        .expect("accept sends");
    let sent: Value = serde_json::from_str(recorder.snapshot().last().unwrap()).unwrap();
    let extra = sent["@extra"].as_str().unwrap().to_string();
    driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"groupCallInfo","group_call_id":555,"join_payload":"payload-1","@extra":"{extra}"}}"#
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    // The fetch queue is drained by `maybe_fetch_group_calls`
    // during ingest — assert the `getGroupCall` went out.
    let sent: Value = serde_json::from_str(recorder.snapshot().last().unwrap()).unwrap();
    assert_eq!(sent["@type"], "getGroupCall");
    assert_eq!(sent["group_call_id"], 555);
    assert_eq!(driver.session.group_call_error, None);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Phase C2f: auto-rejoin discipline — a `need_rejoin` group call
/// rejoins automatically (max 3 attempts, retaining the join
/// parameters and self-mute); a clean joined `updateGroupCall`
/// resets the counter; manual retry resets it too.
#[test]
fn driver_auto_rejoin_group_call_discipline() {
    let (dir, recorder, mut driver, seq) = group_call_test_driver();
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let join_sends = || {
        recorder
            .snapshot()
            .into_iter()
            .filter(|json| json.contains("\"joinVideoChat\""))
            .count()
    };
    let fail_last_join = |driver: &mut ConnectDriver<Arc<RecordingSender>>| {
        let sent: Value = serde_json::from_str(recorder.snapshot().last().unwrap()).unwrap();
        let extra = sent["@extra"].as_str().unwrap().to_string();
        driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"error","code":500,"message":"BOOM","@extra":"{extra}"}}"#
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    };

    // No tracked call: nothing to rejoin.
    assert!(driver.maybe_auto_rejoin_group_call().is_ok());
    assert_eq!(join_sends(), 0);

    driver.session.active_group_call = Some(tracked_group_call(true, false, false));
    driver
        .session
        .active_group_call
        .as_mut()
        .unwrap()
        .reconnecting = true;

    // Three attempts, each followed by a TDLib join failure that
    // re-arms the reconnecting banner (the C2d discipline).
    for attempt in 1..=3 {
        driver.maybe_auto_rejoin_group_call().expect("rejoin sends");
        assert_eq!(join_sends(), attempt);
        let sent: Value = serde_json::from_str(recorder.snapshot().last().unwrap()).unwrap();
        assert_eq!(sent["@type"], "joinVideoChat");
        assert_eq!(sent["group_call_id"], 77);
        // Honest no-device params, retaining the self-mute.
        assert_eq!(sent["join_parameters"]["audio_source_id"], 0);
        assert_eq!(sent["join_parameters"]["is_muted"], true);
        assert_eq!(
            driver
                .session
                .active_group_call
                .as_ref()
                .unwrap()
                .rejoin_attempts,
            attempt
        );
        fail_last_join(&mut driver);
    }
    // Exhausted: the error line is honest and no more attempts go
    // out — the banner + manual Rejoin remain the way out.
    assert_eq!(
        driver.session.group_call_error.as_deref(),
        Some("Reconnect attempts exhausted.")
    );
    assert!(driver.maybe_auto_rejoin_group_call().is_ok());
    assert_eq!(join_sends(), 3);

    // A clean joined `updateGroupCall` resets the counter, clears
    // the banner, and drops the stale error line.
    driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateGroupCall","group_call":{"@type":"groupCall","id":77,"title":"Team voice","is_active":true,"is_video_chat":false,"is_joined":true,"need_rejoin":false,"can_be_managed":false,"is_owned":false,"participant_count":1,"loaded_all_participants":false,"recent_speakers":[],"is_my_video_enabled":false,"is_my_video_paused":false,"can_enable_video":false,"mute_new_participants":false,"can_toggle_mute_new_participants":false,"can_change_title":false,"can_change_desc":false,"can_change_emoji":false,"scheduled_start_date":0,"title":"","description":"","emoji":""}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    let call = driver.session.active_group_call.as_ref().unwrap();
    assert_eq!(call.rejoin_attempts, 0);
    assert!(!call.reconnecting);
    assert_eq!(driver.session.group_call_error, None);

    // Manual retry resets the counter: with `need_rejoin` back,
    // three fresh auto attempts are allowed after
    // `rejoin_group_call(true)`.
    call_state_for_rejoin(&mut driver);
    driver.rejoin_group_call(true).expect("manual retry sends");
    assert_eq!(
        driver
            .session
            .active_group_call
            .as_ref()
            .unwrap()
            .rejoin_attempts,
        1
    );
    let _ = std::fs::remove_dir_all(&dir);
}

fn ready_group_call_driver() -> GroupCallDriverHarness {
    let (dir, mut driver, recorder, sink, seq) = call_driver();
    let mock = MockEngine::new();
    let handle = mock.clone();
    driver.set_call_engine(Box::new(mock));
    ingest_call_json(
        &mut driver,
        &seq,
        &sink,
        r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
    );
    ingest_call_json(
        &mut driver,
        &seq,
        &sink,
        r#"{"@type":"updateNewChat","chat":{"id":51,"title":"Design voice","type":{"@type":"chatTypeSupergroup","supergroup_id":51,"is_channel":false},"unread_count":0}}"#,
    );
    ingest_call_json(
        &mut driver,
        &seq,
        &sink,
        r#"{"@type":"updateChatVideoChat","chat_id":51,"video_chat":{"@type":"videoChat","group_call_id":555,"has_participants":true,"default_participant_id":null}}"#,
    );
    ingest_call_json(
        &mut driver,
        &seq,
        &sink,
        r#"{"@type":"updateGroupCall","group_call":{"@type":"groupCall","id":555,"unique_id":"999","title":"Design voice","invite_link":"","paid_message_star_count":0,"scheduled_start_date":0,"enabled_start_notification":false,"is_active":true,"is_video_chat":true,"is_live_story":false,"is_rtmp_stream":false,"is_joined":false,"need_rejoin":false,"is_owned":false,"can_be_managed":true,"participant_count":0,"has_hidden_listeners":false,"loaded_all_participants":false,"message_sender_id":null,"recent_speakers":[],"is_my_video_enabled":false,"is_my_video_paused":false,"can_enable_video":true,"mute_new_participants":false,"can_toggle_mute_new_participants":true,"can_send_messages":true,"are_messages_allowed":true,"can_toggle_are_messages_allowed":false,"can_delete_messages":false,"record_duration":0,"is_video_recorded":false,"duration":0}}"#,
    );
    (dir, driver, recorder, handle, sink, seq)
}

/// Phase C2g: the join carries the native offer (not the no-device
/// fallback) and the audio SSRC parsed from it.
#[test]
fn group_join_carries_engine_offer() {
    let (dir, mut driver, recorder, handle, _sink, _seq) = ready_group_call_driver();
    driver.join_video_chat(555).expect("join tracked call");
    assert_eq!(handle.group_offers(), vec![(555, 51)]);
    let join = sent_request(&recorder, "joinVideoChat");
    assert_eq!(join["join_parameters"]["payload"], "mock-group-offer-555");
    // The mock offer carries no `a=ssrc:` lines: honest 0.
    assert_eq!(join["join_parameters"]["audio_source_id"], 0);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Phase C2g: the `joinVideoChat` Text answer finishes the native
/// handshake exactly once.
#[test]
fn group_join_answer_connects_native_transport() {
    let (dir, mut driver, _recorder, handle, sink, seq) = ready_group_call_driver();
    let extra = driver.join_video_chat(555).expect("join tracked call");
    ingest_call_json(
        &mut driver,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"text","text":"join-answer","@extra":{}}}"#,
            extra.0
        ),
    );
    assert_eq!(
        handle.group_connects(),
        vec![(555, "join-answer".to_string(), false)]
    );
    assert!(
        driver
            .session
            .active_group_call
            .as_ref()
            .is_some_and(|call| call.transport_ready)
    );
    // A second pump (no new answer) must not reconnect.
    ingest_call_json(
        &mut driver,
        &seq,
        &sink,
        r#"{"@type":"updateGroupCall","group_call":{"@type":"groupCall","id":555,"unique_id":"999","title":"Design voice","invite_link":"","paid_message_star_count":0,"scheduled_start_date":0,"enabled_start_notification":false,"is_active":true,"is_video_chat":true,"is_live_story":false,"is_rtmp_stream":false,"is_joined":true,"need_rejoin":false,"is_owned":false,"can_be_managed":true,"participant_count":0,"has_hidden_listeners":false,"loaded_all_participants":false,"message_sender_id":null,"recent_speakers":[],"is_my_video_enabled":false,"is_my_video_paused":false,"can_enable_video":true,"mute_new_participants":false,"can_toggle_mute_new_participants":true,"can_send_messages":true,"are_messages_allowed":true,"can_toggle_are_messages_allowed":false,"can_delete_messages":false,"record_duration":0,"is_video_recorded":false,"duration":0}}"#,
    );
    assert_eq!(handle.group_connects().len(), 1);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Phase C2g (review fix): a rejoin resets the transport handshake —
/// the new `joinVideoChat` answer reconnects the fresh native context
/// instead of being dropped by the pump's `transport_ready` filter,
/// and an orphaned presentation is stopped.
#[test]
fn group_rejoin_answer_reconnects_native_transport() {
    let (dir, mut driver, recorder, mut handle, sink, seq) = ready_group_call_driver();
    let need_rejoin = r#"{"@type":"updateGroupCall","group_call":{"@type":"groupCall","id":555,"unique_id":"999","title":"Design voice","invite_link":"","paid_message_star_count":0,"scheduled_start_date":0,"enabled_start_notification":false,"is_active":true,"is_video_chat":true,"is_live_story":false,"is_rtmp_stream":false,"is_joined":true,"need_rejoin":true,"is_owned":false,"can_be_managed":true,"participant_count":0,"has_hidden_listeners":false,"loaded_all_participants":false,"message_sender_id":null,"recent_speakers":[],"is_my_video_enabled":false,"is_my_video_paused":false,"can_enable_video":true,"mute_new_participants":false,"can_toggle_mute_new_participants":true,"can_send_messages":true,"are_messages_allowed":true,"can_toggle_are_messages_allowed":false,"can_delete_messages":false,"record_duration":0,"is_video_recorded":false,"duration":0}}"#;
    // Join → answer → the native transport is connected.
    let extra = driver.join_video_chat(555).expect("join tracked call");
    ingest_call_json(
        &mut driver,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"text","text":"join-answer","@extra":{}}}"#,
            extra.0
        ),
    );
    assert!(
        driver
            .session
            .active_group_call
            .as_ref()
            .is_some_and(|call| call.transport_ready)
    );
    assert_eq!(handle.group_connects().len(), 1);
    // A live presentation plus tracked screen-share state: the
    // rejoin must stop the orphaned native presentation and clear
    // the stale flags.
    handle.start_screen_share(555).expect("mock presentation");
    driver
        .session
        .active_group_call
        .as_mut()
        .expect("tracked call")
        .screen_sharing = true;
    driver
        .session
        .active_group_call
        .as_mut()
        .expect("tracked call")
        .screen_share_answer = "old-answer".into();
    // `need_rejoin` auto-rejoins with a fresh offer and a reset
    // handshake gate.
    ingest_call_json(&mut driver, &seq, &sink, need_rejoin);
    assert_eq!(handle.group_offers().len(), 2);
    assert_eq!(handle.screen_share_stops(), vec![555]);
    let call = driver
        .session
        .active_group_call
        .as_ref()
        .expect("tracked call");
    assert!(!call.transport_ready);
    assert!(call.join_payload.is_empty());
    assert!(!call.screen_sharing && !call.screen_share_pending);
    assert!(call.screen_share_answer.is_empty());
    // The new answer reconnects the fresh native context.
    let extra = sent_request(&recorder, "joinVideoChat")["@extra"]
        .as_str()
        .unwrap()
        .to_string();
    ingest_call_json(
        &mut driver,
        &seq,
        &sink,
        &format!(r#"{{"@type":"text","text":"rejoin-answer","@extra":"{extra}"}}"#),
    );
    assert_eq!(handle.group_connects().len(), 2);
    assert!(
        driver
            .session
            .active_group_call
            .as_ref()
            .is_some_and(|call| call.transport_ready)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// Phase C2g: participant `video_info` / `screen_sharing_video_info`
/// become engine subscriptions; paused endpoints and the local user
/// are skipped.
#[test]
fn group_participant_video_syncs_subscriptions() {
    let (dir, mut driver, _recorder, handle, sink, seq) = ready_group_call_driver();
    let extra = driver.join_video_chat(555).expect("join tracked call");
    ingest_call_json(
        &mut driver,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"text","text":"join-answer","@extra":{}}}"#,
            extra.0
        ),
    );
    let camera = r#"{"@type":"groupCallParticipantVideoInfo","source_groups":[{"@type":"groupCallVideoSourceGroup","semantics":"SIM","source_ids":[111,112]}],"endpoint_id":"ep-42","is_paused":false}"#;
    let screen = r#"{"@type":"groupCallParticipantVideoInfo","source_groups":[{"@type":"groupCallVideoSourceGroup","semantics":"SIM","source_ids":[222]}],"endpoint_id":"ep-42-screen","is_paused":false}"#;
    ingest_call_json(
        &mut driver,
        &seq,
        &sink,
        &group_participant_json(42, false, camera, screen),
    );
    // Paused endpoint: skipped.
    let paused = r#"{"@type":"groupCallParticipantVideoInfo","source_groups":[{"@type":"groupCallVideoSourceGroup","semantics":"SIM","source_ids":[333]}],"endpoint_id":"ep-43","is_paused":true}"#;
    ingest_call_json(
        &mut driver,
        &seq,
        &sink,
        &group_participant_json(43, false, paused, "null"),
    );
    // Local user: skipped even with video info.
    ingest_call_json(
        &mut driver,
        &seq,
        &sink,
        &group_participant_json(777, true, camera, "null"),
    );
    let syncs = handle.group_video_syncs();
    let (call_id, sources) = syncs.last().expect("sync recorded");
    assert_eq!(*call_id, 555);
    assert_eq!(sources.len(), 2);
    let camera_source = sources
        .iter()
        .find(|source| source.endpoint == "ep-42")
        .expect("camera source");
    assert_eq!(camera_source.user_id, 42);
    assert_eq!(camera_source.ssrc_groups.len(), 1);
    assert_eq!(camera_source.ssrc_groups[0].semantics, "SIM");
    assert_eq!(camera_source.ssrc_groups[0].ssrcs, vec![111, 112]);
    let screen_source = sources
        .iter()
        .find(|source| source.endpoint == "ep-42-screen")
        .expect("screen source");
    assert_eq!(screen_source.user_id, 42);
    assert_eq!(screen_source.ssrc_groups[0].ssrcs, vec![222]);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Phase C2g: frames with `participant_user_id` land in the group
/// slots (camera vs screen), never in the 1:1 slots.
#[test]
fn group_video_frames_route_to_participant_slots() {
    let (dir, driver, _recorder, handle, _sink, _seq) = ready_group_call_driver();
    let frame = |screen: bool| VideoFrame {
        seq: 0,
        width: 2,
        height: 2,
        rgba: vec![0u8; 16],
        is_local: false,
        participant_user_id: Some(42),
        is_screen: screen,
    };
    handle.emit_video_frame(555, frame(false));
    handle.emit_video_frame(555, frame(true));
    assert!(driver.latest_group_video_frame(555, 42, false).is_some());
    assert!(driver.latest_group_video_frame(555, 42, true).is_some());
    assert!(driver.latest_group_video_frame(555, 43, false).is_none());
    assert!(driver.latest_video_frame(555, false).is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

/// Slice calls-group-self-tile: a group-local frame (`is_local`,
/// no participant — what the engine now delivers for group
/// CAPTURE frames) lands in the shared (call id, is_local) slots,
/// never in the participant slots.
#[test]
fn group_local_frame_routes_to_shared_local_slot() {
    let (dir, driver, _recorder, handle, _sink, _seq) = ready_group_call_driver();
    handle.emit_video_frame(
        555,
        VideoFrame {
            seq: 0,
            width: 2,
            height: 2,
            rgba: vec![0u8; 16],
            is_local: true,
            participant_user_id: None,
            is_screen: false,
        },
    );
    assert!(driver.latest_video_frame(555, true).is_some());
    assert!(driver.latest_video_frame(555, false).is_none());
    assert!(driver.latest_group_video_frame(555, 42, false).is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

/// Slice calls-group-self-tile: leaving the group call clears the
/// self-tile slot along with the participant slots.
#[test]
fn group_call_leave_clears_local_frame_slot() {
    let (dir, mut driver, _recorder, handle, _sink, _seq) = ready_group_call_driver();
    handle.emit_video_frame(
        555,
        VideoFrame {
            seq: 0,
            width: 2,
            height: 2,
            rgba: vec![0u8; 16],
            is_local: true,
            participant_user_id: None,
            is_screen: false,
        },
    );
    assert!(driver.latest_video_frame(555, true).is_some());
    driver.leave_group_call().expect("leave");
    assert!(driver.latest_video_frame(555, true).is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

/// Phase C2g: leaving tears the native transport down and drops the
/// retained frames.
#[test]
fn group_call_leave_tears_down_transport_and_frames() {
    let (dir, mut driver, _recorder, mut handle, _sink, _seq) = ready_group_call_driver();
    handle.emit_video_frame(
        555,
        VideoFrame {
            seq: 0,
            width: 2,
            height: 2,
            rgba: vec![0u8; 16],
            is_local: false,
            participant_user_id: Some(42),
            is_screen: false,
        },
    );
    assert!(driver.latest_group_video_frame(555, 42, false).is_some());
    // Review fix: a live presentation is stopped before the call
    // (privacy — capture ends first).
    handle.start_screen_share(555).expect("mock presentation");
    driver.leave_group_call().expect("leave");
    assert_eq!(handle.group_leaves(), vec![555]);
    assert_eq!(handle.screen_share_stops(), vec![555]);
    assert!(driver.latest_group_video_frame(555, 42, false).is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

/// Phase C2g: screen-share toggle runs the presentation handshake —
/// offer into `startGroupCallScreenSharing`, answer into the engine,
/// stop pairs `endGroupCallScreenSharing` with the engine stop.
#[test]
fn group_screen_share_toggle_handshake() {
    let (dir, mut driver, recorder, handle, sink, seq) = ready_group_call_driver();
    // The engine enumerates a screen source; the driver picks it up
    // on connect.
    handle.set_devices(vec![MediaDevice {
        id: "screen-0".into(),
        name: "Test screen".into(),
        kind: MediaDeviceKind::Screen,
    }]);
    // Join and finish the native handshake first, as in reality.
    let join_extra = driver.join_video_chat(555).expect("join tracked call");
    ingest_call_json(
        &mut driver,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"text","text":"join-answer","@extra":{}}}"#,
            join_extra.0
        ),
    );
    assert!(
        driver
            .session
            .active_group_call
            .as_ref()
            .is_some_and(|call| call.transport_ready)
    );
    // TDLib confirms the join via `updateGroupCall is_joined`; the
    // toggle gates on it.
    driver
        .session
        .active_group_call
        .as_mut()
        .expect("tracked call")
        .is_joined = true;
    let start_extra = driver
        .toggle_group_call_screen_share()
        .expect("start sharing");
    assert_eq!(handle.screen_share_offers(), vec![555]);
    let start = sent_request(&recorder, "startGroupCallScreenSharing");
    assert_eq!(start["group_call_id"], 555);
    assert_eq!(start["payload"], "mock-presentation-offer-555");
    assert!(
        driver
            .session
            .active_group_call
            .as_ref()
            .is_some_and(|call| call.screen_share_pending && !call.screen_sharing)
    );
    ingest_call_json(
        &mut driver,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"text","text":"share-answer","@extra":{}}}"#,
            start_extra.0
        ),
    );
    assert_eq!(
        handle.screen_share_connects(),
        vec![(555, "share-answer".to_string())]
    );
    assert!(
        driver
            .session
            .active_group_call
            .as_ref()
            .is_some_and(|call| call.screen_sharing && !call.screen_share_pending)
    );
    driver
        .toggle_group_call_screen_share()
        .expect("stop sharing");
    let end = sent_request(&recorder, "endGroupCallScreenSharing");
    assert_eq!(end["group_call_id"], 555);
    assert_eq!(handle.screen_share_stops(), vec![555]);
    assert!(
        driver
            .session
            .active_group_call
            .as_ref()
            .is_some_and(|call| !call.screen_sharing && !call.screen_share_pending)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// Phase C2g: a failed screen-share handshake (the reducer clears
/// the tracked flags on a `startGroupCallScreenSharing` error)
/// leaves no stray native presentation: the pump stops it.
#[test]
fn group_screen_share_failure_stops_native_presentation() {
    let (dir, mut driver, recorder, handle, sink, seq) = ready_group_call_driver();
    handle.set_devices(vec![MediaDevice {
        id: "screen-0".into(),
        name: "Test screen".into(),
        kind: MediaDeviceKind::Screen,
    }]);
    let join_extra = driver.join_video_chat(555).expect("join tracked call");
    ingest_call_json(
        &mut driver,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"text","text":"join-answer","@extra":{}}}"#,
            join_extra.0
        ),
    );
    driver
        .session
        .active_group_call
        .as_mut()
        .expect("tracked call")
        .is_joined = true;
    driver
        .toggle_group_call_screen_share()
        .expect("start sharing");
    assert!(handle.presentation_active(555));
    // Simulate the reducer's error arm: flags cleared, error
    // surfaced, native presentation untouched.
    if let Some(call) = driver.session.active_group_call.as_mut() {
        call.screen_share_pending = false;
        call.screen_sharing = false;
        call.screen_share_answer.clear();
    }
    ingest_call_json(
        &mut driver,
        &seq,
        &sink,
        r#"{"@type":"updateGroupCall","group_call":{"@type":"groupCall","id":555,"unique_id":"999","title":"Design voice","invite_link":"","paid_message_star_count":0,"scheduled_start_date":0,"enabled_start_notification":false,"is_active":true,"is_video_chat":true,"is_live_story":false,"is_rtmp_stream":false,"is_joined":true,"need_rejoin":false,"is_owned":false,"can_be_managed":true,"participant_count":0,"has_hidden_listeners":false,"loaded_all_participants":false,"message_sender_id":null,"recent_speakers":[],"is_my_video_enabled":false,"is_my_video_paused":false,"can_enable_video":true,"mute_new_participants":false,"can_toggle_mute_new_participants":true,"can_send_messages":true,"are_messages_allowed":true,"can_toggle_are_messages_allowed":false,"can_delete_messages":false,"record_duration":0,"is_video_recorded":false,"duration":0}}"#,
    );
    assert_eq!(handle.screen_share_stops(), vec![555]);
    assert!(!handle.presentation_active(555));
    assert!(recorder.snapshot().len() >= 2);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Phase C2g: without an enumerated screen source the toggle is
/// rejected and no request goes out ("No screen source available").
#[test]
fn group_screen_share_rejected_without_screen_source() {
    let (dir, mut driver, recorder, _handle, _sink, _seq) = ready_group_call_driver();
    driver
        .session
        .active_group_call
        .as_mut()
        .expect("tracked call")
        .is_joined = true;
    assert!(driver.toggle_group_call_screen_share().is_err());
    assert!(
        !driver
            .session
            .active_group_call
            .as_ref()
            .is_some_and(|call| call.screen_share_pending || call.screen_sharing)
    );
    assert!(
        recorder
            .snapshot()
            .iter()
            .all(|json| !json.contains("startGroupCallScreenSharing"))
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// Muting yourself in a voice chat silences the microphone and tells
/// Telegram, so everyone sees it (it used to only flip a local flag).
#[test]
fn group_call_self_mute_reaches_the_microphone_and_telegram() {
    let (dir, mut driver, recorder, handle, sink, seq) = ready_group_call_driver();
    ingest_call_json(
        &mut driver,
        &seq,
        &sink,
        &group_participant_json(7, true, "null", "null"),
    );
    driver.toggle_group_call_self_mute();
    assert_eq!(handle.group_mutes(), vec![(555, true)]);
    let sent = sent_request(&recorder, "toggleGroupCallParticipantIsMuted");
    assert_eq!(sent["participant_id"]["user_id"], 7);
    assert_eq!(sent["is_muted"], true);
    driver.toggle_group_call_self_mute();
    assert_eq!(handle.group_mutes(), vec![(555, true), (555, false)]);
    let _ = std::fs::remove_dir_all(&dir);
}
