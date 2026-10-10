//! Call signaling replay tests.
//! Split from `tests/replay.rs` — pure code motion.
mod replay_common;
use quill::state::CallsPurpose;
use replay_common::*;

/// Phase C1: the full call-signaling lifecycle through the reducer —
/// incoming pending → exchanging keys → ready (with honestly queued
/// signaling data) → discarded (summary + rating flag); a second
/// incoming call while one is active raises the swap prompt; a
/// failed `createCall` surfaces `call_error`.
#[test]
fn replay_call_signaling_lifecycle() {
    use quill::telegram::envelope::CallState;

    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn quill::diagnostics::DiagnosticSink> = sink.clone();
    let mut session = Session::new(AccountKey::primary(), dyn_sink);
    let seq = AtomicU64::new(0);

    // Incoming pending call from Zed (user 41).
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            r#"{"@type":"updateCall","call":{"@type":"call","id":77,"unique_id":"99","user_id":41,"is_outgoing":false,"is_video":false,"state":{"@type":"callStatePending","is_created":true,"is_received":false}}}"#,
        ],
    );
    let call = session.active_call.as_ref().expect("incoming call tracked");
    assert_eq!(call.id, 77);
    assert_eq!(call.user_id, 41);
    assert!(!call.is_outgoing);
    assert!(matches!(
        call.state,
        CallState::Pending {
            is_created: true,
            is_received: false
        }
    ));
    assert!(session.call_summary.is_none());

    // A second incoming call while one is active raises the swap
    // prompt (the busy-decline queue stays empty for the first one).
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            r#"{"@type":"updateCall","call":{"@type":"call","id":78,"unique_id":"100","user_id":42,"is_outgoing":false,"is_video":false,"state":{"@type":"callStatePending","is_created":true,"is_received":false}}}"#,
        ],
    );
    assert_eq!(session.active_call.as_ref().expect("still call 77").id, 77);
    assert_eq!(session.call_swap_pending, Some((78, 42, false)));
    assert!(session.call_busy_decline_queue.is_empty());

    // Keys exchange, then Ready; signaling data is queued honestly.
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            r#"{"@type":"updateCall","call":{"@type":"call","id":77,"unique_id":"99","user_id":41,"is_outgoing":false,"is_video":false,"state":{"@type":"callStateExchangingKeys"}}}"#,
            r#"{"@type":"updateNewCallSignalingData","call_id":77,"data":"AAEC"}"#,
            r#"{"@type":"updateNewCallSignalingData","call_id":78,"data":"AAEC"}"#,
            r#"{"@type":"updateCall","call":{"@type":"call","id":77,"unique_id":"99","user_id":41,"is_outgoing":false,"is_video":false,"state":{"@type":"callStateReady","protocol":{"@type":"callProtocol","udp_p2p":true,"udp_reflector":true,"min_layer":65,"max_layer":92,"library_versions":[]},"servers":[],"config":"{}","encryption_key":"","emojis":[],"allow_p2p":false,"is_group_call_supported":false,"custom_parameters":"{}"}}}"#,
        ],
    );
    let call = session.active_call.as_ref().expect("call 77 ready");
    assert!(matches!(call.state, CallState::Ready));
    assert!(call.ready_at.is_some());
    // Only the tracked call's signaling data is kept (call 78's is
    // dropped — no tracked call with that id).
    assert_eq!(call.signaling_queue.len(), 1);
    assert_eq!(call.signaling_queue[0], vec![0x00, 0x01, 0x02]);

    // Remote hangup with need_rating → summary drives the end screen.
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            r#"{"@type":"updateCall","call":{"@type":"call","id":77,"unique_id":"99","user_id":41,"is_outgoing":false,"is_video":false,"state":{"@type":"callStateDiscarded","reason":{"@type":"callDiscardReasonHungUp"},"need_rating":true,"need_debug_information":false,"need_log":false}}}"#,
        ],
    );
    assert!(session.active_call.is_none());
    let summary = session.call_summary.as_ref().expect("end summary");
    assert_eq!(summary.call_id, 77);
    assert_eq!(summary.end_line, "Call ended");
    assert!(summary.need_rating);
    assert!(!summary.rating_sent);
    assert!(!summary.need_debug_information);
    assert!(!summary.need_log);

    // A rejected `createCall` surfaces the error (TDLib's message text
    // is never stored — it can contain secrets).
    let extra = session.request_for_user(
        RequestPurpose::Calls(CallsPurpose::CreateCall { is_video: false }),
        41,
    );
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[&format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"PHONE_CALL_PROTOCOL_ERROR"}}"#,
            extra.0
        )],
    );
    assert!(session.active_call.is_none());
    let error = session.call_error.as_ref().expect("call error shown");
    assert!(error.contains("Could not start the call"));
    assert!(error.contains("400"));
    assert!(!error.contains("PHONE_CALL_PROTOCOL_ERROR"));

    // The `callId` answer starts tracking the outgoing call.
    let extra = session.request_for_user(
        RequestPurpose::Calls(CallsPurpose::CreateCall { is_video: false }),
        41,
    );
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[&format!(
            r#"{{"@type":"callId","@extra":"{}","id":79}}"#,
            extra.0
        )],
    );
    let call = session.active_call.as_ref().expect("outgoing tracked");
    assert_eq!(call.id, 79);
    assert!(call.is_outgoing);
    // The failed request did not leave a tracked call behind earlier,
    // and the error is cleared when a new call is tracked.
    assert!(session.call_error.is_none());

    // Hang up the outgoing call.
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            r#"{"@type":"updateCall","call":{"@type":"call","id":79,"unique_id":"103","user_id":41,"is_outgoing":true,"is_video":false,"state":{"@type":"callStateDiscarded","reason":{"@type":"callDiscardReasonHungUp"},"need_rating":false,"need_debug_information":false,"need_log":false}}}"#,
        ],
    );
    assert!(session.active_call.is_none());

    // A missed incoming call we never tracked still records a summary.
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            r#"{"@type":"updateCall","call":{"@type":"call","id":80,"unique_id":"101","user_id":41,"is_outgoing":false,"is_video":false,"state":{"@type":"callStateDiscarded","reason":{"@type":"callDiscardReasonMissed"},"need_rating":false,"need_debug_information":false,"need_log":false}}}"#,
        ],
    );
    let summary = session.call_summary.as_ref().expect("missed summary");
    assert_eq!(summary.end_line, "Missed call");

    // A call error with the documented 4005000 timeout code.
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            r#"{"@type":"updateCall","call":{"@type":"call","id":81,"unique_id":"102","user_id":41,"is_outgoing":true,"is_video":false,"state":{"@type":"callStateError","error":{"@type":"error","code":4005000,"message":"CALL_TIMEOUT"}}}}"#,
        ],
    );
    let summary = session.call_summary.as_ref().expect("error summary");
    assert!(summary.end_line.contains("timed out"));
}

/// Phase C1b: video-call signaling through the reducer — an outgoing
/// video `createCall` tracks `is_video: true` from the request args
/// (the `callId` answer carries no `is_video`, schema 1.8.67 :7034) →
/// exchanging keys → ready → discard keeps `is_video: true` on the
/// summary; an incoming video call tracks `is_video` from `updateCall`
/// itself (the `call` type carries it, schema 1.8.67 :7287); a second
/// incoming video call while one is active raises the swap prompt
/// with `is_video: true`.
#[test]
fn replay_video_call_signaling() {
    use quill::telegram::envelope::CallState;

    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn quill::diagnostics::DiagnosticSink> = sink.clone();
    let mut session = Session::new(AccountKey::primary(), dyn_sink);
    let seq = AtomicU64::new(0);

    // Outgoing video `createCall`: the `callId` answer starts tracking
    // with `is_video` derived from the request args.
    let extra = session.request_for_user(
        RequestPurpose::Calls(CallsPurpose::CreateCall { is_video: true }),
        41,
    );
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[&format!(
            r#"{{"@type":"callId","@extra":"{}","id":90}}"#,
            extra.0
        )],
    );
    let call = session
        .active_call
        .as_ref()
        .expect("outgoing video tracked");
    assert_eq!(call.id, 90);
    assert!(call.is_outgoing);
    assert!(call.is_video);
    assert!(!call.muted);

    // Exchanging keys → ready; `is_video` survives state advances.
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            r#"{"@type":"updateCall","call":{"@type":"call","id":90,"unique_id":"200","user_id":41,"is_outgoing":true,"is_video":true,"state":{"@type":"callStateExchangingKeys"}}}"#,
            r#"{"@type":"updateCall","call":{"@type":"call","id":90,"unique_id":"200","user_id":41,"is_outgoing":true,"is_video":true,"state":{"@type":"callStateReady","protocol":{"@type":"callProtocol","udp_p2p":false,"udp_reflector":false,"min_layer":65,"max_layer":92,"library_versions":[]},"servers":[],"config":"{}","encryption_key":"","emojis":[],"allow_p2p":false,"is_group_call_supported":false,"custom_parameters":"{}"}}}"#,
        ],
    );
    let call = session.active_call.as_ref().expect("call 90 ready");
    assert!(matches!(call.state, CallState::Ready));
    assert!(call.is_video);
    assert!(call.ready_at.is_some());

    // A second incoming video call while one is active raises the
    // swap prompt, keeping its `is_video`.
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            r#"{"@type":"updateCall","call":{"@type":"call","id":91,"unique_id":"201","user_id":42,"is_outgoing":false,"is_video":true,"state":{"@type":"callStatePending","is_created":true,"is_received":false}}}"#,
        ],
    );
    assert_eq!(session.active_call.as_ref().expect("still call 90").id, 90);
    assert_eq!(session.call_swap_pending, Some((91, 42, true)));

    // Remote hangup → the summary keeps `is_video: true` (the driver
    // sends it back in `discardCall`).
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            r#"{"@type":"updateCall","call":{"@type":"call","id":90,"unique_id":"200","user_id":41,"is_outgoing":true,"is_video":true,"state":{"@type":"callStateDiscarded","reason":{"@type":"callDiscardReasonHungUp"},"need_rating":true,"need_debug_information":false,"need_log":false}}}"#,
        ],
    );
    assert!(session.active_call.is_none());
    let summary = session.call_summary.as_ref().expect("end summary");
    assert_eq!(summary.call_id, 90);
    assert!(summary.is_video);
    assert!(summary.need_rating);

    // Incoming video call: `is_video` comes from `updateCall` itself —
    // no inference needed.
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            r#"{"@type":"updateCall","call":{"@type":"call","id":92,"unique_id":"202","user_id":41,"is_outgoing":false,"is_video":true,"state":{"@type":"callStatePending","is_created":true,"is_received":false}}}"#,
        ],
    );
    let call = session
        .active_call
        .as_ref()
        .expect("incoming video tracked");
    assert_eq!(call.id, 92);
    assert!(!call.is_outgoing);
    assert!(call.is_video);
}

/// Phase C3a: group-call signaling replay — create → join →
/// participants load → speaking update → mute → hand raise → leave.
/// All injected; no live Telegram. Honest no-transport throughout:
/// the `joinVideoChat` payload is stored, never consumed.
#[test]
fn replay_group_call_signaling() {
    use quill::telegram::envelope::MessageSender;

    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn quill::diagnostics::DiagnosticSink> = sink.clone();
    let mut session = Session::new(AccountKey::primary(), dyn_sink);
    let seq = AtomicU64::new(0);

    // Users: Alice (41, the current user), Bob (42), Carol (43).
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            r#"{"@type":"updateUser","user":{"id":41,"first_name":"Alice","last_name":"A","type":{"@type":"userTypeRegular"}}}"#,
            r#"{"@type":"updateUser","user":{"id":42,"first_name":"Bob","last_name":"B","type":{"@type":"userTypeRegular"}}}"#,
            r#"{"@type":"updateUser","user":{"id":43,"first_name":"Carol","last_name":"C","type":{"@type":"userTypeRegular"}}}"#,
            r#"{"@type":"updateNewChat","chat":{"id":100,"title":"Demo voice","type":{"@type":"chatTypeSupergroup","supergroup_id":100,"is_channel":false},"unread_count":0}}"#,
            r#"{"@type":"updateChatVideoChat","chat_id":100,"video_chat":{"@type":"videoChat","group_call_id":555,"has_participants":false,"default_participant_id":null}}"#,
        ],
    );
    let chat = session.chats.get(&100).expect("demo chat");
    let vc = chat.video_chat.as_ref().expect("video chat tracked");
    assert_eq!(vc.group_call_id, 555);
    assert!(!vc.has_participants);

    // Full `groupCall` JSON per schema 1.8.67 line 7154.
    let group_call_json = |is_joined: bool, need_rejoin: bool, is_active: bool| {
        format!(
            r#"{{"@type":"updateGroupCall","group_call":{{"@type":"groupCall","id":555,"unique_id":"999","title":"Demo voice","invite_link":"","paid_message_star_count":0,"scheduled_start_date":0,"enabled_start_notification":false,"is_active":{is_active},"is_video_chat":true,"is_live_story":false,"is_rtmp_stream":false,"is_joined":{is_joined},"need_rejoin":{need_rejoin},"is_owned":false,"can_be_managed":true,"participant_count":3,"has_hidden_listeners":false,"loaded_all_participants":false,"message_sender_id":null,"recent_speakers":[{{"@type":"groupCallRecentSpeaker","participant_id":{{"@type":"messageSenderUser","user_id":42}},"is_speaking":true}}],"is_my_video_enabled":false,"is_my_video_paused":false,"can_enable_video":true,"mute_new_participants":false,"can_toggle_mute_new_participants":true,"can_send_messages":true,"are_messages_allowed":true,"can_toggle_are_messages_allowed":false,"can_delete_messages":false,"record_duration":0,"is_video_recorded":false,"duration":0}}}}"#
        )
    };
    let participant_json = |user_id: i64, flags: &str, order: &str| {
        format!(
            r#"{{"@type":"updateGroupCallParticipant","group_call_id":555,"participant":{{"@type":"groupCallParticipant","participant_id":{{"@type":"messageSenderUser","user_id":{user_id}}},"audio_source_id":0,"screen_sharing_audio_source_id":0,"video_info":null,"screen_sharing_video_info":null,"bio":"","is_current_user":false,"is_speaking":false,"is_hand_raised":false,"can_be_muted_for_all_users":true,"can_be_unmuted_for_all_users":true,"can_be_muted_for_current_user":true,"can_be_unmuted_for_current_user":true,"is_muted_for_all_users":false,"is_muted_for_current_user":false,"can_unmute_self":false,"volume_level":10000,"order":"{order}"{flags}}}}}"#
        )
    };

    // The call exists but isn't joined yet.
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[&group_call_json(false, false, true)],
    );
    let call = session.active_group_call.as_ref().expect("tracked");
    assert_eq!(call.id, 555);
    assert_eq!(call.title, "Demo voice");
    assert!(!call.is_joined);
    assert!(!call.reconnecting);
    assert!(call.can_be_managed);
    assert!(call.can_toggle_mute_new_participants);
    assert!(call.participants.is_empty());

    // Join: `updateGroupCall` flips `is_joined`; the `joinVideoChat`
    // `text` answer is stored, never consumed.
    let extra = session.request(
        RequestPurpose::Calls(CallsPurpose::JoinVideoChat { group_call_id: 555 }),
        None,
    );
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            &group_call_json(true, false, true),
            &format!(
                r#"{{"@type":"text","@extra":"{}","text":"JOIN_PAYLOAD_BYTES"}}"#,
                extra.0
            ),
        ],
    );
    let call = session.active_group_call.as_ref().expect("joined");
    assert!(call.is_joined);
    assert_eq!(call.join_payload, "JOIN_PAYLOAD_BYTES");

    // Participants load: Alice (self), Bob (speaking), Carol (hand
    // raised + muted for all). Bob leads via recent_speakers.
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            &participant_json(41, r#","is_current_user":true"#, "a3"),
            &participant_json(42, r#","is_speaking":true"#, "a2"),
            &participant_json(
                43,
                r#","is_hand_raised":true,"is_muted_for_all_users":true"#,
                "a1",
            ),
        ],
    );
    let call = session.active_group_call.as_ref().expect("participants");
    assert_eq!(call.participants.len(), 3);
    // Bob first (recent speaker), then Alice, then Carol by `order`.
    let ids: Vec<i64> = call
        .participants
        .iter()
        .map(|p| match p.participant_id {
            MessageSender::User { user_id } => user_id,
            MessageSender::Chat { .. } => -1,
        })
        .collect();
    assert_eq!(ids, vec![42, 41, 43]);
    let bob = &call.participants[0];
    assert!(bob.is_speaking);
    let carol = call
        .participants
        .iter()
        .find(|p| p.participant_id == MessageSender::User { user_id: 43 })
        .expect("carol");
    assert!(carol.is_hand_raised);
    assert!(carol.is_muted_for_all_users);
    let alice = call
        .participants
        .iter()
        .find(|p| p.participant_id == MessageSender::User { user_id: 41 })
        .expect("alice");
    assert!(alice.is_current_user);

    // Bob stops speaking → flag clears.
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[&participant_json(42, "", "a2")],
    );
    let call = session.active_group_call.as_ref().expect("updated");
    let bob = call
        .participants
        .iter()
        .find(|p| p.participant_id == MessageSender::User { user_id: 42 })
        .expect("bob");
    assert!(!bob.is_speaking);

    // `updateGroupCallParticipants` drops Carol (not in the id list).
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            r#"{"@type":"updateGroupCallParticipants","group_call_id":555,"participant_user_ids":[41,42]}"#,
        ],
    );
    let call = session.active_group_call.as_ref().expect("pruned");
    assert_eq!(call.participants.len(), 2);

    // E2E verification state arrives.
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            r#"{"@type":"updateGroupCallVerificationState","group_call_id":555,"generation":7,"emojis":["🍎","🍌"]}"#,
        ],
    );
    let call = session.active_group_call.as_ref().expect("verified");
    let v = call.verification.as_ref().expect("verification stored");
    assert_eq!(v.generation, 7);
    assert_eq!(v.emojis, vec!["🍎", "🍌"]);

    // Local self-mute toggle (local-only — no TDLib request).
    session.set_group_call_self_muted(true);
    assert!(
        session
            .active_group_call
            .as_ref()
            .expect("call")
            .is_muted_self
    );

    // `need_rejoin` → reconnecting banner state.
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[&group_call_json(true, true, true)],
    );
    let call = session.active_group_call.as_ref().expect("rejoining");
    assert!(call.need_rejoin);
    assert!(call.reconnecting);
    session.clear_group_call_reconnecting();
    assert!(
        !session
            .active_group_call
            .as_ref()
            .expect("call")
            .reconnecting
    );

    // Call ends → tracked call cleared.
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[&group_call_json(false, false, false)],
    );
    assert!(session.active_group_call.is_none());

    // Local leave also clears.
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[&group_call_json(true, false, true)],
    );
    assert!(session.active_group_call.is_some());
    session.leave_group_call_local();
    assert!(session.active_group_call.is_none());
}

/// Phase C3a (reviewer-found regression): `getGroupCall` (schema 1.8.67,
/// line 14274) answers with a bare `groupCall` object, not wrapped in
/// `updateGroupCall`. The parse arm must route it through the same
/// handling so the fetch path creates the tracker. Before the fix the
/// response died as a parse error, the pending request leaked, and the
/// tracked call was never created — the header "Voice chat" button led
/// to a dead "Joining voice chat…" state.
#[test]
fn replay_get_group_call_bare_response_populates_tracker() {
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn quill::diagnostics::DiagnosticSink> = sink.clone();
    let mut session = Session::new(AccountKey::primary(), dyn_sink);
    let seq = AtomicU64::new(0);

    // Register the in-flight `getGroupCall` exactly like the driver does.
    let req_id = session.requests.register(
        quill::ids::AccountGeneration(1),
        RequestPurpose::Calls(CallsPurpose::GetGroupCall { group_call_id: 555 }),
        None,
        None,
    );

    // Bare `groupCall` response with the matching `@extra` (fields per
    // schema 1.8.67 line 7154, same shape as `updateGroupCall`'s payload).
    let json = format!(
        r#"{{"@type":"groupCall","@extra":"{}","id":555,"unique_id":"999","title":"Demo voice","invite_link":"","paid_message_star_count":0,"scheduled_start_date":0,"enabled_start_notification":false,"is_active":true,"is_video_chat":true,"is_live_story":false,"is_rtmp_stream":false,"is_joined":false,"need_rejoin":false,"is_owned":false,"can_be_managed":true,"participant_count":3,"has_hidden_listeners":false,"loaded_all_participants":false,"message_sender_id":null,"recent_speakers":[],"is_my_video_enabled":false,"is_my_video_paused":false,"can_enable_video":true,"mute_new_participants":false,"can_toggle_mute_new_participants":true,"can_send_messages":true,"are_messages_allowed":true,"can_toggle_are_messages_allowed":false,"can_delete_messages":false,"record_duration":0,"is_video_recorded":false,"duration":0}}"#,
        req_id.0
    );
    apply_all_seq(&mut session, &sink, &seq, &[&json]);

    // Tracker populated from the bare response…
    let call = session
        .active_group_call
        .as_ref()
        .expect("tracker created from bare groupCall");
    assert_eq!(call.id, 555);
    assert_eq!(call.title, "Demo voice");
    assert!(!call.is_joined);
    // …and the pending request was consumed, not leaked.
    assert!(
        session
            .requests
            .pending_extra_for(
                RequestPurpose::Calls(CallsPurpose::GetGroupCall { group_call_id: 555 }),
                None
            )
            .is_none()
    );
}
