//! Screenshot demo: the message menu over a poll option, a checklist task, a
//! shared contact and a post sent on behalf of a channel.

use crate::ui::app::QuillApp;
use crate::ui::message_menu_ui::item_rows::{CHANNEL, GROUP};
use crate::ui::screenshot_demo::{DemoSpec, register_demos};
use gpui_kit::*;
use quill::state::Session;

register_demos![
    // The message menu's item rows (injected, no live Telegram).
    // `QUILL_DEMO_ITEMS` is `poll`, `task`, `contact` or `author`: which
    // message the menu facts (what TDLib allowed) are injected for.
    DemoSpec::chats(
        "ready-item-menus",
        "screenshot demo — menu over a poll option, task, contact and post author (injected, no live Telegram)"
    )
    .setup(QuillApp::demo_item_menus),
];

impl QuillApp {
    fn demo_item_menus(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        use std::sync::atomic::Ordering;
        let mode = std::env::var("QUILL_DEMO_ITEMS").unwrap_or_else(|_| "poll".into());
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_item_menus(session, &self.demo_ui.sink, &self.demo_ui.seq);
            session.open_chat(if mode == "author" { CHANNEL } else { GROUP });
            crate::ui::message_menu_ui::item_rows::inject_menu_facts(session, &mode);
        }
        cx.notify();
    }
}

/// A group with a poll, a checklist and a shared contact, and a channel with
/// a post whose real author is a member.
fn apply_item_menus(
    session: &mut Session,
    sink: &std::sync::Arc<quill::diagnostics::MemorySink>,
    seq: &std::sync::atomic::AtomicU64,
) {
    use quill::diagnostics::DiagnosticSink;
    use quill::telegram::client::copy_and_parse;
    let dyn_sink: std::sync::Arc<dyn DiagnosticSink> = sink.clone();
    let text =
        |value: &str| format!(r#"{{"@type":"formattedText","text":"{value}","entities":[]}}"#);
    let message = |id: i64, chat: i64, sender: &str, content: String| {
        format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":{id},"chat_id":{chat},"sender_id":{sender},"is_outgoing":false,"date":1700000{id},"content":{content}}}}}"#
        )
    };
    let user = |id: i64| format!(r#"{{"@type":"messageSenderUser","user_id":{id}}}"#);
    // Option "a" is the user's vote; option "b" was added by Nora.
    let option = |id: &str, label: &str, votes: i32, percent: i32| {
        let chosen = id == "a";
        let added = if id == "b" {
            r#","author":{"@type":"messageSenderUser","user_id":32},"addition_date":1700000000"#
        } else {
            ""
        };
        format!(
            r#"{{"@type":"pollOption","id":"{id}","text":{},"voter_count":{votes},"vote_percentage":{percent},"is_chosen":{chosen}{added}}}"#,
            text(label)
        )
    };
    let poll = format!(
        r#"{{"@type":"messagePoll","poll":{{"@type":"poll","id":9200,"question":{},"options":[{},{},{}],"total_voter_count":5,"is_anonymous":true,"allows_multiple_answers":false,"allows_revoting":true,"is_closed":false,"type":{{"@type":"pollTypeRegular"}}}},"description":{},"can_add_option":true}}"#,
        text("Where should we go on Saturday?"),
        option("a", "Hike", 2, 40),
        option("b", "Beach", 3, 60),
        option("c", "Museum", 0, 0),
        text("")
    );
    let task = |id: i32, label: &str| {
        format!(
            r#"{{"@type":"checklistTask","id":{id},"text":{},"completion_date":0}}"#,
            text(label)
        )
    };
    let checklist = format!(
        r#"{{"@type":"messageChecklist","list":{{"@type":"checklist","title":{},"tasks":[{},{},{}],"others_can_add_tasks":true,"can_add_tasks":true,"others_can_mark_tasks_as_done":true,"can_mark_tasks_as_done":true}}}}"#,
        text("Packing list"),
        task(1, "Sunscreen"),
        task(2, "Towels"),
        task(3, "Snacks"),
    );
    let contact = r#"{"@type":"messageContact","contact":{"@type":"contact","phone_number":"+15551234567","first_name":"Mira","last_name":"Stone","vcard":"","user_id":0}}"#.to_string();
    let jsons = [
        r#"{"@type":"updateUser","user":{"id":31,"first_name":"Ada","last_name":"Lovelace","type":{"@type":"userTypeRegular"},"status":{"@type":"userStatusOnline","expires":1}}}"#.to_string(),
        r#"{"@type":"updateUser","user":{"id":32,"first_name":"Nora","last_name":"Vale","type":{"@type":"userTypeRegular"},"status":{"@type":"userStatusOffline","was_online":1}}}"#.to_string(),
        r#"{"@type":"updateNewChat","chat":{"id":-2003,"title":"Weekend plans","type":{"@type":"chatTypeBasicGroup","basic_group_id":2003},"unread_count":0}}"#.to_string(),
        r#"{"@type":"updateChatPosition","chat_id":-2003,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"60","is_pinned":false}}"#.to_string(),
        r#"{"@type":"updateNewChat","chat":{"id":-1002000000002,"title":"Quill news","type":{"@type":"chatTypeSupergroup","supergroup_id":2000000002,"is_channel":true},"unread_count":0}}"#.to_string(),
        r#"{"@type":"updateChatPosition","chat_id":-1002000000002,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"52","is_pinned":false}}"#.to_string(),
        message(20, -2003, &user(31), poll),
        message(21, -2003, &user(31), checklist),
        message(22, -2003, &user(31), contact),
        message(
            30,
            -1002000000002,
            r#"{"@type":"messageSenderChat","chat_id":-1002000000002}"#,
            format!(
                r#"{{"@type":"messageText","text":{}}}"#,
                text("Release notes are up")
            ),
        ),
    ];
    for json in jsons {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
    if let Some(chat) = session.chats.get_mut(&CHANNEL.0) {
        chat.set_member_status(
            quill::telegram::envelope::ChannelMemberStatus::Creator,
            None,
        );
    }
}
