//! `ready-bubble-headers` screenshot demo (codex:bubble-headers): every
//! reply, forward and footer variant in one group chat, injected through the
//! normal reducer (recorded TDLib JSON shapes, no live Telegram).

use super::demo::{demo_file_json, demo_thumb_png_path};
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::{ChatId, MessageId};
use quill::state::{RequestPurpose, Session};
use quill::telegram::client::copy_and_parse;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

const GROUP: i64 = -2001;
const CHANNEL: i64 = -2002;
const PRIVATE: i64 = 51;

fn user(id: i64, name: &str, accent: i32, username: &str) -> String {
    let usernames = if username.is_empty() {
        "null".to_string()
    } else {
        format!(
            r#"{{"@type":"usernames","active_usernames":["{username}"],"disabled_usernames":[],"editable_username":"{username}"}}"#
        )
    };
    let kind = if username.is_empty() {
        r#"{"@type":"userTypeRegular"}"#
    } else {
        r#"{"@type":"userTypeBot","is_inline":true}"#
    };
    format!(
        r#"{{"@type":"updateUser","user":{{"@type":"user","id":{id},"first_name":"{name}","last_name":"","usernames":{usernames},"phone_number":"","status":{{"@type":"userStatusRecently"}},"profile_photo":null,"accent_color_id":{accent},"is_contact":true,"type":{kind}}}}}"#
    )
}

fn text_content(text: &str) -> String {
    let text = serde_json::to_string(text).unwrap_or_default();
    format!(
        r#"{{"@type":"messageText","text":{{"@type":"formattedText","text":{text},"entities":[]}}}}"#
    )
}

fn photo_content(caption: &str) -> String {
    let photo = demo_file_json(48, &demo_thumb_png_path(), true);
    let caption = serde_json::to_string(caption).unwrap_or_default();
    format!(
        r#"{{"@type":"messagePhoto","photo":{{"@type":"photo","has_stickers":false,"minithumbnail":null,"sizes":[{{"@type":"photoSize","type":"m","photo":{photo},"width":320,"height":240,"progressive_sizes":[]}}]}},"caption":{{"@type":"formattedText","text":{caption},"entities":[]}},"show_caption_above_media":false,"has_spoiler":false,"is_secret":false}}"#
    )
}

fn reply(chat: i64, message: i64, quote: Option<&str>) -> String {
    let quote = match quote {
        Some(text) => format!(
            r#"{{"@type":"textQuote","text":{{"@type":"formattedText","text":"{text}","entities":[]}},"position":0,"is_manual":true}}"#
        ),
        None => "null".to_string(),
    };
    format!(
        r#","reply_to":{{"@type":"messageReplyToMessage","chat_id":{chat},"message_id":{message},"quote":{quote},"checklist_task_id":0,"poll_option_id":"","origin":null,"origin_send_date":0,"content":null}}"#
    )
}

fn forwarded(origin: &str) -> String {
    format!(
        r#","forward_info":{{"@type":"messageForwardInfo","origin":{origin},"date":1789000000,"source":null,"public_service_announcement_type":""}}"#
    )
}

#[allow(clippy::too_many_arguments)]
fn message(
    id: i64,
    sender: i64,
    outgoing: bool,
    date: i64,
    content: &str,
    extra: &str,
) -> String {
    format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":{id},"chat_id":{GROUP},"sender_id":{{"@type":"messageSenderUser","user_id":{sender}}},"is_outgoing":{outgoing},"date":{date},"content":{content}{extra}}}}}"#
    )
}

pub(super) fn apply_ready_bubble_headers(
    session: &mut Session,
    sink: &Arc<MemorySink>,
    seq: &AtomicU64,
) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let now = quill::local_time::now_unix() - 3600;
    let apply = |session: &mut Session, json: String| {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    };
    apply(session, user(51, "Maya Cohen", 0, ""));
    apply(session, user(52, "Ben Ito", 5, ""));
    apply(session, user(53, "Dana Weiss", 2, ""));
    apply(session, user(54, "GIF Bot", 1, "gifbot"));
    for (id, title, kind) in [
        (
            GROUP,
            "Trail Crew",
            r#"{"@type":"chatTypeBasicGroup","basic_group_id":2001}"#,
        ),
        (
            CHANNEL,
            "Trail News",
            r#"{"@type":"chatTypeSupergroup","supergroup_id":2002,"is_channel":true}"#,
        ),
        (
            PRIVATE,
            "Maya Cohen",
            r#"{"@type":"chatTypePrivate","user_id":51}"#,
        ),
    ] {
        apply(session, format!(
            r#"{{"@type":"updateNewChat","chat":{{"id":{id},"title":"{title}","type":{kind},"unread_count":0}}}}"#
        ));
        apply(session, format!(
            r#"{{"@type":"updateChatPosition","chat_id":{id},"position":{{"@type":"chatPosition","list":{{"@type":"chatListMain"}},"order":"{}","is_pinned":false}}}}"#,
            3000 + id.abs() % 100
        ));
    }

    // Replies.
    apply(session, message(
        101,
        51,
        false,
        now,
        &text_content("Anyone up for the ridge trail on Saturday? Starting 7:30 at the trailhead."),
        r#","edit_date":1790633500"#,
    ));
    apply(session, message(
        102,
        52,
        false,
        now + 60,
        &text_content("I'm in, bringing the good thermos."),
        &reply(GROUP, 101, None),
    ));
    apply(session, message(
        103,
        0,
        true,
        now + 120,
        &text_content("Same. Is 7:30 sharp or flexible?"),
        &reply(GROUP, 101, Some("Starting 7:30 at the trailhead")),
    ));
    apply(session, message(
        104,
        53,
        false,
        now + 180,
        &photo_content("Sunset from the summit last week"),
        "",
    ));
    apply(session, message(
        105,
        52,
        false,
        now + 240,
        &text_content("Wow, that colour. Is it edited?"),
        &reply(GROUP, 104, None),
    ));
    // A reply to a message that was deleted.
    apply(session, message(99, 53, false, now - 60, &text_content("(deleted)"), ""));
    apply(session, format!(
        r#"{{"@type":"updateDeleteMessages","chat_id":{GROUP},"message_ids":[99],"is_permanent":true,"from_cache":false}}"#
    ));
    apply(session, message(
        106,
        51,
        false,
        now + 300,
        &text_content("Oops, scratch what I asked above."),
        &reply(GROUP, 99, None),
    ));
    // A reply to a message far outside the window, answered by
    // `getRepliedMessage`.
    apply(session, message(
        107,
        53,
        false,
        now + 330,
        &text_content("Answering the permit question from March."),
        &reply(GROUP, 12, None),
    ));
    // A reply from another chat: TDLib sends origin and media.
    apply(session, message(
        108,
        52,
        false,
        now + 360,
        &text_content("Forwarding Maya's route note into this chat."),
        &format!(
            r#","reply_to":{{"@type":"messageReplyToMessage","chat_id":{PRIVATE},"message_id":9,"quote":null,"checklist_task_id":0,"poll_option_id":"","origin":{{"@type":"messageOriginUser","sender_user_id":51}},"origin_send_date":1790000000,"content":null}}"#
        ),
    ));

    // Forwards.
    apply(session, message(
        109,
        52,
        false,
        now + 420,
        &text_content("Trail conditions look good."),
        &forwarded(r#"{"@type":"messageOriginUser","sender_user_id":53}"#),
    ));
    apply(session, message(
        110,
        52,
        false,
        now + 450,
        &text_content("Hidden accounts cannot be opened."),
        &forwarded(r#"{"@type":"messageOriginHiddenUser","sender_name":"Grace Hopper"}"#),
    ));
    apply(session, message(
        111,
        53,
        false,
        now + 480,
        &text_content("Trail closed above 2,000 m until further notice."),
        &format!(
            r#"{},"interaction_info":{{"@type":"messageInteractionInfo","view_count":12400,"forward_count":8,"reply_info":null,"reactions":null}},"is_pinned":true"#,
            forwarded(&format!(
                r#"{{"@type":"messageOriginChannel","chat_id":{CHANNEL},"message_id":77,"author_signature":"Ana"}}"#
            ))
        ),
    ));
    apply(session, message(
        112,
        51,
        false,
        now + 540,
        &text_content("Welcome back to the group, everyone."),
        r#","import_info":{"@type":"messageImportInfo","sender_name":"Noa","date":1600000000}"#,
    ));
    apply(session, message(
        113,
        51,
        false,
        now + 600,
        &text_content("Summit selfie coming up."),
        r#","via_bot_user_id":54"#,
    ));
    apply(session, message(
        114,
        0,
        true,
        now + 660,
        &text_content("Forwarded and replied: see you at the trailhead."),
        &format!(
            "{}{}",
            forwarded(r#"{"@type":"messageOriginUser","sender_user_id":51}"#),
            reply(GROUP, 102, None)
        ),
    ));

    // Answers to the fetches the driver would send.
    let fetched = [
        (
            107_i64,
            GROUP,
            r#","id":12,"sender_id":{"@type":"messageSenderUser","user_id":52}"#,
            text_content("Do we need a permit for the ridge trail?"),
        ),
        (
            108,
            PRIVATE,
            r#","id":9,"sender_id":{"@type":"messageSenderUser","user_id":51}"#,
            photo_content("Route map v2"),
        ),
    ];
    for (replying, chat, head, content) in fetched {
        let extra = session.request(
            RequestPurpose::GetRepliedMessage {
                chat_id: ChatId(GROUP),
                message_id: MessageId(replying),
            },
            Some(ChatId(GROUP)),
        );
        apply(session, format!(
            r#"{{"@type":"message","@extra":"{}"{head},"chat_id":{chat},"is_outgoing":false,"date":{},"content":{content}}}"#,
            extra.0,
            now - 200_000
        ));
    }
    session.open_chat(ChatId(GROUP));
}
