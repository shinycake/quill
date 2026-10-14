//! Request bodies for the send box's media options: a GIF sent from a local
//! file with a caption (`inputMessageAnimation`), a video's cover and start
//! timestamp (`inputVideo`), and paid media (`inputMessagePaidMedia`).

use super::*;
use crate::ids::{ChatId, RequestId};
use serde_json::{Value, json};

/// Fields for a local `inputAnimation` (TDLib 1.8.68).
pub struct LocalAnimation {
    pub duration: i32,
    pub width: i32,
    pub height: i32,
}

/// `inputMessageAnimation` body for a local file, with a caption (TDLib
/// 1.8.68, `schema/td_api.tl:6425`). The thumbnail is left to TDLib.
pub fn input_message_animation_local(
    path: &str,
    animation: &LocalAnimation,
    caption: &str,
    caption_above: bool,
    strip_blockquote: bool,
) -> Value {
    json!({
        "@type": "inputMessageAnimation",
        "animation": {
            "@type": "inputAnimation",
            "animation": { "@type": "inputFileLocal", "path": path },
            "thumbnail": Value::Null,
            "added_sticker_file_ids": [],
            "duration": animation.duration,
            "width": animation.width,
            "height": animation.height
        },
        "caption": formatted_caption(caption, strip_blockquote),
        "show_caption_above_media": caption_above,
        "has_spoiler": false
    })
}

/// `sendMessage` + a local `inputMessageAnimation`.
#[allow(clippy::too_many_arguments)] // mirrors `send_video`
pub fn send_local_animation(
    extra: RequestId,
    chat_id: ChatId,
    topic_id: Option<i32>,
    path: &str,
    animation: &LocalAnimation,
    caption: &str,
    caption_above: bool,
    reply_to: Option<SendReply>,
    strip_blockquote: bool,
) -> String {
    send_content(
        extra,
        chat_id,
        topic_id,
        reply_to,
        input_message_animation_local(path, animation, caption, caption_above, strip_blockquote),
    )
}

/// Put the cover picked with "Edit Cover" and the cover frame chosen in the
/// video editor on an `inputMessageVideo` (`inputVideo.cover`,
/// `inputVideo.start_timestamp`). A zero timestamp and no cover leave the
/// content as it was.
pub fn with_video_options(mut content: Value, cover: Option<&str>, start_timestamp: i32) -> Value {
    if let Some(cover) = cover {
        crate::json_access::set_path(
            &mut content,
            &["video", "cover"],
            json!({ "@type": "inputFileLocal", "path": cover }),
        );
    }
    if start_timestamp > 0 {
        crate::json_access::set_path(
            &mut content,
            &["video", "start_timestamp"],
            json!(start_timestamp),
        );
    }
    content
}

/// One photo or video of a paid media message.
pub enum PaidMediaItem<'a> {
    Photo {
        path: &'a str,
    },
    Video {
        path: &'a str,
        duration: i32,
        width: i32,
        height: i32,
        supports_streaming: bool,
        cover: Option<&'a str>,
        start_timestamp: i32,
    },
}

fn input_paid_media(item: &PaidMediaItem<'_>) -> Value {
    match item {
        PaidMediaItem::Photo { path } => json!({
            "@type": "inputPaidMedia",
            "type": { "@type": "inputPaidMediaTypePhoto", "video": Value::Null },
            "media": { "@type": "inputFileLocal", "path": path },
            "thumbnail": Value::Null,
            "added_sticker_file_ids": [],
            "width": 0,
            "height": 0
        }),
        PaidMediaItem::Video {
            path,
            duration,
            width,
            height,
            supports_streaming,
            cover,
            start_timestamp,
        } => json!({
            "@type": "inputPaidMedia",
            "type": {
                "@type": "inputPaidMediaTypeVideo",
                "cover": cover.map_or(Value::Null, |cover| json!({
                    "@type": "inputFileLocal",
                    "path": cover
                })),
                "start_timestamp": start_timestamp,
                "duration": duration,
                "supports_streaming": supports_streaming
            },
            "media": { "@type": "inputFileLocal", "path": path },
            "thumbnail": Value::Null,
            "added_sticker_file_ids": [],
            "width": width,
            "height": height
        }),
    }
}

/// `inputMessagePaidMedia` (TDLib 1.8.68, `schema/td_api.tl:6443`): 1 to 10
/// photos and videos behind a price in Stars. Only for channels that allow
/// paid media; `payload` is for bots and stays empty.
pub fn input_message_paid_media(
    star_count: i64,
    items: &[PaidMediaItem<'_>],
    caption: &str,
    caption_above: bool,
    strip_blockquote: bool,
) -> Value {
    json!({
        "@type": "inputMessagePaidMedia",
        "star_count": star_count,
        "paid_media": items.iter().map(input_paid_media).collect::<Vec<_>>(),
        "caption": formatted_caption(caption, strip_blockquote),
        "show_caption_above_media": caption_above,
        "payload": ""
    })
}

/// `sendMessage` + `inputMessagePaidMedia`.
pub fn send_paid_media(
    extra: RequestId,
    chat_id: ChatId,
    topic_id: Option<i32>,
    reply_to: Option<SendReply>,
    content: Value,
) -> String {
    send_content(extra, chat_id, topic_id, reply_to, content)
}

fn send_content(
    extra: RequestId,
    chat_id: ChatId,
    topic_id: Option<i32>,
    reply_to: Option<SendReply>,
    content: Value,
) -> String {
    json!({
        "@type": "sendMessage",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "topic_id": message_topic_value(topic_id),
        "reply_to": send_reply_value(reply_to.as_ref()),
        "options": Value::Null,
        "reply_markup": Value::Null,
        "input_message_content": content
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_gif_goes_out_as_an_animation_with_its_caption() {
        let json = send_local_animation(
            RequestId(5),
            ChatId(-100),
            None,
            "/tmp/fun.gif",
            &LocalAnimation {
                duration: 3,
                width: 320,
                height: 240,
            },
            "look",
            true,
            None,
            false,
        );
        let v: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "sendMessage");
        let content = &v["input_message_content"];
        assert_eq!(content["@type"], "inputMessageAnimation");
        assert_eq!(content["animation"]["animation"]["@type"], "inputFileLocal");
        assert_eq!(content["animation"]["animation"]["path"], "/tmp/fun.gif");
        assert_eq!(content["animation"]["duration"], 3);
        assert_eq!(content["animation"]["width"], 320);
        assert_eq!(content["caption"]["text"], "look");
        assert_eq!(content["show_caption_above_media"], true);
        assert_eq!(content["has_spoiler"], false);
    }

    #[test]
    fn video_options_set_the_cover_and_start_only_when_chosen() {
        let base = input_message_video(
            "/tmp/v.mp4",
            &VideoSend {
                duration: 9,
                width: 640,
                height: 360,
                supports_streaming: true,
                self_destruct: None,
            },
            "",
            false,
            false,
        );
        let untouched = with_video_options(base.clone(), None, 0);
        assert_eq!(untouched, base);
        let both = with_video_options(base, Some("/tmp/c.jpg"), 4);
        assert_eq!(both["video"]["cover"]["@type"], "inputFileLocal");
        assert_eq!(both["video"]["cover"]["path"], "/tmp/c.jpg");
        assert_eq!(both["video"]["start_timestamp"], 4);
    }

    #[test]
    fn paid_media_lists_each_item_with_its_type() {
        let content = input_message_paid_media(
            250,
            &[
                PaidMediaItem::Photo { path: "/tmp/a.jpg" },
                PaidMediaItem::Video {
                    path: "/tmp/b.mp4",
                    duration: 12,
                    width: 1280,
                    height: 720,
                    supports_streaming: true,
                    cover: Some("/tmp/c.jpg"),
                    start_timestamp: 3,
                },
            ],
            "caption",
            true,
            false,
        );
        assert_eq!(content["@type"], "inputMessagePaidMedia");
        assert_eq!(content["star_count"], 250);
        assert_eq!(content["caption"]["text"], "caption");
        assert_eq!(content["show_caption_above_media"], true);
        let items = content["paid_media"].as_array().unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0]["type"]["@type"], "inputPaidMediaTypePhoto");
        assert_eq!(items[0]["type"]["video"], Value::Null);
        assert_eq!(items[0]["media"]["path"], "/tmp/a.jpg");
        assert_eq!(items[1]["type"]["@type"], "inputPaidMediaTypeVideo");
        assert_eq!(items[1]["type"]["cover"]["path"], "/tmp/c.jpg");
        assert_eq!(items[1]["type"]["start_timestamp"], 3);
        assert_eq!(items[1]["type"]["duration"], 12);
        assert_eq!(items[1]["width"], 1280);
        let json = send_paid_media(RequestId(2), ChatId(-100), Some(7), None, content);
        let v: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "sendMessage");
        assert_eq!(v["chat_id"], -100);
        assert_eq!(v["input_message_content"]["@type"], "inputMessagePaidMedia");
    }

    #[test]
    fn a_video_without_a_cover_sends_a_null_one() {
        let content = input_message_paid_media(
            1,
            &[PaidMediaItem::Video {
                path: "/tmp/b.mp4",
                duration: 1,
                width: 2,
                height: 3,
                supports_streaming: false,
                cover: None,
                start_timestamp: 0,
            }],
            "",
            false,
            false,
        );
        assert_eq!(content["paid_media"][0]["type"]["cover"], Value::Null);
        assert_eq!(content["caption"]["text"], "");
    }
}
