//! State reducer tests: chat_list.
use super::common::*;
use super::*;

#[test]
fn chat_list_sorts_by_tdlib_order_then_id() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewChat","chat":{"id":2,"title":"b","type":{"@type":"chatTypePrivate","user_id":2},"unread_count":0}}"#,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewChat","chat":{"id":1,"title":"a","type":{"@type":"chatTypePrivate","user_id":1},"unread_count":0}}"#,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateChatPosition","chat_id":1,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"10","is_pinned":false}}"#,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateChatPosition","chat_id":2,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"10","is_pinned":false}}"#,
    );
    let ids: Vec<i64> = session.ordered_chats().iter().map(|c| c.id.0).collect();
    assert_eq!(ids, vec![2, 1]);
}

#[test]
fn new_chat_after_position_keeps_main_list_membership() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateChatPosition","chat_id":9,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"4","is_pinned":true}}"#,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewChat","chat":{"id":9,"title":"after","type":{"@type":"chatTypePrivate","user_id":9},"unread_count":2}}"#,
    );
    let chat = session.chats.get(&9).unwrap();
    assert_eq!(chat.title, "after");
    assert!(chat.in_main_list);
    assert!(chat.is_pinned);
    assert_eq!(chat.order, 4);
    assert_eq!(session.ordered_chats().len(), 1);
}

#[test]
fn last_message_positions_replace_main_list_membership() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewChat","chat":{"id":3,"title":"c","type":{"@type":"chatTypePrivate","user_id":3},"unread_count":0}}"#,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateChatLastMessage","chat_id":3,"last_message":{"id":1,"chat_id":3,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"CANARY_PREVIEW_hi","entities":[]}}},"positions":[{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"8","is_pinned":false}]}"#,
    );
    let chat = session.chats.get(&3).unwrap();
    assert!(chat.in_main_list);
    assert_eq!(chat.last_preview, "CANARY_PREVIEW_hi");
    assert_eq!(session.ordered_chats()[0].id.0, 3);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateChatLastMessage","chat_id":3,"last_message":null,"positions":[]}"#,
    );
    assert!(!session.chats.get(&3).unwrap().in_main_list);
    assert!(session.ordered_chats().is_empty());
    assert!(!sink.rendered().contains("CANARY_PREVIEW"));
}

#[test]
fn edits_refresh_the_preview_only_for_the_chats_last_message() {
    // Telegram X `TGChat.updateMessageContent`: the row preview follows an
    // edit when the edited id is `chat.last_message.id`, whether or not the
    // history is loaded. While the last message is unknown
    // (`updateChatLastMessage` with null, schema 1.8.67 line 10504) the
    // preview stays empty.
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewChat","chat":{"id":11,"title":"c","type":{"@type":"chatTypePrivate","user_id":11},"unread_count":0}}"#,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateChatLastMessage","chat_id":11,"last_message":{"id":100,"chat_id":11,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"hi","entities":[]}}},"positions":[{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"8","is_pinned":false}]}"#,
    );
    // History never loaded: the edit still reaches the row preview.
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateMessageContent","chat_id":11,"message_id":100,"new_content":{"@type":"messageText","text":{"@type":"formattedText","text":"edited","entities":[]}}}"#,
    );
    assert_eq!(session.chats[&11].last_preview, "edited");
    // The last message becomes unknown.
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateChatLastMessage","chat_id":11,"last_message":null,"positions":[{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"8","is_pinned":false}]}"#,
    );
    assert_eq!(session.chats[&11].last_preview, "");
    assert!(session.chats[&11].last_message.is_none());
    assert!(session.chats[&11].in_main_list);
    // A loaded row being edited does not invent a preview.
    session.open_chat(ChatId(11));
    let extra = session.request(RequestPurpose::GetHistory, Some(ChatId(11)));
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"messages","@extra":"{}","total_count":1,"messages":[{{"id":100,"chat_id":11,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"edited","entities":[]}}}}}}]}}"#,
            extra.0
        ),
    );
    assert!(session.histories[&11].contains(MessageId(100)));
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateMessageContent","chat_id":11,"message_id":100,"new_content":{"@type":"messageText","text":{"@type":"formattedText","text":"again","entities":[]}}}"#,
    );
    assert_eq!(session.chats[&11].last_preview, "");
}

#[test]
fn archive_position_does_not_clear_main_list() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewChat","chat":{"id":5,"title":"keep","type":{"@type":"chatTypePrivate","user_id":5},"unread_count":0}}"#,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateChatPosition","chat_id":5,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"6","is_pinned":false}}"#,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateChatPosition","chat_id":5,"position":{"@type":"chatPosition","list":{"@type":"chatListArchive"},"order":"3","is_pinned":false}}"#,
    );
    let chat = session.chats.get(&5).unwrap();
    assert!(chat.in_main_list);
    assert!(chat.in_archive);
    assert_eq!(chat.archive_order, 3);
    assert_eq!(chat.order, 6);
    assert_eq!(session.ordered_chats().len(), 1);
    assert_eq!(session.ordered_archived_chats().len(), 1);
}

#[test]
fn last_message_positions_are_a_full_set_including_archive() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewChat","chat":{"id":6,"title":"mixed","type":{"@type":"chatTypePrivate","user_id":6},"unread_count":0}}"#,
    );
    // Archive after Main in the array must not wipe Main membership.
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateChatLastMessage","chat_id":6,"last_message":{"id":2,"chat_id":6,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"hi","entities":[]}}},"positions":[{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"9","is_pinned":true},{"@type":"chatPosition","list":{"@type":"chatListArchive"},"order":"1","is_pinned":false}]}"#,
    );
    let chat = session.chats.get(&6).unwrap();
    assert!(chat.in_main_list);
    assert!(chat.is_pinned);
    assert_eq!(chat.order, 9);
    // Full set without Main removes from the main list.
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateChatLastMessage","chat_id":6,"last_message":null,"positions":[{"@type":"chatPosition","list":{"@type":"chatListArchive"},"order":"1","is_pinned":false}]}"#,
    );
    let chat = session.chats.get(&6).unwrap();
    assert!(!chat.in_main_list);
    assert!(chat.in_archive);
    assert!(session.ordered_chats().is_empty());
    assert_eq!(session.ordered_archived_chats()[0].id.0, 6);
}

#[test]
fn update_supergroup_resolves_forum_flag() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewChat","chat":{"id":16,"title":"Forum","type":{"@type":"chatTypeSupergroup","supergroup_id":16,"is_channel":false},"unread_count":0}}"#,
    );
    assert_eq!(session.chats.get(&16).unwrap().is_forum, None);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateSupergroup","supergroup":{"@type":"supergroup","id":16,"is_forum":true}}"#,
    );
    assert!(session.chats.get(&16).unwrap().is_forum_chat());
    // The getSupergroup response path is gated on the pending purpose.
    let extra = session.request(RequestPurpose::GetSupergroup, Some(ChatId(16)));
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"supergroup","@extra":"{}","id":16,"is_forum":false}}"#,
            extra.0
        ),
    );
    assert!(!session.chats.get(&16).unwrap().is_forum_chat());
    // Same payload without the pending purpose is ignored (it is not an update).
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"supergroup","id":16,"is_forum":true}"#,
    );
    assert!(!session.chats.get(&16).unwrap().is_forum_chat());
}

#[test]
fn chat_photo_remembered_from_update_new_chat() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewChat","chat":{"id":11,"title":"Demo","type":{"@type":"chatTypePrivate","user_id":11},"unread_count":0,"photo":{"@type":"chatPhotoInfo","small":{"@type":"file","id":91,"size":24,"expected_size":24,"local":{"@type":"localFile","path":"","can_be_downloaded":true,"can_be_deleted":false,"is_downloading_active":false,"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0},"remote":{"@type":"remoteFile","id":"x","unique_id":"u","is_uploading_active":false,"is_uploading_completed":false,"uploaded_size":0}},"big":{"@type":"file","id":92,"size":0,"expected_size":0,"local":{"@type":"localFile","path":"","can_be_downloaded":true,"can_be_deleted":false,"is_downloading_active":false,"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0},"remote":{"@type":"remoteFile","id":"x","unique_id":"u","is_uploading_active":false,"is_uploading_completed":false,"uploaded_size":0}},"minithumbnail":null,"has_animation":false,"is_personal":false}}}"#,
    );
    assert_eq!(session.chats.get(&11).unwrap().photo_file_id, Some(91));
    assert!(session.files.contains_key(&91));
    // `updateNewChat` without a photo leaves no avatar.
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewChat","chat":{"id":12,"title":"No photo","type":{"@type":"chatTypePrivate","user_id":12},"unread_count":0}}"#,
    );
    assert_eq!(session.chats.get(&12).unwrap().photo_file_id, None);
}

#[test]
fn supergroup_username_and_linked_chat_cached() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewChat","chat":{"id":13,"title":"Demo channel","type":{"@type":"chatTypeSupergroup","supergroup_id":13,"is_channel":true},"unread_count":0}}"#,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewChat","chat":{"id":14,"title":"Demo group","type":{"@type":"chatTypeSupergroup","supergroup_id":14,"is_channel":false},"unread_count":0}}"#,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateSupergroup","supergroup":{"@type":"supergroup","id":13,"usernames":{"@type":"usernames","active_usernames":["demochannel"],"disabled_usernames":[],"editable_username":"demochannel","collectible_usernames":[]},"is_forum":false,"is_channel":true}}"#,
    );
    assert_eq!(session.supergroup_username(13), Some("demochannel"));
    assert_eq!(session.discussion_chat_id(ChatId(13)), None);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateSupergroupFullInfo","supergroup_id":13,"supergroup_full_info":{"@type":"supergroupFullInfo","description":"CANARY channel","member_count":12345,"linked_chat_id":14}}"#,
    );
    let info = session.supergroup_full_info(13).unwrap();
    assert_eq!(info.description, "CANARY channel");
    assert_eq!(info.member_count, 12345);
    assert_eq!(info.linked_chat_id, 14);
    // The linked discussion group resolves to its chat id.
    assert_eq!(session.discussion_chat_id(ChatId(13)), Some(14));
    // Non-channels never get a "Discuss" affordance, even with a link.
    assert_eq!(session.discussion_chat_id(ChatId(14)), None);
    // Empty username stores an empty sentinel (renders as no username,
    // keeps the dedupe cache filled so re-opens don't refetch).
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateSupergroup","supergroup":{"@type":"supergroup","id":13,"usernames":null,"is_forum":false,"is_channel":true}}"#,
    );
    assert_eq!(session.supergroup_username(13), Some(""));
    assert!(session.supergroup_usernames.contains_key(&13));
}

#[test]
fn chat_list_photo_file_ids_dedupes() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    for (chat_id, file_id) in [(11, 91), (12, 92), (13, 93)] {
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                "{{\"@type\":\"updateNewChat\",\"chat\":{{\"id\":{chat_id},\"title\":\"c{chat_id}\",\"type\":{{\"@type\":\"chatTypePrivate\",\"user_id\":{chat_id}}},\"unread_count\":0,\"photo\":{{\"@type\":\"chatPhotoInfo\",\"small\":{{\"@type\":\"file\",\"id\":{file_id},\"size\":24,\"expected_size\":24,\"local\":{{\"@type\":\"localFile\",\"path\":\"\",\"can_be_downloaded\":true,\"can_be_deleted\":false,\"is_downloading_active\":false,\"is_downloading_completed\":false,\"download_offset\":0,\"downloaded_prefix_size\":0,\"downloaded_size\":0}},\"remote\":{{\"@type\":\"remoteFile\",\"id\":\"x\",\"unique_id\":\"u\",\"is_uploading_active\":false,\"is_uploading_completed\":false,\"uploaded_size\":0}}}},\"big\":null,\"minithumbnail\":null,\"has_animation\":false,\"is_personal\":false}}}}}}",
            ),
        );
    }
    // 92 completes locally; 93 is already in flight.
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateFile","file":{"@type":"file","id":92,"size":24,"expected_size":24,"local":{"@type":"localFile","path":"/tmp/x.png","can_be_downloaded":true,"can_be_deleted":false,"is_downloading_active":false,"is_downloading_completed":true,"download_offset":0,"downloaded_prefix_size":24,"downloaded_size":24},"remote":{"@type":"remoteFile","id":"x","unique_id":"u","is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":24}}}"#,
    );
    session.begin_download(FileId(93));
    let ids: Vec<i32> = session
        .chat_list_photo_file_ids()
        .into_iter()
        .map(|id| id.0)
        .collect();
    assert_eq!(ids, vec![91]);
    // A completed file resolves its display path.
    session.open_chat(ChatId(12));
    assert_eq!(session.chat_photo_path(ChatId(12)), Some("/tmp/x.png"));
    assert_eq!(session.chat_photo_path(ChatId(11)), None);
}

#[test]
fn chat_folders_update_replaces_list() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateChatFolders","chat_folders":[{"@type":"chatFolderInfo","id":3,"name":{"@type":"chatFolderName","text":{"@type":"formattedText","text":"Work","entities":[]},"animate_custom_emoji":false},"icon":{"@type":"chatFolderIcon","name":"Work"},"color_id":2,"is_shareable":false,"has_my_invite_links":false}],"main_chat_list_position":0,"are_tags_enabled":false}"#,
    );
    assert_eq!(session.chat_folders.len(), 1);
    assert_eq!(session.folder_name(3), Some("Work"));
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateChatFolders","chat_folders":[],"main_chat_list_position":0,"are_tags_enabled":false}"#,
    );
    assert!(session.chat_folders.is_empty());
    assert_eq!(session.folder_name(3), None);
}

#[test]
fn folder_position_membership_and_order() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    for (chat_id, title, order) in [(5, "alpha", "60"), (6, "beta", "50")] {
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"updateNewChat","chat":{{"id":{chat_id},"title":"{title}","type":{{"@type":"chatTypePrivate","user_id":{chat_id}}},"unread_count":0}}}}"#
            ),
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"updateChatPosition","chat_id":{chat_id},"position":{{"@type":"chatPosition","list":{{"@type":"chatListFolder","chat_folder_id":3}},"order":"{order}","is_pinned":false}}}}"#
            ),
        );
    }
    let folders = session.ordered_folder_chats(3);
    assert_eq!(folders.len(), 2);
    assert_eq!(folders[0].id.0, 5);
    assert_eq!(folders[1].id.0, 6);
    // Full positions set without the folder evicts it.
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateChatLastMessage","chat_id":5,"last_message":null,"positions":[{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"6","is_pinned":false}]}"#,
    );
    assert!(session.ordered_folder_chats(3).iter().all(|c| c.id.0 != 5));
    assert_eq!(session.ordered_folder_chats(3).len(), 1);
}

#[test]
fn folder_membership_add_remove() {
    // Folder rows follow `chatPosition`s only: `updateChatAddedToList` /
    // `updateChatRemovedFromList` describe `chat.chat_lists`, which may
    // disagree with positions (schema 1.8.67, line 3595).
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewChat","chat":{"id":5,"title":"alpha","type":{"@type":"chatTypePrivate","user_id":5},"unread_count":0}}"#,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateChatAddedToList","chat_id":5,"chat_list":{"@type":"chatListFolder","chat_folder_id":3}}"#,
    );
    assert!(session.ordered_folder_chats(3).is_empty());
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateChatPosition","chat_id":5,"position":{"@type":"chatPosition","list":{"@type":"chatListFolder","chat_folder_id":3},"order":"9","is_pinned":false}}"#,
    );
    assert!(session.ordered_folder_chats(3).iter().any(|c| c.id.0 == 5));
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateChatRemovedFromList","chat_id":5,"chat_list":{"@type":"chatListFolder","chat_folder_id":3}}"#,
    );
    assert!(session.ordered_folder_chats(3).iter().any(|c| c.id.0 == 5));
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateChatPosition","chat_id":5,"position":{"@type":"chatPosition","list":{"@type":"chatListFolder","chat_folder_id":3},"order":"0","is_pinned":false}}"#,
    );
    assert!(session.ordered_folder_chats(3).is_empty());
}

#[test]
fn folder_chats_to_leave_cached_per_folder() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let extra = session.request_for_folder(RequestPurpose::GetChatFolderChatsToLeave, 3);
    let json = format!(
        r#"{{"@type":"chats","@extra":"{}","total_count":2,"chat_ids":[5,6]}}"#,
        extra.0
    );
    apply_json(&mut session, &seq, &sink, &json);
    assert_eq!(session.folder_chats_to_leave.get(&3), Some(&vec![5, 6]));
    // Same payload shape, different purpose: cache untouched.
    let extra2 = session.request(RequestPurpose::SearchChats, None);
    let json2 = json.replace(
        &format!("\"@extra\":\"{}\"", extra.0),
        &format!("\"@extra\":\"{}\"", extra2.0),
    );
    session.folder_chats_to_leave.clear();
    apply_json(&mut session, &seq, &sink, &json2);
    assert!(!session.folder_chats_to_leave.contains_key(&3));
}

#[test]
fn cl1_pin_error_rolls_back_and_surfaces() {
    // Slice CL1: the optimistic pin restores the previous flag when
    // TDLib answers `error`, and the refusal surfaces in
    // `chat_action_error` (drained by the UI) — never silent.
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.chats.insert(11, placeholder_chat(ChatId(11)));
    let extra = session.request(RequestPurpose::ToggleChatIsPinned, Some(ChatId(11)));
    session
        .requests
        .pending_mut(extra)
        .expect("pending")
        .rollback = Some(RequestRollback::ChatPin {
        previous: false,
        archived: false,
    });
    session.chats.get_mut(&11).expect("chat").is_pinned = true;
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"PINNED_CHATS_LIMIT_EXCEEDED"}}"#,
            extra.0
        ),
    );
    assert!(!session.chats.get(&11).expect("chat").is_pinned);
    assert_eq!(
        session.chat_action_error.as_deref(),
        Some("could not pin the chat (error 400)")
    );
}

#[test]
fn cl1_marked_as_unread_update_and_rollback() {
    // Slice CL1: `updateChatIsMarkedAsUnread` flips the row badge
    // flag, and a refused toggle restores it.
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.chats.insert(12, placeholder_chat(ChatId(12)));
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateChatIsMarkedAsUnread","chat_id":12,"is_marked_as_unread":true}"#,
    );
    assert!(session.chats.get(&12).expect("chat").is_marked_as_unread);
    let extra = session.request(RequestPurpose::ToggleChatIsMarkedAsUnread, Some(ChatId(12)));
    session
        .requests
        .pending_mut(extra)
        .expect("pending")
        .rollback = Some(RequestRollback::ChatMarkedAsUnread { previous: true });
    session
        .chats
        .get_mut(&12)
        .expect("chat")
        .is_marked_as_unread = false;
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"CHAT_NOT_MODIFIED"}}"#,
            extra.0
        ),
    );
    assert!(session.chats.get(&12).expect("chat").is_marked_as_unread);
    assert_eq!(
        session.chat_action_error.as_deref(),
        Some("could not change read state (error 400)")
    );
}

#[test]
fn cl3_is_unread_guards_bulk_mark_read() {
    // CL3 review blocker: `mark_selected_read` skips chats with no
    // unread state (the single-chat path is a genuine toggle — on a
    // fully-read chat it would mark it *unread*).
    let read = placeholder_chat(ChatId(11));
    assert!(!read.is_unread());
    let mut with_unread = placeholder_chat(ChatId(12));
    with_unread.unread_count = 3;
    assert!(with_unread.is_unread());
    let mut marked = placeholder_chat(ChatId(13));
    marked.is_marked_as_unread = true;
    assert!(marked.is_unread());
}

#[test]
fn cl1_clear_history_error_surfaces() {
    // Slice CL1: a refused `deleteChatHistory` surfaces in
    // `chat_action_error`.
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let extra = session.request(RequestPurpose::DeleteChatHistory, Some(ChatId(13)));
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"CHAT_HISTORY_NOT_MODIFIED"}}"#,
            extra.0
        ),
    );
    assert_eq!(
        session.chat_action_error.as_deref(),
        Some("could not clear history (error 400)")
    );
}

#[test]
fn cl1_pin_limit_options_tracked() {
    // Slice CL1: `updateOption` for the pin limits (schema 1.8.67,
    // line 13674) feeds the client-side pin pre-check.
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    assert_eq!(session.pinned_chat_count_max, 5);
    assert_eq!(session.pinned_archived_chat_count_max, 100);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateOption","name":"pinned_chat_count_max","value":{"@type":"optionValueInteger","value":10}}"#,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateOption","name":"pinned_archived_chat_count_max","value":{"@type":"optionValueInteger","value":200}}"#,
    );
    assert_eq!(session.pinned_chat_count_max, 10);
    assert_eq!(session.pinned_archived_chat_count_max, 200);
}

#[test]
fn cl3_mention_reaction_counts_parse_and_update() {
    // Slice CL3: `chat.unread_mention_count` /
    // `chat.unread_reaction_count` (schema 1.8.67, lines 3611-3612)
    // and the updates (lines 10567/10570) feed the @ / ♥ badges.
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewChat","chat":{"id":14,"title":"Mentions","type":{"@type":"chatTypePrivate","user_id":14},"unread_count":3,"unread_mention_count":2,"unread_reaction_count":1,"can_be_reported":true}}"#,
    );
    let chat = session.chats.get(&14).expect("chat");
    assert_eq!(chat.unread_mention_count, 2);
    assert_eq!(chat.unread_reaction_count, 1);
    assert!(chat.can_be_reported);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateChatUnreadMentionCount","chat_id":14,"unread_mention_count":0}"#,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateChatUnreadReactionCount","chat_id":14,"unread_reaction_count":0}"#,
    );
    let chat = session.chats.get(&14).expect("chat");
    assert_eq!(chat.unread_mention_count, 0);
    assert_eq!(chat.unread_reaction_count, 0);
}

#[test]
fn cl3_report_chat_result_ok_and_more_info() {
    // Slice CL3: `reportChatResultOk` → "chat reported";
    // `reportChatResultOptionRequired` (and its siblings) → the
    // honest "more info required" note, never success.
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let extra = session.request(RequestPurpose::ReportChat, Some(ChatId(14)));
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(r#"{{"@type":"reportChatResultOk","@extra":"{}"}}"#, extra.0),
    );
    assert_eq!(
        session.report_chat_outcome.as_deref(),
        Some("chat reported")
    );

    let extra = session.request(RequestPurpose::ReportChat, Some(ChatId(14)));
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"reportChatResultOptionRequired","@extra":"{}"}}"#,
            extra.0
        ),
    );
    assert_eq!(
        session.report_chat_outcome.as_deref(),
        Some("report needs a reason or messages — the chat list only sends simple spam reports")
    );
}

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
        session.chat_action_error.as_deref(),
        Some("could not report the chat (error 400)")
    );
    let extra = session.request(
        RequestPurpose::SetMessageSenderBlockList { block: true },
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
        session.chat_action_error.as_deref(),
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
        session.chat_action_error.as_deref(),
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
    session.archive_settings_loading = true;
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
        session.archive_chat_list_settings,
        Some(ArchiveChatListSettings {
            archive_and_mute_new_chats_from_unknown_users: true,
            keep_unmuted_chats_archived: false,
            keep_chats_from_folders_archived: true,
        })
    );
    assert!(!session.archive_settings_loading);

    let old = session.archive_chat_list_settings;
    let extra = session.request(RequestPurpose::SetArchiveChatListSettings, None);
    session
        .requests
        .pending_mut(extra)
        .expect("pending")
        .rollback = Some(RequestRollback::ArchiveChatListSettings { previous: old });
    session.archive_chat_list_settings = Some(ArchiveChatListSettings {
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
    assert_eq!(session.archive_chat_list_settings, old);
    assert_eq!(
        session.chat_action_error.as_deref(),
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
        session.chat_action_error.as_deref(),
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
        session.chat_action_error.as_deref(),
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
