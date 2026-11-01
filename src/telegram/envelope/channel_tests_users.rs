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
        EnvelopePayload::Users(UsersPayload::UpdateUser { user_id, user }) => {
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
            EnvelopePayload::Users(UsersPayload::CheckChatUsernameResult(result)) => {
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
        EnvelopePayload::Users(UsersPayload::UserFullInfo { photo_id, .. }) => {
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
        EnvelopePayload::Users(UsersPayload::UpdateUserStatus {
            user_id, status, ..
        }) => {
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
        EnvelopePayload::Users(UsersPayload::UpdateUserStatus { status, .. }) => {
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
        EnvelopePayload::Chats(ChatsPayload::UpdateProfileAccentColors {
            colors,
            available_ids,
        }) => {
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
        EnvelopePayload::Chats(ChatsPayload::UpdateProfileAccentColors {
            colors,
            available_ids,
        }) => {
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
        EnvelopePayload::Users(UsersPayload::Users { user_ids }) => {
            assert_eq!(user_ids, vec![31, 32])
        }
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
        EnvelopePayload::Users(UsersPayload::UserFullInfo {
            extras: _,
            bio,
            bot_info,
            photo,
            photo_id: _,
            blocked,
            share_theme: _,
            ratings: _,
            voice_notes_restricted,
        }) => {
            assert_eq!(bio, "CANARY bio text");
            assert!(bot_info.is_none());
            assert!(photo.is_none());
            assert!(!blocked);
            assert!(!voice_notes_restricted, "absent flag reads false");
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
        EnvelopePayload::Users(UsersPayload::UserFullInfo { photo, .. }) => {
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
        EnvelopePayload::Users(UsersPayload::UserFullInfo { photo, .. }) => {
            assert!(photo.is_none())
        }
        other => panic!("{other:?}"),
    }
}

// Rich-text premium gate: `user.is_premium` (schema 1.8.67 line 2403)
// parses through — `Session::my_is_premium` gates
// `premiumFeatureRichMessages` on it.
#[test]
fn update_user_parses_is_premium() {
    let json = r#"{"@type":"updateUser","user":{"@type":"user","id":31,"first_name":"Ada","last_name":"","usernames":null,"phone_number":"","status":null,"profile_photo":null,"accent_color_id":0,"background_custom_emoji_id":0,"upgraded_gift_colors":null,"profile_accent_color_id":-1,"profile_background_custom_emoji_id":0,"emoji_status":null,"is_contact":false,"is_mutual_contact":false,"is_close_friend":false,"verification_status":null,"is_premium":true,"is_support":false,"restriction_info":null,"active_story_state":null,"restricts_new_chats":false,"paid_message_star_count":0,"have_access":true,"type":{"@type":"userTypeRegular"},"language_code":"en","added_to_attachment_menu":false}}"#;
    let env = parse_envelope(json).unwrap();
    match env.payload {
        EnvelopePayload::Users(UsersPayload::UpdateUser { user, .. }) => assert!(user.is_premium),
        other => panic!("{other:?}"),
    }
}

// The TON wallet card on a profile (`userFullInfo.ton_wallet_card`) gives
// the profile a balance; without a card there is none.
#[test]
fn a_wallet_card_gives_the_profile_a_balance() {
    let with_card = serde_json::json!({"ton_wallet_card": {
        "@type": "tonWalletCard", "address": "EQabc", "gram_amount": 1_500_000_000i64,
        "stickers": [], "background_nft_address": ""}});
    let extras = envelope_types::parse_user_profile_extras(Some(&with_card));
    assert_eq!(extras.ton_wallet_card_balance, Some(1_500_000_000));
    let none = serde_json::json!({"ton_wallet_card": null});
    let extras = envelope_types::parse_user_profile_extras(Some(&none));
    assert_eq!(extras.ton_wallet_card_balance, None);
    let extras = envelope_types::parse_user_profile_extras(None);
    assert_eq!(extras.ton_wallet_card_balance, None);
}

#[test]
fn cover_facts_parsed_from_user_status_and_full_info() {
    let env = parse_envelope(
        r#"{"@type":"updateUser","user":{"id":21,"first_name":"Forecast","is_support":false,"status":{"@type":"userStatusEmpty"},"type":{"@type":"userTypeBot","can_be_edited":false,"can_join_groups":true,"can_read_all_group_messages":false,"has_main_web_app":false,"has_topics":false,"allows_users_to_create_topics":false,"can_manage_bots":false,"is_inline":false,"inline_query_placeholder":"","supports_guest_queries":false,"is_guard":false,"need_location":false,"can_connect_to_business":false,"can_be_added_to_attachment_menu":false,"active_user_count":1284567}}}"#,
    )
    .unwrap();
    match env.payload {
        EnvelopePayload::Users(UsersPayload::UpdateUser { user, .. }) => {
            assert_eq!(user.bot_active_users, 1_284_567);
            assert!(!user.is_support);
        }
        other => panic!("{other:?}"),
    }
    let env = parse_envelope(
        r#"{"@type":"updateUser","user":{"id":42,"first_name":"Support","is_support":true,"status":{"@type":"userStatusRecently","by_my_privacy_settings":true},"type":{"@type":"userTypeRegular","active_user_count":5}}}"#,
    )
    .unwrap();
    match env.payload {
        EnvelopePayload::Users(UsersPayload::UpdateUser { user, .. }) => {
            assert!(user.is_support);
            assert!(user.status_hidden_by_me);
            assert_eq!(user.bot_active_users, 0, "only bots have monthly users");
        }
        other => panic!("{other:?}"),
    }
    let env = parse_envelope(
        r#"{"@type":"updateUserStatus","user_id":31,"status":{"@type":"userStatusLastMonth","by_my_privacy_settings":true}}"#,
    )
    .unwrap();
    match env.payload {
        EnvelopePayload::Users(UsersPayload::UpdateUserStatus { hidden_by_me, .. }) => {
            assert!(hidden_by_me)
        }
        other => panic!("{other:?}"),
    }
    let env = parse_envelope(
        r#"{"@type":"updateUserFullInfo","user_id":31,"user_full_info":{"@type":"userFullInfo","rating":{"@type":"userRating","level":7,"is_maximum_level_reached":false,"rating":1840,"current_level_rating":1500,"next_level_rating":2500},"pending_rating":null,"pending_rating_date":0}}"#,
    )
    .unwrap();
    match env.payload {
        EnvelopePayload::Users(UsersPayload::UpdateUserFullInfo { ratings, .. }) => {
            assert_eq!(ratings.rating.map(|r| r.level), Some(7));
            assert_eq!(ratings.pending, None);
        }
        other => panic!("{other:?}"),
    }
}

// robustness-8: names are filtered once, where they are parsed, so the
// chat list, call tiles, member lists, forward headers and window titles
// all get the same text.
#[test]
fn names_drop_stacked_marks_and_tag_characters_when_parsed() {
    let zalgo = "Ada\u{301}\u{302}\u{303}\u{304}\u{305}";
    let tagged = "Love\u{E0041}\u{E0042}lace";
    let user =
        serde_json::json!({"@type": "user", "id": 31, "first_name": zalgo, "last_name": tagged});
    let user = parse_user(&user).unwrap();
    assert_eq!(user.display_name(), "Ada\u{301}\u{302} Lovelace");
    let origin = serde_json::json!({"@type": "messageOriginHiddenUser", "sender_name": zalgo});
    match parse_message_origin(Some(&origin)) {
        Some(MessageOrigin::HiddenUser { sender_name }) => {
            assert_eq!(sender_name, "Ada\u{301}\u{302}");
        }
        other => panic!("not a hidden origin: {other:?}"),
    }
    let title = serde_json::json!({"@type": "updateChatTitle", "chat_id": 5, "title": tagged});
    match parse_envelope(&title.to_string()).unwrap().payload {
        EnvelopePayload::Chats(ChatsPayload::UpdateChatTitle { title, .. }) => {
            assert_eq!(title, "Lovelace");
        }
        _ => panic!("not a title update"),
    }
}

#[test]
fn contact_call_and_invite_titles_drop_marks_and_tags_when_parsed() {
    let zalgo = "Ada\u{301}\u{302}\u{303}\u{304}\u{305}";
    let tagged = "Love\u{E0041}\u{E0042}lace";
    let contact = serde_json::json!({"@type": "messageContact", "contact":
        {"phone_number": "1", "first_name": zalgo, "last_name": tagged, "vcard": "", "user_id": 0}});
    match parse_message_contact(&contact).0 {
        MessageContent::Contact(card) => {
            assert_eq!(card.first_name, "Ada\u{301}\u{302}");
            assert_eq!(card.last_name, "Lovelace");
        }
        other => panic!("not a contact: {other:?}"),
    }
    let call = serde_json::json!({"@type": "groupCall", "id": 4, "title": tagged});
    assert_eq!(
        parse_group_call(Some(&call)).map(|call| call.title),
        Some("Lovelace".to_string())
    );
    let invite = serde_json::json!({"@type": "chatInviteLinkInfo", "title": zalgo});
    match parse_envelope(&invite.to_string()).unwrap().payload {
        EnvelopePayload::Groups(GroupsPayload::ChatInviteLinkInfo { title, .. }) => {
            assert_eq!(title, "Ada\u{301}\u{302}");
        }
        _ => panic!("not an invite preview"),
    }
}
