//! `ready-event-log`: the recent-actions page of a channel the viewer
//! administers, with a first page loaded and older events still to come
//! (the spinner row at the end). `QUILL_DEMO_EVENT_LOG=exhausted` shows
//! the end of the log without the spinner, `=empty` a log with no events. Injected, no live Telegram.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::demo::seed_ready_chats_session;
use super::dialogs::group_admin_settings::GroupSettingsView;
use super::screenshot_demo::{DemoSpec, register_demos};
use gpui_kit::*;
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::ChatId;
use quill::local_time::now_unix;
use quill::state::{GroupsPurpose, RequestPurpose, Session};
use quill::telegram::client::copy_and_parse;
use quill::telegram::envelope::ChannelMemberStatus;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

register_demos![
    DemoSpec::ready(
        "ready-event-log",
        seed_event_log_session,
        "screenshot demo — a channel's recent actions (injected, no live Telegram)"
    )
    .setup(QuillApp::demo_event_log),
];

const CHANNEL: ChatId = ChatId(13);

fn event(id: i64, minutes_ago: i64, actor: i64, action: &str) -> String {
    let date = now_unix() - minutes_ago * 60;
    format!(
        r#"{{"@type":"chatEvent","id":{id},"date":{date},"member_id":{{"@type":"messageSenderUser","user_id":{actor}}},"action":{action}}}"#
    )
}

fn message(id: i64, text: &str) -> String {
    format!(
        r#"{{"@type":"message","id":{id},"chat_id":13,"date":1700000000,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"{text}","entities":[]}}}}}}"#
    )
}

fn seed_event_log_session(sink: Arc<MemorySink>) -> Session {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let mut session = seed_ready_chats_session(sink);
    let seq = AtomicU64::new(session.last_seq);
    for json in [
        r#"{"@type":"updateUser","user":{"id":21,"first_name":"Maya","last_name":"Chen","accent_color_id":2,"type":{"@type":"userTypeRegular"}}}"#,
        r#"{"@type":"updateUser","user":{"id":22,"first_name":"Leo","last_name":"Park","accent_color_id":5,"type":{"@type":"userTypeRegular"}}}"#,
    ] {
        if let Some(owned) = copy_and_parse(json, &seq, &dyn_sink) {
            session.apply(owned);
        }
    }
    if let Some(chat) = session.chats.get_mut(&CHANNEL.0) {
        chat.set_member_status(ChannelMemberStatus::Administrator, None);
    }
    let texts = [
        "Release notes are up. Questions welcome below.",
        "Maintenance tonight at 22:00 UTC.",
        "Welcome to the channel, everyone.",
        "Poll closes on Friday.",
    ];
    let mut events = Vec::new();
    let count = match std::env::var("QUILL_DEMO_EVENT_LOG").as_deref() {
        Ok("exhausted") => 48,
        Ok("empty") => 0,
        _ => 100,
    };
    for ix in 0..count {
        let id = 1_000 - ix;
        let actor = if ix % 3 == 0 { 22 } else { 21 };
        let text = texts.get(usize::try_from(ix).unwrap_or(0) % texts.len());
        let text = text.copied().unwrap_or_default();
        let action = match ix % 4 {
            0 => r#"{"@type":"chatEventMemberJoined"}"#.to_string(),
            1 => format!(
                r#"{{"@type":"chatEventMessageEdited","old_message":{},"new_message":{}}}"#,
                message(300 + ix, "Draft"),
                message(300 + ix, text)
            ),
            2 => format!(
                r#"{{"@type":"chatEventMessageDeleted","message":{}}}"#,
                message(400 + ix, text)
            ),
            _ => format!(
                r#"{{"@type":"chatEventMessagePinned","message":{}}}"#,
                message(500 + ix, text)
            ),
        };
        events.push(event(id, 3 + ix * 7, actor, &action));
    }
    let extra = session.request(
        RequestPurpose::Groups(GroupsPurpose::GetChatEventLog { from_event_id: 0 }),
        Some(CHANNEL),
    );
    let json = format!(
        r#"{{"@type":"chatEvents","@extra":"{}","events":[{}]}}"#,
        extra.0,
        events.join(",")
    );
    if let Some(owned) = copy_and_parse(&json, &seq, &dyn_sink) {
        session.apply(owned);
    }
    session
}

impl QuillApp {
    fn demo_event_log(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.ensure_event_log_search(window, cx);
        self.open_group_settings_view(CHANNEL, GroupSettingsView::RecentActions, cx);
    }
}
