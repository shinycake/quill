//! State reducer tests: chat-row draft line, send status and title badges.
use super::common::*;
use super::*;
use crate::peer_badge::TitleBadge;
use crate::state::{RowStatus, SidebarLine};

const NEW_CHAT_WITH_DRAFT: &str = r#"{"@type":"updateNewChat","chat":{"id":11,"title":"Ada","type":{"@type":"chatTypePrivate","user_id":11},"unread_count":UNREAD,"draft_message":{"@type":"draftMessage","reply_to":null,"date":1,"content":{"@type":"draftMessageContentText","text":{"@type":"formattedText","text":"meet at 6","entities":[]},"link_preview_options":null},"effect_id":"0","suggested_post_info":null}}}"#;

fn last_message_json(id: i64, outgoing: bool, sending_state: &str) -> String {
    format!(
        r#"{{"@type":"updateChatLastMessage","chat_id":11,"last_message":{{"id":{id},"chat_id":11,"date":1700000000,"is_outgoing":{outgoing},{sending_state}"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"hi","entities":[]}}}}}},"positions":[{{"@type":"chatPosition","list":{{"@type":"chatListMain"}},"order":"8","is_pinned":false}}]}}"#
    )
}

fn open_chat(unread: i32) -> (Session, Arc<MemorySink>, AtomicU64) {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &NEW_CHAT_WITH_DRAFT.replace("UNREAD", &unread.to_string()),
    );
    (session, sink, seq)
}

#[test]
fn draft_replaces_the_preview_when_nothing_is_unread() {
    let (mut session, sink, seq) = open_chat(0);
    apply_json(&mut session, &seq, &sink, &last_message_json(5, false, ""));
    let chat = session.chats.get(&11).unwrap();
    match chat.sidebar_line() {
        SidebarLine::Draft(draft) => {
            assert_eq!(draft.text, "meet at 6");
            assert!(!draft.reply);
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(chat.sidebar_preview(), "Draft: meet at 6");
}

#[test]
fn unread_messages_beat_the_draft() {
    // Telegram Desktop: "Draw item, if there are unread messages."
    let (mut session, sink, seq) = open_chat(2);
    apply_json(&mut session, &seq, &sink, &last_message_json(5, false, ""));
    let chat = session.chats.get(&11).unwrap();
    assert_eq!(chat.draft_preview(), None);
    assert_eq!(chat.sidebar_line(), SidebarLine::Text("hi".into()));
}

#[test]
fn typing_beats_the_draft() {
    let (mut session, sink, seq) = open_chat(0);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateChatAction","chat_id":11,"topic_id":null,"sender_id":{"@type":"messageSenderUser","user_id":11},"action":{"@type":"chatActionTyping"}}"#,
    );
    let chat = session.chats.get(&11).unwrap();
    assert_eq!(chat.sidebar_line(), SidebarLine::Text("typing".into()));
}

#[test]
fn row_status_follows_the_send_state() {
    let (mut session, sink, seq) = open_chat(0);
    // A draft hides the mark.
    apply_json(&mut session, &seq, &sink, &last_message_json(5, true, ""));
    assert_eq!(
        session.chats.get(&11).unwrap().row_status(),
        RowStatus::None
    );

    // Without the draft: sending, failed, sent, read.
    session.chats.get_mut(&11).unwrap().draft = None;
    let pending = r#""sending_state":{"@type":"messageSendingStatePending","sending_id":0},"#;
    apply_json(
        &mut session,
        &seq,
        &sink,
        &last_message_json(-1, true, pending),
    );
    assert_eq!(
        session.chats.get(&11).unwrap().row_status(),
        RowStatus::Sending
    );

    let failed = r#""sending_state":{"@type":"messageSendingStateFailed","can_retry":true},"#;
    apply_json(
        &mut session,
        &seq,
        &sink,
        &last_message_json(-1, true, failed),
    );
    assert_eq!(
        session.chats.get(&11).unwrap().row_status(),
        RowStatus::Failed
    );

    apply_json(&mut session, &seq, &sink, &last_message_json(9, true, ""));
    assert_eq!(
        session.chats.get(&11).unwrap().row_status(),
        RowStatus::Sent
    );
    session
        .chats
        .get_mut(&11)
        .unwrap()
        .last_read_outbox_message_id = MessageId(9);
    assert_eq!(
        session.chats.get(&11).unwrap().row_status(),
        RowStatus::Read
    );

    // Incoming messages never get a mark.
    apply_json(&mut session, &seq, &sink, &last_message_json(10, false, ""));
    assert_eq!(
        session.chats.get(&11).unwrap().row_status(),
        RowStatus::None
    );
}

#[test]
fn send_succeeded_turns_the_clock_into_a_check() {
    let (mut session, sink, seq) = open_chat(0);
    session.chats.get_mut(&11).unwrap().draft = None;
    let pending = r#""sending_state":{"@type":"messageSendingStatePending","sending_id":0},"#;
    apply_json(
        &mut session,
        &seq,
        &sink,
        &last_message_json(-1, true, pending),
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateMessageSendSucceeded","old_message_id":-1,"message":{"id":42,"chat_id":11,"date":1700000001,"is_outgoing":true,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"hi","entities":[]}}}}"#,
    );
    let chat = session.chats.get(&11).unwrap();
    assert_eq!(chat.row_status(), RowStatus::Sent);
    assert_eq!(chat.last_message.unwrap().id, MessageId(42));
}

#[test]
fn send_failed_marks_the_last_message_failed() {
    let (mut session, sink, seq) = open_chat(0);
    session.chats.get_mut(&11).unwrap().draft = None;
    let pending = r#""sending_state":{"@type":"messageSendingStatePending","sending_id":0},"#;
    apply_json(
        &mut session,
        &seq,
        &sink,
        &last_message_json(-1, true, pending),
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateMessageSendFailed","old_message_id":-1,"message":{"id":-1,"chat_id":11,"is_outgoing":true,"sending_state":{"@type":"messageSendingStateFailed","can_retry":true},"content":{"@type":"messageText","text":{"@type":"formattedText","text":"hi","entities":[]}}},"error":{"code":400,"message":"x"}}"#,
    );
    assert_eq!(
        session.chats.get(&11).unwrap().row_status(),
        RowStatus::Failed
    );
}

#[test]
fn user_title_badges_come_from_the_user_record() {
    let (mut session, sink, seq) = open_chat(0);
    let chat = session.chats.get(&11).unwrap().clone();
    assert_eq!(session.chat_title_badge(&chat), None);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateUser","user":{"id":11,"first_name":"Ada","type":{"@type":"userTypeRegular"},"is_premium":true,"verification_status":{"@type":"verificationStatus","is_verified":true,"is_scam":false,"is_fake":false,"bot_verification_icon_custom_emoji_id":"0"}}}"#,
    );
    // Verified hides the star.
    assert_eq!(session.chat_title_badge(&chat), Some(TitleBadge::Verified));
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateUser","user":{"id":11,"first_name":"Ada","type":{"@type":"userTypeRegular"},"is_premium":true,"emoji_status":{"@type":"emojiStatus","type":{"@type":"emojiStatusTypeCustomEmoji","custom_emoji_id":"5368324170671202286"},"expiration_date":0},"verification_status":{"@type":"verificationStatus","is_verified":false,"is_scam":false,"is_fake":false}}}"#,
    );
    assert_eq!(
        session.chat_title_badge(&chat),
        Some(TitleBadge::EmojiStatus(5368324170671202286))
    );
    // The status emoji is queued for resolution like preview emoji.
    assert!(
        session
            .message_custom_emoji_ids_to_resolve()
            .contains(&5368324170671202286)
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateUser","user":{"id":11,"first_name":"Ada","type":{"@type":"userTypeRegular"},"verification_status":{"@type":"verificationStatus","is_verified":false,"is_scam":true,"is_fake":false}}}"#,
    );
    assert_eq!(session.chat_title_badge(&chat), Some(TitleBadge::Scam));
}

#[test]
fn supergroup_title_badge_follows_verification() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewChat","chat":{"id":-100,"title":"News","type":{"@type":"chatTypeSupergroup","supergroup_id":100,"is_channel":true},"unread_count":0}}"#,
    );
    let chat = session.chats.get(&-100).unwrap().clone();
    assert_eq!(session.chat_title_badge(&chat), None);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateSupergroup","supergroup":{"@type":"supergroup","id":100,"is_channel":true,"verification_status":{"@type":"verificationStatus","is_verified":true,"is_scam":false,"is_fake":false}}}"#,
    );
    assert_eq!(session.chat_title_badge(&chat), Some(TitleBadge::Verified));
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateSupergroup","supergroup":{"@type":"supergroup","id":100,"is_channel":true,"verification_status":{"@type":"verificationStatus","is_verified":false,"is_scam":false,"is_fake":true}}}"#,
    );
    assert_eq!(session.chat_title_badge(&chat), Some(TitleBadge::Fake));
}
