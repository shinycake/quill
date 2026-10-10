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

mod transport;

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

    driver.session.calls.active_group_call = Some(tracked_group_call(false, true, false));

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
    driver.session.calls.active_group_call = Some(tracked_group_call(false, true, false));
    assert_eq!(
        driver.ban_group_call_participant(9),
        Err(ConnectSendError::InvalidRequest),
        "can_be_managed=true but is_owned=false must refuse"
    );
    driver.session.calls.active_group_call = Some(tracked_group_call(false, false, true));
    driver
        .ban_group_call_participant(9)
        .expect("ban sends for owner");
    let sent = last_sent();
    assert_eq!(sent["@type"], "banGroupCallParticipants");
    assert_eq!(sent["group_call_id"], 77);
    assert_eq!(sent["user_ids"], serde_json::json!([9]));
    driver.session.calls.active_group_call = Some(tracked_group_call(false, true, true));

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
    driver.session.calls.active_call = Some(ActiveCall {
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

    driver.session.calls.active_group_call = Some(tracked_group_call(false, true, false));
    driver
        .session
        .calls
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
        .calls
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
        .calls
        .active_group_call
        .as_mut()
        .unwrap()
        .can_be_managed = true;

    // Start now: `startScheduledVideoChat` (:14277), gated on
    // `can_be_managed` for a still-scheduled call.
    driver
        .session
        .calls
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
        .calls
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
        .calls
        .active_group_call
        .as_mut()
        .unwrap()
        .can_be_managed = true;
    driver
        .session
        .calls
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
        .calls
        .active_group_call
        .as_mut()
        .unwrap()
        .scheduled_start_date = 1_788_000_000;
    driver
        .session
        .calls
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
        .calls
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
        .calls
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
        let call = driver.session.calls.active_group_call.as_mut().unwrap();
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
        .calls
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
        .calls
        .active_group_call
        .as_mut()
        .unwrap()
        .are_messages_allowed = true;
    // Chat toggle: `toggleGroupCallAreMessagesAllowed` (:14322),
    // gated on `can_toggle_are_messages_allowed`; flips the flag.
    driver
        .session
        .calls
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
    driver
        .session
        .calls
        .active_group_call
        .as_mut()
        .unwrap()
        .is_owned = true;
    driver
        .replace_video_chat_rtmp_url()
        .expect("rtmp replace sends");
    let sent = last_sent();
    assert_eq!(sent["@type"], "replaceVideoChatRtmpUrl");
    assert_eq!(sent["chat_id"], 51);

    // Scheduling: `createVideoChat` start_date (schema :14256) —
    // 0 starts immediately; scheduled dates must be ≥10s and ≤8d
    // ahead. Requires no tracked call (a start/join target).
    driver.session.calls.active_group_call = None;
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
    assert_eq!(driver.session.calls.group_call_error, None);
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

    driver.session.calls.active_group_call = Some(tracked_group_call(true, false, false));
    driver
        .session
        .calls
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
                .calls
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
        driver.session.calls.group_call_error.as_deref(),
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
    let call = driver.session.calls.active_group_call.as_ref().unwrap();
    assert_eq!(call.rejoin_attempts, 0);
    assert!(!call.reconnecting);
    assert_eq!(driver.session.calls.group_call_error, None);

    // Manual retry resets the counter: with `need_rejoin` back,
    // three fresh auto attempts are allowed after
    // `rejoin_group_call(true)`.
    call_state_for_rejoin(&mut driver);
    driver.rejoin_group_call(true).expect("manual retry sends");
    assert_eq!(
        driver
            .session
            .calls
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
