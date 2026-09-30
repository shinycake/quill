use super::*;
use crate::ids::UserId;

// Phase 6: full `user` parse (schema 1.8.67 line 2403) — names,
// username from `usernames.active_usernames` (no singular `username`
// field in 1.8.67), phone, contact flag, status, and the
// `profile_photo.small` file id.
#[test]
fn update_user_parses_full_user() {
    let json = r#"{"@type":"updateUser","user":{"id":31,"first_name":"Ada","last_name":"Lovelace","usernames":{"@type":"usernames","active_usernames":["adalove"],"disabled_usernames":[],"editable_username":"adalove","collectible_usernames":[]},"phone_number":"+15550131","status":{"@type":"userStatusOnline","expires":9999999999},"profile_photo":{"@type":"profilePhoto","id":7,"small":{"@type":"file","id":41,"size":0,"expected_size":0,"local":{"@type":"localFile","path":"","can_be_downloaded":true,"can_delete":false,"is_downloading_active":false,"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0},"remote":{"@type":"remoteFile","id":"","is_uploading_active":false,"is_uploading_completed":false,"uploaded_size":0}},"big":{"@type":"file","id":42,"size":0,"expected_size":0,"local":{"@type":"localFile","path":"","can_be_downloaded":true,"can_delete":false,"is_downloading_active":false,"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0},"remote":{"@type":"remoteFile","id":"","is_uploading_active":false,"is_uploading_completed":false,"uploaded_size":0}},"minithumbnail":null,"has_animation":false,"is_personal":false},"accent_color_id":0,"background_custom_emoji_id":0,"upgraded_gift_colors":null,"profile_accent_color_id":-1,"profile_background_custom_emoji_id":0,"emoji_status":null,"is_contact":true,"is_mutual_contact":true,"is_close_friend":false,"verification_status":null,"is_premium":false,"is_support":false,"restriction_info":null,"active_story_state":null,"restricts_new_chats":false,"paid_message_star_count":0,"have_access":true,"type":{"@type":"userTypeRegular"},"language_code":"en","added_to_attachment_menu":false}}"#;
    let env = parse_envelope(json).unwrap();
    match env.payload {
        EnvelopePayload::UpdateUser { user_id, user } => {
            assert_eq!(user_id, UserId(31));
            assert_eq!(user.first_name, "Ada");
            assert_eq!(user.last_name, "Lovelace");
            assert_eq!(user.display_name(), "Ada Lovelace");
            assert_eq!(user.initials(), "AL");
            assert_eq!(user.username, "adalove");
            // A5: full username lists parsed from `usernames`.
            assert_eq!(user.active_usernames, vec!["adalove".to_string()]);
            assert!(user.disabled_usernames.is_empty());
            assert_eq!(user.editable_username, "adalove");
            assert_eq!(user.phone_number, "+15550131");
            assert!(user.is_contact);
            assert!(!user.is_bot);
            assert_eq!(user.status, UserStatusKind::Online);
            assert!(user.status.is_online());
            assert_eq!(user.photo_small_file_id, 41);
        }
        other => panic!("{other:?}"),
    }
}

// A5: `checkChatUsernameResult*` (schema 1.8.67, lines 8583–8598)
// parse to `UsernameCheckResult`.
#[test]
fn check_chat_username_results_parsed() {
    let cases = [
        ("checkChatUsernameResultOk", UsernameCheckResult::Available),
        (
            "checkChatUsernameResultUsernameOccupied",
            UsernameCheckResult::Occupied,
        ),
        (
            "checkChatUsernameResultUsernameInvalid",
            UsernameCheckResult::Invalid,
        ),
        (
            "checkChatUsernameResultUsernamePurchasable",
            UsernameCheckResult::Purchasable,
        ),
        (
            "checkChatUsernameResultPublicChatsTooMany",
            UsernameCheckResult::PublicChatsTooMany,
        ),
        (
            "checkChatUsernameResultPublicGroupsUnavailable",
            UsernameCheckResult::PublicGroupsUnavailable,
        ),
    ];
    for (type_name, expected) in cases {
        let env = parse_envelope(&format!("{{\"@type\":\"{type_name}\"}}")).unwrap();
        match env.payload {
            EnvelopePayload::CheckChatUsernameResult(result) => {
                assert_eq!(result, expected, "{type_name}");
            }
            other => panic!("{type_name}: {other:?}"),
        }
    }
}

// A5: `userFullInfo.photo.id` (`chatPhoto.id`, schema 1.8.67 line
// 1030) is kept as the `deleteProfilePhoto` target.
#[test]
fn user_full_info_photo_id_parsed() {
    let env = parse_envelope(
            r#"{"@type":"userFullInfo","bio":null,"photo":{"@type":"chatPhoto","id":987,"sizes":[]},"block_list":null,"can_be_called":false,"has_private_calls":false,"has_private_forwards":false,"has_read_receipts":false,"no_upgraded_gift_colors":false}"#,
        )
        .unwrap();
    match env.payload {
        EnvelopePayload::UserFullInfo { photo_id, .. } => {
            assert_eq!(photo_id, Some(987));
        }
        other => panic!("{other:?}"),
    }
}

// Phase 6: status buckets map to their display text; offline with a
// timestamp formats relative to "now"; unknown constructors → Empty.
#[test]
fn user_status_display_buckets() {
    assert_eq!(UserStatusKind::Online.display_at(1_700_000_000), "online");
    assert_eq!(
        UserStatusKind::Recently.display_at(1_700_000_000),
        "last seen recently"
    );
    assert_eq!(
        UserStatusKind::LastWeek.display_at(1_700_000_000),
        "last seen within a week"
    );
    assert_eq!(
        UserStatusKind::LastMonth.display_at(1_700_000_000),
        "last seen within a month"
    );
    assert_eq!(
        UserStatusKind::Offline {
            was_online: 1_699_999_970
        }
        .display_at(1_700_000_000),
        "last seen just now"
    );
    assert_eq!(
        UserStatusKind::Offline {
            was_online: 1_699_999_400
        }
        .display_at(1_700_000_000),
        "last seen 10m ago"
    );
    assert_eq!(
        UserStatusKind::Offline { was_online: 0 }.display_at(1_700_000_000),
        "last seen a long time ago"
    );
    assert!(UserStatusKind::Empty.display_at(1_700_000_000).is_empty());
}

#[test]
fn update_user_status_parsed() {
    // Schema 1.8.67 line 10729.
    let env = parse_envelope(
            r#"{"@type":"updateUserStatus","user_id":31,"status":{"@type":"userStatusLastWeek","by_my_privacy_settings":false}}"#,
        )
        .unwrap();
    match env.payload {
        EnvelopePayload::UpdateUserStatus { user_id, status } => {
            assert_eq!(user_id, UserId(31));
            assert_eq!(status, UserStatusKind::LastWeek);
        }
        other => panic!("{other:?}"),
    }
    // Unknown status constructor → Empty, never a parse failure.
    let env = parse_envelope(
        r#"{"@type":"updateUserStatus","user_id":31,"status":{"@type":"userStatusFromTheFuture"}}"#,
    )
    .unwrap();
    match env.payload {
        EnvelopePayload::UpdateUserStatus { status, .. } => {
            assert_eq!(status, UserStatusKind::Empty)
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn update_profile_accent_colors_parsed() {
    // Slice A12: `updateProfileAccentColors` (schema 1.8.67, line
    // 10964) — the palette and the settable accent ids.
    let env = parse_envelope(
            r#"{"@type":"updateProfileAccentColors","colors":[{"@type":"profileAccentColor","id":3,"light_theme_colors":{"@type":"profileAccentColors","palette_colors":[43776,65280],"background_colors":[1313280],"story_colors":[1966080,255]},"dark_theme_colors":{"@type":"profileAccentColors","palette_colors":[262144],"background_colors":[],"story_colors":[]}}],"available_accent_color_ids":[1,3,5]}"#,
        )
        .unwrap();
    match env.payload {
        EnvelopePayload::UpdateProfileAccentColors {
            colors,
            available_ids,
        } => {
            assert_eq!(available_ids, vec![1, 3, 5]);
            assert_eq!(colors.len(), 1);
            assert_eq!(colors[0].id, 3);
            assert_eq!(colors[0].swatch_rgb(), 0xAB00);
        }
        other => panic!("{other:?}"),
    }
    // Empty palette → tolerated, never a parse failure.
    let env = parse_envelope(
        r#"{"@type":"updateProfileAccentColors","colors":[],"available_accent_color_ids":[]}"#,
    )
    .unwrap();
    match env.payload {
        EnvelopePayload::UpdateProfileAccentColors {
            colors,
            available_ids,
        } => {
            assert!(colors.is_empty());
            assert!(available_ids.is_empty());
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn users_response_parsed() {
    // `getContacts` → `users` (schema 1.8.67 lines 14520 / 2471).
    let env =
        parse_envelope(r#"{"@type":"users","@extra":"3","total_count":2,"user_ids":[31,32]}"#)
            .unwrap();
    match env.payload {
        EnvelopePayload::Users { user_ids } => assert_eq!(user_ids, vec![31, 32]),
        other => panic!("{other:?}"),
    }
}

#[test]
fn user_full_info_bio_parsed() {
    // Phase 6: `bio:formattedText` is kept alongside `bot_info`.
    let env = parse_envelope(
            r#"{"@type":"userFullInfo","@extra":"4","block_list":null,"bio":{"@type":"formattedText","text":"CANARY bio text","entities":[]},"birthdate":null,"bot_info":null}"#,
        )
        .unwrap();
    match env.payload {
        EnvelopePayload::UserFullInfo {
            bio,
            bot_info,
            photo,
            photo_id: _,
            blocked,
        } => {
            assert_eq!(bio, "CANARY bio text");
            assert!(bot_info.is_none());
            assert!(photo.is_none());
            assert!(!blocked);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn user_full_info_photo_parsed_from_chat_photo() {
    // `userFullInfo.photo:chatPhoto` (schema 1.8.67, lines 1030/2468):
    // the preferred size is `type == "m"`; the file is kept.
    let file = |id: i32| {
        format!(
            r#"{{"@type":"file","id":{id},"size":100,"expected_size":100,"local":{{"@type":"localFile","path":"","can_be_downloaded":true,"can_delete":false,"is_downloading_active":false,"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0}},"remote":{{"@type":"remoteFile","id":"","is_uploading_active":false,"is_uploading_completed":false,"uploaded_size":0}}}}"#
        )
    };
    let json = format!(
        r#"{{"@type":"userFullInfo","@extra":"9","bio":null,"bot_info":null,"photo":{{"@type":"chatPhoto","id":1,"added_date":1,"minithumbnail":null,"sizes":[{{"@type":"photoSize","type":"s","photo":{s},"width":90,"height":90,"progressive_sizes":[]}},{{"@type":"photoSize","type":"m","photo":{m},"width":320,"height":320,"progressive_sizes":[]}}],"animation":null,"small_animation":null,"sticker":null}}}}"#,
        s = file(901),
        m = file(902),
    );
    let env = parse_envelope(&json).unwrap();
    match env.payload {
        EnvelopePayload::UserFullInfo { photo, .. } => {
            let photo = photo.expect("chatPhoto size");
            assert_eq!(photo.id.0, 902);
        }
        other => panic!("{other:?}"),
    }
    // No photo field at all → None.
    let env =
        parse_envelope(r#"{"@type":"userFullInfo","@extra":"10","bio":null,"bot_info":null}"#)
            .unwrap();
    match env.payload {
        EnvelopePayload::UserFullInfo { photo, .. } => assert!(photo.is_none()),
        other => panic!("{other:?}"),
    }
}
