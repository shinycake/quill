//! State tests: how urgently each TDLib envelope needs a redraw.
use super::common::*;
use super::*;
use crate::telegram::envelope::parse_envelope;

fn need(session: &Session, json: &str) -> RedrawNeed {
    redraw_need(session, &parse_envelope(json).expect("envelope parses"))
}

/// Private chats 11 (Ada) and 12 (Bob), Ada online, Ada's chat open.
fn two_chats() -> Session {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    for (id, name) in [(11, "Ada"), (12, "Bob")] {
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"updateUser","user":{{"id":{id},"first_name":"{name}","type":{{"@type":"userTypeRegular"}},"status":{{"@type":"userStatusOnline","expires":1}}}}}}"#
            ),
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"updateNewChat","chat":{{"id":{id},"title":"{name}","type":{{"@type":"chatTypePrivate","user_id":{id}}},"unread_count":0}}}}"#
            ),
        );
    }
    session.open_chat = Some(ChatId(11));
    session
}

fn status(user_id: i64, status: &str) -> String {
    format!(r#"{{"@type":"updateUserStatus","user_id":{user_id},"status":{status}}}"#)
}

const OFFLINE: &str = r#"{"@type":"userStatusOffline","was_online":100}"#;
const ONLINE: &str = r#"{"@type":"userStatusOnline","expires":200}"#;

#[test]
fn a_status_change_redraws_the_open_chat_header_at_once() {
    let session = two_chats();
    assert_eq!(need(&session, &status(11, OFFLINE)), RedrawNeed::Now);
}

#[test]
fn another_contact_going_offline_only_touches_the_chat_list() {
    let session = two_chats();
    assert_eq!(need(&session, &status(12, OFFLINE)), RedrawNeed::ChatList);
}

#[test]
fn a_status_that_did_not_change_needs_nothing() {
    // Online again with a new expiry: the same `Online` status.
    let session = two_chats();
    assert_eq!(need(&session, &status(12, ONLINE)), RedrawNeed::Nothing);
    // A user Quill never loaded is not drawn anywhere.
    assert_eq!(need(&session, &status(99, OFFLINE)), RedrawNeed::Nothing);
}

#[test]
fn last_seen_changes_wait_for_a_batched_redraw() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateUser","user":{"id":12,"first_name":"Bob","type":{"@type":"userTypeRegular"},"status":{"@type":"userStatusOffline","was_online":1}}}"#,
    );
    assert_eq!(need(&session, &status(12, OFFLINE)), RedrawNeed::Later);
}

#[test]
fn typing_redraws_now_only_in_the_open_chat() {
    let session = two_chats();
    let typing = |chat: i64| {
        format!(
            r#"{{"@type":"updateChatAction","chat_id":{chat},"topic_id":null,"sender_id":{{"@type":"messageSenderUser","user_id":{chat}}},"action":{{"@type":"chatActionTyping"}}}}"#
        )
    };
    assert_eq!(need(&session, &typing(11)), RedrawNeed::Now);
    assert_eq!(need(&session, &typing(12)), RedrawNeed::ChatList);
}

#[test]
fn chat_row_updates_of_other_chats_only_touch_the_chat_list() {
    let session = two_chats();
    let read = |chat: i64| {
        format!(
            r#"{{"@type":"updateChatReadInbox","chat_id":{chat},"last_read_inbox_message_id":5,"unread_count":0}}"#
        )
    };
    assert_eq!(need(&session, &read(11)), RedrawNeed::Now);
    assert_eq!(need(&session, &read(12)), RedrawNeed::ChatList);
    let position = r#"{"@type":"updateChatPosition","chat_id":12,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"5","is_pinned":false,"source":null}}"#;
    assert_eq!(need(&session, position), RedrawNeed::ChatList);
}

#[test]
fn the_comment_thread_counts_as_shown() {
    let mut session = two_chats();
    session.open_chat = Some(ChatId(12));
    session.thread = Some(ThreadView {
        origin_chat_id: ChatId(12),
        origin_message_id: MessageId(1),
        chat_id: ChatId(77),
        thread_id: 0,
        status: ThreadStatus::Ready,
        reply_count: 0,
        unread_count: 0,
        last_read_inbox_message_id: 0,
        root_ids: Vec::new(),
        history: Default::default(),
        unread_anchor: None,
        root_jump_serial: 0,
        needs_chat_switch: false,
        reading_started: false,
        forum_topic_id: None,
    });
    let read = r#"{"@type":"updateChatReadInbox","chat_id":77,"last_read_inbox_message_id":5,"unread_count":0}"#;
    assert_eq!(need(&session, read), RedrawNeed::Now);
}

#[test]
fn unread_totals_and_unknown_updates_need_nothing() {
    let session = two_chats();
    assert_eq!(
        need(
            &session,
            r#"{"@type":"updateUnreadMessageCount","chat_list":{"@type":"chatListMain"},"unread_count":3,"unread_unmuted_count":1}"#
        ),
        RedrawNeed::Nothing
    );
    assert_eq!(
        need(
            &session,
            r#"{"@type":"updateHavePendingNotifications","have_delayed_notifications":false,"have_unreceived_notifications":false}"#
        ),
        RedrawNeed::Nothing
    );
}

#[test]
fn answers_to_requests_redraw_now_except_fire_and_forget_ones() {
    let mut session = two_chats();
    let generation = session.account_generation;
    let view = session
        .requests
        .register(generation, RequestPurpose::ViewMessages, None, None);
    let ok = |id: RequestId| format!(r#"{{"@type":"ok","@extra":"{}"}}"#, id.0);
    assert_eq!(need(&session, &ok(view)), RedrawNeed::Nothing);
    let history = session
        .requests
        .register(generation, RequestPurpose::GetHistory, None, None);
    assert_eq!(need(&session, &ok(history)), RedrawNeed::Now);
}

#[test]
fn automatic_downloads_redraw_later_and_user_downloads_now() {
    let mut session = two_chats();
    let progress = |id: i32| {
        format!(
            r#"{{"@type":"updateFile","file":{{"@type":"file","id":{id},"size":24,"expected_size":24,"local":{{"@type":"localFile","path":"","can_be_downloaded":true,"can_be_deleted":false,"is_downloading_active":true,"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":8}},"remote":{{"@type":"remoteFile","id":"r","unique_id":"u","is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":24}}}}}}"#
        )
    };
    session.downloading.insert(5);
    assert_eq!(need(&session, &progress(5)), RedrawNeed::Later);
    session.user_downloads.insert(5);
    assert_eq!(need(&session, &progress(5)), RedrawNeed::Now);
    // Not a download of ours (an upload in flight): at once.
    assert_eq!(need(&session, &progress(6)), RedrawNeed::Now);
}

#[test]
fn ungraded_updates_redraw_now() {
    let session = two_chats();
    assert_eq!(
        need(
            &session,
            r#"{"@type":"updateConnectionState","state":{"@type":"connectionStateUpdating"}}"#
        ),
        RedrawNeed::Now
    );
}

#[test]
fn needs_order_from_nothing_to_now() {
    assert!(RedrawNeed::Nothing < RedrawNeed::ChatList);
    assert!(RedrawNeed::ChatList < RedrawNeed::Later);
    assert!(RedrawNeed::Later < RedrawNeed::Now);
}

fn new_message(chat: i64) -> String {
    format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":7,"chat_id":{chat},"sender_id":{{"@type":"messageSenderUser","user_id":{chat}}},"is_outgoing":false,"date":1700000000,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"hi","entities":[]}}}}}}}}"#
    )
}

fn file(id: i32, completed: bool) -> String {
    format!(
        r#"{{"@type":"updateFile","file":{{"@type":"file","id":{id},"size":24,"expected_size":24,"local":{{"@type":"localFile","path":"{path}","can_be_downloaded":true,"can_be_deleted":false,"is_downloading_active":{active},"is_downloading_completed":{completed},"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":{size}}},"remote":{{"@type":"remoteFile","id":"r","unique_id":"u","is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":24}}}}}}"#,
        path = if completed {
            "/tmp/quill-redraw-test"
        } else {
            ""
        },
        active = !completed,
        size = if completed { 24 } else { 8 },
    )
}

#[test]
fn a_new_message_redraws_now_only_in_the_chat_shown() {
    let session = two_chats();
    assert_eq!(need(&session, &new_message(11)), RedrawNeed::Now);
    assert_eq!(need(&session, &new_message(12)), RedrawNeed::ChatList);
}

#[test]
fn chat_list_grades_follow_the_open_chat() {
    let mut session = two_chats();
    let read = |chat: i64| {
        format!(
            r#"{{"@type":"updateChatReadOutbox","chat_id":{chat},"last_read_outbox_message_id":9}}"#
        )
    };
    assert_eq!(need(&session, &read(12)), RedrawNeed::ChatList);
    // The user opens Bob's chat: the same update now redraws at once.
    session.open_chat = Some(ChatId(12));
    assert_eq!(need(&session, &read(12)), RedrawNeed::Now);
    assert_eq!(need(&session, &read(11)), RedrawNeed::ChatList);
    // No chat open: only the chat list shows chats.
    session.open_chat = None;
    assert_eq!(need(&session, &read(11)), RedrawNeed::ChatList);
    assert_eq!(need(&session, &status(11, OFFLINE)), RedrawNeed::ChatList);
}

#[test]
fn auth_connection_and_calls_redraw_now() {
    let session = two_chats();
    for json in [
        r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateWaitPhoneNumber"}}"#,
        r#"{"@type":"updateConnectionState","state":{"@type":"connectionStateReady"}}"#,
        r#"{"@type":"updateCall","call":{"@type":"call","id":77,"unique_id":"99","user_id":12,"is_outgoing":false,"is_video":false,"state":{"@type":"callStatePending","is_created":true,"is_received":false}}}"#,
    ] {
        assert_eq!(need(&session, json), RedrawNeed::Now, "{json}");
    }
}

#[test]
fn errors_answering_our_requests_redraw_now() {
    let mut session = two_chats();
    let generation = session.account_generation;
    let id = session
        .requests
        .register(generation, RequestPurpose::ViewMessages, None, None);
    // A failed `viewMessages` is still an answer the UI may show.
    let error = format!(
        r#"{{"@type":"error","code":400,"message":"CHAT_INVALID","@extra":"{}"}}"#,
        id.0
    );
    assert_eq!(need(&session, &error), RedrawNeed::Now);
    // So is an answer to a request Quill no longer tracks.
    assert_eq!(
        need(&session, r#"{"@type":"ok","@extra":"424242"}"#),
        RedrawNeed::Now
    );
}

#[test]
fn user_records_redraw_now_for_the_open_peer_and_ourselves() {
    let mut session = two_chats();
    let user = |id: i64, name: &str| {
        format!(
            r#"{{"@type":"updateUser","user":{{"id":{id},"first_name":"{name}","type":{{"@type":"userTypeRegular"}},"status":{{"@type":"userStatusOnline","expires":1}}}}}}"#
        )
    };
    assert_eq!(need(&session, &user(11, "Ada L")), RedrawNeed::Now);
    assert_eq!(need(&session, &user(12, "Bob B")), RedrawNeed::Later);
    // Our own record answers profile edits (name, photo, username).
    session.my_user_id = Some(12);
    assert_eq!(need(&session, &user(12, "Bob B")), RedrawNeed::Now);
}

#[test]
fn options_that_change_what_is_drawn_redraw_now() {
    let session = two_chats();
    let option = |name: &str, value: &str| {
        format!(r#"{{"@type":"updateOption","name":"{name}","value":{value}}}"#)
    };
    assert_eq!(
        need(
            &session,
            &option("my_id", r#"{"@type":"optionValueInteger","value":"5"}"#)
        ),
        RedrawNeed::Now
    );
    assert_eq!(
        need(
            &session,
            &option(
                "is_premium",
                r#"{"@type":"optionValueBoolean","value":true}"#
            )
        ),
        RedrawNeed::Now
    );
    assert_eq!(
        need(
            &session,
            &option(
                "message_caption_length_max",
                r#"{"@type":"optionValueInteger","value":"1024"}"#
            )
        ),
        RedrawNeed::Later
    );
}

#[test]
fn group_records_and_online_counts_redraw_now_for_the_open_group() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewChat","chat":{"id":-100100,"title":"Group","type":{"@type":"chatTypeSupergroup","supergroup_id":100,"is_channel":false},"unread_count":0}}"#,
    );
    let supergroup = |id: i64| {
        format!(
            r#"{{"@type":"updateSupergroup","supergroup":{{"@type":"supergroup","id":{id},"member_count":5,"is_channel":false}}}}"#
        )
    };
    let online = |chat: i64| {
        format!(
            r#"{{"@type":"updateChatOnlineMemberCount","chat_id":{chat},"online_member_count":3}}"#
        )
    };
    assert_eq!(need(&session, &supergroup(100)), RedrawNeed::Later);
    assert_eq!(need(&session, &online(-100100)), RedrawNeed::Later);
    session.open_chat = Some(ChatId(-100100));
    assert_eq!(need(&session, &supergroup(100)), RedrawNeed::Now);
    assert_eq!(need(&session, &supergroup(101)), RedrawNeed::Later);
    assert_eq!(need(&session, &online(-100100)), RedrawNeed::Now);
}

#[test]
fn a_finished_download_redraws_now_wherever_it_shows() {
    let mut session = two_chats();
    session.downloading.insert(5);
    // An avatar or thumbnail in flight draws nothing yet.
    assert_eq!(need(&session, &file(5, false)), RedrawNeed::Later);
    // The update that completes it shows the picture at once.
    assert_eq!(need(&session, &file(5, true)), RedrawNeed::Now);
}

#[test]
fn the_open_chats_media_downloads_show_live_progress() {
    let mut session = two_chats();
    session.downloading.insert(5);
    session.open_chat_media_downloads.insert(5);
    assert_eq!(need(&session, &file(5, false)), RedrawNeed::Now);
}

#[test]
fn download_manager_updates_redraw_now_for_downloads_the_user_started() {
    let mut session = two_chats();
    let download = r#"{"@type":"updateFileDownload","file_id":31,"complete_date":0,"is_paused":false,"counts":{"@type":"downloadedFileCounts","being_downloaded":1,"recently_downloaded":0}}"#;
    assert_eq!(need(&session, download), RedrawNeed::Later);
    session.user_downloads.insert(31);
    assert_eq!(need(&session, download), RedrawNeed::Now);
}
