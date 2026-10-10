//! `ready-render-followups` screenshot demo: a private chat with Dana Cole
//! showing the follow-ups of the render slice. `QUILL_DEMO_FOLLOWUPS_VIEW`
//! picks the scene: `media` (default) has two slot machines, a running live
//! location, an expired photo and a video with timestamp links; `replies`
//! has reply strips with a picture, a quote, the replied sender's emoji
//! pattern, another chat's name and a story. Injected through the normal
//! reducer; no live Telegram.

use super::demo::{demo_file_json, demo_media_allowlist, demo_thumb_png_path};
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::{ChatId, FileId};
use quill::state::Session;
use quill::telegram::client::copy_and_parse;
use quill::telegram::envelope::{StickerFormat, StickerItem};
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

const ME: i64 = 9;
const DANA: i64 = 1;
const OTHER_CHAT: i64 = 2;
const CHAT: i64 = DANA;
/// The custom emoji Dana's replies are patterned with.
const PATTERN_EMOJI: i64 = 4001;
const PATTERN_FILE: i32 = 7001;

fn fixture(name: &str) -> String {
    demo_media_allowlist()
        .join(name)
        .to_string_lossy()
        .into_owned()
}

fn text(text: &str) -> String {
    format!(
        r#"{{"@type":"messageText","text":{{"@type":"formattedText","text":{},"entities":[]}}}}"#,
        serde_json::to_string(text).unwrap_or_default()
    )
}

/// A 🎰 whose five layers are the fixture pictures `layers` (background,
/// left, center, right reel, lever) with file ids from `first_id`.
fn slot_machine(value: i32, first_id: i32, layers: [&str; 5]) -> String {
    let keys = [
        "background",
        "left_reel",
        "center_reel",
        "right_reel",
        "lever",
    ];
    let parts: Vec<String> = keys
        .iter()
        .zip(layers)
        .zip(first_id..)
        .map(|((key, name), id)| {
            let file = demo_file_json(id, &fixture(name), true);
            format!(
                r#""{key}":{{"@type":"sticker","id":{id},"set_id":3,"width":256,"height":256,"emoji":"🎰","format":{{"@type":"stickerFormatWebp"}},"sticker":{file}}}"#
            )
        })
        .collect();
    format!(
        r#"{{"@type":"messageDice","initial_state":{{"@type":"diceStickersSlotMachine"}},"final_state":{{"@type":"diceStickersSlotMachine",{}}},"emoji":"🎰","value":{value},"success_animation_frame_number":0}}"#,
        parts.join(",")
    )
}

fn video_with_timestamps() -> String {
    let caption = "Skip to 0:45 for the best part, or 1:10 for the ending";
    let entity = |needle: &str, seconds: i32| {
        let offset = caption[..caption.find(needle).unwrap_or(0)]
            .encode_utf16()
            .count();
        format!(
            r#"{{"@type":"textEntity","offset":{offset},"length":{},"type":{{"@type":"textEntityTypeMediaTimestamp","media_timestamp":{seconds}}}}}"#,
            needle.encode_utf16().count()
        )
    };
    let clip = demo_file_json(7100, &fixture("demo-clip-12s.mp4"), true);
    let thumb = demo_file_json(7101, &demo_thumb_png_path(), true);
    format!(
        r#"{{"@type":"messageVideo","video":{{"@type":"video","duration":95,"width":640,"height":360,"file_name":"trip.mp4","mime_type":"video/mp4","has_stickers":false,"supports_streaming":true,"minithumbnail":null,"thumbnail":{{"@type":"thumbnail","format":{{"@type":"thumbnailFormatJpeg"}},"width":640,"height":360,"file":{thumb}}},"video":{clip}}},"alternative_videos":[],"storyboards":[],"cover":null,"start_timestamp":0,"caption":{{"@type":"formattedText","text":{},"entities":[{},{}]}},"show_caption_above_media":false,"has_spoiler":false,"is_secret":false}}"#,
        serde_json::to_string(caption).unwrap_or_default(),
        entity("0:45", 45),
        entity("1:10", 70),
    )
}

/// `(id, sender user, is_outgoing, content json, reply_to json)` per message.
type Row = (i64, i64, bool, String, Option<String>);

fn media_script() -> Vec<Row> {
    let live = |expires_in: i32| {
        format!(
            r#"{{"@type":"messageLiveLocation","location":{{"@type":"liveLocation","location":{{"@type":"location","latitude":48.8566,"longitude":2.3522,"horizontal_accuracy":0}},"live_period":900,"heading":0,"proximity_alert_radius":0}},"expires_in":{expires_in}}}"#
        )
    };
    vec![
        (
            4001,
            DANA,
            false,
            slot_machine(
                64,
                7200,
                [
                    "slot-bg.png",
                    "slot-seven-l.png",
                    "slot-seven-c.png",
                    "slot-seven-r.png",
                    "slot-lever.png",
                ],
            ),
            None,
        ),
        (
            4002,
            ME,
            true,
            slot_machine(
                7,
                7210,
                [
                    "slot-bg.png",
                    "slot-lemon-l.png",
                    "slot-grapes-c.png",
                    "slot-bar-r.png",
                    "slot-lever.png",
                ],
            ),
            None,
        ),
        (4003, DANA, false, live(540), None),
        (
            4004,
            DANA,
            false,
            r#"{"@type":"messageExpiredPhoto"}"#.to_string(),
            None,
        ),
        (4005, DANA, false, video_with_timestamps(), None),
    ]
}

fn replies_script() -> Vec<Row> {
    let photo_file = demo_file_json(7300, &demo_thumb_png_path(), true);
    let photo = format!(
        r#"{{"@type":"messagePhoto","photo":{{"@type":"photo","has_stickers":false,"sizes":[{{"@type":"photoSize","type":"m","photo":{photo_file},"width":320,"height":240,"progressive_sizes":[]}}]}},"caption":{{"@type":"formattedText","text":"Trip photo","entities":[]}},"show_caption_above_media":false,"has_spoiler":false,"is_secret":false}}"#
    );
    let reply = |id: i64| {
        format!(
            r#"{{"@type":"messageReplyToMessage","chat_id":0,"message_id":{id},"quote":null,"origin":null,"origin_send_date":0,"content":null}}"#
        )
    };
    let quote = |id: i64, quoted: &str| {
        format!(
            r#"{{"@type":"messageReplyToMessage","chat_id":0,"message_id":{id},"quote":{{"@type":"textQuote","text":{{"@type":"formattedText","text":{},"entities":[]}},"position":0,"is_manual":true}},"origin":null,"origin_send_date":0,"content":null}}"#,
            serde_json::to_string(quoted).unwrap_or_default()
        )
    };
    let other_chat = format!(
        r#"{{"@type":"messageReplyToMessage","chat_id":{OTHER_CHAT},"message_id":55,"quote":null,"origin":{{"@type":"messageOriginUser","sender_user_id":{DANA}}},"origin_send_date":1790000000,"content":{}}}"#,
        text("Meet at 6 by the station")
    );
    let story =
        format!(r#"{{"@type":"messageReplyToStory","story_poster_chat_id":{DANA},"story_id":7}}"#);
    vec![
        (5001, DANA, false, photo, None),
        (
            5002,
            ME,
            true,
            text("Great shot, where was this?"),
            Some(reply(5001)),
        ),
        (
            5003,
            DANA,
            false,
            text("Which route did you take in the end? The coast road or the pass?"),
            None,
        ),
        (
            5004,
            ME,
            true,
            text("The coast road, much better light"),
            Some(reply(5003)),
        ),
        (
            5005,
            ME,
            true,
            text("Taking the pass next time"),
            Some(quote(5003, "the coast road or the pass")),
        ),
        (
            5006,
            DANA,
            false,
            text("Pinning the plan for Saturday"),
            Some(other_chat),
        ),
        (5007, ME, true, text("Love this one"), Some(story)),
    ]
}

pub(super) fn apply_ready_render_followups(
    session: &mut Session,
    sink: &Arc<MemorySink>,
    seq: &AtomicU64,
    view: &str,
) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let now = quill::local_time::now_unix() - 1200;
    let mut jsons: Vec<String> = vec![
        format!(
            r#"{{"@type":"updateOption","name":"my_id","value":{{"@type":"optionValueInteger","value":"{ME}"}}}}"#
        ),
        format!(
            r#"{{"@type":"updateNewChat","chat":{{"id":{CHAT},"title":"Dana Cole","type":{{"@type":"chatTypePrivate","user_id":{DANA}}},"unread_count":0,"accent_color_id":5,"background_custom_emoji_id":"{PATTERN_EMOJI}"}}}}"#
        ),
        format!(
            r#"{{"@type":"updateChatPosition","chat_id":{CHAT},"position":{{"@type":"chatPosition","list":{{"@type":"chatListMain"}},"order":"4999","is_pinned":false}}}}"#
        ),
        format!(
            r#"{{"@type":"updateNewChat","chat":{{"id":{OTHER_CHAT},"title":"Weekend plans","type":{{"@type":"chatTypeSupergroup","supergroup_id":{OTHER_CHAT},"is_channel":false}},"unread_count":0}}}}"#
        ),
        format!(
            r#"{{"@type":"updateChatPosition","chat_id":{OTHER_CHAT},"position":{{"@type":"chatPosition","list":{{"@type":"chatListMain"}},"order":"4998","is_pinned":false}}}}"#
        ),
        demo_file_json(PATTERN_FILE, &fixture("emoji-heart.png"), true),
        format!(
            r#"{{"@type":"updateUser","user":{{"@type":"user","id":{ME},"first_name":"Idan","last_name":"Birman","usernames":null,"phone_number":"","status":{{"@type":"userStatusRecently"}},"profile_photo":null,"is_contact":true,"type":{{"@type":"userTypeRegular"}}}}}}"#
        ),
        format!(
            r#"{{"@type":"updateUser","user":{{"@type":"user","id":{DANA},"first_name":"Dana","last_name":"Cole","usernames":null,"phone_number":"","status":{{"@type":"userStatusRecently"}},"profile_photo":null,"accent_color_id":5,"background_custom_emoji_id":"{PATTERN_EMOJI}","is_contact":true,"type":{{"@type":"userTypeRegular"}}}}}}"#
        ),
    ];
    let script = if view == "replies" {
        // Dana posted a story; its picture fills the last strip.
        let story_file = demo_file_json(7301, &demo_thumb_png_path(), true);
        jsons.push(format!(
            r#"{{"@type":"story","id":7,"poster_chat_id":{DANA},"date":1700000000,"content":{{"@type":"storyContentPhoto","photo":{{"@type":"photo","has_stickers":false,"sizes":[{{"@type":"photoSize","type":"y","photo":{story_file},"width":960,"height":1280,"progressive_sizes":[]}}]}}}},"caption":{{"@type":"formattedText","text":"","entities":[]}}}}"#
        ));
        replies_script()
    } else {
        media_script()
    };
    for (id, sender, outgoing, content, reply_to) in script {
        let date = now + (id % 100) * 60;
        let reply_to = reply_to
            .map(|reply| format!(r#","reply_to":{reply}"#))
            .unwrap_or_default();
        jsons.push(format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":{id},"chat_id":{CHAT},"sender_id":{{"@type":"messageSenderUser","user_id":{sender}}},"is_outgoing":{outgoing},"date":{date},"content":{content}{reply_to}}}}}"#
        ));
    }
    for json in jsons {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
    // The emoji Dana's strips are patterned with: a heart, resolved
    // already so the capture does not wait for a request.
    session.emoji.custom_emoji_stickers.push(StickerItem {
        custom_emoji_id: Some(PATTERN_EMOJI),
        id: PATTERN_EMOJI,
        set_id: 0,
        emoji: "\u{2764}".to_string(),
        width: 128,
        height: 128,
        format: StickerFormat::Webp,
        file_id: FileId(PATTERN_FILE),
        thumb_file_id: None,
        thumb_width: 0,
        thumb_height: 0,
        requires_premium: false,
    });
    session.open_chat(ChatId(CHAT));
}

#[cfg(test)]
mod tests {
    use super::{media_script, replies_script};

    #[test]
    fn scripts_have_unique_ascending_ids() {
        for script in [media_script(), replies_script()] {
            let ids: Vec<i64> = script.iter().map(|row| row.0).collect();
            let mut sorted = ids.clone();
            sorted.sort_unstable();
            sorted.dedup();
            assert_eq!(ids, sorted);
        }
    }
}
