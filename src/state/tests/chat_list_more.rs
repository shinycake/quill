//! State reducer tests: the chat list (continued from chat_list.rs).
use super::common::*;
use super::*;

#[test]
fn cl3_block_list_update_sets_blocked() {
    // Slice CL3: `updateChatBlockList` (schema 1.8.67, line 10594)
    // tracks the peer's block state.
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.chats.insert(14, placeholder_chat(ChatId(14)));
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateChatBlockList","chat_id":14,"block_list":{"@type":"blockListMain"}}"#,
    );
    assert!(session.chats.get(&14).expect("chat").blocked);
}

#[test]
fn cl3_report_and_block_errors_surface() {
    // Slice CL3: a refused `reportChat` /
    // `setMessageSenderBlockList` surfaces in `chat_action_error` —
    // never shown as success.
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let extra = session.request(RequestPurpose::ReportChat, Some(ChatId(14)));
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"CHAT_REPORT_FAILED"}}"#,
            extra.0
        ),
    );
    assert_eq!(
        session.chats_state.chat_action_error.last(),
        Some("could not report the chat (error 400)")
    );
    let extra = session.request(
        RequestPurpose::Users(UsersPurpose::SetMessageSenderBlockList { block: true }),
        Some(ChatId(14)),
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":403,"message":"FORBIDDEN"}}"#,
            extra.0
        ),
    );
    assert_eq!(
        session.chats_state.chat_action_error.last(),
        Some("could not change the block state (error 403)")
    );
}

#[test]
fn cl2_reorder_pinned_chats_swaps_orders() {
    // Slice CL2: `reorder_pinned_chats` permutes the pinned chats'
    // `order` values into the new sequence so `rebuild_main_order`
    // keeps it; the returned pairs restore the old arrangement.
    let (mut session, _sink) = session();
    for (id, order) in [(11i64, 300i64), (12, 200), (13, 100), (14, 50)] {
        let mut chat = placeholder_chat(ChatId(id));
        chat.in_main_list = true;
        chat.order = order;
        chat.is_pinned = id != 14;
        session.chats.insert(id, chat);
    }
    session.rebuild_main_order();
    let previous = session.reorder_pinned_chats(false, &[13, 11, 12]);
    assert_eq!(previous, vec![(11, 300), (12, 200), (13, 100)]);
    let ids: Vec<i64> = session.ordered_chats().iter().map(|c| c.id.0).collect();
    assert_eq!(ids, vec![13, 11, 12, 14]);
    // Id-set mismatch changes nothing.
    let noop = session.reorder_pinned_chats(false, &[13, 11]);
    assert!(noop.is_empty());
    let ids: Vec<i64> = session.ordered_chats().iter().map(|c| c.id.0).collect();
    assert_eq!(ids, vec![13, 11, 12, 14]);
}

#[test]
fn cl2_pin_order_error_rolls_back_and_surfaces() {
    // Slice CL2: a refused `setPinnedChats` restores the pre-reorder
    // order values and surfaces in `chat_action_error`.
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    for (id, order) in [(11i64, 300i64), (12, 200)] {
        let mut chat = placeholder_chat(ChatId(id));
        chat.in_main_list = true;
        chat.order = order;
        chat.is_pinned = true;
        session.chats.insert(id, chat);
    }
    let extra = session.request(RequestPurpose::SetPinnedChats, None);
    session
        .requests
        .pending_mut(extra)
        .expect("pending")
        .rollback = Some(RequestRollback::ChatPinOrder {
        previous: vec![(11, 300), (12, 200)],
        archived: false,
    });
    session.reorder_pinned_chats(false, &[12, 11]);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"CHAT_NOT_MODIFIED"}}"#,
            extra.0
        ),
    );
    assert_eq!(session.chats.get(&11).expect("chat").order, 300);
    assert_eq!(session.chats.get(&12).expect("chat").order, 200);
    assert_eq!(
        session.chats_state.chat_action_error.last(),
        Some("could not reorder pinned chats (error 400)")
    );
}

#[test]
fn cl2_archive_settings_fetch_stores_and_set_rolls_back() {
    // Slice CL2: the `getArchiveChatListSettings` answer lands in
    // the session (purpose-matched); a refused
    // `setArchiveChatListSettings` restores the previous settings
    // and surfaces.
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let extra = session.request(RequestPurpose::GetArchiveChatListSettings, None);
    session.chat_list.archive_settings_loading = true;
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"archiveChatListSettings","@extra":"{}","archive_and_mute_new_chats_from_unknown_users":true,"keep_unmuted_chats_archived":false,"keep_chats_from_folders_archived":true}}"#,
            extra.0
        ),
    );
    assert_eq!(
        session.chat_list.archive_chat_list_settings,
        Some(ArchiveChatListSettings {
            archive_and_mute_new_chats_from_unknown_users: true,
            keep_unmuted_chats_archived: false,
            keep_chats_from_folders_archived: true,
        })
    );
    assert!(!session.chat_list.archive_settings_loading);

    let old = session.chat_list.archive_chat_list_settings;
    let extra = session.request(RequestPurpose::SetArchiveChatListSettings, None);
    session
        .requests
        .pending_mut(extra)
        .expect("pending")
        .rollback = Some(RequestRollback::ArchiveChatListSettings { previous: old });
    session.chat_list.archive_chat_list_settings = Some(ArchiveChatListSettings {
        keep_unmuted_chats_archived: true,
        ..old.unwrap_or_default()
    });
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"ARCHIVE_SETTINGS_INVALID"}}"#,
            extra.0
        ),
    );
    assert_eq!(session.chat_list.archive_chat_list_settings, old);
    assert_eq!(
        session.chats_state.chat_action_error.last(),
        Some("could not save archive settings (error 400)")
    );
}

#[test]
fn cl2_mark_all_read_and_clear_recents_errors_surface() {
    // Slice CL2: refused `readChatList` / `clearRecentlyFoundChats`
    // surface in `chat_action_error` — never silent.
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let extra = session.request(RequestPurpose::ReadChatList, None);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":500,"message":"READ_FAILED"}}"#,
            extra.0
        ),
    );
    assert_eq!(
        session.chats_state.chat_action_error.last(),
        Some("could not mark all chats as read (error 500)")
    );
    let extra = session.request(RequestPurpose::ClearRecentlyFoundChats, None);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":500,"message":"CLEAR_FAILED"}}"#,
            extra.0
        ),
    );
    assert_eq!(
        session.chats_state.chat_action_error.last(),
        Some("could not clear recent searches (error 500)")
    );
}

#[test]
fn chat_list_membership_updates_do_not_move_chat_positions() {
    // `chat.chat_lists` (schema 1.8.67, line 3595): "A chat can have a
    // non-zero position in a chat list even if it doesn't belong to the
    // chat list and have no position in a chat list even if it belongs to
    // the chat list". Telegram X keeps `chat.chatLists` separate from
    // `chat.positions` (`Tdlib.updateChatAddedToList`).
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    for id in [4, 5] {
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"updateNewChat","chat":{{"id":{id},"title":"c{id}","type":{{"@type":"chatTypePrivate","user_id":{id}}},"unread_count":0}}}}"#
            ),
        );
    }
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateChatPosition","chat_id":4,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"5","is_pinned":false}}"#,
    );
    // Removal from `chat_lists` leaves the positioned row in place.
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateChatRemovedFromList","chat_id":4,"chat_list":{"@type":"chatListMain"}}"#,
    );
    let ids: Vec<i64> = session.ordered_chats().iter().map(|c| c.id.0).collect();
    assert_eq!(ids, vec![4]);
    // Membership without a position is not a row (no order to sort by).
    for list in [
        r#"{"@type":"chatListMain"}"#,
        r#"{"@type":"chatListArchive"}"#,
        r#"{"@type":"chatListFolder","chat_folder_id":3}"#,
    ] {
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(r#"{{"@type":"updateChatAddedToList","chat_id":5,"chat_list":{list}}}"#),
        );
    }
    let ids: Vec<i64> = session.ordered_chats().iter().map(|c| c.id.0).collect();
    assert_eq!(ids, vec![4]);
    assert!(session.ordered_archived_chats().is_empty());
    assert!(session.ordered_folder_chats(3).is_empty());
}

#[test]
fn new_chat_positions_place_the_chat_in_its_lists() {
    // `updateNewChat` / a `chat` answer carries `chat.positions`; Telegram
    // X adds every non-zero position to its list (`Tdlib.updateNewChat`).
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewChat","chat":{"id":11,"title":"pinned","type":{"@type":"chatTypePrivate","user_id":11},"unread_count":0,"positions":[{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"90","is_pinned":true},{"@type":"chatPosition","list":{"@type":"chatListFolder","chat_folder_id":2},"order":"40","is_pinned":false}]}}"#,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewChat","chat":{"id":12,"title":"archived","type":{"@type":"chatTypePrivate","user_id":12},"unread_count":0,"positions":[{"@type":"chatPosition","list":{"@type":"chatListArchive"},"order":"30","is_pinned":false}]}}"#,
    );
    let main: Vec<i64> = session.ordered_chats().iter().map(|c| c.id.0).collect();
    assert_eq!(main, vec![11]);
    assert!(session.chats.get(&11).unwrap().is_pinned);
    let archive: Vec<i64> = session
        .ordered_archived_chats()
        .iter()
        .map(|c| c.id.0)
        .collect();
    assert_eq!(archive, vec![12]);
    let folder: Vec<i64> = session
        .ordered_folder_chats(2)
        .iter()
        .map(|c| c.id.0)
        .collect();
    assert_eq!(folder, vec![11]);
}

#[test]
fn message_level_read_updates_refresh_chat_badge_counts() {
    // Reading a mention / reaction reports the chat's new counter on the
    // message-level update (schema 1.8.67, lines 10443 and 10450).
    // Telegram X copies the count onto the chat
    // (`Tdlib.updateMessageMentionRead` / `Tdlib.updateMessageUnreadReactions`).
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewChat","chat":{"id":21,"title":"group","type":{"@type":"chatTypeBasicGroup","basic_group_id":21},"unread_count":4,"unread_mention_count":2,"unread_reaction_count":3}}"#,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateMessageMentionRead","chat_id":21,"message_id":1048576,"unread_mention_count":1}"#,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateMessageUnreadReactions","chat_id":21,"message_id":2097152,"unread_reactions":[],"unread_reaction_count":0}"#,
    );
    let chat = session.chats.get(&21).unwrap();
    assert_eq!(chat.unread_mention_count, 1);
    assert_eq!(chat.unread_reaction_count, 0);
}

#[test]
fn new_chat_carries_its_last_message_preview() {
    // `updateNewChat.chat.last_message` is the starting preview; TDLib only
    // reports later changes through `updateChatLastMessage`.
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewChat","chat":{"id":7,"title":"Ada","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":0,"last_message":{"id":42,"chat_id":7,"is_outgoing":true,"date":1790631720,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"See you soon","entities":[]}}}}}"#,
    );
    let chat = &session.chats[&7];
    assert_eq!(chat.last_preview, "See you soon");
    assert_eq!(
        chat.last_message.map(|last| (last.id.0, last.is_outgoing)),
        Some((42, true))
    );
    // A newer `updateChatLastMessage` that arrived first is never replaced
    // by a re-sent chat object carrying an older message.
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateChatLastMessage","chat_id":7,"last_message":{"id":50,"chat_id":7,"is_outgoing":false,"date":1790631800,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"Newer","entities":[]}}},"positions":[]}"#,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewChat","chat":{"id":7,"title":"Ada","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":0,"last_message":{"id":42,"chat_id":7,"is_outgoing":true,"date":1790631720,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"See you soon","entities":[]}}}}}"#,
    );
    assert_eq!(session.chats[&7].last_preview, "Newer");
}

#[test]
fn folder_icon_and_share_flags_parse_from_the_update() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateChatFolders","chat_folders":[{"@type":"chatFolderInfo","id":3,"name":{"@type":"chatFolderName","text":{"@type":"formattedText","text":"Work","entities":[]},"animate_custom_emoji":false},"icon":{"@type":"chatFolderIcon","name":"Work"},"color_id":2,"is_shareable":true,"has_my_invite_links":true}],"main_chat_list_position":0,"are_tags_enabled":false}"#,
    );
    let folder = &session.chat_list.chat_folders[0];
    assert_eq!(folder.icon_name, "Work");
    assert!(folder.is_shareable && folder.has_my_invite_links);
}

#[test]
fn folder_invite_links_list_create_edit_and_errors() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateChatFolders","chat_folders":[{"@type":"chatFolderInfo","id":3,"name":{"@type":"chatFolderName","text":{"@type":"formattedText","text":"Work","entities":[]}},"icon":null,"color_id":-1,"is_shareable":false,"has_my_invite_links":false}],"main_chat_list_position":0,"are_tags_enabled":false}"#,
    );
    // getChatFolderInviteLinks fills the folder's list.
    let extra = session.request_for_folder(RequestPurpose::GetChatFolderInviteLinks, 3);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"chatFolderInviteLinks","@extra":"{}","invite_links":[{{"@type":"chatFolderInviteLink","invite_link":"https://t.me/addlist/a","name":"Team","chat_ids":[5,6]}}]}}"#,
            extra.0
        ),
    );
    let links = |session: &Session| {
        session.chat_list.folder_shares[&3]
            .links
            .data()
            .cloned()
            .unwrap_or_default()
    };
    assert_eq!(links(&session).len(), 1);
    assert_eq!(links(&session)[0].chat_ids, vec![5, 6]);
    // A create adds a link and flips the one-shot + the folder flags.
    let extra = session.request_for_folder(RequestPurpose::CreateChatFolderInviteLink, 3);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"chatFolderInviteLink","@extra":"{}","invite_link":"https://t.me/addlist/b","name":"","chat_ids":[5]}}"#,
            extra.0
        ),
    );
    assert_eq!(links(&session).len(), 2);
    // The box that sent the create takes its answer by request id.
    assert_eq!(
        session.chat_list.dialog_answers.take(extra),
        Some(DialogAnswer::Done)
    );
    assert_eq!(session.chat_list.dialog_answers.take(extra), None, "once");
    assert!(session.chat_list.chat_folders[0].is_shareable);
    // An edit replaces by link, never duplicates.
    let extra = session.request_for_folder(RequestPurpose::EditChatFolderInviteLink, 3);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"chatFolderInviteLink","@extra":"{}","invite_link":"https://t.me/addlist/b","name":"Renamed","chat_ids":[6]}}"#,
            extra.0
        ),
    );
    assert_eq!(links(&session).len(), 2);
    assert_eq!(links(&session)[1].name, "Renamed");
    // Too many links opens the limit box rather than an error line.
    let extra = session.request_for_folder(RequestPurpose::CreateChatFolderInviteLink, 3);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"INVITES_TOO_MUCH"}}"#,
            extra.0
        ),
    );
    assert_eq!(
        session.chat_list.folder_limit_hit.take(),
        Some((
            crate::folder_limits::FolderLimitKind::InviteLinks,
            Some(extra)
        ))
    );
    // Any other refusal surfaces for the dialog that sent it.
    let extra = session.request_for_folder(RequestPurpose::CreateChatFolderInviteLink, 3);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"CHAT_ADMIN_REQUIRED"}}"#,
            extra.0
        ),
    );
    assert!(matches!(
        session.chat_list.dialog_answers.take(extra),
        Some(DialogAnswer::Failed(_))
    ));
}

#[test]
fn recommended_folders_and_addlist_check_are_cached() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let extra = session.request(RequestPurpose::GetRecommendedChatFolders, None);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"recommendedChatFolders","@extra":"{}","chat_folders":[{{"@type":"recommendedChatFolder","folder":{{"@type":"chatFolder","name":{{"@type":"chatFolderName","text":{{"@type":"formattedText","text":"Unread","entities":[]}}}},"icon":{{"@type":"chatFolderIcon","name":"Unread"}},"color_id":-1,"is_shareable":false,"pinned_chat_ids":[],"included_chat_ids":[],"excluded_chat_ids":[],"exclude_muted":false,"exclude_read":true,"exclude_archived":false,"include_contacts":true,"include_non_contacts":true,"include_bots":true,"include_groups":true,"include_channels":true}},"description":"All unread chats"}}]}}"#,
            extra.0
        ),
    );
    let recommended = session
        .chat_list
        .recommended_folders
        .as_ref()
        .expect("cached");
    assert_eq!(recommended[0].spec.name, "Unread");
    assert_eq!(recommended[0].spec.icon_name.as_deref(), Some("Unread"));
    assert!(recommended[0].spec.exclude_read);
    assert_eq!(recommended[0].description, "All unread chats");

    let extra = session.request(RequestPurpose::CheckChatFolderInviteLink, None);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"chatFolderInviteLinkInfo","@extra":"{}","chat_folder_info":{{"@type":"chatFolderInfo","id":0,"name":{{"@type":"chatFolderName","text":{{"@type":"formattedText","text":"Shared","entities":[]}}}},"icon":null,"color_id":-1,"is_shareable":true,"has_my_invite_links":false}},"missing_chat_ids":[8,9],"added_chat_ids":[4]}}"#,
            extra.0
        ),
    );
    let Some(DialogAnswer::Checked(info)) = session.chat_list.dialog_answers.take(extra) else {
        panic!("the check answers its box with the folder");
    };
    assert_eq!(info.folder.id, 0);
    assert_eq!(info.missing_chat_ids, vec![8, 9]);
    assert_eq!(info.added_chat_ids, vec![4]);

    // Adding confirms through `ok`; a failure shows its reason.
    let extra = session.request(RequestPurpose::AddChatFolderByInviteLink, None);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0),
    );
    assert_eq!(
        session.chat_list.dialog_answers.take(extra),
        Some(DialogAnswer::Done)
    );
    let extra = session.request(RequestPurpose::AddChatFolderByInviteLink, None);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"INVITE_SLUG_EXPIRED"}}"#,
            extra.0
        ),
    );
    assert!(matches!(
        session.chat_list.dialog_answers.take(extra),
        Some(DialogAnswer::Failed(_))
    ));
}

#[test]
fn folder_link_chats_are_cached_per_folder() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let extra = session.request_for_folder(RequestPurpose::GetChatsForFolderInviteLink, 3);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"chats","@extra":"{}","total_count":2,"chat_ids":[5,6]}}"#,
            extra.0
        ),
    );
    assert_eq!(
        session.chat_list.folder_shares[&3].chats.data(),
        Some(&vec![5, 6])
    );
}

#[test]
fn installed_backgrounds_and_default_updates_are_cached() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let extra = session.request(RequestPurpose::GetInstalledBackgrounds, None);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"backgrounds","@extra":"{}","backgrounds":[{{"@type":"background","id":5,"is_default":false,"is_dark":false,"name":"sky","document":{{"@type":"document","document":{{"@type":"file","id":31,"size":9,"local":{{"path":"","is_downloading_completed":false}}}}}},"type":{{"@type":"backgroundTypeWallpaper","is_blurred":false,"is_moving":false}}}},{{"@type":"background","id":6,"is_default":true,"is_dark":false,"name":"mint","type":{{"@type":"backgroundTypeFill","fill":{{"@type":"backgroundFillSolid","color":10092441}}}}}}]}}"#,
            extra.0
        ),
    );
    let list = session
        .chats_state
        .installed_backgrounds
        .data()
        .expect("list");
    assert_eq!(list.len(), 2);
    assert!(
        session.media.files.contains_key(&31),
        "photo file is tracked"
    );

    // The account default for a theme arrives as an update.
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateDefaultBackground","for_dark_theme":true,"background":{"@type":"background","id":6,"is_default":true,"is_dark":true,"name":"night","type":{"@type":"backgroundTypeFill","fill":{"@type":"backgroundFillSolid","color":1}}}}"#,
    );
    assert_eq!(session.chats_state.default_backgrounds[&true].id, 6);
    assert!(!session.chats_state.default_backgrounds.contains_key(&false));

    // A set answer files under the theme it was sent for; errors surface.
    let extra = session.request(
        RequestPurpose::Chats(ChatsPurpose::SetDefaultBackground {
            for_dark_theme: false,
        }),
        None,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"background","@extra":"{}","id":5,"is_default":true,"is_dark":false,"name":"sky","type":{{"@type":"backgroundTypeFill","fill":{{"@type":"backgroundFillSolid","color":2}}}}}}"#,
            extra.0
        ),
    );
    assert_eq!(session.chats_state.default_backgrounds[&false].id, 5);
    let extra = session.request(
        RequestPurpose::Chats(ChatsPurpose::RemoveInstalledBackground { background_id: 1 }),
        None,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"BACKGROUND_INVALID"}}"#,
            extra.0
        ),
    );
    assert!(session.chats_state.background_error.is_some());
}

#[test]
fn a_theme_named_wallpaper_shows_the_themes_picture_with_the_chats_dimming() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let photo = |id: i64, file: i64| {
        format!(
            r#"{{"@type":"background","id":"{id}","name":"p","document":{{"@type":"document","document":{{"@type":"file","id":{file},"size":9}}}},"type":{{"@type":"backgroundTypeWallpaper"}}}}"#
        )
    };
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"updateEmojiChatThemes","chat_themes":[{{"@type":"emojiChatTheme","name":"T","light_settings":{{"@type":"themeSettings","accent_color":255,"background":{}}},"dark_settings":{{"@type":"themeSettings","accent_color":255,"background":{}}}}}]}}"#,
            photo(1, 31),
            photo(2, 32),
        ),
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateChatBackground","chat_id":5,"background":{"@type":"chatBackground","dark_theme_dimming":30,"background":{"@type":"background","id":"9","name":"","type":{"@type":"backgroundTypeChatTheme","theme_name":"T"}}}}"#,
    );
    let (shown, dimming) = session.chat_wallpaper(5, true).unwrap();
    assert_eq!((shown.id, dimming), (2, 30));
    assert_eq!(session.chat_wallpaper(5, false).unwrap().0.id, 1);
    assert_eq!(
        session.chat_look_file_ids(5, false),
        [FileId(31), FileId(32)],
    );
    assert!(session.chat_look_file_ids(6, false).is_empty());
}

#[test]
fn chat_wallpaper_prefers_the_chats_own_over_the_theme() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let fill = |id: i64, color: i64| {
        format!(
            r#"{{"@type":"background","id":{id},"name":"n","type":{{"@type":"backgroundTypeFill","fill":{{"@type":"backgroundFillSolid","color":{color}}}}}}}"#
        )
    };
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"updateEmojiChatThemes","chat_themes":[{{"@type":"emojiChatTheme","name":"T","light_settings":{{"@type":"themeSettings","accent_color":255,"background":{}}},"dark_settings":{{"@type":"themeSettings","accent_color":255}}}}]}}"#,
            fill(1, 1)
        ),
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateChatTheme","chat_id":5,"theme":{"@type":"chatThemeEmoji","name":"T"}}"#,
    );
    assert_eq!(session.chat_wallpaper(5, false).unwrap().0.id, 1);
    assert!(session.chat_wallpaper(5, true).is_none());
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"updateChatBackground","chat_id":5,"background":{{"@type":"chatBackground","dark_theme_dimming":30,"background":{}}}}}"#,
            fill(2, 2)
        ),
    );
    let (own, dimming) = session.chat_wallpaper(5, false).unwrap();
    assert_eq!((own.id, dimming), (2, 30));
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateChatBackground","chat_id":5}"#,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateChatTheme","chat_id":5}"#,
    );
    assert!(session.chat_wallpaper(5, false).is_none());
}

#[test]
fn searching_a_background_keeps_the_answer_without_installing_it() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let extra = session.request(RequestPurpose::SearchBackground, None);
    session.chats_state.background_search = crate::state::BackgroundSearch::Waiting(extra);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"background","@extra":"{}","id":8,"name":"found","type":{{"@type":"backgroundTypeFill","fill":{{"@type":"backgroundFillSolid","color":3}}}}}}"#,
            extra.0
        ),
    );
    assert_eq!(
        session.chats_state.background_search.found().map(|b| b.id),
        Some(8)
    );
    assert!(session.chats_state.default_backgrounds.is_empty());
}

/// Link A's search answers (or fails) after link B's preview opened: B
/// shows only B's background, and A's failure never shows under B.
#[test]
fn a_late_background_search_answer_never_lands_under_another_link() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let background = |extra: RequestId, id: i64| {
        format!(
            r#"{{"@type":"background","@extra":"{}","id":{id},"name":"n{id}","type":{{"@type":"backgroundTypeFill","fill":{{"@type":"backgroundFillSolid","color":3}}}}}}"#,
            extra.0
        )
    };
    let link_a = session.request(RequestPurpose::SearchBackground, None);
    let failing_a = session.request(RequestPurpose::SearchBackground, None);
    let link_b = session.request(RequestPurpose::SearchBackground, None);
    session.chats_state.background_search = crate::state::BackgroundSearch::Waiting(link_b);
    apply_json(&mut session, &seq, &sink, &background(link_a, 8));
    assert!(
        session.chats_state.background_search.found().is_none(),
        "A's answer"
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"WALLPAPER_INVALID"}}"#,
            failing_a.0
        ),
    );
    assert!(session.chats_state.background_error.is_none(), "A's error");
    apply_json(&mut session, &seq, &sink, &background(link_b, 9));
    assert_eq!(
        session.chats_state.background_search.found().map(|b| b.id),
        Some(9)
    );
}

#[test]
fn chat_look_requests_settle_per_request_and_keep_the_wallpaper_error_out() {
    use crate::state::Answer;
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let purposes = [
        RequestPurpose::SetChatTheme,
        RequestPurpose::SetChatBackground,
        RequestPurpose::DeleteChatBackground,
    ];
    let mut ids = Vec::new();
    for purpose in purposes {
        let extra = session.request(purpose, Some(ChatId(7)));
        session.acks.expect(extra);
        ids.push(extra);
    }
    // One answer settles one request, in either order.
    for (i, extra) in ids.iter().enumerate().rev() {
        let json = if i == 1 {
            format!(
                r#"{{"@type":"error","@extra":"{}","code":400,"message":"BACKGROUND_INVALID"}}"#,
                extra.0
            )
        } else {
            format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0)
        };
        apply_json(&mut session, &seq, &sink, &json);
    }
    let acks = &mut session.acks;
    assert_eq!(acks.take(ids[0]), Some(Answer::Done));
    assert!(matches!(acks.take(ids[1]), Some(Answer::Failed(_))));
    assert_eq!(acks.take(ids[2]), Some(Answer::Done));
    assert!(
        session.chats_state.background_error.is_none(),
        "the chat look's error is not the wallpaper pages'"
    );

    // An answer of a request no dialog waits for is not kept.
    let stray = session.request(RequestPurpose::SetChatTheme, Some(ChatId(7)));
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, stray.0),
    );
    assert_eq!(session.acks.take(stray), None);
}

#[test]
fn gift_themes_apply_their_own_settings_and_replace_emoji_themes() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateChatTheme","chat_id":5,"theme":{"@type":"chatThemeEmoji","name":"T"}}"#,
    );
    assert_eq!(
        session
            .chats_state
            .chat_theme_names
            .get(&5)
            .map(String::as_str),
        Some("T")
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateChatTheme","chat_id":5,"theme":{"@type":"chatThemeGift","gift_theme":{"@type":"giftChatTheme","gift":{"@type":"upgradedGift","name":"Pepe-1"},"light_settings":{"@type":"themeSettings","accent_color":1193046},"dark_settings":{"@type":"themeSettings","accent_color":6636321}}}}"#,
    );
    assert!(!session.chats_state.chat_theme_names.contains_key(&5));
    assert_eq!(
        session
            .chat_theme_settings(5, false)
            .map(|s| s.accent_color),
        Some(0x123456)
    );
    assert_eq!(
        session.chat_theme_settings(5, true).map(|s| s.accent_color),
        Some(0x654321)
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateChatTheme","chat_id":5}"#,
    );
    assert!(session.chat_theme_settings(5, false).is_none());
}

#[test]
fn background_share_links_land_once_and_errors_surface() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let extra = session.request(RequestPurpose::GetBackgroundUrl, None);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"httpUrl","@extra":"{}","url":"https://t.me/bg/abc?mode=blur"}}"#,
            extra.0
        ),
    );
    assert_eq!(
        session.chats_state.background_answers.take_for(&[extra]),
        vec![(
            extra,
            Ok(BackgroundOutcome::Link(
                "https://t.me/bg/abc?mode=blur".into()
            ))
        )]
    );
    let extra = session.request(RequestPurpose::GetBackgroundUrl, None);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"BACKGROUND_INVALID"}}"#,
            extra.0
        ),
    );
    assert!(session.chats_state.background_error.is_some());
}

/// 05-067: two sets for different themes in flight each land under their
/// own theme, whichever answers last.
#[test]
fn set_default_background_answers_file_under_their_own_theme() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let purpose = |for_dark_theme| {
        RequestPurpose::Chats(ChatsPurpose::SetDefaultBackground { for_dark_theme })
    };
    let light = session.request(purpose(false), None);
    let dark = session.request(purpose(true), None);
    let background = |id: i64, extra: RequestId| {
        format!(
            r#"{{"@type":"background","@extra":"{}","id":{id},"is_default":true,"is_dark":false,"name":"w","type":{{"@type":"backgroundTypeFill","fill":{{"@type":"backgroundFillSolid","color":2}}}}}}"#,
            extra.0
        )
    };
    apply_json(&mut session, &seq, &sink, &background(10, dark));
    apply_json(&mut session, &seq, &sink, &background(11, light));
    assert_eq!(session.chats_state.default_backgrounds[&true].id, 10);
    assert_eq!(session.chats_state.default_backgrounds[&false].id, 11);
}
