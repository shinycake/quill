//! Bubble headers and footer facts: reply strips, forward lines, "via @bot",
//! edit dates — recorded TDLib JSON through the reducer.
use super::common::*;
use super::*;
use crate::ids::UserId;
use crate::state::footer_tooltip;

const GROUP: i64 = -1001;

fn seeded() -> (Session, Arc<MemorySink>, AtomicU64) {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    for json in [
        r#"{"@type":"updateUser","user":{"id":31,"first_name":"Ada","last_name":"Lovelace","accent_color_id":3,"type":{"@type":"userTypeRegular"},"status":{"@type":"userStatusOnline","expires":1}}}"#,
        r#"{"@type":"updateUser","user":{"id":77,"first_name":"Gif","usernames":{"@type":"usernames","active_usernames":["gifbot"],"disabled_usernames":[],"editable_username":"gifbot"},"type":{"@type":"userTypeBot","is_inline":true},"status":{"@type":"userStatusOffline","was_online":1}}}"#,
        r#"{"@type":"updateNewChat","chat":{"id":-1001,"title":"Rustaceans","type":{"@type":"chatTypeBasicGroup","basic_group_id":1},"unread_count":0}}"#,
        r#"{"@type":"updateNewChat","chat":{"id":-1002,"title":"News Channel","type":{"@type":"chatTypeSupergroup","supergroup_id":2,"is_channel":true},"unread_count":0}}"#,
        r#"{"@type":"updateNewChat","chat":{"id":31,"title":"Ada Lovelace","type":{"@type":"chatTypePrivate","user_id":31},"unread_count":0}}"#,
    ] {
        apply_json(&mut session, &seq, &sink, json);
    }
    session.open_chat(ChatId(GROUP));
    (session, sink, seq)
}

fn text_message(id: i64, sender: i64, text: &str, extra: &str) -> String {
    format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":{id},"chat_id":{GROUP},"sender_id":{{"@type":"messageSenderUser","user_id":{sender}}},"is_outgoing":false,"date":1700000000,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"{text}","entities":[]}}}}{extra}}}}}"#
    )
}

fn message(session: &Session, id: i64) -> HistoryMessage {
    session.histories[&GROUP].messages[&id].clone()
}

#[test]
fn reply_header_names_the_original_sender_in_their_color() {
    let (mut session, sink, seq) = seeded();
    apply_json(
        &mut session,
        &seq,
        &sink,
        &text_message(10, 31, "original words", ""),
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &text_message(
            11,
            77,
            "answer",
            r#","reply_to":{"@type":"messageReplyToMessage","chat_id":-1001,"message_id":10,"quote":null,"checklist_task_id":0,"poll_option_id":"","origin":null,"origin_send_date":0,"content":null}"#,
        ),
    );
    let header = session.reply_header(&message(&session, 11)).unwrap();
    assert_eq!(header.state, ReplyState::Ready);
    assert_eq!(header.name.as_deref(), Some("Ada Lovelace"));
    assert_eq!(header.accent, Some(3));
    assert_eq!(header.text, "original words");
    assert!(!header.is_quote);
    assert!(header.clickable);
    assert_eq!(header.external_chat, None);
    // Everything needed is loaded: no fetch.
    assert!(session.reply_fetch_candidates().is_empty());
}

#[test]
fn reply_header_shows_the_quote_instead_of_the_message() {
    let (mut session, sink, seq) = seeded();
    apply_json(
        &mut session,
        &seq,
        &sink,
        &text_message(10, 31, "one two three", ""),
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &text_message(
            11,
            77,
            "answer",
            r#","reply_to":{"@type":"messageReplyToMessage","chat_id":-1001,"message_id":10,"quote":{"@type":"textQuote","text":{"@type":"formattedText","text":"two","entities":[]},"position":4,"is_manual":true},"checklist_task_id":0,"poll_option_id":"","origin":null,"origin_send_date":0,"content":null}"#,
        ),
    );
    let header = session.reply_header(&message(&session, 11)).unwrap();
    assert!(header.is_quote);
    assert_eq!(header.text, "two");
    assert_eq!(header.name.as_deref(), Some("Ada Lovelace"));
}

#[test]
fn reply_outside_the_window_is_fetched_and_filled_in() {
    let (mut session, sink, seq) = seeded();
    apply_json(
        &mut session,
        &seq,
        &sink,
        &text_message(
            500,
            77,
            "late answer",
            r#","reply_to":{"@type":"messageReplyToMessage","chat_id":-1001,"message_id":3,"quote":null,"checklist_task_id":0,"poll_option_id":"","origin":null,"origin_send_date":0,"content":null}"#,
        ),
    );
    let before = session.reply_header(&message(&session, 500)).unwrap();
    assert_eq!(before.state, ReplyState::Loading);
    assert_eq!(before.text, "Loading…");
    assert!(!before.clickable);
    assert_eq!(
        session.reply_fetch_candidates(),
        vec![(ChatId(GROUP), MessageId(500))]
    );

    let extra = session.request(
        RequestPurpose::GetRepliedMessage {
            chat_id: ChatId(GROUP),
            message_id: MessageId(500),
        },
        Some(ChatId(GROUP)),
    );
    session
        .reply_targets
        .insert((GROUP, 500), ReplyTarget::Loading);
    assert!(session.reply_fetch_candidates().is_empty());
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"message","@extra":"{}","id":3,"chat_id":-1001,"sender_id":{{"@type":"messageSenderUser","user_id":31}},"is_outgoing":false,"date":1699990000,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"the old question","entities":[]}}}}}}"#,
            extra.0
        ),
    );
    // The answer stays beside the history.
    assert!(!session.histories[&GROUP].messages.contains_key(&3));
    let header = session.reply_header(&message(&session, 500)).unwrap();
    assert_eq!(header.state, ReplyState::Ready);
    assert_eq!(header.name.as_deref(), Some("Ada Lovelace"));
    assert_eq!(header.text, "the old question");
    assert!(header.clickable);
}

#[test]
fn reply_to_a_deleted_message_reads_deleted_message() {
    let (mut session, sink, seq) = seeded();
    apply_json(
        &mut session,
        &seq,
        &sink,
        &text_message(
            500,
            77,
            "late answer",
            r#","reply_to":{"@type":"messageReplyToMessage","chat_id":-1001,"message_id":3,"quote":null,"checklist_task_id":0,"poll_option_id":"","origin":null,"origin_send_date":0,"content":null}"#,
        ),
    );
    let extra = session.request(
        RequestPurpose::GetRepliedMessage {
            chat_id: ChatId(GROUP),
            message_id: MessageId(500),
        },
        Some(ChatId(GROUP)),
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":404,"message":"Message not found"}}"#,
            extra.0
        ),
    );
    let header = session.reply_header(&message(&session, 500)).unwrap();
    assert_eq!(header.state, ReplyState::Deleted);
    assert_eq!(header.text, "Deleted message");
    assert_eq!(header.name, None);
    assert!(!header.clickable);
    // A failed fetch is not retried on every ingest.
    assert!(session.reply_fetch_candidates().is_empty());
}

#[test]
fn reply_from_another_chat_names_sender_and_chat() {
    let (mut session, sink, seq) = seeded();
    apply_json(
        &mut session,
        &seq,
        &sink,
        &text_message(
            12,
            77,
            "cross answer",
            r#","reply_to":{"@type":"messageReplyToMessage","chat_id":31,"message_id":9,"quote":null,"checklist_task_id":0,"poll_option_id":"","origin":{"@type":"messageOriginUser","sender_user_id":31},"origin_send_date":1699000000,"content":{"@type":"messagePhoto","photo":{"@type":"photo","has_stickers":false,"minithumbnail":null,"sizes":[{"@type":"photoSize","type":"m","photo":{"@type":"file","id":900,"size":10,"expected_size":10,"local":{"@type":"localFile","path":"","can_be_downloaded":true,"can_be_deleted":false,"is_downloading_active":false,"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0},"remote":{"@type":"remoteFile","id":"r","unique_id":"u","is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":10}},"width":320,"height":240,"progressive_sizes":[]}]},"caption":{"@type":"formattedText","text":"a sunset","entities":[]},"show_caption_above_media":false,"has_spoiler":false,"is_secret":false}}"#,
        ),
    );
    let reply = message(&session, 12).reply_to.unwrap();
    assert_eq!(reply.origin_send_date, 1699000000);
    assert!(matches!(reply.origin, Some(MessageOrigin::User { .. })));
    assert!(reply.content.is_some());
    let header = session.reply_header(&message(&session, 12)).unwrap();
    assert_eq!(header.state, ReplyState::Ready);
    assert_eq!(header.external_chat.as_deref(), Some("Ada Lovelace"));
    assert_eq!(header.name.as_deref(), Some("Ada Lovelace"));
    assert_eq!(header.thumb, vec![FileId(900)]);
    assert!(header.clickable);
    assert_eq!(
        header.name_line().as_deref(),
        Some("Ada Lovelace › Ada Lovelace")
    );
}

#[test]
fn forward_header_covers_every_origin() {
    let (mut session, sink, seq) = seeded();
    let cases = [
        (
            20,
            r#"{"@type":"messageOriginUser","sender_user_id":31}"#,
            "Ada Lovelace",
            None,
            ForwardLink::User(UserId(31)),
        ),
        (
            21,
            r#"{"@type":"messageOriginHiddenUser","sender_name":"Grace H."}"#,
            "Grace H.",
            None,
            ForwardLink::Hidden,
        ),
        (
            22,
            r#"{"@type":"messageOriginChat","sender_chat_id":-1001,"author_signature":"admin"}"#,
            "Rustaceans",
            Some("admin"),
            ForwardLink::Chat {
                chat_id: ChatId(-1001),
                message_id: None,
            },
        ),
        (
            23,
            r#"{"@type":"messageOriginChannel","chat_id":-1002,"message_id":4242,"author_signature":"Eve"}"#,
            "News Channel",
            Some("Eve"),
            ForwardLink::Chat {
                chat_id: ChatId(-1002),
                message_id: Some(MessageId(4242)),
            },
        ),
    ];
    for (id, origin, name, signature, link) in cases {
        apply_json(
            &mut session,
            &seq,
            &sink,
            &text_message(
                id,
                77,
                "fwd",
                &format!(
                    r#","forward_info":{{"@type":"messageForwardInfo","origin":{origin},"date":1690000000,"source":null,"public_service_announcement_type":""}}"#
                ),
            ),
        );
        let header = session.forward_header(&message(&session, id)).unwrap();
        assert_eq!(header.name, name, "origin {origin}");
        assert_eq!(header.signature.as_deref(), signature);
        assert_eq!(header.link, link);
        assert_eq!(header.original_date, 1690000000);
        assert!(!header.imported);
    }
    let hidden = session.forward_header(&message(&session, 21)).unwrap();
    assert_eq!(
        hidden.tooltip(),
        Some("The account was hidden by the user.")
    );
    let channel = session.forward_header(&message(&session, 23)).unwrap();
    assert_eq!(channel.display_name(), "News Channel (Eve)");
}

#[test]
fn forward_from_an_unknown_channel_falls_back_without_a_link() {
    let (mut session, sink, seq) = seeded();
    apply_json(
        &mut session,
        &seq,
        &sink,
        &text_message(
            24,
            77,
            "fwd",
            r#","forward_info":{"@type":"messageForwardInfo","origin":{"@type":"messageOriginChannel","chat_id":-1009,"message_id":5,"author_signature":""},"date":1,"source":null,"public_service_announcement_type":""}"#,
        ),
    );
    let header = session.forward_header(&message(&session, 24)).unwrap();
    assert_eq!(header.name, "");
    assert_eq!(header.link, ForwardLink::None);
}

#[test]
fn imported_messages_read_as_forwards_with_a_notice() {
    let (mut session, sink, seq) = seeded();
    apply_json(
        &mut session,
        &seq,
        &sink,
        &text_message(
            25,
            77,
            "old chat",
            r#","import_info":{"@type":"messageImportInfo","sender_name":"Mallory","date":1600000000}"#,
        ),
    );
    let msg = message(&session, 25);
    let header = session.forward_header(&msg).unwrap();
    assert_eq!(header.name, "Mallory");
    assert!(header.imported);
    assert_eq!(header.link, ForwardLink::Imported);
    assert!(
        header
            .tooltip()
            .unwrap()
            .contains("imported from another app")
    );
    let tip = footer_tooltip(&msg, |unix| format!("@{unix}")).unwrap();
    assert!(tip.starts_with("This message was imported"));
    assert!(tip.contains("Original: @1600000000"));
}

#[test]
fn via_bot_and_edit_date_are_parsed_and_kept_current() {
    let (mut session, sink, seq) = seeded();
    apply_json(
        &mut session,
        &seq,
        &sink,
        &text_message(
            26,
            31,
            "via a bot",
            r#","via_bot_user_id":77,"edit_date":1700000500"#,
        ),
    );
    let msg = message(&session, 26);
    assert_eq!(msg.extras.via_bot_user_id, 77);
    assert_eq!(msg.extras.edit_date, 1700000500);
    assert_eq!(session.via_bot_label(&msg).as_deref(), Some("@gifbot"));

    apply_json(
        &mut session,
        &seq,
        &sink,
        &text_message(27, 31, "plain", ""),
    );
    let plain = message(&session, 27);
    assert_eq!(plain.extras.edit_date, 0);
    assert_eq!(session.via_bot_label(&plain), None);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateMessageEdited","chat_id":-1001,"message_id":27,"edit_date":1700009999,"reply_markup":null}"#,
    );
    assert_eq!(message(&session, 27).extras.edit_date, 1700009999);
}

#[test]
fn footer_tooltip_lists_sent_edited_and_original_dates() {
    let (mut session, sink, seq) = seeded();
    apply_json(
        &mut session,
        &seq,
        &sink,
        &text_message(
            28,
            31,
            "everything",
            r#","edit_date":1700000900,"forward_info":{"@type":"messageForwardInfo","origin":{"@type":"messageOriginUser","sender_user_id":31},"date":1650000000,"source":null,"public_service_announcement_type":""}"#,
        ),
    );
    let tip = footer_tooltip(&message(&session, 28), |unix| format!("@{unix}")).unwrap();
    assert_eq!(
        tip,
        "@1700000000\nEdited: @1700000900\nOriginal: @1650000000"
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &text_message(29, 31, "plain", ""),
    );
    assert_eq!(
        footer_tooltip(&message(&session, 29), |unix| format!("@{unix}")).as_deref(),
        Some("@1700000000")
    );
}
