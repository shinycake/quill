//! State reducer tests: calls.
use super::common::*;
use super::*;

#[test]
fn group_call_messages_route_to_tracked_call() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.active_group_call = Some(ActiveGroupCall::fresh(555));
    let new_message = |id: i32, call: i32| {
        format!(
            r#"{{"@type":"updateNewGroupCallMessage","group_call_id":{call},"message":{{"@type":"groupCallMessage","message_id":{id},"sender_id":{{"@type":"messageSenderUser","user_id":41}},"date":1788000000,"text":{{"@type":"formattedText","text":"hello {id}","entities":[]}},"paid_message_star_count":0,"is_from_owner":false,"can_be_deleted":true}}}}"#
        )
    };
    apply_json(&mut session, &seq, &sink, &new_message(1, 555));
    apply_json(&mut session, &seq, &sink, &new_message(2, 999));
    let messages = &session.active_group_call.as_ref().unwrap().messages;
    assert_eq!(messages.len(), 1, "foreign call id must not append");
    assert_eq!(messages[0].message_id, 1);
    assert_eq!(messages[0].text, "hello 1");
    // Re-delivery of the same id dedupes rather than duplicating.
    apply_json(&mut session, &seq, &sink, &new_message(1, 555));
    assert_eq!(
        session.active_group_call.as_ref().unwrap().messages.len(),
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
    let call = session.active_group_call.as_ref().expect("tracked");
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
    });
    session.chats.insert(51, chat);
    session.active_group_call = Some(ActiveGroupCall::fresh(555));
    let extra = session.request(RequestPurpose::GetVideoChatRtmpUrl { chat_id: 51 }, None);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"rtmpUrl","@extra":"{}","url":"rtmp://dc1-rtmp.telegram.org:443/live","stream_key":"secret-key"}}"#,
            extra.0
        ),
    );
    let call = session.active_group_call.as_ref().unwrap();
    assert_eq!(
        call.rtmp_url.as_deref(),
        Some("rtmp://dc1-rtmp.telegram.org:443/live")
    );
    assert_eq!(call.rtmp_stream_key.as_deref(), Some("secret-key"));
}

#[test]
fn call_privacy_get_maps_rules_and_set_failure_clears() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let extra = session.request(
        RequestPurpose::GetCallPrivacyRules {
            setting: CallPrivacySetting::AllowCalls,
        },
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
    assert_eq!(session.call_privacy_allow_calls, Some(PrivacyWho::Contacts));
    assert!(!session.call_privacy_error);

    // Optimistic set, then a TDLib error: the optimistic value is
    // cleared (the next fetch restores the truth) and the error
    // flag is set.
    session.call_privacy_allow_calls = Some(PrivacyWho::Nobody);
    let extra = session.request(
        RequestPurpose::SetCallPrivacyRules {
            setting: CallPrivacySetting::AllowCalls,
        },
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
    assert_eq!(session.call_privacy_allow_calls, None);
    assert!(session.call_privacy_error);
}

#[test]
fn call_privacy_loading_clears_only_after_both_gets_land() {
    // `fetch_call_privacy` fires two gets (AllowCalls + PeerToPeer):
    // clearing on the first would briefly render the radios with
    // nothing selected instead of "Loading…".
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.call_privacy_loading = true;
    session.call_privacy_pending = 2;
    for setting in [
        CallPrivacySetting::AllowCalls,
        CallPrivacySetting::PeerToPeer,
    ] {
        let extra = session.request(RequestPurpose::GetCallPrivacyRules { setting }, None);
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
            assert!(session.call_privacy_loading);
            assert_eq!(session.call_privacy_p2p, None);
        }
    }
    assert!(!session.call_privacy_loading);
    assert_eq!(
        session.call_privacy_allow_calls,
        Some(PrivacyWho::Everybody)
    );
    assert_eq!(session.call_privacy_p2p, Some(PrivacyWho::Everybody));
}
