//! State reducer tests: Premium emoji statuses next to names (chat rows,
//! channels, message senders) and when Premium marks show at all.
use super::common::*;
use super::*;
use crate::peer_badge::TitleBadge;

const GROUP: i64 = -200;
const CHANNEL: i64 = -300;

fn world() -> (Session, Arc<MemorySink>, AtomicU64) {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    for json in [
        r#"{"@type":"updateUser","user":{"id":11,"first_name":"Ada","type":{"@type":"userTypeRegular"},"is_premium":true,"emoji_status":{"@type":"emojiStatus","type":{"@type":"emojiStatusTypeCustomEmoji","custom_emoji_id":"777"},"expiration_date":0}}}"#,
        r#"{"@type":"updateUser","user":{"id":12,"first_name":"Bo","type":{"@type":"userTypeRegular"},"is_premium":true}}"#,
        r#"{"@type":"updateUser","user":{"id":13,"first_name":"Cy","type":{"@type":"userTypeRegular"}}}"#,
        r#"{"@type":"updateNewChat","chat":{"id":11,"title":"Ada","type":{"@type":"chatTypePrivate","user_id":11},"unread_count":0}}"#,
        r#"{"@type":"updateNewChat","chat":{"id":-200,"title":"Club","type":{"@type":"chatTypeBasicGroup","basic_group_id":2},"unread_count":0}}"#,
        r#"{"@type":"updateNewChat","chat":{"id":-300,"title":"News","type":{"@type":"chatTypeSupergroup","supergroup_id":300,"is_channel":true},"unread_count":0}}"#,
    ] {
        apply_json(&mut session, &seq, &sink, json);
    }
    session.open_chat(ChatId(GROUP));
    (session, sink, seq)
}

fn post(
    session: &mut Session,
    (sink, seq): (&Arc<MemorySink>, &AtomicU64),
    id: i64,
    chat: i64,
    sender: &str,
) -> HistoryMessage {
    session.open_chat(ChatId(chat));
    apply_json(
        session,
        seq,
        sink,
        &format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":{id},"chat_id":{chat},"sender_id":{sender},"is_outgoing":false,"date":1700000000,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"hi","entities":[]}}}}}}}}"#
        ),
    );
    session.histories[&chat].messages[&id].clone()
}

fn user(id: i64) -> String {
    format!(r#"{{"@type":"messageSenderUser","user_id":{id}}}"#)
}

fn chat(id: i64) -> String {
    format!(r#"{{"@type":"messageSenderChat","chat_id":{id}}}"#)
}

const CHANNEL_STATUS: &str = r#"{"@type":"updateChatEmojiStatus","chat_id":-300,"emoji_status":{"@type":"emojiStatus","type":{"@type":"emojiStatusTypeCustomEmoji","custom_emoji_id":"4242"},"expiration_date":0}}"#;

#[test]
fn a_channel_with_an_emoji_status_carries_it_in_its_row() {
    let (mut session, sink, seq) = world();
    let channel = session.chats.get(&CHANNEL).unwrap().clone();
    assert_eq!(session.chat_title_badge(&channel), None);
    apply_json(&mut session, &seq, &sink, CHANNEL_STATUS);
    assert_eq!(
        session.chat_title_badge(&channel),
        Some(TitleBadge::EmojiStatus(4242))
    );
    // Its image is asked for with the other row emoji.
    assert!(
        session
            .message_custom_emoji_ids_to_resolve()
            .contains(&4242)
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateChatEmojiStatus","chat_id":-300,"emoji_status":null}"#,
    );
    assert_eq!(session.chat_title_badge(&channel), None);
}

#[test]
fn premium_marks_hide_where_premium_cannot_be_bought() {
    let (mut session, sink, seq) = world();
    let row = session.chats.get(&11).unwrap().clone();
    assert!(session.premium_badges_shown());
    assert_eq!(
        session.chat_title_badge(&row),
        Some(TitleBadge::EmojiStatus(777))
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateOption","name":"is_premium_available","value":{"@type":"optionValueBoolean","value":false}}"#,
    );
    // `Session::premiumBadgesShown`: without Premium and without a way to
    // buy it, no star and no status.
    assert!(!session.premium_badges_shown());
    assert_eq!(session.chat_title_badge(&row), None);
    let sent = post(&mut session, (&sink, &seq), 5, GROUP, &user(11));
    assert_eq!(session.sender_status(&sent), None);
}

#[test]
fn a_group_sender_shows_the_status_the_star_or_nothing() {
    let (mut session, sink, seq) = world();
    let args = (&sink, &seq);
    // A Premium user with a status: that status.
    let with_status = post(&mut session, args, 1, GROUP, &user(11));
    assert_eq!(session.sender_status(&with_status), Some(777));
    assert_eq!(session.sender_status_id(&with_status), Some(777));
    // Premium without a status: the bare star.
    let star = post(&mut session, args, 2, GROUP, &user(12));
    assert_eq!(session.sender_status(&star), Some(0));
    assert_eq!(session.sender_status_id(&star), None);
    // A regular member: nothing.
    let plain = post(&mut session, args, 3, GROUP, &user(13));
    assert_eq!(session.sender_status(&plain), None);
}

#[test]
fn a_channel_sender_shows_its_status_unless_it_is_the_chat_itself() {
    let (mut session, sink, seq) = world();
    apply_json(&mut session, &seq, &sink, CHANNEL_STATUS);
    let args = (&sink, &seq);
    // The channel posting in a group.
    let in_group = post(&mut session, args, 1, GROUP, &chat(CHANNEL));
    assert_eq!(session.sender_status(&in_group), Some(4242));
    // Posts in the channel's own chat (`from != history()->peer`).
    let own = post(&mut session, args, 2, CHANNEL, &chat(CHANNEL));
    assert_eq!(session.sender_status(&own), None);
    // A chat sender without a status.
    let other = post(&mut session, args, 3, GROUP, &chat(GROUP));
    assert_eq!(session.sender_status(&other), None);
}

#[test]
fn the_statuses_of_the_open_chat_senders_are_asked_for() {
    let (mut session, sink, seq) = world();
    post(&mut session, (&sink, &seq), 1, GROUP, &user(11));
    session.open_chat(ChatId(GROUP));
    assert!(session.message_custom_emoji_ids_to_resolve().contains(&777));
}
