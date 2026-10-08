//! Chat-list previews of service messages use the service wording.
use super::common::*;
use super::*;

fn apply_all(jsons: &[&str]) -> Session {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    for json in jsons {
        apply_json(&mut session, &seq, &sink, json);
    }
    session
}

const GROUP: &str = r#"{"@type":"updateNewChat","chat":{"id":61,"title":"Club","type":{"@type":"chatTypeBasicGroup","basic_group_id":61},"unread_count":0}}"#;
const DANA: &str = r#"{"@type":"updateUser","user":{"@type":"user","id":1,"first_name":"Dana","last_name":"Cole","usernames":null,"phone_number":"","status":null,"profile_photo":null,"is_contact":true,"type":{"@type":"userTypeRegular"}}}"#;

#[test]
fn pinned_message_preview_names_the_actor_and_excerpt() {
    let session = apply_all(&[
        GROUP,
        DANA,
        r#"{"@type":"updateNewMessage","message":{"id":10,"chat_id":61,"is_outgoing":false,"sender_id":{"@type":"messageSenderUser","user_id":1},"date":5,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"Lunch at noon, bring snacks!","entities":[]}}}}"#,
        r#"{"@type":"updateChatLastMessage","chat_id":61,"last_message":{"id":11,"chat_id":61,"is_outgoing":false,"sender_id":{"@type":"messageSenderUser","user_id":1},"date":6,"content":{"@type":"messagePinMessage","message_id":10}},"positions":[]}"#,
    ]);
    let chat = &session.chats[&61];
    assert_eq!(
        chat.last_preview,
        "Dana Cole pinned \"Lunch at noon, b\u{2026}\""
    );
    assert!(chat.last_preview_style.service);
    // No "Dana:" prefix on top of the actor already named in the text.
    assert_eq!(session.chat_preview_sender(chat), None);
    assert_eq!(chat.last_preview_sender, "");
}

#[test]
fn member_joined_preview_uses_the_user_name() {
    let session = apply_all(&[
        GROUP,
        DANA,
        r#"{"@type":"updateChatLastMessage","chat_id":61,"last_message":{"id":12,"chat_id":61,"is_outgoing":false,"sender_id":{"@type":"messageSenderUser","user_id":1},"date":6,"content":{"@type":"messageChatAddMembers","member_user_ids":[1]}},"positions":[]}"#,
    ]);
    assert_eq!(
        session.chats[&61].last_preview,
        "Dana Cole joined the group"
    );
}
