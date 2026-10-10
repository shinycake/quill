use crate::ids::RequestId;
use crate::telegram::requests::*;

/// Phase C1: call request shapes — verified against the pinned
/// schema (1.8.67) constructors, never invented.
#[test]
fn call_request_shapes() {
    // The advertised protocol is signaling-only: no media
    // capability claimed (min/max layer pinned by the schema).
    let p = call_protocol();
    assert_eq!(p["@type"], "callProtocol");
    assert_eq!(p["udp_p2p"], false);
    assert_eq!(p["udp_reflector"], false);
    assert_eq!(p["min_layer"], 65);
    assert_eq!(p["max_layer"], 92);
    assert_eq!(p["library_versions"].as_array().unwrap().len(), 0);

    let v: serde_json::Value = serde_json::from_str(&create_call(RequestId(1), 41, false)).unwrap();
    assert_eq!(v["@type"], "createCall");
    assert_eq!(v["user_id"], 41);
    assert_eq!(v["is_video"], false);
    assert_eq!(v["protocol"]["@type"], "callProtocol");

    // Phase C1b: a video call sends `is_video: true` (signaling
    // only — no transport yet).
    let v: serde_json::Value = serde_json::from_str(&create_call(RequestId(5), 41, true)).unwrap();
    assert_eq!(v["@type"], "createCall");
    assert_eq!(v["user_id"], 41);
    assert_eq!(v["is_video"], true);

    let v: serde_json::Value = serde_json::from_str(&accept_call(RequestId(2), 77)).unwrap();
    assert_eq!(v["@type"], "acceptCall");
    assert_eq!(v["call_id"], 77);
    assert_eq!(v["protocol"]["@type"], "callProtocol");

    let v: serde_json::Value =
        serde_json::from_str(&discard_call(RequestId(3), 77, false, 42, false)).unwrap();
    assert_eq!(v["@type"], "discardCall");
    assert_eq!(v["call_id"], 77);
    assert_eq!(v["is_disconnected"], false);
    assert_eq!(v["invite_link"], "");
    assert_eq!(v["duration"], 42);
    assert_eq!(v["is_video"], false);
    assert_eq!(v["connection_id"], 0);

    // Phase C1b: discarding a video call reports `is_video: true`
    // (schema 1.8.67, :14227).
    let v: serde_json::Value =
        serde_json::from_str(&discard_call(RequestId(6), 78, false, 7, true)).unwrap();
    assert_eq!(v["@type"], "discardCall");
    assert_eq!(v["call_id"], 78);
    assert_eq!(v["duration"], 7);
    assert_eq!(v["is_video"], true);

    let v: serde_json::Value =
        serde_json::from_str(&send_call_rating(RequestId(4), 77, 5)).unwrap();
    assert_eq!(v["@type"], "sendCallRating");
    assert_eq!(v["call_id"]["@type"], "inputCallDiscarded");
    assert_eq!(v["call_id"]["call_id"], 77);
    assert_eq!(v["rating"], 5);
    assert_eq!(v["comment"], "");
    assert_eq!(v["problems"].as_array().unwrap().len(), 0);
}

#[test]
fn delete_all_call_messages_shape_matches_1_8_67() {
    // `deleteAllCallMessages revoke:Bool = Ok` (schema line 12348).
    let v: serde_json::Value =
        serde_json::from_str(&delete_all_call_messages(RequestId(12), true)).unwrap();
    assert_eq!(v["@type"], "deleteAllCallMessages");
    assert_eq!(v["@extra"], "12");
    assert_eq!(v["revoke"], true);
}

#[test]
fn call_history_and_settings_shapes_match_1_8_67() {
    // Phase C2i: `searchCallMessages offset:string limit:int32
    // only_missed:Bool = FoundMessages` (schema 1.8.67 line 11903).
    let v: serde_json::Value =
        serde_json::from_str(&search_call_messages(RequestId(10), "", 40)).unwrap();
    assert_eq!(v["@type"], "searchCallMessages");
    assert_eq!(v["offset"], "");
    assert_eq!(v["limit"], 40);
    assert_eq!(v["only_missed"], false);

    // Phase C2i: full rating detail — `sendCallRating
    // call_id:InputCall rating:int32 comment:string
    // problems:vector<CallProblem> = Ok` (schema 1.8.67 line 14234).
    let v: serde_json::Value = serde_json::from_str(&send_call_rating_detail(
        RequestId(11),
        77,
        2,
        "robotic voice",
        &["callProblemEcho", "callProblemDistortedSpeech"],
    ))
    .unwrap();
    assert_eq!(v["@type"], "sendCallRating");
    assert_eq!(v["call_id"]["@type"], "inputCallDiscarded");
    assert_eq!(v["rating"], 2);
    assert_eq!(v["comment"], "robotic voice");
    let problems = v["problems"].as_array().unwrap();
    assert_eq!(problems.len(), 2);
    assert_eq!(problems[0]["@type"], "callProblemEcho");
    assert_eq!(problems[1]["@type"], "callProblemDistortedSpeech");

    // Phase C2i: `sendCallLog call_id:InputCall log_file:InputFile
    // = Ok` (schema 1.8.67 line 14240); only inputFileLocal /
    // inputFileGenerated are supported.
    let v: serde_json::Value =
        serde_json::from_str(&send_call_log(RequestId(12), 77, "/tmp/quill-call-77.log")).unwrap();
    assert_eq!(v["@type"], "sendCallLog");
    assert_eq!(v["call_id"]["@type"], "inputCallDiscarded");
    assert_eq!(v["log_file"]["@type"], "inputFileLocal");
    assert_eq!(v["log_file"]["path"], "/tmp/quill-call-77.log");

    // Phase C2i: `getUserPrivacySettingRules
    // setting:UserPrivacySetting = UserPrivacySettingRules` (schema
    // 1.8.67 line 15620); `userPrivacySettingAllowCalls` (:9006).
    let v: serde_json::Value = serde_json::from_str(&get_user_privacy_setting_rules(
        RequestId(13),
        CallPrivacySetting::AllowCalls,
    ))
    .unwrap();
    assert_eq!(v["@type"], "getUserPrivacySettingRules");
    assert_eq!(v["setting"]["@type"], "userPrivacySettingAllowCalls");
    let v: serde_json::Value = serde_json::from_str(&get_user_privacy_setting_rules(
        RequestId(14),
        CallPrivacySetting::PeerToPeer,
    ))
    .unwrap();
    assert_eq!(
        v["setting"]["@type"],
        "userPrivacySettingAllowPeerToPeerCalls"
    );

    // Phase C2i: `setUserPrivacySettingRules
    // setting:UserPrivacySetting rules:userPrivacySettingRules = Ok`
    // (schema 1.8.67 line 15617); Nobody =
    // `[userPrivacySettingRuleRestrictAll]` (:8961).
    let v: serde_json::Value = serde_json::from_str(&set_user_privacy_setting_rules(
        RequestId(15),
        CallPrivacySetting::AllowCalls,
        PrivacyWho::Nobody,
    ))
    .unwrap();
    assert_eq!(v["@type"], "setUserPrivacySettingRules");
    assert_eq!(v["setting"]["@type"], "userPrivacySettingAllowCalls");
    assert_eq!(v["rules"]["@type"], "userPrivacySettingRules");
    let rules = v["rules"]["rules"].as_array().unwrap();
    assert_eq!(rules.len(), 1);
    assert_eq!(rules[0]["@type"], "userPrivacySettingRuleRestrictAll");

    // Everybody / Contacts map to AllowAll / AllowContacts; server
    // rule names map back, mixed/custom rules map to None.
    assert_eq!(
        PrivacyWho::Everybody.rules()[0]["@type"],
        "userPrivacySettingRuleAllowAll"
    );
    assert_eq!(
        PrivacyWho::Contacts.rules()[0]["@type"],
        "userPrivacySettingRuleAllowContacts"
    );
    assert_eq!(
        PrivacyWho::from_rule_names(&["userPrivacySettingRuleAllowContacts".to_string()]),
        Some(PrivacyWho::Contacts)
    );
    assert_eq!(
        PrivacyWho::from_rule_names(&[
            "userPrivacySettingRuleAllowUsers".to_string(),
            "userPrivacySettingRuleRestrictAll".to_string()
        ]),
        Some(PrivacyWho::Nobody)
    );
    assert_eq!(PrivacyWho::from_rule_names(&[]), None);
}

#[test]
fn send_call_signaling_data_shape() {
    let v: serde_json::Value =
        serde_json::from_str(&send_call_signaling_data(RequestId(7), 77, b"signal-bytes")).unwrap();
    assert_eq!(v["@type"], "sendCallSignalingData");
    assert_eq!(v["call_id"], 77);
    assert_eq!(v["data"], "c2lnbmFsLWJ5dGVz");
}

#[test]
fn send_call_debug_information_shape() {
    let v: serde_json::Value = serde_json::from_str(&send_call_debug_information(
        RequestId(8),
        77,
        r#"{"transport":"failed"}"#,
    ))
    .unwrap();
    assert_eq!(v["@type"], "sendCallDebugInformation");
    assert_eq!(v["call_id"]["@type"], "inputCallDiscarded");
    assert_eq!(v["call_id"]["call_id"], 77);
    assert_eq!(v["debug_information"], r#"{"transport":"failed"}"#);
}

/// Phase C3a: group-call request shapes — verified against the
/// pinned schema (1.8.67) constructors, never invented.
#[test]
fn group_call_request_shapes() {
    // The honest signaling-only join params: no device, no payload.
    let p = GroupCallJoinParams::honest_no_device();
    let pv = p.to_value();
    assert_eq!(pv["@type"], "groupCallJoinParameters");
    assert_eq!(pv["audio_source_id"], 0);
    assert_eq!(pv["payload"], "");
    assert_eq!(pv["is_muted"], false);
    assert_eq!(pv["is_my_video_enabled"], false);

    assert_eq!(
        MessageSenderRef::User(41).to_value()["@type"],
        "messageSenderUser"
    );
    assert_eq!(MessageSenderRef::User(41).to_value()["user_id"], 41);
    assert_eq!(
        MessageSenderRef::Chat(100).to_value()["@type"],
        "messageSenderChat"
    );
    assert_eq!(MessageSenderRef::Chat(100).to_value()["chat_id"], 100);

    assert_eq!(
        InputGroupCallRef::Link("https://t.me/abc".to_string()).to_value()["@type"],
        "inputGroupCallLink"
    );
    let mv = InputGroupCallRef::Message {
        chat_id: 100,
        message_id: 7,
    }
    .to_value();
    assert_eq!(mv["@type"], "inputGroupCallMessage");
    assert_eq!(mv["chat_id"], 100);
    assert_eq!(mv["message_id"], 7);

    let v: serde_json::Value =
        serde_json::from_str(&create_video_chat(RequestId(1), 100, "Standup", 0, false)).unwrap();
    assert_eq!(v["@type"], "createVideoChat");
    assert_eq!(v["chat_id"], 100);
    assert_eq!(v["title"], "Standup");
    assert_eq!(v["start_date"], 0);
    assert_eq!(v["is_rtmp_stream"], false);

    // `None` → null join_parameters: create the link only, don't join.
    let v: serde_json::Value =
        serde_json::from_str(&create_group_call(RequestId(2), None)).unwrap();
    assert_eq!(v["@type"], "createGroupCall");
    assert!(v["join_parameters"].is_null());
    let v: serde_json::Value =
        serde_json::from_str(&create_group_call(RequestId(3), Some(&p))).unwrap();
    assert_eq!(v["join_parameters"]["@type"], "groupCallJoinParameters");

    // `None` participant → null: join as self.
    let v: serde_json::Value =
        serde_json::from_str(&join_video_chat(RequestId(4), 555, None, &p, "")).unwrap();
    assert_eq!(v["@type"], "joinVideoChat");
    assert_eq!(v["group_call_id"], 555);
    assert!(v["participant_id"].is_null());
    assert_eq!(v["join_parameters"]["@type"], "groupCallJoinParameters");
    assert_eq!(v["invite_hash"], "");

    let v: serde_json::Value = serde_json::from_str(&join_video_chat(
        RequestId(5),
        555,
        Some(&MessageSenderRef::User(42)),
        &p,
        "hash",
    ))
    .unwrap();
    assert_eq!(v["participant_id"]["@type"], "messageSenderUser");
    assert_eq!(v["participant_id"]["user_id"], 42);

    let link = InputGroupCallRef::Link("https://t.me/abc".to_string());
    let v: serde_json::Value =
        serde_json::from_str(&join_group_call(RequestId(6), &link, &p)).unwrap();
    assert_eq!(v["@type"], "joinGroupCall");
    assert_eq!(v["input_group_call"]["@type"], "inputGroupCallLink");

    let v: serde_json::Value = serde_json::from_str(&get_group_call(RequestId(7), 555)).unwrap();
    assert_eq!(v["@type"], "getGroupCall");
    assert_eq!(v["group_call_id"], 555);

    let v: serde_json::Value =
        serde_json::from_str(&get_group_call_participants(RequestId(8), &link, 50)).unwrap();
    assert_eq!(v["@type"], "getGroupCallParticipants");
    assert_eq!(v["limit"], 50);

    let v: serde_json::Value =
        serde_json::from_str(&load_group_call_participants(RequestId(9), 555, 100)).unwrap();
    assert_eq!(v["@type"], "loadGroupCallParticipants");
    assert_eq!(v["group_call_id"], 555);
    assert_eq!(v["limit"], 100);

    let v: serde_json::Value = serde_json::from_str(&leave_group_call(RequestId(10), 555)).unwrap();
    assert_eq!(v["@type"], "leaveGroupCall");
    assert_eq!(v["group_call_id"], 555);

    let v: serde_json::Value = serde_json::from_str(&end_group_call(RequestId(11), 555)).unwrap();
    assert_eq!(v["@type"], "endGroupCall");
    assert_eq!(v["group_call_id"], 555);

    let v: serde_json::Value = serde_json::from_str(&toggle_group_call_is_my_video_enabled(
        RequestId(12),
        555,
        true,
    ))
    .unwrap();
    assert_eq!(v["@type"], "toggleGroupCallIsMyVideoEnabled");
    assert_eq!(v["is_my_video_enabled"], true);

    let v: serde_json::Value = serde_json::from_str(&toggle_group_call_is_my_video_paused(
        RequestId(13),
        555,
        true,
    ))
    .unwrap();
    assert_eq!(v["@type"], "toggleGroupCallIsMyVideoPaused");
    assert_eq!(v["is_my_video_paused"], true);

    let v: serde_json::Value = serde_json::from_str(&toggle_group_call_participant_is_muted(
        RequestId(14),
        555,
        &MessageSenderRef::User(42),
        true,
    ))
    .unwrap();
    assert_eq!(v["@type"], "toggleGroupCallParticipantIsMuted");
    assert_eq!(v["participant_id"]["user_id"], 42);
    assert_eq!(v["is_muted"], true);

    let v: serde_json::Value = serde_json::from_str(&toggle_group_call_participant_is_hand_raised(
        RequestId(15),
        555,
        &MessageSenderRef::User(41),
        true,
    ))
    .unwrap();
    assert_eq!(v["@type"], "toggleGroupCallParticipantIsHandRaised");
    assert_eq!(v["is_hand_raised"], true);

    // Exact schema name: toggleVideoChatMuteNewParticipants (not
    // toggleGroupCallMuteNewParticipants).
    let v: serde_json::Value = serde_json::from_str(&toggle_video_chat_mute_new_participants(
        RequestId(16),
        555,
        true,
    ))
    .unwrap();
    assert_eq!(v["@type"], "toggleVideoChatMuteNewParticipants");
    assert_eq!(v["mute_new_participants"], true);

    let v: serde_json::Value =
        serde_json::from_str(&set_video_chat_title(RequestId(17), 555, "Standup")).unwrap();
    assert_eq!(v["@type"], "setVideoChatTitle");
    assert_eq!(v["title"], "Standup");

    let v: serde_json::Value =
        serde_json::from_str(&get_video_chat_invite_link(RequestId(18), 555, true)).unwrap();
    assert_eq!(v["@type"], "getVideoChatInviteLink");
    assert_eq!(v["can_self_unmute"], true);

    let v: serde_json::Value =
        serde_json::from_str(&get_video_chat_available_participants(RequestId(19), -100)).unwrap();
    assert_eq!(v["@type"], "getVideoChatAvailableParticipants");
    assert_eq!(v["chat_id"], -100);

    let v: serde_json::Value = serde_json::from_str(&set_video_chat_default_participant(
        RequestId(20),
        -100,
        &MessageSenderRef::Chat(-200),
    ))
    .unwrap();
    assert_eq!(v["@type"], "setVideoChatDefaultParticipant");
    assert_eq!(v["default_participant_id"]["@type"], "messageSenderChat");
    assert_eq!(v["default_participant_id"]["chat_id"], -200);

    // Phase C2h: the video-chat management requests.
    let v: serde_json::Value =
        serde_json::from_str(&revoke_group_call_invite_link(RequestId(21), 555)).unwrap();
    assert_eq!(v["@type"], "revokeGroupCallInviteLink");
    assert_eq!(v["group_call_id"], 555);

    let v: serde_json::Value = serde_json::from_str(&start_group_call_recording(
        RequestId(22),
        555,
        "Sync",
        true,
        false,
    ))
    .unwrap();
    assert_eq!(v["@type"], "startGroupCallRecording");
    assert_eq!(v["group_call_id"], 555);
    assert_eq!(v["title"], "Sync");
    assert_eq!(v["record_video"], true);
    assert_eq!(v["use_portrait_orientation"], false);

    let v: serde_json::Value =
        serde_json::from_str(&end_group_call_recording(RequestId(23), 555)).unwrap();
    assert_eq!(v["@type"], "endGroupCallRecording");
    assert_eq!(v["group_call_id"], 555);

    let v: serde_json::Value =
        serde_json::from_str(&start_scheduled_video_chat(RequestId(24), 555)).unwrap();
    assert_eq!(v["@type"], "startScheduledVideoChat");
    assert_eq!(v["group_call_id"], 555);

    // `toggleVideoChatEnabledStartNotification` (schema 1.8.67,
    // :14282).
    let v: serde_json::Value = serde_json::from_str(&toggle_video_chat_enabled_start_notification(
        RequestId(25),
        555,
        true,
    ))
    .unwrap();
    assert_eq!(v["@type"], "toggleVideoChatEnabledStartNotification");
    assert_eq!(v["group_call_id"], 555);
    assert_eq!(v["enabled_start_notification"], true);

    let v: serde_json::Value =
        serde_json::from_str(&get_video_chat_rtmp_url(RequestId(26), 51)).unwrap();
    assert_eq!(v["@type"], "getVideoChatRtmpUrl");
    assert_eq!(v["chat_id"], 51);

    let v: serde_json::Value =
        serde_json::from_str(&replace_video_chat_rtmp_url(RequestId(27), 51)).unwrap();
    assert_eq!(v["@type"], "replaceVideoChatRtmpUrl");
    assert_eq!(v["chat_id"], 51);

    let v: serde_json::Value =
        serde_json::from_str(&send_group_call_message(RequestId(28), 555, "hello")).unwrap();
    assert_eq!(v["@type"], "sendGroupCallMessage");
    assert_eq!(v["group_call_id"], 555);
    assert_eq!(v["text"]["@type"], "formattedText");
    assert_eq!(v["text"]["text"], "hello");
    assert_eq!(v["paid_message_star_count"], 0);

    let v: serde_json::Value = serde_json::from_str(&toggle_group_call_are_messages_allowed(
        RequestId(29),
        555,
        false,
    ))
    .unwrap();
    assert_eq!(v["@type"], "toggleGroupCallAreMessagesAllowed");
    assert_eq!(v["are_messages_allowed"], false);

    let v: serde_json::Value =
        serde_json::from_str(&decline_group_call_invitation(RequestId(19), 100, 7)).unwrap();
    assert_eq!(v["@type"], "declineGroupCallInvitation");
    assert_eq!(v["chat_id"], 100);
    assert_eq!(v["message_id"], 7);
}

#[test]
fn group_call_participant_management_shapes_match_1_8_67() {
    // Phase C2f: `inviteGroupCallParticipant group_call_id:int32
    // user_id:int53 is_video:Bool = InviteGroupCallParticipantResult`
    // (schema 1.8.67, line 14375).
    let v: serde_json::Value =
        serde_json::from_str(&invite_group_call_participant(RequestId(31), 555, 42, true)).unwrap();
    assert_eq!(v["@type"], "inviteGroupCallParticipant");
    assert_eq!(v["@extra"], "31");
    assert_eq!(v["group_call_id"], 555);
    assert_eq!(v["user_id"], 42);
    assert_eq!(v["is_video"], true);

    // Phase C2f: `banGroupCallParticipants group_call_id:int32
    // user_ids:vector<int64> = Ok` (schema 1.8.67, line 14385).
    let v: serde_json::Value =
        serde_json::from_str(&ban_group_call_participants(RequestId(32), 555, &[42, 43])).unwrap();
    assert_eq!(v["@type"], "banGroupCallParticipants");
    assert_eq!(v["@extra"], "32");
    assert_eq!(v["group_call_id"], 555);
    assert_eq!(v["user_ids"], serde_json::json!([42, 43]));

    // Phase C2f: `setGroupCallParticipantVolumeLevel
    // group_call_id:int32 participant_id:MessageSender
    // volume_level:int32 = Ok` (schema 1.8.67, line 14438).
    let v: serde_json::Value = serde_json::from_str(&set_group_call_participant_volume_level(
        RequestId(33),
        555,
        &MessageSenderRef::User(42),
        15000,
    ))
    .unwrap();
    assert_eq!(v["@type"], "setGroupCallParticipantVolumeLevel");
    assert_eq!(v["@extra"], "33");
    assert_eq!(v["group_call_id"], 555);
    assert_eq!(v["participant_id"]["@type"], "messageSenderUser");
    assert_eq!(v["participant_id"]["user_id"], 42);
    assert_eq!(v["volume_level"], 15000);
}
