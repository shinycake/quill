use super::*;
use serde_json::Value;

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

/// Phase D3a: `chatInviteLink` parses every schema field (TDLib 1.8.67,
/// `schema/td_api.tl:2627`), including `starSubscriptionPricing`
/// (line 1252) when present.
#[test]
fn invite_link_parses_all_fields() {
    let json = r#"{"@type":"chatInviteLink","invite_link":"https://t.me/+paid","name":"Quill","creator_user_id":101,"date":1700000000,"edit_date":1700000001,"expiration_date":1800000000,"subscription_pricing":{"@type":"starSubscriptionPricing","period":2592000,"star_count":250},"member_limit":50,"member_count":12,"expired_member_count":3,"pending_join_request_count":4,"creates_join_request":true,"is_primary":false,"is_revoked":false}"#;
    let env = parse_envelope(json).unwrap();
    match env.payload {
        EnvelopePayload::Groups(GroupsPayload::ChatInviteLink { link }) => {
            assert_eq!(link.invite_link, "https://t.me/+paid");
            assert_eq!(link.name, "Quill");
            assert_eq!(link.creator_user_id, 101);
            assert_eq!(link.date, 1_700_000_000);
            assert_eq!(link.edit_date, 1_700_000_001);
            assert_eq!(link.expiration_date, 1_800_000_000);
            assert_eq!(
                link.subscription_pricing,
                Some(StarSubscriptionPricing {
                    period: 2_592_000,
                    star_count: 250,
                })
            );
            assert_eq!(link.member_limit, 50);
            assert_eq!(link.member_count, 12);
            assert_eq!(link.expired_member_count, 3);
            assert_eq!(link.pending_join_request_count, 4);
            assert!(link.creates_join_request);
            assert!(!link.is_primary);
            assert!(!link.is_revoked);
        }
        other => panic!("unexpected {other:?}"),
    }
}

/// Phase D3a: links without `subscription_pricing` parse to `None`.
#[test]
fn invite_link_without_subscription_pricing() {
    let json = r#"{"@type":"chatInviteLink","invite_link":"https://t.me/+free","name":"","creator_user_id":101,"date":1700000000,"edit_date":0,"expiration_date":0,"member_limit":0,"member_count":12,"expired_member_count":0,"pending_join_request_count":0,"creates_join_request":false,"is_primary":true,"is_revoked":false}"#;
    let env = parse_envelope(json).unwrap();
    match env.payload {
        EnvelopePayload::Groups(GroupsPayload::ChatInviteLink { link }) => {
            assert_eq!(link.invite_link, "https://t.me/+free");
            assert_eq!(link.subscription_pricing, None);
            assert!(link.is_primary);
        }
        other => panic!("unexpected {other:?}"),
    }
}

/// Phase D3a: `chatInviteLinks` list (line 2630) and `chatJoinRequests`
/// list (line 2691).
#[test]
fn invite_links_and_join_requests_lists_parse() {
    let json = r#"{"@type":"chatInviteLinks","total_count":2,"invite_links":[{"@type":"chatInviteLink","invite_link":"https://t.me/+one","name":"One","creator_user_id":101,"date":1700000000,"edit_date":0,"expiration_date":0,"member_limit":0,"member_count":5,"expired_member_count":0,"pending_join_request_count":1,"creates_join_request":false,"is_primary":false,"is_revoked":false},{"@type":"chatInviteLink","invite_link":"https://t.me/+two","name":"Two","creator_user_id":101,"date":1700000000,"edit_date":0,"expiration_date":0,"member_limit":10,"member_count":0,"expired_member_count":0,"pending_join_request_count":0,"creates_join_request":true,"is_primary":false,"is_revoked":true}]}"#;
    let env = parse_envelope(json).unwrap();
    match env.payload {
        EnvelopePayload::Groups(GroupsPayload::ChatInviteLinks { total_count, links }) => {
            assert_eq!(total_count, 2);
            assert_eq!(links.len(), 2);
            assert_eq!(links[0].invite_link, "https://t.me/+one");
            assert_eq!(links[0].pending_join_request_count, 1);
            assert!(links[1].is_revoked);
            assert!(links[1].creates_join_request);
        }
        other => panic!("unexpected {other:?}"),
    }
    let json = r#"{"@type":"chatJoinRequests","total_count":1,"requests":[{"@type":"chatJoinRequest","user_id":7001,"date":1700000100,"bio":"Hello from Quill"}]}"#;
    let env = parse_envelope(json).unwrap();
    match env.payload {
        EnvelopePayload::Groups(GroupsPayload::ChatJoinRequests {
            total_count,
            requests,
        }) => {
            assert_eq!(total_count, 1);
            assert_eq!(
                requests,
                vec![ParsedChatJoinRequest {
                    user_id: 7001,
                    date: 1_700_000_100,
                    bio: "Hello from Quill".to_owned(),
                }]
            );
        }
        other => panic!("unexpected {other:?}"),
    }
}

/// Phase D3a: `updateNewChatJoinRequest` (line 11210) and
/// `updateChatPendingJoinRequests` (line 10555).
#[test]
fn join_request_updates_parse() {
    let json = r#"{"@type":"updateNewChatJoinRequest","chat_id":-1001234567890,"request":{"@type":"chatJoinRequest","user_id":7002,"date":1700000200,"bio":"Please let me in"},"user_chat_id":9002,"invite_link":{"@type":"chatInviteLink","invite_link":"https://t.me/+request","name":"","creator_user_id":101,"date":1700000000,"edit_date":0,"expiration_date":0,"member_limit":0,"member_count":0,"expired_member_count":0,"pending_join_request_count":0,"creates_join_request":true,"is_primary":false,"is_revoked":false},"query_id":8000000000}"#;
    let env = parse_envelope(json).unwrap();
    match env.payload {
        EnvelopePayload::Groups(GroupsPayload::UpdateNewChatJoinRequest {
            chat_id,
            request,
            user_chat_id,
            invite_link,
            query_id,
        }) => {
            assert_eq!(chat_id, -1001234567890);
            assert_eq!(request.user_id, 7002);
            assert_eq!(request.bio, "Please let me in");
            assert_eq!(user_chat_id, 9002);
            assert_eq!(invite_link.invite_link, "https://t.me/+request");
            assert_eq!(query_id, 8_000_000_000);
        }
        other => panic!("unexpected {other:?}"),
    }
    let json = r#"{"@type":"updateChatPendingJoinRequests","chat_id":-1001234567890,"pending_join_requests":{"@type":"chatJoinRequestsInfo","total_count":3,"user_ids":[7001,7003]}}"#;
    let env = parse_envelope(json).unwrap();
    match env.payload {
        EnvelopePayload::Groups(GroupsPayload::UpdateChatPendingJoinRequests {
            chat_id,
            total_count,
            user_ids,
        }) => {
            assert_eq!(chat_id, -1001234567890);
            assert_eq!(total_count, 3);
            assert_eq!(user_ids, vec![7001, 7003]);
        }
        other => panic!("unexpected {other:?}"),
    }
}

/// Phase D3a: every constructor this slice relies on must exist verbatim
/// in the pinned schema (1.8.67) — never invent constructors or fields.
/// (`deleteChatInviteLink` is deliberately absent: revocation is the
/// only delete path in this schema version.)
#[test]
fn d3a_schema_pins_exist_verbatim() {
    let schema = include_str!("../../../schema/td_api.tl");
    for line in [
        "chatInviteLink invite_link:string name:string creator_user_id:int53 date:int32 edit_date:int32 expiration_date:int32 subscription_pricing:starSubscriptionPricing member_limit:int32 member_count:int32 expired_member_count:int32 pending_join_request_count:int32 creates_join_request:Bool is_primary:Bool is_revoked:Bool = ChatInviteLink;",
        "chatInviteLinks total_count:int32 invite_links:vector<chatInviteLink> = ChatInviteLinks;",
        "chatJoinRequest user_id:int53 date:int32 bio:string = ChatJoinRequest;",
        "chatJoinRequests total_count:int32 requests:vector<chatJoinRequest> = ChatJoinRequests;",
        "chatJoinRequestsInfo total_count:int32 user_ids:vector<int53> = ChatJoinRequestsInfo;",
        "starSubscriptionPricing period:int32 star_count:int53 = StarSubscriptionPricing;",
        "getChatInviteLinks chat_id:int53 creator_user_id:int53 is_revoked:Bool offset_date:int32 offset_invite_link:string limit:int32 = ChatInviteLinks;",
        "createChatInviteLink chat_id:int53 name:string expiration_date:int32 member_limit:int32 creates_join_request:Bool = ChatInviteLink;",
        "editChatInviteLink chat_id:int53 invite_link:string name:string expiration_date:int32 member_limit:int32 creates_join_request:Bool = ChatInviteLink;",
        "revokeChatInviteLink chat_id:int53 invite_link:string = ChatInviteLinks;",
        "getChatJoinRequests chat_id:int53 invite_link:string query:string offset_request:chatJoinRequest limit:int32 = ChatJoinRequests;",
        "processChatJoinRequest chat_id:int53 user_id:int53 approve:Bool = Ok;",
        "updateChatPendingJoinRequests chat_id:int53 pending_join_requests:chatJoinRequestsInfo = Update;",
        "updateNewChatJoinRequest chat_id:int53 request:chatJoinRequest user_chat_id:int53 invite_link:chatInviteLink query_id:int64 = Update;",
    ] {
        assert!(
            schema.lines().any(|l| l == line),
            "schema pin missing: {line}"
        );
    }
    assert!(
        !schema
            .lines()
            .any(|l| l.starts_with("deleteChatInviteLink ")),
        "deleteChatInviteLink must not exist in 1.8.67"
    );
}

/// Phase D3b: `chatAdministrators` (line 2485) parses owner +
/// administrators with custom titles and `can_be_edited` flags.
#[test]
fn chat_administrators_list_parses() {
    let json = r#"{"@type":"chatAdministrators","administrators":[{"@type":"chatAdministrator","user_id":777,"custom_title":"","is_owner":true,"can_be_edited":false},{"@type":"chatAdministrator","user_id":888,"custom_title":"News Desk","is_owner":false,"can_be_edited":true},{"@type":"chatAdministrator","user_id":999,"custom_title":"","is_owner":false,"can_be_edited":false}]}"#;
    let env = parse_envelope(json).unwrap();
    match env.payload {
        EnvelopePayload::Groups(GroupsPayload::ChatAdministrators { administrators }) => {
            assert_eq!(administrators.len(), 3);
            assert!(administrators[0].is_owner);
            assert_eq!(administrators[0].user_id, 777);
            assert_eq!(administrators[1].custom_title, "News Desk");
            assert!(administrators[1].can_be_edited);
            assert!(!administrators[2].can_be_edited);
        }
        other => panic!("unexpected {other:?}"),
    }
}

/// Phase D3b: `chatMembers` (line 2529) parses, with an administrator
/// member carrying the full rights block.
#[test]
fn chat_members_list_parses_with_admin_rights() {
    let json = r#"{"@type":"chatMembers","total_count":2,"members":[{"@type":"chatMember","member_id":{"@type":"messageSenderUser","user_id":888},"tag":"","inviter_user_id":777,"joined_chat_date":1700000000,"status":{"@type":"chatMemberStatusAdministrator","can_be_edited":true,"rights":{"@type":"chatAdministratorRights","can_manage_chat":true,"can_change_info":true,"can_post_messages":true,"can_edit_messages":true,"can_delete_messages":true,"can_invite_users":true,"can_restrict_members":true,"can_pin_messages":true,"can_manage_topics":false,"can_promote_members":true,"can_manage_video_chats":true,"can_post_stories":true,"can_edit_stories":true,"can_delete_stories":true,"can_manage_direct_messages":false,"can_manage_tags":false,"can_send_welcome_messages":false,"is_anonymous":false}}},{"@type":"chatMember","member_id":{"@type":"messageSenderUser","user_id":111},"tag":"","inviter_user_id":777,"joined_chat_date":1700000100,"status":{"@type":"chatMemberStatusMember","member_until_date":0}}]}"#;
    let env = parse_envelope(json).unwrap();
    match env.payload {
        EnvelopePayload::Groups(GroupsPayload::SupergroupMembers {
            members,
            total_count,
        }) => {
            assert_eq!(total_count, 2);
            assert_eq!(members.len(), 2);
            let admin = &members[0];
            assert_eq!(admin.status, ChannelMemberStatus::Administrator);
            let rights = admin.admin_rights.expect("admin rights parsed");
            assert!(rights.can_promote_members);
            assert!(rights.can_invite_users);
            assert!(!rights.can_manage_topics);
            assert!(!rights.is_anonymous);
            assert_eq!(admin.admin_can_invite_users, Some(true));
            assert_eq!(members[1].status, ChannelMemberStatus::Member);
            assert_eq!(members[1].admin_rights, None);
        }
        other => panic!("unexpected {other:?}"),
    }
}

/// Phase D3b: `ChatAdminRights::to_json` round-trips through
/// `parse_chat_admin_rights`; missing rights block -> `None`.
#[test]
fn chat_admin_rights_round_trip() {
    let rights = ChatAdminRights {
        can_post_messages: true,
        can_promote_members: true,
        is_anonymous: true,
        ..Default::default()
    };
    let json = rights.to_json();
    let parsed = parse_chat_admin_rights(Some(&json)).expect("rights parse");
    assert_eq!(parsed, rights);
    // Wrong @type / absent -> None.
    assert_eq!(
        parse_chat_admin_rights(Some(&serde_json::json!({"@type":"chatMemberStatusMember"}))),
        None
    );
    assert_eq!(parse_chat_admin_rights(None), None);
    // A bare administrator status without rights: no rights claim.
    let json = r#"{"@type":"chatMember","member_id":{"@type":"messageSenderUser","user_id":888},"status":{"@type":"chatMemberStatusAdministrator","can_be_edited":true}}"#;
    let env = parse_envelope(json).unwrap();
    match env.payload {
        EnvelopePayload::Groups(GroupsPayload::ChatMember { member }) => {
            assert_eq!(member.status, ChannelMemberStatus::Administrator);
            assert_eq!(member.admin_rights, None);
        }
        other => panic!("unexpected {other:?}"),
    }
}

/// Phase D3b: every constructor this slice relies on must exist verbatim
/// in the pinned schema (1.8.67) — never invent constructors or fields.
/// (`setChatAdministratorCustomTitle` is deliberately absent: custom
/// titles are read-only in this schema version.)
#[test]
fn d3b_schema_pins_exist_verbatim() {
    let schema = include_str!("../../../schema/td_api.tl");
    for line in [
        "chatAdministrator user_id:int53 custom_title:string is_owner:Bool can_be_edited:Bool = ChatAdministrator;",
        "chatAdministrators administrators:vector<chatAdministrator> = ChatAdministrators;",
        "chatAdministratorRights can_manage_chat:Bool can_change_info:Bool can_post_messages:Bool can_edit_messages:Bool can_delete_messages:Bool can_invite_users:Bool can_restrict_members:Bool can_pin_messages:Bool can_manage_topics:Bool can_promote_members:Bool can_manage_video_chats:Bool can_post_stories:Bool can_edit_stories:Bool can_delete_stories:Bool can_manage_direct_messages:Bool can_manage_tags:Bool can_send_welcome_messages:Bool is_anonymous:Bool = ChatAdministratorRights;",
        "chatMemberStatusCreator is_anonymous:Bool is_member:Bool = ChatMemberStatus;",
        "chatMemberStatusAdministrator can_be_edited:Bool rights:chatAdministratorRights = ChatMemberStatus;",
        "chatMemberStatusMember member_until_date:int32 = ChatMemberStatus;",
        "chatMember member_id:MessageSender tag:string inviter_user_id:int53 joined_chat_date:int32 status:ChatMemberStatus = ChatMember;",
        "chatMembers total_count:int32 members:vector<chatMember> = ChatMembers;",
        "supergroupMembersFilterRecent = SupergroupMembersFilter;",
        "supergroupMembersFilterSearch query:string = SupergroupMembersFilter;",
        "messageSenderUser user_id:int53 = MessageSender;",
        "getChatAdministrators chat_id:int53 = ChatAdministrators;",
        "setChatMemberStatus chat_id:int53 member_id:MessageSender status:ChatMemberStatus = Ok;",
        "getChatMember chat_id:int53 member_id:MessageSender = ChatMember;",
        "getSupergroupMembers supergroup_id:int53 filter:SupergroupMembersFilter offset:int32 limit:int32 = ChatMembers;",
        "updateChatMember chat_id:int53 actor_user_id:int53 date:int32 invite_link:chatInviteLink via_join_request:Bool via_chat_folder_invite_link:Bool old_chat_member:chatMember new_chat_member:chatMember = Update;",
    ] {
        assert!(
            schema.lines().any(|l| l == line),
            "schema pin missing: {line}"
        );
    }
    assert!(
        !schema
            .lines()
            .any(|l| l.starts_with("setChatAdministratorCustomTitle ")),
        "setChatAdministratorCustomTitle must not exist in 1.8.67"
    );
}

/// Phase D3c: `chatEvents` parses all 16 handled action constructors
/// (schema lines 7764/7767/7770/7773/7779/7782/7785/7788/7794/7797/
/// 7812/7830/7842/7886/7889/7892), including promote vs demote and
/// restrict vs ban vs unban distinctions from old/new statuses.
#[test]
fn chat_events_parse_handled_actions() {
    let link = |url: &str, name: &str| {
        format!(
            r#"{{"@type":"chatInviteLink","invite_link":"{url}","name":"{name}","creator_user_id":777,"date":1700000000,"edit_date":0,"expiration_date":0,"member_limit":0,"member_count":0,"expired_member_count":0,"pending_join_request_count":0,"creates_join_request":false,"is_primary":false,"is_revoked":false}}"#
        )
    };
    let msg = |id: i64, text: &str| {
        format!(
            r#"{{"id":{id},"chat_id":13,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"{text}","entities":[]}}}}}}"#
        )
    };
    let member_status = r#"{"@type":"chatMemberStatusMember","member_until_date":0}"#;
    let admin_status = r#"{"@type":"chatMemberStatusAdministrator","can_be_edited":true}"#;
    let restricted_status = r#"{"@type":"chatMemberStatusRestricted"}"#;
    let banned_status = r#"{"@type":"chatMemberStatusBanned"}"#;
    let user = |id: i64| format!(r#"{{"@type":"messageSenderUser","user_id":{id}}}"#);
    let event = |id: i64, actor: i64, action: &str| {
        format!(
            r#"{{"@type":"chatEvent","id":{id},"date":1700000000,"member_id":{},"action":{}}}"#,
            user(actor),
            action
        )
    };
    let json = format!(
            r#"{{"@type":"chatEvents","events":[{}]}}"#,
            [
                event(
                    101,
                    777,
                    &format!(
                        r#"{{"@type":"chatEventMessageEdited","old_message":{},"new_message":{}}}"#,
                        msg(55, "before"),
                        msg(55, "after")
                    )
                ),
                event(
                    102,
                    777,
                    &format!(
                        r#"{{"@type":"chatEventMessageDeleted","message":{},"can_report_anti_spam_false_positive":false}}"#,
                        msg(56, "gone")
                    )
                ),
                event(
                    103,
                    777,
                    &format!(
                        r#"{{"@type":"chatEventMessagePinned","message":{}}}"#,
                        msg(57, "pinned post")
                    )
                ),
                event(
                    104,
                    777,
                    &format!(
                        r#"{{"@type":"chatEventMessageUnpinned","message":{}}}"#,
                        msg(57, "pinned post")
                    )
                ),
                event(105, 5, r#"{"@type":"chatEventMemberJoined"}"#),
                event(
                    106,
                    6,
                    &format!(
                        r#"{{"@type":"chatEventMemberJoinedByInviteLink","invite_link":{},"via_chat_folder_invite_link":false}}"#,
                        link("https://t.me/+mods", "Mods")
                    )
                ),
                event(
                    107,
                    7,
                    &format!(
                        r#"{{"@type":"chatEventMemberJoinedByRequest","approver_user_id":777,"invite_link":{}}}"#,
                        link("https://t.me/+req", "")
                    )
                ),
                event(
                    108,
                    777,
                    &format!(
                        r#"{{"@type":"chatEventMemberInvited","user_id":8,"status":{}}}"#,
                        member_status
                    )
                ),
                event(
                    109,
                    777,
                    &format!(
                        r#"{{"@type":"chatEventMemberPromoted","user_id":8,"old_status":{member_status},"new_status":{admin_status}}}"#
                    )
                ),
                event(
                    110,
                    777,
                    &format!(
                        r#"{{"@type":"chatEventMemberPromoted","user_id":9,"old_status":{admin_status},"new_status":{member_status}}}"#
                    )
                ),
                event(
                    111,
                    777,
                    &format!(
                        r#"{{"@type":"chatEventMemberRestricted","member_id":{},"old_status":{member_status},"new_status":{restricted_status}}}"#,
                        user(10)
                    )
                ),
                event(
                    112,
                    777,
                    &format!(
                        r#"{{"@type":"chatEventMemberRestricted","member_id":{},"old_status":{member_status},"new_status":{banned_status}}}"#,
                        user(11)
                    )
                ),
                event(
                    113,
                    777,
                    &format!(
                        r#"{{"@type":"chatEventMemberRestricted","member_id":{},"old_status":{restricted_status},"new_status":{member_status}}}"#,
                        user(12)
                    )
                ),
                event(
                    114,
                    777,
                    r#"{"@type":"chatEventDescriptionChanged","old_description":"old","new_description":"new"}"#
                ),
                event(
                    115,
                    777,
                    r#"{"@type":"chatEventPhotoChanged","old_photo":{"@type":"chatPhoto"},"new_photo":{"@type":"chatPhoto"}}"#
                ),
                event(
                    116,
                    777,
                    r#"{"@type":"chatEventTitleChanged","old_title":"Old name","new_title":"New name"}"#
                ),
                event(
                    117,
                    777,
                    &format!(
                        r#"{{"@type":"chatEventInviteLinkEdited","old_invite_link":{},"new_invite_link":{}}}"#,
                        link("https://t.me/+old", "Old"),
                        link("https://t.me/+new", "New")
                    )
                ),
                event(
                    118,
                    777,
                    &format!(
                        r#"{{"@type":"chatEventInviteLinkRevoked","invite_link":{}}}"#,
                        link("https://t.me/+gone", "Gone")
                    )
                ),
                event(
                    119,
                    777,
                    &format!(
                        r#"{{"@type":"chatEventInviteLinkDeleted","invite_link":{}}}"#,
                        link("https://t.me/+del", "Del")
                    )
                ),
            ]
            .join(",")
        );
    let env = parse_envelope(&json).unwrap();
    let events = match env.payload {
        EnvelopePayload::Groups(GroupsPayload::ChatEvents { events }) => events,
        other => panic!("unexpected {other:?}"),
    };
    assert_eq!(events.len(), 19);
    assert_eq!(
        events[0].action,
        ChatEventAction::MessageEdited {
            message_id: 55,
            text: "after".to_owned()
        }
    );
    assert_eq!(
        events[1].action,
        ChatEventAction::MessageDeleted {
            message_id: 56,
            text: "gone".to_owned()
        }
    );
    assert_eq!(
        events[2].action,
        ChatEventAction::MessagePinned {
            message_id: 57,
            text: "pinned post".to_owned()
        }
    );
    assert_eq!(
        events[3].action,
        ChatEventAction::MessageUnpinned {
            message_id: 57,
            text: "pinned post".to_owned()
        }
    );
    assert_eq!(events[4].action, ChatEventAction::MemberJoined);
    assert_eq!(
        events[5].action,
        ChatEventAction::MemberJoinedByInviteLink {
            invite_link: "https://t.me/+mods".to_owned(),
            invite_link_name: "Mods".to_owned(),
        }
    );
    assert_eq!(
        events[6].action,
        ChatEventAction::MemberJoinedByRequest {
            approver_user_id: 777,
            invite_link: "https://t.me/+req".to_owned(),
        }
    );
    assert_eq!(
        events[7].action,
        ChatEventAction::MemberInvited {
            user_id: 8,
            status: ChannelMemberStatus::Member,
        }
    );
    assert_eq!(
        events[8].action,
        ChatEventAction::MemberPromoted {
            user_id: 8,
            old_status: ChannelMemberStatus::Member,
            new_status: ChannelMemberStatus::Administrator,
        }
    );
    assert_eq!(
        events[9].action,
        ChatEventAction::MemberPromoted {
            user_id: 9,
            old_status: ChannelMemberStatus::Administrator,
            new_status: ChannelMemberStatus::Member,
        }
    );
    assert_eq!(
        events[10].action,
        ChatEventAction::MemberRestricted {
            member_id: MessageSender::User { user_id: 10 },
            old_status: ChannelMemberStatus::Member,
            new_status: ChannelMemberStatus::Restricted,
        }
    );
    assert_eq!(
        events[11].action,
        ChatEventAction::MemberRestricted {
            member_id: MessageSender::User { user_id: 11 },
            old_status: ChannelMemberStatus::Member,
            new_status: ChannelMemberStatus::Banned,
        }
    );
    assert_eq!(
        events[12].action,
        ChatEventAction::MemberRestricted {
            member_id: MessageSender::User { user_id: 12 },
            old_status: ChannelMemberStatus::Restricted,
            new_status: ChannelMemberStatus::Member,
        }
    );
    assert_eq!(
        events[13].action,
        ChatEventAction::DescriptionChanged {
            old_description: "old".to_owned(),
            new_description: "new".to_owned(),
        }
    );
    assert_eq!(events[14].action, ChatEventAction::PhotoChanged);
    assert_eq!(
        events[15].action,
        ChatEventAction::TitleChanged {
            old_title: "Old name".to_owned(),
            new_title: "New name".to_owned(),
        }
    );
    assert_eq!(
        events[16].action,
        ChatEventAction::InviteLinkEdited {
            old_url: "https://t.me/+old".to_owned(),
            old_name: "Old".to_owned(),
            new_url: "https://t.me/+new".to_owned(),
            new_name: "New".to_owned(),
        }
    );
    assert_eq!(
        events[17].action,
        ChatEventAction::InviteLinkRevoked {
            url: "https://t.me/+gone".to_owned(),
            name: "Gone".to_owned(),
        }
    );
    assert_eq!(
        events[18].action,
        ChatEventAction::InviteLinkDeleted {
            url: "https://t.me/+del".to_owned(),
            name: "Del".to_owned(),
        }
    );
    assert_eq!(events[0].id, 101);
    assert_eq!(events[0].date, 1_700_000_000);
    assert_eq!(events[0].member_id, MessageSender::User { user_id: 777 });
}

/// Phase D3c: unhandled `chatEvent*` constructors degrade to an honest
/// generic `Unsupported` (the constructor name is kept for the
/// schema-pin test, never rendered as fabricated details), and events
/// whose `member_id` fails to parse are dropped, never misattributed.
/// B4: `chatEventPollStopped` parses to `PollStopped`; its minimal
/// `{"id":60}` message carries no poll content, so `is_quiz` is
/// honestly false rather than guessed.
#[test]
fn chat_events_unsupported_and_actorless() {
    let json = r#"{"@type":"chatEvents","events":[
            {"@type":"chatEvent","id":201,"date":1700000000,"member_id":{"@type":"messageSenderUser","user_id":777},"action":{"@type":"chatEventPollStopped","message":{"id":60}}},
            {"@type":"chatEvent","id":202,"date":1700000000,"member_id":{"@type":"messageSenderChat","chat_id":13},"action":{"@type":"chatEventMemberLeft"}},
            {"@type":"chatEvent","id":203,"date":1700000000,"member_id":{"@type":"bogus"},"action":{"@type":"chatEventMemberLeft"}},
            {"@type":"chatEvent","id":204,"date":1700000000,"action":{"@type":"chatEventMemberLeft"}}
        ]}"#;
    let env = parse_envelope(json).unwrap();
    let events = match env.payload {
        EnvelopePayload::Groups(GroupsPayload::ChatEvents { events }) => events,
        other => panic!("unexpected {other:?}"),
    };
    // The two actor-less events are dropped; `chatEventMemberLeft`
    // stays an honest generic `Unsupported`.
    assert_eq!(events.len(), 2);
    assert_eq!(
        events[0].action,
        ChatEventAction::PollStopped { is_quiz: false }
    );
    assert_eq!(
        events[1].action,
        ChatEventAction::Unsupported {
            type_name: "chatEventMemberLeft".to_owned()
        }
    );
    assert_eq!(events[1].member_id, MessageSender::Chat { chat_id: 13 });
}

/// B4: `chatEventPollStopped` with a quiz message content parses
/// `is_quiz: true` (TGX `EventLogQuizStopped` vs
/// `EventLogPollStopped` copy).
#[test]
fn chat_event_poll_stopped_quiz_detected() {
    let json = r#"{"@type":"chatEvent","id":205,"date":1700000000,"member_id":{"@type":"messageSenderUser","user_id":777},"action":{"@type":"chatEventPollStopped","message":{"id":60,"content":{"@type":"messagePoll","poll":{"@type":"poll","id":1,"question":{"@type":"formattedText","text":"Q?","entities":[]},"options":[],"total_voter_count":0,"is_anonymous":true,"allows_multiple_answers":false,"allows_revoting":false,"is_closed":true,"type":{"@type":"pollTypeQuiz","correct_option_ids":[0],"explanation":{"@type":"formattedText","text":"","entities":[]}}},"description":{"@type":"formattedText","text":"","entities":[]},"can_add_option":false}}}}"#;
    let value: serde_json::Value = serde_json::from_str(json).unwrap();
    let event = parse_chat_event(&value).unwrap();
    assert_eq!(event.action, ChatEventAction::PollStopped { is_quiz: true });
}

/// Phase D3c: every constructor this slice relies on must exist verbatim
/// in the pinned schema (1.8.67) — never invent constructors or fields.
/// (`chatEventInviteLinkCreated` is deliberately absent: link creation
/// has no event constructor in this schema version.)
#[test]
fn d3c_schema_pins_exist_verbatim() {
    let schema = include_str!("../../../schema/td_api.tl");
    for line in [
        "chatEventMessageEdited old_message:message new_message:message = ChatEventAction;",
        "chatEventMessageDeleted message:message can_report_anti_spam_false_positive:Bool = ChatEventAction;",
        "chatEventMessagePinned message:message = ChatEventAction;",
        "chatEventMessageUnpinned message:message = ChatEventAction;",
        "chatEventPollStopped message:message = ChatEventAction;",
        "chatEventMemberJoined = ChatEventAction;",
        "chatEventMemberJoinedByInviteLink invite_link:chatInviteLink via_chat_folder_invite_link:Bool = ChatEventAction;",
        "chatEventMemberJoinedByRequest approver_user_id:int53 invite_link:chatInviteLink = ChatEventAction;",
        "chatEventMemberInvited user_id:int53 status:ChatMemberStatus = ChatEventAction;",
        "chatEventMemberPromoted user_id:int53 old_status:ChatMemberStatus new_status:ChatMemberStatus = ChatEventAction;",
        "chatEventMemberRestricted member_id:MessageSender old_status:ChatMemberStatus new_status:ChatMemberStatus = ChatEventAction;",
        "chatEventDescriptionChanged old_description:string new_description:string = ChatEventAction;",
        "chatEventPhotoChanged old_photo:chatPhoto new_photo:chatPhoto = ChatEventAction;",
        "chatEventTitleChanged old_title:string new_title:string = ChatEventAction;",
        "chatEventInviteLinkEdited old_invite_link:chatInviteLink new_invite_link:chatInviteLink = ChatEventAction;",
        "chatEventInviteLinkRevoked invite_link:chatInviteLink = ChatEventAction;",
        "chatEventInviteLinkDeleted invite_link:chatInviteLink = ChatEventAction;",
        "chatEvent id:int64 date:int32 member_id:MessageSender action:ChatEventAction = ChatEvent;",
        "chatEvents events:vector<chatEvent> = ChatEvents;",
        "chatEventLogFilters message_edits:Bool message_deletions:Bool message_pins:Bool member_joins:Bool member_leaves:Bool member_invites:Bool member_promotions:Bool member_restrictions:Bool member_tag_changes:Bool info_changes:Bool setting_changes:Bool invite_link_changes:Bool video_chat_changes:Bool forum_changes:Bool subscription_extensions:Bool = ChatEventLogFilters;",
        "getChatEventLog chat_id:int53 query:string from_event_id:int64 limit:int32 filters:chatEventLogFilters user_ids:vector<int53> = ChatEvents;",
    ] {
        assert!(
            schema.lines().any(|l| l == line),
            "schema pin missing: {line}"
        );
    }
    assert!(
        !schema
            .lines()
            .any(|l| l.starts_with("chatEventInviteLinkCreated ")),
        "chatEventInviteLinkCreated must not exist in 1.8.67"
    );
}

#[test]
fn message_group_call_parses_invitation_state() {
    // Phase C2f: `messageGroupCall unique_id:int64 is_active:Bool
    // was_missed:Bool is_video:Bool duration:int32
    // other_participant_ids:vector<MessageSender> = MessageContent`
    // (schema 1.8.67, line 5288).
    let env = parse_envelope(
            r#"{"@type":"updateNewMessage","message":{"id":90,"chat_id":51,"is_outgoing":false,"date":1700000100,"content":{"@type":"messageGroupCall","unique_id":"123456789","is_active":false,"was_missed":false,"is_video":true,"duration":0,"other_participant_ids":[]}}}"#,
        )
        .unwrap();
    match env.payload {
        EnvelopePayload::Messages(MessagesPayload::UpdateNewMessage(message)) => {
            assert_eq!(
                message.content,
                MessageContent::GroupCallInvitation {
                    unique_id: 123456789,
                    is_active: false,
                    was_missed: false,
                    is_video: true,
                }
            );
            assert_eq!(message.content.preview(), "📹 Video chat invitation");
        }
        other => panic!("{other:?}"),
    }
    let schema = include_str!("../../../schema/td_api.tl");
    assert_eq!(
        schema
            .lines()
            .find(|l| l.starts_with("messageGroupCall "))
            .expect("messageGroupCall in schema"),
        "messageGroupCall unique_id:int64 is_active:Bool was_missed:Bool is_video:Bool duration:int32 other_participant_ids:vector<MessageSender> = MessageContent;"
    );
}

#[test]
fn invite_group_call_participant_results_parse() {
    // Phase C2f: the `inviteGroupCallParticipant` answer variants
    // (schema 1.8.67, lines 7216-7227).
    let env = parse_envelope(
        r#"{"@type":"inviteGroupCallParticipantResultSuccess","chat_id":51,"message_id":90}"#,
    )
    .unwrap();
    assert_eq!(
        env.payload,
        EnvelopePayload::Calls(CallsPayload::InviteGroupCallParticipantResult(
            InviteGroupCallParticipantResult::Success {
                chat_id: 51,
                message_id: 90
            }
        ))
    );
    for (json, expected) in [
        (
            r#"{"@type":"inviteGroupCallParticipantResultUserPrivacyRestricted"}"#,
            InviteGroupCallParticipantResult::UserPrivacyRestricted,
        ),
        (
            r#"{"@type":"inviteGroupCallParticipantResultUserAlreadyParticipant"}"#,
            InviteGroupCallParticipantResult::UserAlreadyParticipant,
        ),
        (
            r#"{"@type":"inviteGroupCallParticipantResultUserWasBanned"}"#,
            InviteGroupCallParticipantResult::UserWasBanned,
        ),
    ] {
        let env = parse_envelope(json).unwrap();
        assert_eq!(
            env.payload,
            EnvelopePayload::Calls(CallsPayload::InviteGroupCallParticipantResult(expected))
        );
    }
}

#[test]
fn g1_basic_group_full_info_parses() {
    // Slice G1: `basicGroupFullInfo` (schema 1.8.67, line 2714) — the
    // `getBasicGroupFullInfo` answer (line 11507). Only `members` is
    // kept.
    let env = parse_envelope(
            r#"{"@type":"basicGroupFullInfo","creator_user_id":7,"members":[{"@type":"chatMember","member_id":{"@type":"messageSenderUser","user_id":7},"tag":"","inviter_user_id":0,"joined_chat_date":0,"status":{"@type":"chatMemberStatusCreator"}},{"@type":"chatMember","member_id":{"@type":"messageSenderUser","user_id":8},"tag":"","inviter_user_id":7,"joined_chat_date":0,"status":{"@type":"chatMemberStatusMember"}}]}"#,
        )
        .unwrap();
    match env.payload {
        EnvelopePayload::Groups(GroupsPayload::BasicGroupFullInfo { members }) => {
            assert_eq!(members.len(), 2);
        }
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn group_call_info_parses() {
    // Phase C2f: the `joinGroupCall` answer (schema 1.8.67, line
    // 7190) — invitation acceptance.
    let env = parse_envelope(
        r#"{"@type":"groupCallInfo","group_call_id":555,"join_payload":"tgcalls-payload"}"#,
    )
    .unwrap();
    assert_eq!(
        env.payload,
        EnvelopePayload::Calls(CallsPayload::GroupCallInfo {
            group_call_id: 555,
            join_payload: "tgcalls-payload".to_string(),
        })
    );
}

#[test]
fn g1_created_basic_group_chat_parses() {
    // Slice G1: `createdBasicGroupChat chat_id:int53
    // failed_to_add_members:failedToAddMembers = CreatedBasicGroupChat`
    // (schema 1.8.67, line 3644) — the `createNewBasicGroupChat`
    // answer (line 13327).
    let env = parse_envelope(
            r#"{"@type":"createdBasicGroupChat","chat_id":99,"failed_to_add_members":{"@type":"failedToAddMembers","failed_to_add_members":[]}}"#,
        )
        .unwrap();
    assert_eq!(
        env.payload,
        EnvelopePayload::Groups(GroupsPayload::CreatedBasicGroupChat { chat_id: 99 })
    );
}

#[test]
fn g1_failed_to_add_members_parses() {
    // Slice G1: `failedToAddMembers
    // failed_to_add_members:vector<failedToAddMember> =
    // FailedToAddMembers` (schema 1.8.67, line 3640) — the
    // `addChatMembers` answer (line 13584). Only the failure count is
    // kept.
    let env = parse_envelope(
            r#"{"@type":"failedToAddMembers","failed_to_add_members":[{"@type":"failedToAddMember","user_id":7,"premium_would_allow_invite":false,"premium_required_to_send_messages":false},{"@type":"failedToAddMember","user_id":8,"premium_would_allow_invite":false,"premium_required_to_send_messages":false}]}"#,
        )
        .unwrap();
    assert_eq!(
        env.payload,
        EnvelopePayload::Groups(GroupsPayload::FailedToAddMembers { failed_count: 2 })
    );
}

#[test]
fn g1_chat_permissions_round_trip() {
    // Slice G1: `chatPermissions` (schema 1.8.67, line 1070) parses
    // field-by-field and serializes back with the same `@type`.
    let json = r#"{"@type":"chatPermissions","can_send_basic_messages":true,"can_send_audios":false,"can_send_documents":true,"can_send_photos":true,"can_send_videos":true,"can_send_video_notes":true,"can_send_voice_notes":true,"can_send_polls":false,"can_send_other_messages":true,"can_add_link_previews":true,"can_react_to_messages":true,"can_edit_tag":false,"can_change_info":false,"can_invite_users":true,"can_pin_messages":false,"can_create_topics":false}"#;
    let value: Value = serde_json::from_str(json).unwrap();
    let perms = parse_chat_permissions(Some(&value)).unwrap();
    assert!(perms.can_send_basic_messages);
    assert!(!perms.can_send_audios);
    assert!(perms.can_invite_users);
    assert!(!perms.can_create_topics);
    let back = perms.to_json();
    assert_eq!(back["@type"], "chatPermissions");
    assert_eq!(back["can_send_polls"], Value::Bool(false));
    assert_eq!(back["can_send_documents"], Value::Bool(true));
    // Wrong `@type` / null → None (deny-by-default, no fabricated block).
    assert!(parse_chat_permissions(None).is_none());
    assert!(parse_chat_permissions(Some(&Value::Null)).is_none());
    let wrong = serde_json::json!({"@type": "chatAdministratorRights"});
    assert!(parse_chat_permissions(Some(&wrong)).is_none());
}

/// Phase 9.5: all three `reportStoryResult*` variants parse —
/// `Ok`, the option picker (`reportOption` ids are kept verbatim),
/// and the text step (`is_optional` honored).
#[test]
fn report_story_results_parse() {
    let ok = parse_envelope(r#"{"@type":"reportStoryResultOk"}"#).unwrap();
    assert!(matches!(
        ok.payload,
        EnvelopePayload::Stories(StoriesPayload::ReportStoryResult(ReportStoryResult::Ok))
    ));
    let options = parse_envelope(
            r#"{"@type":"reportStoryResultOptionRequired","title":"Why report?","options":[{"@type":"reportOption","id":"aGk=","text":"Spam"},{"@type":"reportOption","id":"","text":""}]}"#,
        )
        .unwrap();
    match options.payload {
        EnvelopePayload::Stories(StoriesPayload::ReportStoryResult(
            ReportStoryResult::OptionRequired { title, options },
        )) => {
            assert_eq!(title, "Why report?");
            assert_eq!(options.len(), 2);
            assert_eq!(options[0].id, "aGk=");
            assert_eq!(options[0].text, "Spam");
        }
        other => panic!("unexpected {other:?}"),
    }
    let text = parse_envelope(
        r#"{"@type":"reportStoryResultTextRequired","option_id":"aGk=","is_optional":true}"#,
    )
    .unwrap();
    assert!(matches!(
        text.payload,
        EnvelopePayload::Stories(StoriesPayload::ReportStoryResult(ReportStoryResult::TextRequired {
            option_id,
            is_optional: true,
        })) if option_id == "aGk="
    ));
}

/// Phase 9.5: `storyInteractions` parses viewers with view/reaction
/// / forward / repost kinds; unknown actor or interaction types are
/// skipped without dropping the page.
#[test]
fn story_interactions_parse() {
    let json = r#"{"@type":"storyInteractions","total_count":4,"total_forward_count":1,"total_reaction_count":1,"interactions":[
            {"actor_id":{"@type":"messageSenderUser","user_id":777},"interaction_date":1700000100,"block_list":null,"type":{"@type":"storyInteractionTypeView","chosen_reaction_type":{"@type":"reactionTypeEmoji","emoji":"❤"}}},
            {"actor_id":{"@type":"messageSenderChat","chat_id":11},"interaction_date":1700000200,"block_list":null,"type":{"@type":"storyInteractionTypeView","chosen_reaction_type":null}},
            {"actor_id":{"@type":"messageSenderUser","user_id":778},"interaction_date":1700000300,"block_list":null,"type":{"@type":"storyInteractionTypeForward","message":{"@type":"message"}}},
            {"actor_id":{"@type":"messageSenderUser"},"interaction_date":1,"block_list":null,"type":{"@type":"storyInteractionTypeView"}},
            {"actor_id":{"@type":"messageSenderUser","user_id":779},"interaction_date":1,"block_list":null,"type":{"@type":"storyInteractionTypeMystery"}}
        ],"next_offset":"50"}"#;
    let env = parse_envelope(json).unwrap();
    match env.payload {
        EnvelopePayload::Stories(StoriesPayload::StoryInteractions { interactions }) => {
            assert_eq!(interactions.total_count, 4);
            assert_eq!(interactions.next_offset, "50");
            assert_eq!(interactions.interactions.len(), 3);
            let first = &interactions.interactions[0];
            assert_eq!(first.actor, MessageSender::User { user_id: 777 });
            assert_eq!(first.interaction_date, 1700000100);
            assert_eq!(first.kind, StoryInteractionKind::View);
            assert_eq!(first.reaction_emoji.as_deref(), Some("❤"));
            let second = &interactions.interactions[1];
            assert_eq!(second.actor, MessageSender::Chat { chat_id: 11 });
            assert_eq!(second.reaction_emoji, None);
            assert_eq!(second.reaction_extra, None);
            assert_eq!(
                interactions.interactions[2].kind,
                StoryInteractionKind::Forward
            );
        }
        other => panic!("unexpected {other:?}"),
    }
}

/// Phase 9.5: `updateStoryStealthMode` parses the two timestamps.
#[test]
fn update_story_stealth_mode_parses() {
    let env = parse_envelope(
            r#"{"@type":"updateStoryStealthMode","active_until_date":1700003600,"cooldown_until_date":1700007200}"#,
        )
        .unwrap();
    assert!(matches!(
        env.payload,
        EnvelopePayload::Stories(StoriesPayload::UpdateStoryStealthMode {
            active_until_date: 1700003600,
            cooldown_until_date: 1700007200,
        })
    ));
}

/// Bots slice: `inlineQueryResults` (schema 1.8.67, line 7716) —
/// article + photo + sticker variants, next_offset, and both button
/// shapes (null → None, present → parsed).
#[test]
fn inline_query_results_parses() {
    let env = parse_envelope(
        r#"{
                "@type": "inlineQueryResults",
                "inline_query_id": 9001,
                "button": null,
                "results": [
                    {"@type": "inlineQueryResultArticle", "id": "a1", "url": "https://x.test",
                     "title": "An article", "description": "A description"},
                    {"@type": "inlineQueryResultPhoto", "id": "p1",
                     "title": "A photo", "description": ""},
                    {"@type": "inlineQueryResultSticker", "id": "s1"}
                ],
                "next_offset": "25"
            }"#,
    )
    .unwrap();
    let page = match env.payload {
        EnvelopePayload::Bots(BotsPayload::InlineQueryResults(page)) => page,
        other => panic!("unexpected {other:?}"),
    };
    assert_eq!(page.inline_query_id, 9001);
    assert_eq!(page.button, None);
    assert_eq!(page.next_offset, "25");
    assert_eq!(page.results.len(), 3);
    assert_eq!(page.results[0].id, "a1");
    assert_eq!(page.results[0].kind, "article");
    assert_eq!(page.results[0].title, "An article");
    assert_eq!(page.results[0].description, "A description");
    assert_eq!(page.results[1].kind, "photo");
    assert_eq!(page.results[1].title, "A photo");
    // Sticker has no title/description fields — lenient empty strings.
    assert_eq!(page.results[2].id, "s1");
    assert_eq!(page.results[2].kind, "sticker");
    assert_eq!(page.results[2].title, "");
    assert_eq!(page.results[2].description, "");

    let env = parse_envelope(
            r#"{
                "@type": "inlineQueryResults",
                "inline_query_id": 9002,
                "button": {"@type": "inlineQueryResultsButton", "text": "More",
                           "type": {"@type": "inlineQueryResultsButtonTypeWebApp", "url": "https://x.test"}},
                "results": [],
                "next_offset": ""
            }"#,
        )
        .unwrap();
    match env.payload {
        EnvelopePayload::Bots(BotsPayload::InlineQueryResults(page)) => {
            assert_eq!(page.inline_query_id, 9002);
            assert_eq!(
                page.button,
                Some(InlineQueryResultsButton {
                    text: "More".to_string(),
                    kind: "web_app".to_string(),
                    parameter: String::new(),
                    url: "https://x.test".to_string(),
                })
            );
            assert!(page.results.is_empty());
            assert_eq!(page.next_offset, "");
        }
        other => panic!("unexpected {other:?}"),
    }
}
