//! `ready-service-media` screenshot demo: a private chat with Dana whose
//! history shows the service and media cards added with this slice: a
//! suggested photo and birthday, expired media, live locations and link
//! previews with a "View channel / bot / message" call to action. Injected
//! through the normal reducer; no live Telegram.

use super::demo::{demo_file_json, demo_thumb_png_path};
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::ChatId;
use quill::state::Session;
use quill::telegram::client::copy_and_parse;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

const ME: i64 = 9;
const DANA: i64 = 1;
const CHAT: i64 = DANA;

fn text_with_preview(text: &str, preview_type: &str, title: &str, site: &str, url: &str) -> String {
    let text = serde_json::to_string(text).unwrap_or_default();
    let title = serde_json::to_string(title).unwrap_or_default();
    let site = serde_json::to_string(site).unwrap_or_default();
    format!(
        r#"{{"@type":"messageText","text":{{"@type":"formattedText","text":{text},"entities":[]}},"link_preview":{{"@type":"linkPreview","url":"{url}","display_url":"{url}","site_name":{site},"title":{title},"description":{{"@type":"formattedText","text":"","entities":[]}},"type":{preview_type},"has_large_media":false,"show_large_media":false,"show_media_above_description":false,"show_above_text":false,"instant_view_version":0}}}}"#
    )
}

/// `(sender user, is_outgoing, content json)` per message, oldest first.
fn script() -> Vec<(i64, bool, String)> {
    let photo_file = demo_file_json(90, &demo_thumb_png_path(), true);
    let photo = format!(
        r#"{{"@type":"chatPhoto","id":"5","added_date":1,"minithumbnail":null,"sizes":[{{"@type":"photoSize","type":"a","photo":{photo_file},"width":160,"height":160,"progressive_sizes":[]}}]}}"#
    );
    let live = |expires_in: i32| {
        format!(
            r#"{{"@type":"messageLiveLocation","location":{{"@type":"liveLocation","location":{{"@type":"location","latitude":48.8566,"longitude":2.3522,"horizontal_accuracy":0}},"live_period":900,"heading":0,"proximity_alert_radius":0}},"expires_in":{expires_in}}}"#
        )
    };
    vec![
        (
            DANA,
            false,
            format!(r#"{{"@type":"messageSuggestProfilePhoto","photo":{photo}}}"#),
        ),
        (
            DANA,
            false,
            r#"{"@type":"messageSuggestBirthdate","birthdate":{"@type":"birthdate","day":6,"month":10,"year":1990}}"#
                .to_string(),
        ),
        (DANA, false, r#"{"@type":"messageExpiredPhoto"}"#.to_string()),
        (
            DANA,
            false,
            r#"{"@type":"messageExpiredVoiceNote"}"#.to_string(),
        ),
        (ME, true, live(600)),
        (DANA, false, live(1500)),
        (
            DANA,
            false,
            text_with_preview(
                "Our announcements live here",
                r#"{"@type":"linkPreviewTypeChat","type":{"@type":"inviteLinkChatTypeChannel"},"photo":null,"creates_join_request":false}"#,
                "Launch Channel",
                "Telegram",
                "https://t.me/launch",
            ),
        ),
        (
            DANA,
            false,
            text_with_preview(
                "Try the helper bot",
                r#"{"@type":"linkPreviewTypeUser","photo":null,"is_bot":true}"#,
                "Helper Bot",
                "Telegram",
                "https://t.me/helper_bot",
            ),
        ),
        (
            ME,
            true,
            text_with_preview(
                "The thread I mentioned",
                r#"{"@type":"linkPreviewTypeMessage"}"#,
                "Design Club",
                "Telegram",
                "https://t.me/c/1/42",
            ),
        ),
    ]
}

pub(super) fn apply_ready_service_media(
    session: &mut Session,
    sink: &Arc<MemorySink>,
    seq: &AtomicU64,
) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let now = quill::local_time::now_unix() - 1200;
    let mut jsons: Vec<String> = vec![
        format!(
            r#"{{"@type":"updateOption","name":"my_id","value":{{"@type":"optionValueInteger","value":"{ME}"}}}}"#
        ),
        format!(
            r#"{{"@type":"updateNewChat","chat":{{"id":{CHAT},"title":"Dana Cole","type":{{"@type":"chatTypePrivate","user_id":{DANA}}},"unread_count":0}}}}"#
        ),
        format!(
            r#"{{"@type":"updateChatPosition","chat_id":{CHAT},"position":{{"@type":"chatPosition","list":{{"@type":"chatListMain"}},"order":"4999","is_pinned":false}}}}"#
        ),
    ];
    for (id, first, last) in [(ME, "Idan", "Birman"), (DANA, "Dana", "Cole")] {
        jsons.push(format!(
            r#"{{"@type":"updateUser","user":{{"@type":"user","id":{id},"first_name":"{first}","last_name":"{last}","usernames":null,"phone_number":"","status":{{"@type":"userStatusRecently"}},"profile_photo":null,"is_contact":true,"type":{{"@type":"userTypeRegular"}}}}}}"#
        ));
    }
    for (index, (sender, outgoing, content)) in script().into_iter().enumerate() {
        let id = 3001 + index as i64;
        let date = now + index as i64 * 60;
        jsons.push(format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":{id},"chat_id":{CHAT},"sender_id":{{"@type":"messageSenderUser","user_id":{sender}}},"is_outgoing":{outgoing},"date":{date},"content":{content}}}}}"#
        ));
    }
    for json in jsons {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
    session.open_chat(ChatId(CHAT));
}
