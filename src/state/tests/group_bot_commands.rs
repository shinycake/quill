//! The composer's `/` menu in groups: every bot's commands from the group's
//! full info, through the reducer.
use super::common::*;
use super::*;

const BASIC: i64 = -1001;
const MEGAGROUP: i64 = -1002;
const CHANNEL: i64 = -1003;

fn bot_json(id: i64, username: &str) -> String {
    format!(
        r#"{{"@type":"updateUser","user":{{"id":{id},"first_name":"Bot{id}","usernames":{{"@type":"usernames","active_usernames":["{username}"],"disabled_usernames":[],"editable_username":"{username}"}},"type":{{"@type":"userTypeBot","is_inline":false}},"status":{{"@type":"userStatusOffline","was_online":1}}}}}}"#
    )
}

fn commands_json(bot: i64, names: &[&str]) -> String {
    let list: Vec<String> = names
        .iter()
        .map(|n| {
            format!(
                r#"{{"@type":"botCommand","command":"{n}","description":"{n} of {bot}","is_ephemeral":false}}"#
            )
        })
        .collect();
    format!(
        r#"{{"@type":"botCommands","bot_user_id":{bot},"commands":[{}]}}"#,
        list.join(",")
    )
}

fn seeded() -> (Session, Arc<MemorySink>, AtomicU64) {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    for json in [
        bot_json(41, "first_bot"),
        bot_json(42, "second_bot"),
        r#"{"@type":"updateNewChat","chat":{"id":-1001,"title":"Basic","type":{"@type":"chatTypeBasicGroup","basic_group_id":1},"unread_count":0}}"#.to_string(),
        r#"{"@type":"updateNewChat","chat":{"id":-1002,"title":"Mega","type":{"@type":"chatTypeSupergroup","supergroup_id":2,"is_channel":false},"unread_count":0}}"#.to_string(),
        r#"{"@type":"updateNewChat","chat":{"id":-1003,"title":"News","type":{"@type":"chatTypeSupergroup","supergroup_id":3,"is_channel":true},"unread_count":0}}"#.to_string(),
    ] {
        apply_json(&mut session, &seq, &sink, &json);
    }
    (session, sink, seq)
}

fn rows(session: &Session, chat: i64) -> Vec<(i64, String, String)> {
    session
        .command_menu_items(ChatId(chat))
        .into_iter()
        .map(|row| {
            let bot = row.bot.unwrap();
            assert!(bot.in_group);
            (bot.user_id, row.command, bot.username)
        })
        .collect()
}

fn supergroup_update(id: i64, bots: &[String]) -> String {
    format!(
        r#"{{"@type":"updateSupergroupFullInfo","supergroup_id":{id},"supergroup_full_info":{{"description":"","member_count":3,"bot_commands":[{}]}}}}"#,
        bots.join(",")
    )
}

#[test]
fn a_megagroup_lists_the_commands_of_all_its_bots() {
    let (mut session, sink, seq) = seeded();
    assert!(session.command_menu_items(ChatId(MEGAGROUP)).is_empty());
    apply_json(
        &mut session,
        &seq,
        &sink,
        &supergroup_update(
            2,
            &[
                commands_json(41, &["start", "help"]),
                commands_json(42, &["start"]),
            ],
        ),
    );
    assert_eq!(
        rows(&session, MEGAGROUP),
        vec![
            (41, "start".to_string(), "first_bot".to_string()),
            (41, "help".to_string(), "first_bot".to_string()),
            (42, "start".to_string(), "second_bot".to_string()),
        ]
    );
}

#[test]
fn the_full_info_answer_fills_the_menu_too() {
    let (mut session, sink, seq) = seeded();
    let extra = session.request_for_supergroup(RequestPurpose::GetSupergroupFullInfo, 2);
    let json = format!(
        r#"{{"@type":"supergroupFullInfo","@extra":"{}","description":"","member_count":3,"bot_commands":[{}]}}"#,
        extra.0,
        commands_json(42, &["ping"])
    );
    apply_json(&mut session, &seq, &sink, &json);
    assert_eq!(
        rows(&session, MEGAGROUP),
        vec![(42, "ping".to_string(), "second_bot".to_string())]
    );
}

#[test]
fn a_broadcast_channel_has_no_command_menu() {
    let (mut session, sink, seq) = seeded();
    apply_json(
        &mut session,
        &seq,
        &sink,
        &supergroup_update(3, &[commands_json(41, &["start"])]),
    );
    assert!(session.command_menu_items(ChatId(CHANNEL)).is_empty());
}

#[test]
fn a_bot_the_app_does_not_know_yet_is_left_out() {
    let (mut session, sink, seq) = seeded();
    apply_json(
        &mut session,
        &seq,
        &sink,
        &supergroup_update(
            2,
            &[commands_json(99, &["ghost"]), commands_json(41, &["start"])],
        ),
    );
    assert_eq!(
        rows(&session, MEGAGROUP),
        vec![(41, "start".to_string(), "first_bot".to_string())]
    );
}

fn message_from(chat: i64, id: i64, sender: i64) -> String {
    format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":{id},"chat_id":{chat},"sender_id":{{"@type":"messageSenderUser","user_id":{sender}}},"is_outgoing":false,"date":1700000000,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"hi","entities":[]}}}}}}}}"#
    )
}

#[test]
fn a_basic_group_lists_the_bots_that_wrote_last_first() {
    let (mut session, sink, seq) = seeded();
    session.open_chat(ChatId(BASIC));
    let extra = session.request(RequestPurpose::GetBasicGroupFullInfo, Some(ChatId(BASIC)));
    let json = format!(
        r#"{{"@type":"basicGroupFullInfo","@extra":"{}","description":"","members":[],"bot_commands":[{},{}]}}"#,
        extra.0,
        commands_json(41, &["start"]),
        commands_json(42, &["start"]),
    );
    apply_json(&mut session, &seq, &sink, &json);
    // No messages yet: TDLib's order.
    let order = |session: &Session| -> Vec<i64> {
        rows(session, BASIC).into_iter().map(|row| row.0).collect()
    };
    assert_eq!(order(&session), vec![41, 42]);
    apply_json(&mut session, &seq, &sink, &message_from(BASIC, 10, 41));
    apply_json(&mut session, &seq, &sink, &message_from(BASIC, 11, 42));
    assert_eq!(order(&session), vec![42, 41]);
    // The group changing its bots arrives as an update.
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateBasicGroupFullInfo","basic_group_id":1,"basic_group_full_info":{"description":"","members":[],"bot_commands":[{"@type":"botCommands","bot_user_id":41,"commands":[{"@type":"botCommand","command":"only","description":"","is_ephemeral":false}]}]}}"#,
    );
    assert_eq!(
        rows(&session, BASIC),
        vec![(41, "only".to_string(), "first_bot".to_string())]
    );
}
