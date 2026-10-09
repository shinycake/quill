//! Screenshot-demo fixtures for scheduled messages: the composer button,
//! the date+time picker, the scheduled list and the Saved Messages
//! "reminder" wording (injected through the real reducer paths, no live
//! Telegram). `QUILL_DEMO_SCHEDULED=button|picker|list|reminder|reminder-list`.

use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::ChatId;
use quill::local_time::now_unix;
use quill::state::{RequestPurpose, Session};
use quill::telegram::client::copy_and_parse;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

const FRIEND: i64 = 51;
const SAVED: i64 = 61;

/// Which scenario `QUILL_DEMO_SCHEDULED` asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ScheduledView {
    Button,
    Picker,
    List,
    Reminder,
    ReminderList,
}

impl ScheduledView {
    pub(super) fn from_env() -> Self {
        match std::env::var("QUILL_DEMO_SCHEDULED").as_deref() {
            Ok("picker") => Self::Picker,
            Ok("list") => Self::List,
            Ok("reminder") => Self::Reminder,
            Ok("reminder-list") => Self::ReminderList,
            _ => Self::Button,
        }
    }

    pub(super) fn saved(self) -> bool {
        matches!(self, Self::Reminder | Self::ReminderList)
    }
}

fn text_message(id: i64, chat: i64, date: i64, outgoing: bool, body: &str, extra: &str) -> String {
    format!(
        r#"{{"id":{id},"chat_id":{chat},"date":{date},"is_outgoing":{outgoing},"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"{body}","entities":[]}}}}{extra}}}"#
    )
}

fn at_date(send_date: i64) -> String {
    format!(
        r#","scheduling_state":{{"@type":"messageSchedulingStateSendAtDate","send_date":{send_date},"repeat_period":0}}"#
    )
}

/// A chat to schedule in (a friend, or Saved Messages for the reminder
/// wording), opened, with two scheduled messages and the chat flag set.
pub(super) fn apply_ready_scheduled(
    session: &mut Session,
    sink: &Arc<MemorySink>,
    seq: &AtomicU64,
    view: ScheduledView,
) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let (chat, title) = if view.saved() {
        (SAVED, "Saved Messages")
    } else {
        (FRIEND, "Maya Cohen")
    };
    if view.saved() {
        session.my_user_id = Some(SAVED);
    }
    let now = now_unix();
    let mut jsons = vec![
        format!(
            r#"{{"@type":"updateNewChat","chat":{{"id":{chat},"title":"{title}","type":{{"@type":"chatTypePrivate","user_id":{chat}}},"unread_count":0,"has_scheduled_messages":true}}}}"#
        ),
        format!(
            r#"{{"@type":"updateChatPosition","chat_id":{chat},"position":{{"@type":"chatPosition","list":{{"@type":"chatListMain"}},"order":"99","is_pinned":false}}}}"#
        ),
        format!(
            r#"{{"@type":"updateNewMessage","message":{}}}"#,
            text_message(401, chat, now - 3600, false, "Dinner on Friday?", "")
        ),
    ];
    let (first, second) = if view.saved() {
        ("Renew the car insurance", "Call the dentist")
    } else {
        (
            "Happy birthday! Let's celebrate this weekend.",
            "Don't forget the tickets.",
        )
    };
    let extra = session.request(RequestPurpose::GetChatScheduledMessages, Some(ChatId(chat)));
    jsons.push(format!(
        r#"{{"@type":"messages","@extra":"{}","total_count":2,"messages":[{},{}]}}"#,
        extra.0,
        text_message(501, chat, 0, true, first, &at_date(now + 2 * 3600)),
        text_message(502, chat, 0, true, second, &at_date(now + 30 * 3600)),
    ));
    for json in jsons {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
    session.open_chat(ChatId(chat));
}
