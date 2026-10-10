use super::*;

const SOUND_FILE: &str = r#"{"@type":"file","id":77,"size":12,"expected_size":12,"local":{"@type":"localFile","path":"","can_be_downloaded":true,"can_be_deleted":false,"is_downloading_active":false,"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0},"remote":{"@type":"remoteFile","id":"r","unique_id":"u","is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":12}}"#;

#[test]
fn notification_sounds_parsed() {
    // `getSavedNotificationSounds` response (schema 1.8.67 lines 8857–8860).
    let json = format!(
        r#"{{"@type":"notificationSounds","notification_sounds":[{{"@type":"notificationSound","id":99,"duration":2,"date":1700000000,"title":"Chime","data":"","sound":{}}}]}}"#,
        SOUND_FILE
    );
    let env = parse_envelope(&json).unwrap();
    match env.payload {
        EnvelopePayload::Settings(SettingsPayload::NotificationSounds { sounds }) => {
            assert_eq!(sounds.len(), 1);
            assert_eq!(sounds[0].id, 99);
            assert_eq!(sounds[0].title, "Chime");
            assert_eq!(sounds[0].duration, 2);
            assert_eq!(sounds[0].sound.id.0, 77);
        }
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn update_saved_notification_sounds_parsed() {
    // Schema 1.8.67 line 10947.
    let env = parse_envelope(
        r#"{"@type":"updateSavedNotificationSounds","notification_sound_ids":[7,8]}"#,
    )
    .unwrap();
    match env.payload {
        EnvelopePayload::Settings(SettingsPayload::UpdateSavedNotificationSounds { sound_ids }) => {
            assert_eq!(sound_ids, vec![7, 8]);
        }
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn scope_notification_settings_parsed() {
    // Schema 1.8.67 line 3375.
    let env = parse_envelope(
            r#"{"@type":"scopeNotificationSettings","mute_for":3600,"sound_id":-1,"show_preview":true,"use_default_mute_stories":true,"mute_stories":false,"story_sound_id":-1,"show_story_poster":true,"disable_pinned_message_notifications":false,"disable_mention_notifications":true}"#,
        )
        .unwrap();
    match env.payload {
        EnvelopePayload::Settings(SettingsPayload::ScopeNotificationSettings {
            settings, ..
        }) => {
            assert_eq!(settings.mute_for, 3600);
            assert_eq!(settings.sound_id, -1);
            assert!(settings.show_preview);
            assert!(settings.disable_mention_notifications);
        }
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn update_scope_notification_settings_parsed() {
    // Schema 1.8.67 line 10668; scope constructor lines 3337–3343.
    let env = parse_envelope(
            r#"{"@type":"updateScopeNotificationSettings","scope":{"@type":"notificationSettingsScopeGroupChats"},"notification_settings":{"@type":"scopeNotificationSettings","mute_for":0,"sound_id":0,"show_preview":false,"use_default_mute_stories":true,"mute_stories":false,"story_sound_id":-1,"show_story_poster":true,"disable_pinned_message_notifications":false,"disable_mention_notifications":false}}"#,
        )
        .unwrap();
    match env.payload {
        EnvelopePayload::Settings(SettingsPayload::UpdateScopeNotificationSettings {
            scope,
            settings,
        }) => {
            assert_eq!(scope, NotificationSettingsScope::GroupChats);
            assert_eq!(settings.sound_id, 0);
            assert!(!settings.show_preview);
        }
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn update_reaction_notification_settings_parsed() {
    // Schema 1.8.67 lines 3396, 10671.
    let env = parse_envelope(
            r#"{"@type":"updateReactionNotificationSettings","notification_settings":{"@type":"reactionNotificationSettings","message_reaction_source":{"@type":"reactionNotificationSourceContacts"},"story_reaction_source":{"@type":"reactionNotificationSourceAll"},"poll_vote_source":{"@type":"reactionNotificationSourceNone"},"sound_id":-1,"show_preview":true}}"#,
        )
        .unwrap();
    match env.payload {
        EnvelopePayload::Settings(SettingsPayload::UpdateReactionNotificationSettings {
            settings,
        }) => {
            assert_eq!(
                settings.message_reaction_source,
                ReactionNotificationSource::Contacts
            );
            assert_eq!(
                settings.story_reaction_source,
                ReactionNotificationSource::All
            );
            assert_eq!(settings.poll_vote_source, ReactionNotificationSource::None);
            assert_eq!(settings.sound_id, -1);
            assert!(settings.show_preview);
        }
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn unknown_scope_is_rejected() {
    assert_eq!(
        parse_notification_settings_scope(Some("notificationSettingsScopeBots")),
        None
    );
    assert_eq!(
        parse_notification_settings_scope(Some("notificationSettingsScopePrivateChats")),
        Some(NotificationSettingsScope::PrivateChats)
    );
}

#[test]
fn schema_pins_call_constructors() {
    // Every constructor this slice relies on must exist verbatim in
    // the pinned schema (1.8.67) — never invent constructors or
    // fields. Lines: updateCall :10816, updateNewCallSignalingData
    // :10862, callId :7034, createCall :14212, acceptCall :14215,
    // sendCallSignalingData :14218, discardCall :14227,
    // sendCallRating :14234, sendCallDebugInformation :14237,
    // call :7287, callProtocol :7008, states :7058–7086,
    // discard reasons :6984–6999, problems :7253–7277.
    let schema = include_str!("../../../schema/td_api.tl");
    for line in [
        "updateCall call:call = Update;",
        "updateNewCallSignalingData call_id:int32 data:bytes = Update;",
        "callId id:int32 = CallId;",
        "call id:int32 unique_id:int64 user_id:int53 is_outgoing:Bool is_video:Bool state:CallState = Call;",
        "callProtocol udp_p2p:Bool udp_reflector:Bool min_layer:int32 max_layer:int32 library_versions:vector<string> = CallProtocol;",
        "createCall user_id:int53 protocol:callProtocol is_video:Bool = CallId;",
        "toggleVideoChatEnabledStartNotification group_call_id:int32 enabled_start_notification:Bool = Ok;",
        "acceptCall call_id:int32 protocol:callProtocol = Ok;",
        "sendCallSignalingData call_id:int32 data:bytes = Ok;",
        "discardCall call_id:int32 is_disconnected:Bool invite_link:string duration:int32 is_video:Bool connection_id:int64 = Ok;",
        "sendCallRating call_id:InputCall rating:int32 comment:string problems:vector<CallProblem> = Ok;",
        "sendCallDebugInformation call_id:InputCall debug_information:string = Ok;",
        "callStatePending is_created:Bool is_received:Bool = CallState;",
        "callStateExchangingKeys = CallState;",
        "callStateHangingUp = CallState;",
        "callStateDiscarded reason:CallDiscardReason need_rating:Bool need_debug_information:Bool need_log:Bool = CallState;",
        "callDiscardReasonMissed = CallDiscardReason;",
        "callDiscardReasonDeclined = CallDiscardReason;",
        "callDiscardReasonHungUp = CallDiscardReason;",
        "inputCallDiscarded call_id:int32 = InputCall;",
    ] {
        assert!(
            schema.lines().any(|l| l == line),
            "schema pin missing: {line}"
        );
    }
    for prefix in [
        "callStateReady protocol:callProtocol",
        "callStateError error:error = CallState;",
        "callDiscardReasonEmpty = CallDiscardReason;",
    ] {
        assert!(
            schema.lines().any(|l| l.starts_with(prefix)),
            "schema pin missing: {prefix}"
        );
    }
}

/// Phase C1: `updateCall` parses the full `call` record in every
/// state; `updateNewCallSignalingData` keeps base64 bytes; `callId`
/// is the `createCall` answer.
#[test]
fn call_updates_parsed_in_every_state() {
    let pending = r#"{"@type":"updateCall","call":{"@type":"call","id":77,"unique_id":"99","user_id":41,"is_outgoing":false,"is_video":false,"state":{"@type":"callStatePending","is_created":true,"is_received":false}}}"#;
    match parse_envelope(pending).unwrap().payload {
        EnvelopePayload::Calls(CallsPayload::UpdateCall { call }) => {
            assert_eq!(call.id, 77);
            assert_eq!(call.unique_id, 99);
            assert_eq!(call.user_id, 41);
            assert!(!call.is_outgoing);
            assert!(!call.is_video);
            assert_eq!(
                call.state,
                CallState::Pending {
                    is_created: true,
                    is_received: false
                }
            );
            assert!(!call.state.is_terminal());
        }
        other => panic!("unexpected {other:?}"),
    }
    // Phase C1b: a video `updateCall` parses `is_video: true` (the
    // schema's `call` type carries it, 1.8.67 :7287).
    let video = r#"{"@type":"updateCall","call":{"@type":"call","id":83,"unique_id":"105","user_id":41,"is_outgoing":false,"is_video":true,"state":{"@type":"callStateReady","protocol":{"@type":"callProtocol","udp_p2p":false,"udp_reflector":false,"min_layer":65,"max_layer":92,"library_versions":[]},"servers":[],"config":"{}","encryption_key":"","emojis":[],"allow_p2p":false,"is_group_call_supported":false,"custom_parameters":"{}"}}}"#;
    match parse_envelope(video).unwrap().payload {
        EnvelopePayload::Calls(CallsPayload::UpdateCall { call }) => {
            assert!(call.is_video);
            assert!(!call.is_outgoing);
            assert!(matches!(call.state, CallState::Ready));
        }
        other => panic!("unexpected {other:?}"),
    }
    for (state_json, terminal) in [
        (r#"{"@type":"callStateExchangingKeys"}"#, false),
        (
            r#"{"@type":"callStateReady","protocol":{"@type":"callProtocol","udp_p2p":true,"udp_reflector":true,"min_layer":65,"max_layer":92,"library_versions":[]},"servers":[],"config":"{}","encryption_key":"","emojis":[],"allow_p2p":false,"is_group_call_supported":false,"custom_parameters":"{}"}"#,
            false,
        ),
        (r#"{"@type":"callStateHangingUp"}"#, false),
        (
            r#"{"@type":"callStateDiscarded","reason":{"@type":"callDiscardReasonHungUp"},"need_rating":true,"need_debug_information":false,"need_log":false}"#,
            true,
        ),
        (
            r#"{"@type":"callStateError","error":{"@type":"error","code":4005000,"message":"CALL_TIMEOUT"}}"#,
            true,
        ),
        (r#"{"@type":"callStateFuture"}"#, false),
    ] {
        let json = format!(
            r#"{{"@type":"updateCall","call":{{"@type":"call","id":78,"unique_id":"100","user_id":41,"is_outgoing":true,"is_video":false,"state":{state_json}}}}}"#
        );
        match parse_envelope(&json).unwrap().payload {
            EnvelopePayload::Calls(CallsPayload::UpdateCall { call }) => {
                assert!(call.is_outgoing);
                assert_eq!(call.state.is_terminal(), terminal, "for {state_json}");
            }
            other => panic!("unexpected {other:?}"),
        }
    }
    // `callStateError` keeps the numeric code only — TDLib error
    // message text is never stored (it can contain secrets). Parse a
    // message that would leak if retained and assert it is gone.
    let err = r#"{"@type":"updateCall","call":{"@type":"call","id":82,"unique_id":"104","user_id":41,"is_outgoing":false,"is_video":false,"state":{"@type":"callStateError","error":{"@type":"error","code":500,"message":"SECRET_LEAK_TEXT"}}}}"#;
    match parse_envelope(err).unwrap().payload {
        EnvelopePayload::Calls(CallsPayload::UpdateCall { call }) => {
            assert_eq!(call.state, CallState::Error { code: 500 });
            assert!(
                !format!("{call:?}").contains("SECRET_LEAK_TEXT"),
                "TDLib error message text must not be retained"
            );
        }
        other => panic!("unexpected {other:?}"),
    }
    // Discard reason summaries.
    assert_eq!(CallDiscardReason::Missed.summary(false), "Missed call");
    assert_eq!(CallDiscardReason::Missed.summary(true), "Call not answered");
    assert_eq!(
        CallDiscardReason::Declined.summary(false),
        "You declined the call"
    );
    assert_eq!(CallDiscardReason::Declined.summary(true), "Declined");
    // Signaling data arrives as base64 bytes.
    let sig = r#"{"@type":"updateNewCallSignalingData","call_id":77,"data":"AAEC"}"#;
    match parse_envelope(sig).unwrap().payload {
        EnvelopePayload::Calls(CallsPayload::UpdateNewCallSignalingData { call_id, data }) => {
            assert_eq!(call_id, 77);
            assert_eq!(data, vec![0x00, 0x01, 0x02]);
        }
        other => panic!("unexpected {other:?}"),
    }
    // `callId` is the `createCall` answer.
    let id = r#"{"@type":"callId","id":77,"@extra":"9"}"#;
    match parse_envelope(id).unwrap().payload {
        EnvelopePayload::Calls(CallsPayload::CallId { id }) => assert_eq!(id, 77),
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn call_ready_parses_transport_parameters_and_server_kinds() {
    let json = r#"{"@type":"updateCall","call":{"@type":"call","id":77,"unique_id":"99","user_id":41,"is_outgoing":true,"is_video":false,"state":{"@type":"callStateReady","protocol":{"@type":"callProtocol","udp_p2p":true,"udp_reflector":true,"min_layer":92,"max_layer":92,"library_versions":["13.0.0"]},"servers":[{"@type":"callServer","id":"7","ip_address":"149.154.167.40","ipv6_address":"2001:b28:f23d:f001::a","port":443,"type":{"@type":"callServerTypeTelegramReflector","peer_tag":"AAEC","is_tcp":true}},{"@type":"callServer","id":"8","ip_address":"203.0.113.1","ipv6_address":"","port":3478,"type":{"@type":"callServerTypeWebrtc","username":"alice","password":"secret","supports_turn":true,"supports_stun":false}}],"config":"{}","encryption_key":"AQIDBA==","emojis":["🍎","🍌"],"allow_p2p":true,"is_group_call_supported":false,"custom_parameters":"{\"x\":1}"}}}"#;
    let EnvelopePayload::Calls(CallsPayload::UpdateCall { call }) =
        parse_envelope(json).unwrap().payload
    else {
        panic!("expected updateCall");
    };
    assert_eq!(call.state, CallState::Ready);
    let ready = call.ready.expect("ready params");
    assert_eq!(ready.encryption_key, vec![1, 2, 3, 4]);
    assert!(ready.allow_p2p);
    assert_eq!(ready.servers.len(), 2);
    assert_eq!(ready.servers[0].peer_tag, vec![0, 1, 2]);
    assert!(ready.servers[0].tcp);
    assert!(ready.servers[1].turn);
    assert!(!ready.servers[1].stun);
    assert_eq!(ready.servers[1].username, "alice");
    // `callStateReady.emojis` (:7068) — the 1:1 E2E fingerprint.
    assert_eq!(ready.emojis, vec!["🍎".to_string(), "🍌".to_string()]);
}

#[test]
fn schema_pins_notification_sound_constructors() {
    // Every constructor this slice relies on must exist verbatim in the
    // pinned schema (1.8.67) — never invent constructors or fields.
    let schema = include_str!("../../../schema/td_api.tl");
    for line in [
        "notificationSound id:int64 duration:int32 date:int32 title:string data:string sound:file = NotificationSound;",
        "notificationSounds notification_sounds:vector<notificationSound> = NotificationSounds;",
        "updateSavedNotificationSounds notification_sound_ids:vector<int64> = Update;",
        "getSavedNotificationSound notification_sound_id:int64 = NotificationSound;",
        "getSavedNotificationSounds = NotificationSounds;",
        "addSavedNotificationSound sound:InputFile = NotificationSound;",
        "removeSavedNotificationSound notification_sound_id:int64 = Ok;",
        "fileTypeNotificationSound = FileType;",
    ] {
        assert!(
            schema.lines().any(|l| l == line),
            "schema pin missing: {line}"
        );
    }
}
