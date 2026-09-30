use super::*;

#[test]
fn community_updates_parsed() {
    // Slice (communities backend core): `updateCommunity` (schema
    // 1.8.67, line 10726) carries the full `community` (line 2305);
    // `updateCommunityFullInfo` (line 10753) carries the
    // `communityFullInfo` (line 2319) with its own `community_id`.
    let env = parse_envelope(
            r#"{"@type":"updateCommunity","community":{"@type":"community","id":42,"have_access":true,"name":"Rustaceans","date":1759000000,"status":{"@type":"communityMemberStatusCreator"},"permissions":{"@type":"communityPermissions","can_edit_chat_list":true}}}"#,
        )
        .unwrap();
    match env.payload {
        EnvelopePayload::UpdateCommunity { community } => {
            assert_eq!(
                community,
                ParsedCommunity {
                    id: 42,
                    have_access: true,
                    name: "Rustaceans".to_string(),
                    date: 1759000000,
                }
            );
        }
        other => panic!("{other:?}"),
    }
    let env = parse_envelope(
            r#"{"@type":"updateCommunityFullInfo","community_id":42,"community_full_info":{"@type":"communityFullInfo","chats":[{"@type":"communityChat","chat_id":7,"can_view_history":true,"is_hidden":false}],"administrator_count":3,"banned_count":1,"add_chat_request_count":0}}"#,
        )
        .unwrap();
    match env.payload {
        EnvelopePayload::UpdateCommunityFullInfo {
            community_id,
            full_info,
        } => {
            assert_eq!(community_id, 42);
            assert_eq!(
                full_info,
                ParsedCommunityFullInfo {
                    chats: vec![ParsedCommunityChat {
                        chat_id: 7,
                        can_view_history: true,
                        is_hidden: false,
                    }],
                    administrator_count: 3,
                    banned_count: 1,
                    add_chat_request_count: 0,
                }
            );
        }
        other => panic!("{other:?}"),
    }
    // `communityId` (schema line 2264) is the `createCommunity`
    // response (line 11806) — the driver chains it into
    // `loadCommunityFullInfo`.
    let env = parse_envelope(r#"{"@type":"communityId","id":42}"#).unwrap();
    match env.payload {
        EnvelopePayload::CommunityId { id } => assert_eq!(id, 42),
        other => panic!("{other:?}"),
    }
}

#[test]
fn update_supergroup_parses_own_status() {
    // Phase A1: `supergroup.status` (schema 1.8.67 line 2746) is the
    // viewer's own `chatMemberStatus*` — the slow-mode bypass signal.
    let env = parse_envelope(
            r#"{"@type":"updateSupergroup","supergroup":{"@type":"supergroup","id":16,"is_forum":false,"status":{"@type":"chatMemberStatusAdministrator"}}}"#,
        )
        .unwrap();
    match env.payload {
        EnvelopePayload::UpdateSupergroup {
            supergroup_id,
            status,
            ..
        } => {
            assert_eq!(supergroup_id, 16);
            assert_eq!(status, ChannelMemberStatus::Administrator);
        }
        other => panic!("{other:?}"),
    }
    // Missing status → Unknown (gated, no bypass).
    let env = parse_envelope(
            r#"{"@type":"updateSupergroup","supergroup":{"@type":"supergroup","id":17,"is_forum":false}}"#,
        )
        .unwrap();
    match env.payload {
        EnvelopePayload::UpdateSupergroup { status, .. } => {
            assert_eq!(status, ChannelMemberStatus::Unknown);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn update_new_chat_parses_photo_small() {
    // Parity slice: `chat.photo.small` (`chatPhotoInfo`, schema 1.8.67
    // lines 762/3627) is kept for the chat-list avatar.
    let json = r#"{"@type":"updateNewChat","chat":{"id":11,"title":"Demo","type":{"@type":"chatTypePrivate","user_id":11},"unread_count":0,"photo":{"@type":"chatPhotoInfo","small":{"@type":"file","id":91,"size":24,"expected_size":24,"local":{"@type":"localFile","path":"","can_be_downloaded":true,"can_be_deleted":false,"is_downloading_active":false,"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0},"remote":{"@type":"remoteFile","id":"x","unique_id":"u","is_uploading_active":false,"is_uploading_completed":false,"uploaded_size":0}},"big":{"@type":"file","id":92,"size":0,"expected_size":0,"local":{"@type":"localFile","path":"","can_be_downloaded":true,"can_be_deleted":false,"is_downloading_active":false,"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0},"remote":{"@type":"remoteFile","id":"x","unique_id":"u","is_uploading_active":false,"is_uploading_completed":false,"uploaded_size":0}},"minithumbnail":null,"has_animation":false,"is_personal":false}}}"#;
    let env = parse_envelope(json).unwrap();
    match env.payload {
        EnvelopePayload::UpdateNewChat { chat_id, photo, .. } => {
            assert_eq!(chat_id.0, 11);
            let file = photo.expect("chat photo");
            assert_eq!(file.id.0, 91);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn update_new_chat_without_photo_has_none() {
    let json = r#"{"@type":"updateNewChat","chat":{"id":11,"title":"Demo","type":{"@type":"chatTypePrivate","user_id":11},"unread_count":0,"photo":null}}"#;
    let env = parse_envelope(json).unwrap();
    match env.payload {
        EnvelopePayload::UpdateNewChat { photo, .. } => assert!(photo.is_none()),
        other => panic!("{other:?}"),
    }
}

#[test]
fn update_chat_photo_parsed() {
    // Parity slice: `updateChatPhoto` (schema 1.8.67 line 10488).
    let json = r#"{"@type":"updateChatPhoto","chat_id":11,"photo":{"@type":"chatPhotoInfo","small":{"@type":"file","id":93,"size":24,"expected_size":24,"local":{"@type":"localFile","path":"","can_be_downloaded":true,"can_be_deleted":false,"is_downloading_active":false,"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0},"remote":{"@type":"remoteFile","id":"x","unique_id":"u","is_uploading_active":false,"is_uploading_completed":false,"uploaded_size":0}},"big":{"@type":"file","id":94,"size":0,"expected_size":0,"local":{"@type":"localFile","path":"","can_be_downloaded":true,"can_be_deleted":false,"is_downloading_active":false,"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0},"remote":{"@type":"remoteFile","id":"x","unique_id":"u","is_uploading_active":false,"is_uploading_completed":false,"uploaded_size":0}},"minithumbnail":null,"has_animation":false,"is_personal":false}}"#;
    let env = parse_envelope(json).unwrap();
    match env.payload {
        EnvelopePayload::UpdateChatPhoto { chat_id, photo } => {
            assert_eq!(chat_id.0, 11);
            assert_eq!(photo.map(|f| f.id.0), Some(93));
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn update_new_chat_parses_message_auto_delete_time() {
    // Phase B4: `chat.message_auto_delete_time` (schema 1.8.67, lines
    // 3616 / 3627) — chat-level auto-delete / self-destruct timer.
    let json = r#"{"@type":"updateNewChat","chat":{"id":41,"title":"Zed","type":{"@type":"chatTypeSecret","secret_chat_id":7,"user_id":41},"unread_count":0,"message_auto_delete_time":3600}}"#;
    let env = parse_envelope(json).unwrap();
    match env.payload {
        EnvelopePayload::UpdateNewChat {
            message_auto_delete_time,
            ..
        } => assert_eq!(message_auto_delete_time, 3600),
        other => panic!("{other:?}"),
    }
}

#[test]
fn update_new_chat_without_auto_delete_time_defaults_to_zero() {
    let json = r#"{"@type":"updateNewChat","chat":{"id":11,"title":"Demo","type":{"@type":"chatTypePrivate","user_id":11},"unread_count":0}}"#;
    let env = parse_envelope(json).unwrap();
    match env.payload {
        EnvelopePayload::UpdateNewChat {
            message_auto_delete_time,
            ..
        } => assert_eq!(message_auto_delete_time, 0),
        other => panic!("{other:?}"),
    }
}

#[test]
fn update_chat_message_auto_delete_time_parsed() {
    // Phase B4: `updateChatMessageAutoDeleteTime` (schema 1.8.67,
    // line 10549).
    let json = r#"{"@type":"updateChatMessageAutoDeleteTime","chat_id":41,"message_auto_delete_time":86400}"#;
    let env = parse_envelope(json).unwrap();
    match env.payload {
        EnvelopePayload::UpdateChatMessageAutoDeleteTime {
            chat_id,
            message_auto_delete_time,
        } => {
            assert_eq!(chat_id.0, 41);
            assert_eq!(message_auto_delete_time, 86400);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn update_new_chat_video_chat_parsed() {
    // `chat.video_chat` on `updateNewChat` (schema 1.8.67, lines
    // 3576 / 3627): `group_call_id` 0 → None.
    let json = r#"{"@type":"updateNewChat","chat":{"id":100,"title":"Team standup","type":{"@type":"chatTypeSupergroup","supergroup_id":100,"is_channel":false},"unread_count":0,"video_chat":{"@type":"videoChat","group_call_id":555,"has_participants":false,"default_participant_id":null}}}"#;
    let env = parse_envelope(json).unwrap();
    match env.payload {
        EnvelopePayload::UpdateNewChat { video_chat, .. } => {
            let v = video_chat.expect("video chat present");
            assert_eq!(v.group_call_id, 555);
            assert!(!v.has_participants);
        }
        other => panic!("{other:?}"),
    }
    let json = r#"{"@type":"updateNewChat","chat":{"id":100,"title":"Team standup","type":{"@type":"chatTypeSupergroup","supergroup_id":100,"is_channel":false},"unread_count":0,"video_chat":{"@type":"videoChat","group_call_id":0,"has_participants":false,"default_participant_id":null}}}"#;
    let env = parse_envelope(json).unwrap();
    match env.payload {
        EnvelopePayload::UpdateNewChat { video_chat, .. } => {
            assert!(video_chat.is_none());
        }
        other => panic!("{other:?}"),
    }
}
