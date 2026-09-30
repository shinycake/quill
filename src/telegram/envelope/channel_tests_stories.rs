use super::*;
use crate::ids::FileId;

#[test]
fn update_chat_active_stories_parses_tray_fields() {
    // `updateChatActiveStories` (schema 1.8.67 line 10911): keeps the
    // tray sort key and read state; `can_be_archived` is dropped.
    let json = r#"{"@type":"updateChatActiveStories","active_stories":{"@type":"chatActiveStories","chat_id":11,"list":{"@type":"storyListMain"},"order":"9000","can_be_archived":true,"max_read_story_id":4,"stories":[{"@type":"storyInfo","story_id":5,"date":1700000000,"is_for_close_friends":false,"is_live":false},{"@type":"storyInfo","story_id":3,"date":1699990000,"is_for_close_friends":true,"is_live":false}]}}"#;
    let env = parse_envelope(json).unwrap();
    match env.payload {
        EnvelopePayload::UpdateChatActiveStories { active_stories } => {
            assert_eq!(active_stories.chat_id, 11);
            assert_eq!(active_stories.list, Some(StoryListView::Main));
            assert_eq!(active_stories.order, 9000);
            assert_eq!(active_stories.max_read_story_id, 4);
            assert_eq!(active_stories.stories.len(), 2);
            assert_eq!(active_stories.stories[0].story_id, 5);
            assert!(active_stories.stories[1].is_for_close_friends);
            assert!(active_stories.has_unread());
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn chat_active_stories_null_list_and_no_unread() {
    // `list` may be null (schema line 6778); all stories read.
    let json = r#"{"@type":"chatActiveStories","chat_id":12,"list":null,"order":"0","can_be_archived":false,"max_read_story_id":9,"stories":[{"@type":"storyInfo","story_id":9,"date":1,"is_for_close_friends":false,"is_live":false}]}"#;
    let env = parse_envelope(json).unwrap();
    match env.payload {
        EnvelopePayload::ChatActiveStories { active_stories } => {
            assert_eq!(active_stories.list, None);
            assert!(!active_stories.has_unread());
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn story_chosen_reaction_interactions_and_flags_parsed() {
    // Phase 9.2: `chosen_reaction_type` (reactionTypeEmoji), the
    // `storyInteractionInfo` counters, and the `can_be_deleted` /
    // `can_be_replied` / `can_get_interactions` gates (schema 1.8.67
    // lines 6712 / 6742).
    let size = story_photo_file_json(61, "\"\"", false);
    let json = format!(
        r#"{{"@type":"story","id":7,"poster_chat_id":11,"date":1700000000,"content":{{"@type":"storyContentPhoto","photo":{{"@type":"photo","has_stickers":false,"sizes":[{size}]}}}},"chosen_reaction_type":{{"@type":"reactionTypeEmoji","emoji":"❤"}},"interaction_info":{{"@type":"storyInteractionInfo","view_count":42,"forward_count":3,"reaction_count":7,"recent_viewer_user_ids":[]}},"can_be_deleted":true,"can_be_replied":true,"can_get_interactions":true,"caption":{{"@type":"formattedText","text":"","entities":[]}}}}"#,
    );
    let env = parse_envelope(&json).unwrap();
    match env.payload {
        EnvelopePayload::Story { story, .. } => {
            assert_eq!(story.chosen_reaction_emoji.as_deref(), Some("❤"));
            let info = story.interaction_info.expect("interaction_info");
            assert!(info.any_nonzero());
            assert_eq!(info.view_count, 42);
            assert_eq!(info.forward_count, 3);
            assert_eq!(info.reaction_count, 7);
            assert!(story.can_be_deleted);
            assert!(story.can_be_replied);
            assert!(story.can_get_interactions);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn story_reaction_absent_or_non_emoji_parses_to_none() {
    // `chosen_reaction_type: null` and an empty emoji parse to
    // `(None, None)` on both halves.
    let reaction_json = |reaction: &str| {
        format!(
            r#"{{"@type":"story","id":7,"poster_chat_id":11,"date":1,"content":{{"@type":"storyContentUnsupported"}},"chosen_reaction_type":{reaction},"caption":{{"@type":"formattedText","text":"","entities":[]}}}}"#,
        )
    };
    for reaction in ["null", r#"{"@type":"reactionTypeEmoji","emoji":""}"#] {
        let env = parse_envelope(&reaction_json(reaction)).unwrap();
        match env.payload {
            EnvelopePayload::Story { story, .. } => {
                assert_eq!(story.chosen_reaction_emoji, None, "reaction {reaction}");
                assert_eq!(story.chosen_reaction_extra, None, "reaction {reaction}");
            }
            other => panic!("{other:?}"),
        }
    }
}

#[test]
fn story_interaction_info_absent_stays_none() {
    let json = r#"{"@type":"story","id":7,"poster_chat_id":11,"date":1,"content":{"@type":"storyContentUnsupported"},"caption":{"@type":"formattedText","text":"","entities":[]}}"#;
    let env = parse_envelope(json).unwrap();
    match env.payload {
        EnvelopePayload::Story { story, .. } => {
            assert_eq!(story.interaction_info, None);
            assert!(!story.can_be_deleted);
            assert!(!story.can_be_replied);
            assert!(!story.can_get_interactions);
            // Phase 9.5: new gates default off, repost info absent.
            assert!(!story.can_be_edited);
            assert!(!story.can_set_privacy_settings);
            assert!(!story.can_be_forwarded);
            assert!(!story.is_edited);
            assert_eq!(story.repost_info, None);
            assert_eq!(story.area_link_url, None);
            assert!(story.area_reaction_emojis.is_empty());
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn story_repost_info_and_manage_gates_parsed() {
    // Phase 9.5: `repost_info` (storyRepostInfo, td_api.tl:6705) with
    // a public-story origin (td_api.tl:6696), the edit / privacy /
    // forward gates, and `is_edited` (td_api.tl:6742).
    let json = r#"{"@type":"story","id":8,"poster_chat_id":11,"date":1,"is_edited":true,"can_be_edited":true,"can_be_forwarded":true,"can_set_privacy_settings":true,"repost_info":{"@type":"storyRepostInfo","origin":{"@type":"storyOriginPublicStory","chat_id":22,"story_id":3},"is_content_modified":false},"content":{"@type":"storyContentUnsupported"},"caption":{"@type":"formattedText","text":"","entities":[]}}"#;
    let env = parse_envelope(json).unwrap();
    match env.payload {
        EnvelopePayload::Story { story, .. } => {
            assert!(story.can_be_edited);
            assert!(story.can_be_forwarded);
            assert!(story.can_set_privacy_settings);
            assert!(story.is_edited);
            let repost = story.repost_info.expect("repost_info");
            assert_eq!(
                repost.origin,
                StoryOriginView::PublicStory {
                    chat_id: 22,
                    story_id: 3
                }
            );
            assert!(!repost.is_content_modified);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn story_repost_hidden_user_origin_parsed() {
    // `storyOriginHiddenUser` (td_api.tl:6699); unknown origin types
    // and null repost_info parse to None.
    let json = |origin: &str| {
        format!(
            r#"{{"@type":"story","id":8,"poster_chat_id":11,"date":1,"repost_info":{{"@type":"storyRepostInfo","origin":{origin},"is_content_modified":true}},"content":{{"@type":"storyContentUnsupported"}},"caption":{{"@type":"formattedText","text":"","entities":[]}}}}"#,
        )
    };
    let env = parse_envelope(&json(
        r#"{"@type":"storyOriginHiddenUser","poster_name":"Mystery Poster"}"#,
    ))
    .unwrap();
    match env.payload {
        EnvelopePayload::Story { story, .. } => {
            let repost = story.repost_info.expect("repost_info");
            assert_eq!(
                repost.origin,
                StoryOriginView::HiddenUser {
                    poster_name: "Mystery Poster".into()
                }
            );
            assert!(repost.is_content_modified);
        }
        other => panic!("{other:?}"),
    }
    for repost_info in [
            "null".to_string(),
            r#"{"@type":"storyRepostInfo","origin":{"@type":"storyOriginNope"},"is_content_modified":false}"#.to_string(),
        ] {
            let env = parse_envelope(&json(&repost_info)).unwrap();
            match env.payload {
                EnvelopePayload::Story { story, .. } => {
                    assert_eq!(story.repost_info, None, "repost {repost_info}");
                }
                other => panic!("{other:?}"),
            }
        }
}

#[test]
fn story_area_link_and_reaction_texts_prefill_edit() {
    // Phase 9.5: output `storyArea` link (td_api.tl:6552) and
    // suggested-reaction (td_api.tl:6546) areas surface as the edit
    // surface's text inputs; other area types are ignored.
    let json = r#"{"@type":"story","id":8,"poster_chat_id":11,"date":1,"areas":[{"@type":"storyArea","position":{"@type":"storyAreaPosition","x_percentage":1.0,"y_percentage":1.0,"width_percentage":1.0,"height_percentage":1.0,"rotation_angle":0.0,"corner_radius_percentage":0.0},"type":{"@type":"storyAreaTypeLink","url":"https://t.me/quill"}},{"@type":"storyArea","position":{"@type":"storyAreaPosition","x_percentage":1.0,"y_percentage":1.0,"width_percentage":1.0,"height_percentage":1.0,"rotation_angle":0.0,"corner_radius_percentage":0.0},"type":{"@type":"storyAreaTypeSuggestedReaction","reaction_type":{"@type":"reactionTypeEmoji","emoji":"🔥"},"total_count":2,"is_dark":false,"is_flipped":false}},{"@type":"storyArea","position":{"@type":"storyAreaPosition","x_percentage":1.0,"y_percentage":1.0,"width_percentage":1.0,"height_percentage":1.0,"rotation_angle":0.0,"corner_radius_percentage":0.0},"type":{"@type":"storyAreaTypeWeather","temperature":21.0,"emoji":"☀","background_color":0}}],"content":{"@type":"storyContentUnsupported"},"caption":{"@type":"formattedText","text":"","entities":[]}}"#;
    let env = parse_envelope(json).unwrap();
    match env.payload {
        EnvelopePayload::Story { story, .. } => {
            assert_eq!(story.area_link_url.as_deref(), Some("https://t.me/quill"));
            assert_eq!(story.area_reaction_emojis, vec!["🔥".to_string()]);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn update_story_deleted_parsed() {
    // `updateStoryDeleted` (schema 1.8.67 line 10898).
    let env =
        parse_envelope(r#"{"@type":"updateStoryDeleted","story_poster_chat_id":11,"story_id":7}"#)
            .unwrap();
    match env.payload {
        EnvelopePayload::UpdateStoryDeleted {
            poster_chat_id,
            story_id,
        } => {
            assert_eq!(poster_chat_id, 11);
            assert_eq!(story_id, 7);
        }
        other => panic!("{other:?}"),
    }
}

/// Phase C3a: `updateGroupCall` / `updateGroupCallParticipant` /
/// `updateGroupCallParticipants` /
/// `updateGroupCallVerificationState` / `updateChatVideoChat`
/// parsing (schema 1.8.67, lines 10819 / 10824 / 10830 / 10836 /
/// 10576).
#[test]
fn update_group_call_parsed() {
    let json = r#"{"@type":"updateGroupCall","group_call":{"@type":"groupCall","id":555,"unique_id":"999","title":"Team standup","invite_link":"","paid_message_star_count":0,"scheduled_start_date":0,"enabled_start_notification":false,"is_active":true,"is_video_chat":true,"is_live_story":false,"is_rtmp_stream":false,"is_joined":true,"need_rejoin":false,"is_owned":false,"can_be_managed":true,"participant_count":4,"has_hidden_listeners":false,"loaded_all_participants":false,"message_sender_id":null,"recent_speakers":[{"@type":"groupCallRecentSpeaker","participant_id":{"@type":"messageSenderUser","user_id":43},"is_speaking":true}],"is_my_video_enabled":false,"is_my_video_paused":false,"can_enable_video":true,"mute_new_participants":false,"can_toggle_mute_new_participants":true,"can_send_messages":true,"are_messages_allowed":true,"can_toggle_are_messages_allowed":false,"can_delete_messages":false,"record_duration":0,"is_video_recorded":false,"duration":0}}"#;
    let env = parse_envelope(json).unwrap();
    match env.payload {
        EnvelopePayload::UpdateGroupCall { group_call } => {
            assert_eq!(group_call.id, 555);
            assert_eq!(group_call.title, "Team standup");
            assert!(group_call.is_active);
            assert!(group_call.is_video_chat);
            assert!(group_call.is_joined);
            assert!(!group_call.need_rejoin);
            assert!(group_call.can_be_managed);
            assert_eq!(group_call.participant_count, 4);
            assert_eq!(group_call.recent_speakers.len(), 1);
            assert_eq!(
                group_call.recent_speakers[0].0,
                MessageSender::User { user_id: 43 }
            );
            assert!(group_call.recent_speakers[0].1);
            assert!(group_call.can_toggle_mute_new_participants);
            // Phase C2h: recording + in-call chat fields are parsed,
            // not just carried in the fixture.
            assert_eq!(group_call.scheduled_start_date, 0);
            assert!(group_call.can_send_messages);
            assert!(group_call.are_messages_allowed);
            assert!(!group_call.can_toggle_are_messages_allowed);
            assert!(!group_call.can_delete_messages);
            assert_eq!(group_call.record_duration, 0);
            assert!(!group_call.is_video_recorded);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn update_group_call_recording_live_parsed() {
    let json = r#"{"@type":"updateGroupCall","group_call":{"@type":"groupCall","id":555,"title":"Weekly design sync","is_active":true,"is_video_chat":true,"record_duration":125,"is_video_recorded":true}}"#;
    let env = parse_envelope(json).unwrap();
    match env.payload {
        EnvelopePayload::UpdateGroupCall { group_call } => {
            assert_eq!(group_call.record_duration, 125);
            assert!(group_call.is_video_recorded);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn rtmp_url_parsed() {
    let json = r#"{"@type":"rtmpUrl","url":"rtmp://dc1-rtmp.telegram.org:443/live","stream_key":"demo-stream-key-9f3a2b1c"}"#;
    let env = parse_envelope(json).unwrap();
    match env.payload {
        EnvelopePayload::RtmpUrl { url, stream_key } => {
            assert_eq!(url, "rtmp://dc1-rtmp.telegram.org:443/live");
            assert_eq!(stream_key, "demo-stream-key-9f3a2b1c");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn group_call_message_updates_parsed() {
    let json = r#"{"@type":"updateNewGroupCallMessage","group_call_id":555,"message":{"@type":"groupCallMessage","message_id":7,"sender_id":{"@type":"messageSenderUser","user_id":41},"date":1788000000,"text":{"@type":"formattedText","text":"Can everyone hear me?","entities":[]},"paid_message_star_count":0,"is_from_owner":false,"can_be_deleted":true}}"#;
    let env = parse_envelope(json).unwrap();
    match env.payload {
        EnvelopePayload::UpdateNewGroupCallMessage {
            group_call_id,
            message,
        } => {
            assert_eq!(group_call_id, 555);
            assert_eq!(message.message_id, 7);
            assert_eq!(message.sender_id, MessageSender::User { user_id: 41 });
            assert_eq!(message.text, "Can everyone hear me?");
            assert!(!message.is_from_owner);
            assert!(message.can_be_deleted);
        }
        other => panic!("{other:?}"),
    }

    let json = r#"{"@type":"updateGroupCallMessageSendFailed","group_call_id":555,"message_id":9,"error":{"@type":"error","code":400,"message":"MESSAGE_TOO_LONG"}}"#;
    let env = parse_envelope(json).unwrap();
    match env.payload {
        EnvelopePayload::UpdateGroupCallMessageSendFailed {
            group_call_id,
            message_id,
            error,
        } => {
            assert_eq!(group_call_id, 555);
            assert_eq!(message_id, 9);
            assert_eq!(error.code, 400);
        }
        other => panic!("{other:?}"),
    }

    let json =
        r#"{"@type":"updateGroupCallMessagesDeleted","group_call_id":555,"message_ids":[7,8]}"#;
    let env = parse_envelope(json).unwrap();
    match env.payload {
        EnvelopePayload::UpdateGroupCallMessagesDeleted {
            group_call_id,
            message_ids,
        } => {
            assert_eq!(group_call_id, 555);
            assert_eq!(message_ids, vec![7, 8]);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn update_group_call_participant_parsed() {
    let json = r#"{"@type":"updateGroupCallParticipant","group_call_id":555,"participant":{"@type":"groupCallParticipant","participant_id":{"@type":"messageSenderUser","user_id":44},"audio_source_id":7,"screen_sharing_audio_source_id":0,"video_info":null,"screen_sharing_video_info":null,"bio":"","is_current_user":false,"is_speaking":false,"is_hand_raised":true,"can_be_muted_for_all_users":true,"can_be_unmuted_for_all_users":false,"can_be_muted_for_current_user":true,"can_be_unmuted_for_current_user":true,"is_muted_for_all_users":false,"is_muted_for_current_user":false,"can_unmute_self":false,"volume_level":10000,"order":"zz9"}}"#;
    let env = parse_envelope(json).unwrap();
    match env.payload {
        EnvelopePayload::UpdateGroupCallParticipant {
            group_call_id,
            participant,
        } => {
            assert_eq!(group_call_id, 555);
            assert_eq!(
                participant.participant_id,
                MessageSender::User { user_id: 44 }
            );
            assert!(participant.is_hand_raised);
            assert!(!participant.is_speaking);
            assert!(participant.can_be_muted_for_all_users);
            assert_eq!(participant.order, "zz9");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn update_group_call_participant_video_info_parsed() {
    // Phase C2g: `groupCallParticipantVideoInfo` /
    // `groupCallVideoSourceGroup` (TDLib 1.8.67,
    // `schema/td_api.tl:7157` / `:7163`).
    let json = r#"{"@type":"updateGroupCallParticipant","group_call_id":555,"participant":{"@type":"groupCallParticipant","participant_id":{"@type":"messageSenderUser","user_id":42},"audio_source_id":0,"screen_sharing_audio_source_id":0,"video_info":{"@type":"groupCallParticipantVideoInfo","source_groups":[{"@type":"groupCallVideoSourceGroup","semantics":"SIM","source_ids":[111,112]}],"endpoint_id":"ep-42","is_paused":false},"screen_sharing_video_info":{"@type":"groupCallParticipantVideoInfo","source_groups":[{"@type":"groupCallVideoSourceGroup","semantics":"SIM","source_ids":[222]}],"endpoint_id":"ep-42-screen","is_paused":true},"bio":"","is_current_user":false,"is_speaking":false,"is_hand_raised":false,"can_be_muted_for_all_users":false,"can_be_unmuted_for_all_users":false,"can_be_muted_for_current_user":false,"can_be_unmuted_for_current_user":false,"is_muted_for_all_users":false,"is_muted_for_current_user":false,"can_unmute_self":false,"volume_level":10000,"order":"a2"}}"#;
    let env = parse_envelope(json).unwrap();
    match env.payload {
        EnvelopePayload::UpdateGroupCallParticipant { participant, .. } => {
            assert!(participant.video_enabled);
            assert!(participant.screen_sharing_enabled);
            let camera = participant.video_info.expect("camera video info");
            assert_eq!(camera.endpoint_id, "ep-42");
            assert!(!camera.is_paused);
            assert_eq!(camera.source_groups.len(), 1);
            assert_eq!(camera.source_groups[0].semantics, "SIM");
            assert_eq!(camera.source_groups[0].source_ids, vec![111, 112]);
            let screen = participant
                .screen_sharing_video_info
                .expect("screen video info");
            assert_eq!(screen.endpoint_id, "ep-42-screen");
            assert!(screen.is_paused);
            assert_eq!(screen.source_groups[0].source_ids, vec![222]);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn update_group_call_participants_parsed() {
    let json = r#"{"@type":"updateGroupCallParticipants","group_call_id":555,"participant_user_ids":[41,42,43]}"#;
    let env = parse_envelope(json).unwrap();
    match env.payload {
        EnvelopePayload::UpdateGroupCallParticipants {
            group_call_id,
            participant_user_ids,
        } => {
            assert_eq!(group_call_id, 555);
            assert_eq!(participant_user_ids, vec![41, 42, 43]);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn update_group_call_verification_state_parsed() {
    let json = r#"{"@type":"updateGroupCallVerificationState","group_call_id":555,"generation":7,"emojis":["🍎","🍌","🍒","🍇"]}"#;
    let env = parse_envelope(json).unwrap();
    match env.payload {
        EnvelopePayload::UpdateGroupCallVerificationState {
            group_call_id,
            generation,
            emojis,
        } => {
            assert_eq!(group_call_id, 555);
            assert_eq!(generation, 7);
            assert_eq!(emojis, vec!["🍎", "🍌", "🍒", "🍇"]);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn update_chat_video_chat_parsed() {
    // `updateChatVideoChat` (schema 1.8.67, line 10576) with
    // `videoChat` (line 3579).
    let json = r#"{"@type":"updateChatVideoChat","chat_id":100,"video_chat":{"@type":"videoChat","group_call_id":555,"has_participants":true,"default_participant_id":{"@type":"messageSenderUser","user_id":41}}}"#;
    let env = parse_envelope(json).unwrap();
    match env.payload {
        EnvelopePayload::UpdateChatVideoChat {
            chat_id,
            video_chat,
        } => {
            assert_eq!(chat_id, 100);
            assert_eq!(video_chat.group_call_id, 555);
            assert!(video_chat.has_participants);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn update_story_post_succeeded_parsed() {
    // `updateStoryPostSucceeded` (schema 1.8.67 line 10901).
    let json = r#"{"@type":"updateStoryPostSucceeded","story":{"@type":"story","id":7,"poster_chat_id":11,"date":1,"content":{"@type":"storyContentUnsupported"},"caption":{"@type":"formattedText","text":"","entities":[]}},"old_story_id":6}"#;
    let env = parse_envelope(json).unwrap();
    match env.payload {
        EnvelopePayload::UpdateStoryPostSucceeded {
            story,
            old_story_id,
            ..
        } => {
            assert_eq!(story.id, 7);
            assert_eq!(story.poster_chat_id, 11);
            assert_eq!(old_story_id, 6);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn update_story_post_failed_parsed() {
    // `updateStoryPostFailed` (schema 1.8.67 line 10907).
    let json = r#"{"@type":"updateStoryPostFailed","story":{"@type":"story","id":7,"poster_chat_id":11,"date":1,"content":{"@type":"storyContentUnsupported"},"caption":{"@type":"formattedText","text":"","entities":[]}},"error":{"@type":"error","code":400,"message":"STORY_SEND_FAILED"},"error_type":{"@type":"canPostStoryResultOk"}}"#;
    let env = parse_envelope(json).unwrap();
    match env.payload {
        EnvelopePayload::UpdateStoryPostFailed { story, error } => {
            assert_eq!(story.id, 7);
            assert_eq!(story.poster_chat_id, 11);
            assert_eq!(error.code, 400);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn can_post_story_results_parsed() {
    // Phase 9.3: `canPostStoryResult*` (schema 1.8.67 lines
    // 8535–8553) — the `canPostStory` answer.
    let cases = [
        (
            r#"{"@type":"canPostStoryResultOk","story_count":3}"#,
            CanPostStoryResult::Ok { story_count: 3 },
        ),
        (
            r#"{"@type":"canPostStoryResultPremiumNeeded"}"#,
            CanPostStoryResult::PremiumNeeded,
        ),
        (
            r#"{"@type":"canPostStoryResultBoostNeeded"}"#,
            CanPostStoryResult::BoostNeeded,
        ),
        (
            r#"{"@type":"canPostStoryResultActiveStoryLimitExceeded"}"#,
            CanPostStoryResult::ActiveStoryLimitExceeded,
        ),
        (
            r#"{"@type":"canPostStoryResultWeeklyLimitExceeded","retry_after":9000}"#,
            CanPostStoryResult::WeeklyLimitExceeded { retry_after: 9000 },
        ),
        (
            r#"{"@type":"canPostStoryResultMonthlyLimitExceeded","retry_after":86400}"#,
            CanPostStoryResult::MonthlyLimitExceeded { retry_after: 86400 },
        ),
        (
            r#"{"@type":"canPostStoryResultLiveStoryIsActive","story_id":12}"#,
            CanPostStoryResult::LiveStoryIsActive { story_id: 12 },
        ),
    ];
    for (json, expected) in cases {
        let env = parse_envelope(json).unwrap();
        match env.payload {
            EnvelopePayload::CanPostStoryResult { result } => {
                assert_eq!(result, expected);
            }
            other => panic!("{other:?}"),
        }
    }
    assert!(CanPostStoryResult::Ok { story_count: 0 }.can_post());
    assert!(!CanPostStoryResult::PremiumNeeded.can_post());
    assert!(
        CanPostStoryResult::WeeklyLimitExceeded { retry_after: 9000 }
            .user_message()
            .contains("2h 30m")
    );
}

#[test]
fn available_reactions_parsed_and_custom_emoji_dropped() {
    // `availableReactions` (schema 1.8.67 line 7330): the story
    // picker keeps emoji reactions; custom-emoji rows are dropped.
    let json = r#"{"@type":"availableReactions","top_reactions":[{"@type":"availableReaction","type":{"@type":"reactionTypeEmoji","emoji":"❤"},"needs_premium":false},{"@type":"availableReaction","type":{"@type":"reactionTypeCustomEmoji","custom_emoji_id":"123"},"needs_premium":true},{"@type":"availableReaction","type":{"@type":"reactionTypeEmoji","emoji":"👍"},"needs_premium":false}],"recent_reactions":[],"popular_reactions":[],"allow_custom_emoji":false,"are_tags":false,"unavailability_reason":null}"#;
    let env = parse_envelope(json).unwrap();
    match env.payload {
        EnvelopePayload::StoryAvailableReactions { reactions } => {
            assert_eq!(reactions.len(), 2);
            assert_eq!(reactions[0].emoji, "❤");
            assert!(!reactions[0].needs_premium);
            assert_eq!(reactions[1].emoji, "👍");
        }
        other => panic!("{other:?}"),
    }
}

fn story_photo_file_json(id: i32, path: &str, completed: bool) -> String {
    format!(
        r#"{{"@type":"photoSize","type":"x","photo":{{"@type":"file","id":{id},"size":24,"expected_size":24,"local":{{"@type":"localFile","path":{path},"can_be_downloaded":true,"can_be_deleted":false,"is_downloading_active":false,"is_downloading_completed":{completed},"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0}},"remote":{{"@type":"remoteFile","id":"x","unique_id":"u","is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":24}}}},"width":800,"height":600,"progressive_sizes":[]}}"#,
        path = serde_json::to_string(path).unwrap(),
        completed = completed,
    )
}

#[test]
fn get_story_photo_parses_content_and_caption() {
    let size = story_photo_file_json(61, "\"\"", false);
    let json = format!(
        r#"{{"@type":"story","id":5,"poster_chat_id":11,"poster_id":null,"date":1700000000,"is_being_posted":false,"is_being_edited":false,"is_edited":false,"is_posted_to_chat_page":false,"is_visible_only_for_self":false,"can_be_added_to_album":false,"can_be_deleted":false,"can_be_edited":false,"can_be_forwarded":true,"can_be_replied":false,"can_set_privacy_settings":false,"can_toggle_is_posted_to_chat_page":false,"can_get_statistics":false,"can_get_interactions":false,"has_expired_viewers":false,"repost_info":null,"interaction_info":null,"chosen_reaction_type":null,"privacy_settings":null,"content":{{"@type":"storyContentPhoto","photo":{{"@type":"photo","has_stickers":false,"sizes":[{size}]}}}},"areas":[],"caption":{{"@type":"formattedText","text":"CANARY_STORY_caption","entities":[]}},"album_ids":[]}}"#,
    );
    let env = parse_envelope(&json).unwrap();
    match env.payload {
        EnvelopePayload::Story { story, files } => {
            assert_eq!(story.id, 5);
            assert_eq!(story.poster_chat_id, 11);
            assert_eq!(story.date, 1700000000);
            assert_eq!(story.caption, "CANARY_STORY_caption");
            match story.content {
                StoryContentView::Photo { sizes } => {
                    assert_eq!(sizes.len(), 1);
                    assert_eq!(sizes[0].file_id, FileId(61));
                    assert_eq!(sizes[0].width, 800);
                }
                other => panic!("{other:?}"),
            }
            assert!(files.iter().any(|f| f.id == FileId(61)));
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn get_story_video_parses_thumb_and_duration() {
    // `storyVideo` (schema 1.8.67 line 6633): `duration` is a double,
    // the thumbnail is a bare `thumbnail` constructor.
    let thumb = r#"{"@type":"thumbnail","format":{"@type":"thumbnailFormatJpeg"},"width":320,"height":240,"file":{"@type":"file","id":71,"size":10,"expected_size":10,"local":{"@type":"localFile","path":"","can_be_downloaded":true,"can_be_deleted":false,"is_downloading_active":false,"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0},"remote":{"@type":"remoteFile","id":"x","unique_id":"u","is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":10}}}"#;
    let clip = r#"{"@type":"file","id":72,"size":100,"expected_size":100,"local":{"@type":"localFile","path":"","can_be_downloaded":true,"can_be_deleted":false,"is_downloading_active":false,"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0},"remote":{"@type":"remoteFile","id":"y","unique_id":"v","is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":100}}"#;
    let json = format!(
        r#"{{"@type":"story","id":6,"poster_chat_id":11,"date":1700000000,"content":{{"@type":"storyContentVideo","video":{{"@type":"storyVideo","duration":12.4,"width":720,"height":1280,"has_stickers":false,"is_animation":false,"minithumbnail":null,"thumbnail":{thumb},"preload_prefix_size":0,"cover_frame_timestamp":0.0,"video":{clip}}},"alternative_video":null}},"caption":{{"@type":"formattedText","text":"","entities":[]}}}}"#,
    );
    let env = parse_envelope(&json).unwrap();
    match env.payload {
        EnvelopePayload::Story { story, files } => {
            match story.content {
                StoryContentView::Video {
                    thumb_file_id,
                    thumb_width,
                    thumb_height,
                    duration_secs,
                    file_id,
                } => {
                    assert_eq!(thumb_file_id, Some(FileId(71)));
                    assert_eq!(thumb_width, 320);
                    assert_eq!(thumb_height, 240);
                    assert_eq!(duration_secs, 12);
                    assert_eq!(file_id, FileId(72));
                }
                other => panic!("{other:?}"),
            }
            assert!(files.iter().any(|f| f.id == FileId(71)));
            assert!(files.iter().any(|f| f.id == FileId(72)));
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn story_live_and_unsupported_degrade_to_placeholder() {
    for (content, is_live) in [
        (
            r#"{"@type":"storyContentLive","group_call_id":7,"is_rtmp_stream":false}"#,
            true,
        ),
        (r#"{"@type":"storyContentUnsupported"}"#, false),
        (r#"{"@type":"storyContentQuantum"}"#, false),
    ] {
        let json = format!(
            r#"{{"@type":"updateStory","story":{{"@type":"story","id":8,"poster_chat_id":11,"date":1,"content":{content},"caption":{{"@type":"formattedText","text":"","entities":[]}}}}}}"#,
        );
        let env = parse_envelope(&json).unwrap();
        match env.payload {
            EnvelopePayload::Story { story, .. } => match (&story.content, is_live) {
                (StoryContentView::Live, true) => {}
                (StoryContentView::Unsupported, false) => {}
                (other, _) => panic!("{other:?}"),
            },
            other => panic!("{other:?}"),
        }
    }
}
