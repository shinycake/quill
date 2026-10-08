//! `ready-rtl-polish` screenshot demo (codex:rtl-polish): Hebrew and mixed
//! chat-list previews, search results, short and multi-line Hebrew bubbles
//! with their time footers, a Hebrew reply and a Hebrew pinned message.
//! `QUILL_DEMO_RTL_VIEW=chat|search` (default `chat`) picks the view; both
//! are injected through the normal reducer, no live Telegram.

use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::state::{RequestPurpose, Session};
use quill::telegram::client::copy_and_parse;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

/// The Hebrew chats: (chat id, title, last message text, outgoing, unread, order).
const CHATS: [(i64, &str, &str, bool, i32, i64); 5] = [
    (
        31,
        "משפחה כהן",
        "שלום לכולם, מה שלומכם היום? רציתי לשאול אם מישהו יכול להגיע מחר בערב לארוחה אצלנו בבית",
        false,
        2,
        2050,
    ),
    (32, "דנה לוי", "תודה רבה!", false, 0, 2040),
    (
        33,
        "Mira Cohen",
        "Hello שלום, מה קורה היום אצלך?",
        false,
        0,
        2030,
    ),
    (
        34,
        "נועם כץ",
        "היי, ההזמנה 12345 מוכנה ב-Telegram Desktop",
        true,
        0,
        2020,
    ),
    (
        35,
        "Omar Haddad",
        "See you at 6 — תביא את המחברת",
        false,
        0,
        2010,
    ),
];

/// Messages of the open chat (11): (id, outgoing, text).
const MESSAGES: [(i64, bool, &str); 8] = [
    (911, false, "תודה רבה!"),
    (912, true, "בסדר גמור"),
    (
        913,
        false,
        "שלום עולם, מה קורה?\nהכול בסדר אצלי, תודה ששאלת",
    ),
    (914, false, "היי, ההזמנה 12345 מוכנה ב-Telegram Desktop"),
    (915, false, "Meeting at six שלום"),
    (916, false, "שלום לכולם\nok"),
    (
        917,
        false,
        "שלום עולם, מה שלומך היום? זו הודעה ארוכה יותר כדי לראות את הטקסט נשבר לשורות בתוך הבועה.",
    ),
    (918, true, "בשמחה"),
];

/// Applies the fixture; `view` is `chat` or `search`.
pub(super) fn apply_ready_rtl_polish(
    session: &mut Session,
    sink: &Arc<MemorySink>,
    seq: &AtomicU64,
    view: &str,
) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let now = quill::local_time::now_unix() - 900;
    let mut jsons: Vec<String> = Vec::new();
    for (id, title, text, outgoing, unread, order) in CHATS {
        let text = serde_json::to_string(text).unwrap_or_default();
        let title_json = serde_json::to_string(title).unwrap_or_default();
        jsons.push(format!(
            r#"{{"@type":"updateUser","user":{{"@type":"user","id":{id},"first_name":{title_json},"last_name":"","usernames":null,"phone_number":"","status":{{"@type":"userStatusRecently"}},"profile_photo":null,"is_contact":true,"type":{{"@type":"userTypeRegular"}}}}}}"#
        ));
        jsons.push(format!(
            r#"{{"@type":"updateNewChat","chat":{{"id":{id},"title":{title_json},"type":{{"@type":"chatTypePrivate","user_id":{id}}},"unread_count":{unread}}}}}"#
        ));
        jsons.push(format!(
            r#"{{"@type":"updateChatLastMessage","chat_id":{id},"last_message":{{"id":{},"chat_id":{id},"date":{now},"is_outgoing":{outgoing},"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":{text},"entities":[]}}}}}},"positions":[{{"@type":"chatPosition","list":{{"@type":"chatListMain"}},"order":"{order}","is_pinned":false}}]}}"#,
            id * 1_048_576
        ));
    }
    // Group rows: the preview carries the sender ("Shahar: ...") before the Hebrew text.
    for (chat, user, name, text) in [
        (
            36_i64,
            41_i64,
            "Shahar",
            "חחחח שרמיט גדול, אין מצב שזה קרה באמת",
        ),
        (
            37,
            42,
            "בר",
            "אם הם לא מגיעים עד שמונה אנחנו מתחילים בלעדיהם",
        ),
    ] {
        let text = serde_json::to_string(text).unwrap_or_default();
        let name_json = serde_json::to_string(name).unwrap_or_default();
        let title = if chat == 36 {
            "קבוצת חברים"
        } else {
            "Work chat"
        };
        jsons.push(format!(
            r#"{{"@type":"updateUser","user":{{"@type":"user","id":{user},"first_name":{name_json},"last_name":"","usernames":null,"phone_number":"","status":{{"@type":"userStatusRecently"}},"profile_photo":null,"is_contact":true,"type":{{"@type":"userTypeRegular"}}}}}}"#
        ));
        jsons.push(format!(
            r#"{{"@type":"updateNewChat","chat":{{"id":{chat},"title":"{title}","type":{{"@type":"chatTypeBasicGroup","basic_group_id":{chat}}},"unread_count":0}}}}"#
        ));
        jsons.push(format!(
            r#"{{"@type":"updateChatLastMessage","chat_id":{chat},"last_message":{{"id":{},"chat_id":{chat},"date":{now},"is_outgoing":false,"sender_id":{{"@type":"messageSenderUser","user_id":{user}}},"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":{text},"entities":[]}}}}}},"positions":[{{"@type":"chatPosition","list":{{"@type":"chatListMain"}},"order":"{}","is_pinned":false}}]}}"#,
            chat * 1_048_576,
            2000 - chat
        ));
    }
    for (id, outgoing, text) in MESSAGES {
        let text = serde_json::to_string(text).unwrap_or_default();
        jsons.push(format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":{id},"chat_id":11,"is_outgoing":{outgoing},"date":{now},"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":{text},"entities":[]}}}}{}}}}}"#,
            if id == 918 {
                // A reply to the first Hebrew message.
                r#","reply_to":{"@type":"messageReplyToMessage","chat_id":11,"message_id":911,"quote":null,"checklist_task_id":0,"poll_option_id":""}"#
            } else {
                ""
            }
        ));
    }
    jsons.push(
        r#"{"@type":"updateMessageIsPinned","chat_id":11,"message_id":913,"is_pinned":true}"#
            .to_string(),
    );
    for json in jsons {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
    if view == "search" {
        session.open_search();
        let search_gen = session.search.begin_query("שלום");
        let chats_extra = session.request_search(RequestPurpose::SearchChats, search_gen);
        let messages_extra = session.request_search(RequestPurpose::SearchMessages, search_gen);
        let found = |id: i64, chat: i64, text: &str| {
            let text = serde_json::to_string(text).unwrap_or_default();
            format!(
                r#"{{"id":{id},"chat_id":{chat},"date":{now},"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":{text},"entities":[]}}}}}}"#
            )
        };
        let messages = [
            found(
                1,
                31,
                "שלום לכולם, מה שלומכם היום? רציתי לשאול אם מישהו יכול להגיע מחר בערב",
            ),
            found(2, 33, "Hello שלום, מה קורה"),
            found(3, 32, "שלום דנה"),
        ]
        .join(",");
        let results = [
            format!(
                r#"{{"@type":"chats","@extra":"{}","total_count":2,"chat_ids":[31,34]}}"#,
                chats_extra.0
            ),
            format!(
                r#"{{"@type":"foundMessages","@extra":"{}","total_count":3,"next_offset":"","messages":[{messages}]}}"#,
                messages_extra.0
            ),
        ];
        for json in results {
            if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
                session.apply(owned);
            }
        }
    }
}
