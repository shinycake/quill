//! Screenshot-demo fixtures for "find in history": the calendar box, the
//! "From:" member picker and the global search filters (injected through the
//! real reducer paths, no live Telegram).

use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::{ChatId, MessageId};
use quill::search_filters::{
    GlobalSearchFilters, SearchChatType, SearchDateRange, SearchMediaKind,
};
use quill::state::{FromPicker, RequestPurpose, Session};
use quill::telegram::client::copy_and_parse;
use quill::telegram::envelope::{CalendarDay, MessageSender};
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

const GROUP: i64 = 21;
const MEMBERS: [(i64, &str, &str); 4] = [
    (31, "Maya", "Cohen"),
    (32, "Noam", "Levi"),
    (33, "Tal", "Shapiro"),
    (34, "Yael", "Barak"),
];

fn text(id: i64, sender: i64, date: i64, body: &str) -> String {
    format!(
        r#"{{"id":{id},"chat_id":{GROUP},"date":{date},"sender_id":{{"@type":"messageSenderUser","user_id":{sender}}},"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"{body}","entities":[]}}}}}}"#
    )
}

/// A "Book club" supergroup with four members and a few messages, opened.
fn apply_group(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let mut jsons: Vec<String> = MEMBERS
        .iter()
        .map(|(id, first, last)| {
            format!(
                r#"{{"@type":"updateUser","user":{{"@type":"user","id":{id},"first_name":"{first}","last_name":"{last}","usernames":null,"phone_number":"","status":null,"profile_photo":null,"is_contact":false,"is_mutual_contact":false,"is_close_friend":false,"is_verified":false,"is_premium":false,"is_support":false,"restriction_reason":"","is_scam":false,"is_fake":false,"is_bot":false,"type":{{"@type":"userTypeRegular"}}}}}}"#
            )
        })
        .collect();
    jsons.push(format!(
        r#"{{"@type":"updateNewChat","chat":{{"id":{GROUP},"title":"Book club","type":{{"@type":"chatTypeSupergroup","supergroup_id":{GROUP},"is_channel":false}},"unread_count":0}}}}"#
    ));
    jsons.push(format!(
        r#"{{"@type":"updateChatPosition","chat_id":{GROUP},"position":{{"@type":"chatPosition","list":{{"@type":"chatListMain"}},"order":"99","is_pinned":false}}}}"#
    ));
    let msgs = [
        (
            301,
            31,
            1_789_900_000,
            "Finished Dune last night, the ending!",
        ),
        (
            302,
            32,
            1_789_990_000,
            "No spoilers please, I am on chapter 12.",
        ),
        (303, 33, 1_790_100_000, "Next month we read Piranesi?"),
        (304, 31, 1_790_200_000, "Yes, and I will bring snacks."),
    ];
    for (id, sender, date, body) in msgs {
        jsons.push(format!(
            r#"{{"@type":"updateNewMessage","message":{}}}"#,
            text(id, sender, date, body)
        ));
    }
    for json in jsons {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
    session.open_chat(ChatId(GROUP));
}

fn apply_hits(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64, query: &str) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let generation = session.chat_search.begin_query(query);
    let extra = session.request_chat_search(
        RequestPurpose::SearchChatMessages,
        ChatId(GROUP),
        generation,
    );
    let json = format!(
        r#"{{"@type":"foundChatMessages","@extra":"{}","total_count":24,"next_from_message_id":300,"messages":[{},{}]}}"#,
        extra.0,
        text(304, 31, 1_790_200_000, "Yes, and I will bring snacks."),
        text(
            301,
            31,
            1_789_900_000,
            "Finished Dune last night, the ending!"
        ),
    );
    if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
        session.apply(owned);
    }
    let _ = session.begin_chat_search_jump(MessageId(304));
}

/// Calendar box over the group, media tab "Media" so days are highlighted.
pub(super) fn apply_ready_jump_date(
    session: &mut Session,
    sink: &Arc<MemorySink>,
    seq: &AtomicU64,
) {
    apply_group(session, sink, seq);
    session.open_chat_search();
    session.chat_search.media = SearchMediaKind::Media;
    // Fixed "now" inside September 2026 keeps the capture reproducible.
    session.open_history_calendar(1_790_000_000);
    if let Some(calendar) = session.history_calendar.as_mut() {
        let day = |id: i64, date: i32, count: i32| CalendarDay {
            total_count: count,
            message_id: MessageId(id << 20),
            date,
        };
        calendar.accept(vec![
            day(280, 1_789_900_000, 3),
            day(250, 1_789_500_000, 1),
            day(240, 1_789_100_000, 6),
            day(220, 1_788_500_000, 2),
            day(200, 1_787_900_000, 1),
        ]);
    }
}

/// The "From:" picker listing the group's members.
pub(super) fn apply_ready_search_from(
    session: &mut Session,
    sink: &Arc<MemorySink>,
    seq: &AtomicU64,
) {
    apply_group(session, sink, seq);
    session.open_chat_search();
    session.open_from_picker();
    if let Some(picker) = session.chat_search.from_picker.as_mut() {
        *picker = FromPicker {
            query: String::new(),
            members: MEMBERS
                .iter()
                .map(|(id, _, _)| MessageSender::User { user_id: *id })
                .collect(),
            request: None,
        };
    }
}

/// A member chosen: their messages with "N of M".
pub(super) fn apply_ready_search_from_hits(
    session: &mut Session,
    sink: &Arc<MemorySink>,
    seq: &AtomicU64,
) {
    apply_group(session, sink, seq);
    session.open_chat_search();
    session.chat_search.sender = Some(MessageSender::User { user_id: 31 });
    apply_hits(session, sink, seq, "");
}

/// Global search with the filter bar narrowed to group chats this month.
pub(super) fn apply_ready_search_filters(session: &mut Session) {
    session.search.filters = GlobalSearchFilters {
        chat_type: SearchChatType::Groups,
        media: SearchMediaKind::Files,
        date: SearchDateRange::Month,
    };
}
