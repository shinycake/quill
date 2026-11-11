//! State reducer tests: calls.
use super::common::*;
use super::*;

#[test]
fn group_call_messages_route_to_tracked_call() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.calls.active_group_call = Some(ActiveGroupCall::fresh(555));
    let new_message = |id: i32, call: i32| {
        format!(
            r#"{{"@type":"updateNewGroupCallMessage","group_call_id":{call},"message":{{"@type":"groupCallMessage","message_id":{id},"sender_id":{{"@type":"messageSenderUser","user_id":41}},"date":1788000000,"text":{{"@type":"formattedText","text":"hello {id}","entities":[]}},"paid_message_star_count":0,"is_from_owner":false,"can_be_deleted":true}}}}"#
        )
    };
    apply_json(&mut session, &seq, &sink, &new_message(1, 555));
    apply_json(&mut session, &seq, &sink, &new_message(2, 999));
    let messages = &session.calls.active_group_call.as_ref().unwrap().messages;
    assert_eq!(messages.len(), 1, "foreign call id must not append");
    assert_eq!(messages[0].message_id, 1);
    assert_eq!(messages[0].text, "hello 1");
    // Re-delivery of the same id dedupes rather than duplicating.
    apply_json(&mut session, &seq, &sink, &new_message(1, 555));
    assert_eq!(
        session
            .calls
            .active_group_call
            .as_ref()
            .unwrap()
            .messages
            .len(),
        1
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateGroupCallMessagesDeleted","group_call_id":555,"message_ids":[1]}"#,
    );
    assert!(
        session
            .calls
            .active_group_call
            .as_ref()
            .unwrap()
            .messages
            .is_empty()
    );
}

#[test]
fn scheduled_group_call_tracks_start_date() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateGroupCall","group_call":{"@type":"groupCall","id":555,"unique_id":"999","title":"Planned sync","invite_link":"","paid_message_star_count":0,"scheduled_start_date":1788003600,"enabled_start_notification":true,"is_active":false,"is_video_chat":true,"is_live_story":false,"is_rtmp_stream":false,"is_joined":false,"need_rejoin":false,"is_owned":true,"can_be_managed":true,"participant_count":0,"has_hidden_listeners":false,"loaded_all_participants":false,"message_sender_id":null,"recent_speakers":[],"is_my_video_enabled":false,"is_my_video_paused":false,"can_enable_video":true,"mute_new_participants":false,"can_toggle_mute_new_participants":true,"can_send_messages":true,"are_messages_allowed":true,"can_toggle_are_messages_allowed":true,"can_delete_messages":false,"record_duration":0,"is_video_recorded":false,"duration":0}}"#,
    );
    let call = session.calls.active_group_call.as_ref().expect("tracked");
    assert_eq!(call.scheduled_start_date, 1788003600);
    // `enabled_start_notification` (:7154) rides the same update.
    assert!(call.enabled_start_notification);
    assert!(!call.is_joined);
}

#[test]
fn rtmp_url_answer_caches_on_tracked_call() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let mut chat = placeholder_chat(ChatId(51));
    chat.video_chat = Some(VideoChatInfo {
        group_call_id: 555,
        has_participants: false,
        default_participant_id: None,
    });
    session.chats.insert(51, chat);
    session.calls.active_group_call = Some(ActiveGroupCall::fresh(555));
    let extra = session.request(
        RequestPurpose::Calls(CallsPurpose::GetVideoChatRtmpUrl { chat_id: 51 }),
        None,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"rtmpUrl","@extra":"{}","url":"rtmp://dc1-rtmp.telegram.org:443/live","stream_key":"secret-key"}}"#,
            extra.0
        ),
    );
    let call = session.calls.active_group_call.as_ref().unwrap();
    assert_eq!(
        call.rtmp_url.as_deref(),
        Some("rtmp://dc1-rtmp.telegram.org:443/live")
    );
    assert_eq!(call.rtmp_stream_key.as_deref(), Some("secret-key"));
}

#[test]
fn join_as_answer_fills_options_and_preselects_the_saved_default() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let mut chat = placeholder_chat(ChatId(51));
    chat.video_chat = Some(VideoChatInfo {
        group_call_id: 555,
        has_participants: true,
        default_participant_id: Some(MessageSender::Chat { chat_id: -300 }),
    });
    session.chats.insert(51, chat);
    session.calls.active_group_call = Some(ActiveGroupCall::fresh(555));
    let extra = session.request(
        RequestPurpose::Calls(CallsPurpose::GetVideoChatAvailableParticipants {
            group_call_id: 555,
        }),
        None,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"messageSenders","@extra":"{}","total_count":2,"senders":[{{"@type":"messageSenderUser","user_id":7}},{{"@type":"messageSenderChat","chat_id":-300}}]}}"#,
            extra.0
        ),
    );
    let call = session.calls.active_group_call.as_ref().unwrap();
    assert_eq!(call.join_as_options.len(), 2);
    assert_eq!(call.join_as, Some(MessageSender::Chat { chat_id: -300 }));
    // The blocked-users list is untouched by a join-as answer.
    assert!(session.settings.blocked_senders.is_idle());
}

#[test]
fn call_privacy_get_maps_rules_and_set_failure_clears() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let extra = session.request(
        RequestPurpose::Calls(CallsPurpose::GetCallPrivacyRules {
            setting: CallPrivacySetting::AllowCalls,
        }),
        None,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"userPrivacySettingRules","@extra":"{}","rules":[{{"@type":"userPrivacySettingRuleAllowContacts"}}]}}"#,
            extra.0,
        ),
    );
    assert_eq!(
        session.calls.privacy_allow_calls,
        Some(PrivacyWho::Contacts)
    );
    assert!(!session.calls.privacy_error);

    // Optimistic set, then a TDLib error: the optimistic value is
    // cleared (the next fetch restores the truth) and the error
    // flag is set.
    session.calls.privacy_allow_calls = Some(PrivacyWho::Nobody);
    let extra = session.request(
        RequestPurpose::Calls(CallsPurpose::SetCallPrivacyRules {
            setting: CallPrivacySetting::AllowCalls,
        }),
        None,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"PRIVACY_TOO_LONG"}}"#,
            extra.0,
        ),
    );
    assert_eq!(session.calls.privacy_allow_calls, None);
    assert!(session.calls.privacy_error);
}

#[test]
fn call_privacy_loading_clears_only_after_both_gets_land() {
    // `fetch_call_privacy` fires two gets (AllowCalls + PeerToPeer):
    // clearing on the first would briefly render the radios with
    // nothing selected instead of "Loading…".
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.calls.privacy_pending = 2;
    for setting in [
        CallPrivacySetting::AllowCalls,
        CallPrivacySetting::PeerToPeer,
    ] {
        let extra = session.request(
            RequestPurpose::Calls(CallsPurpose::GetCallPrivacyRules { setting }),
            None,
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"userPrivacySettingRules","@extra":"{}","rules":[{{"@type":"userPrivacySettingRuleAllowAll"}}]}}"#,
                extra.0,
            ),
        );
        if setting == CallPrivacySetting::AllowCalls {
            assert!(session.calls.privacy_loading());
            assert_eq!(session.calls.privacy_p2p, None);
        }
    }
    assert!(!session.calls.privacy_loading());
    assert_eq!(
        session.calls.privacy_allow_calls,
        Some(PrivacyWho::Everybody)
    );
    assert_eq!(session.calls.privacy_p2p, Some(PrivacyWho::Everybody));
}

fn swap_pending_json(id: i32, user_id: i64, is_video: bool) -> String {
    format!(
        r#"{{"@type":"updateCall","call":{{"@type":"call","id":{id},"unique_id":"99","user_id":{user_id},"is_outgoing":false,"is_video":{is_video},"state":{{"@type":"callStatePending","is_created":true,"is_received":false}}}}}}"#
    )
}

fn swap_active_json(id: i32, user_id: i64) -> String {
    format!(
        r#"{{"@type":"updateCall","call":{{"@type":"call","id":{id},"unique_id":"98","user_id":{user_id},"is_outgoing":true,"is_video":false,"state":{{"@type":"callStateExchangingKeys"}}}}}}"#
    )
}

fn swap_discarded_json(id: i32, user_id: i64) -> String {
    format!(
        r#"{{"@type":"updateCall","call":{{"@type":"call","id":{id},"unique_id":"97","user_id":{user_id},"is_outgoing":false,"is_video":false,"state":{{"@type":"callStateDiscarded","reason":{{"@type":"callDiscardReasonHungUp"}},"need_rating":false,"need_debug_information":false,"need_log":false}}}}}}"#
    )
}

#[test]
fn incoming_while_active_raises_swap_prompt() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(&mut session, &seq, &sink, &swap_active_json(77, 41));
    assert!(session.calls.active_call.is_some());
    apply_json(&mut session, &seq, &sink, &swap_pending_json(78, 42, false));
    assert_eq!(session.calls.second_call.prompt(), Some((78, 42, false)));
    assert!(
        session.calls.busy_decline_queue.is_empty(),
        "first incoming raises the prompt, not the busy queue"
    );
}

#[test]
fn second_incoming_while_prompt_open_busy_declines() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(&mut session, &seq, &sink, &swap_active_json(77, 41));
    apply_json(&mut session, &seq, &sink, &swap_pending_json(78, 42, false));
    apply_json(&mut session, &seq, &sink, &swap_pending_json(79, 43, true));
    assert_eq!(session.calls.second_call.prompt(), Some((78, 42, false)));
    assert_eq!(session.calls.busy_decline_queue, vec![(79, 43, true)]);
}

#[test]
fn caller_hangup_clears_swap_prompt() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(&mut session, &seq, &sink, &swap_active_json(77, 41));
    apply_json(&mut session, &seq, &sink, &swap_pending_json(78, 42, false));
    apply_json(&mut session, &seq, &sink, &swap_discarded_json(78, 42));
    assert_eq!(session.calls.second_call.prompt(), None);
    assert!(session.calls.active_call.is_some(), "active call untouched");
}

#[test]
fn active_call_end_rings_the_second_call() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(&mut session, &seq, &sink, &swap_active_json(77, 41));
    apply_json(&mut session, &seq, &sink, &swap_pending_json(78, 42, true));
    apply_json(&mut session, &seq, &sink, &swap_discarded_json(77, 41));
    assert_eq!(session.calls.second_call, crate::state::SecondCall::None);
    // The caller still rings and TDLib does not resend its pending
    // update: it becomes the incoming call.
    assert_eq!(
        crate::notify_accounts::ringing_incoming_call(&session),
        Some((78, 42, true))
    );
    // Its later updates move it on as the tracked call.
    apply_json(&mut session, &seq, &sink, &swap_discarded_json(78, 42));
    assert!(session.calls.active_call.is_none());
    assert_eq!(session.calls.summary.as_ref().map(|s| s.call_id), Some(78));
}

#[test]
fn swap_accept_queued_survives_active_call_end() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(&mut session, &seq, &sink, &swap_active_json(77, 41));
    apply_json(&mut session, &seq, &sink, &swap_pending_json(78, 42, false));
    // User chose "End & answer": prompt moves to the accept queue.
    let queued = crate::state::SecondCall::AcceptAfterHangup {
        call_id: 78,
        user_id: 42,
        is_video: false,
    };
    session.calls.second_call = queued;
    apply_json(&mut session, &seq, &sink, &swap_discarded_json(77, 41));
    assert_eq!(session.calls.second_call, queued);
    assert!(session.calls.active_call.is_none());
}

fn hanging_up(session: &mut Session, call_id: i32) -> RequestId {
    let request = session.request(RequestPurpose::DiscardCall, None);
    let call = session.calls.active_call.as_mut().expect("active call");
    let resume = std::mem::replace(&mut call.state, CallState::HangingUp);
    session.calls.hang_up = Some(crate::state::HangUp::new(request, call_id, resume));
    request
}

fn error_json(request: RequestId) -> String {
    format!(
        r#"{{"@type":"error","@extra":"{}","code":400,"message":"CALL_PEER_INVALID"}}"#,
        request.as_extra()
    )
}

fn hanging_up_update_json(id: i32, user_id: i64) -> String {
    format!(
        r#"{{"@type":"updateCall","call":{{"@type":"call","id":{id},"unique_id":"98","user_id":{user_id},"is_outgoing":true,"is_video":false,"state":{{"@type":"callStateHangingUp"}}}}}}"#
    )
}

#[test]
fn non_terminal_update_keeps_a_hang_up_in_flight() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(&mut session, &seq, &sink, &swap_pending_json(78, 42, false));
    let request = hanging_up(&mut session, 78);
    // An update queued before the discard lands.
    apply_json(&mut session, &seq, &sink, &swap_active_json(78, 42));
    let state = |s: &Session| s.calls.active_call.as_ref().map(|c| c.state.clone());
    assert_eq!(state(&session), Some(CallState::HangingUp));
    // The hang-up fails: the call is back in the phase it moved to.
    apply_json(&mut session, &seq, &sink, &error_json(request));
    assert_eq!(state(&session), Some(CallState::ExchangingKeys));
    assert!(session.calls.hang_up.is_none());
    assert!(session.calls.error.is_some());
}

#[test]
fn failed_hang_up_reverts_and_brings_the_swap_prompt_back() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(&mut session, &seq, &sink, &swap_active_json(77, 41));
    apply_json(&mut session, &seq, &sink, &swap_pending_json(78, 42, false));
    session.calls.second_call = crate::state::SecondCall::AcceptAfterHangup {
        call_id: 78,
        user_id: 42,
        is_video: false,
    };
    let request = hanging_up(&mut session, 77);
    apply_json(&mut session, &seq, &sink, &error_json(request));
    let call = session.calls.active_call.as_ref().expect("call goes on");
    assert_eq!(
        (call.id, call.state.clone()),
        (77, CallState::ExchangingKeys)
    );
    assert_eq!(session.calls.second_call.prompt(), Some((78, 42, false)));
}

#[test]
fn other_discard_errors_leave_the_hang_up_alone() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(&mut session, &seq, &sink, &swap_active_json(77, 41));
    hanging_up(&mut session, 77);
    // A busy decline of some other call fails.
    let other = session.request(RequestPurpose::DiscardCall, None);
    apply_json(&mut session, &seq, &sink, &error_json(other));
    let call = session.calls.active_call.as_ref().expect("active call");
    assert_eq!(call.state, CallState::HangingUp);
    assert!(session.calls.hang_up.is_some());
    // TDLib's own hanging-up update is kept as the phase to go back to.
    apply_json(&mut session, &seq, &sink, &hanging_up_update_json(77, 41));
    assert_eq!(
        session.calls.hang_up.as_ref().map(|h| h.resume.clone()),
        Some(CallState::HangingUp)
    );
}

fn rated_summary(session: &mut Session, seq: &AtomicU64, sink: &Arc<MemorySink>) {
    apply_json(session, seq, sink, &swap_active_json(77, 41));
    apply_json(
        session,
        seq,
        sink,
        r#"{"@type":"updateCall","call":{"@type":"call","id":77,"unique_id":"97","user_id":41,"is_outgoing":false,"is_video":false,"state":{"@type":"callStateDiscarded","reason":{"@type":"callDiscardReasonHungUp"},"need_rating":true,"need_debug_information":false,"need_log":false}}}"#,
    );
}

fn rating(session: &Session) -> Option<CallRating> {
    Some(session.calls.summary.as_ref()?.rating.clone())
}

#[test]
fn a_failed_rating_brings_the_card_back_and_an_ok_says_thanks() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    rated_summary(&mut session, &seq, &sink);
    let request = session.request(RequestPurpose::SendCallRating, None);
    session.calls.summary.as_mut().expect("summary").rating = CallRating::Sending(request);
    apply_json(&mut session, &seq, &sink, &error_json(request));
    let failed = rating(&session).expect("summary");
    assert!(matches!(failed, CallRating::Failed(_)));
    assert!(failed.is_open(), "the card is drawn again");
    // The retry is answered ok.
    let retry = session.request(RequestPurpose::SendCallRating, None);
    session.calls.summary.as_mut().expect("summary").rating = CallRating::Sending(retry);
    // An answer to the first, stale request changes nothing.
    apply_json(&mut session, &seq, &sink, &error_json(request));
    assert_eq!(rating(&session), Some(CallRating::Sending(retry)));
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, retry.as_extra()),
    );
    assert_eq!(rating(&session), Some(CallRating::Sent));
}

#[test]
fn a_failed_busy_decline_does_not_blame_the_active_call() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(&mut session, &seq, &sink, &swap_active_json(77, 41));
    let decline = session.request(RequestPurpose::DiscardCall, None);
    apply_json(&mut session, &seq, &sink, &error_json(decline));
    assert!(session.calls.error.is_none());
    // The active call's own hang-up still reports.
    let request = hanging_up(&mut session, 77);
    apply_json(&mut session, &seq, &sink, &error_json(request));
    assert!(session.calls.error.is_some());
}
