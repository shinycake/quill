//! The export's own view of a message, parsed from a raw TDLib `message`
//! object. The worker thread pages raw JSON (like `account_export`), so
//! nothing here depends on the UI session's parsed types.
use super::settings::MediaKind;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Who sent a message or an origin: a user, or a chat acting as sender.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Who {
    User(i64),
    Chat(i64),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EntityKind {
    Bold,
    Italic,
    Underline,
    Strike,
    Spoiler,
    Code,
    Pre,
    Quote,
    /// A bare link: the entity's own text is the address.
    Url,
    Email,
    /// A link with a separate address.
    Link(String),
}

/// A formatting span. `offset` and `length` count UTF-16 units, as TDLib
/// reports them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entity {
    pub offset: usize,
    pub length: usize,
    pub kind: EntityKind,
}

/// A file attached to a message.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MediaRef {
    pub kind: MediaKind,
    pub file_id: i32,
    pub size: i64,
    pub file_name: String,
    pub mime: String,
    pub duration: i32,
    /// Sticker emoji or audio "performer - title".
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Body {
    Text,
    Media(MediaRef),
    Location {
        latitude: f64,
        longitude: f64,
    },
    Venue {
        title: String,
        address: String,
        latitude: f64,
        longitude: f64,
    },
    Contact {
        name: String,
        phone: String,
    },
    Poll {
        question: String,
        options: Vec<(String, i32)>,
        total_voters: i32,
    },
    Dice {
        emoji: String,
        value: i32,
    },
    /// A service line. `text` may hold `{from}` (the sender) and `{users}`
    /// (the names of `users`, joined).
    Service {
        text: String,
        users: Vec<i64>,
        names: Vec<String>,
    },
    /// Content this export does not describe further.
    Other,
}

/// Where a forwarded message came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Origin {
    Name(String),
    Who(Who),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExportMessage {
    pub id: i64,
    pub date: i32,
    pub edited: i32,
    pub outgoing: bool,
    pub sender: Option<Who>,
    /// The sender's display name, filled in by the worker.
    pub from: Option<String>,
    pub reply_to: Option<i64>,
    pub forward: Option<Origin>,
    /// The forward origin's display name, filled in by the worker.
    pub forwarded_from: Option<String>,
    pub text: String,
    pub entities: Vec<Entity>,
    pub body: Body,
}

impl ExportMessage {
    pub fn is_service(&self) -> bool {
        matches!(self.body, Body::Service { .. })
    }

    /// Every user or chat whose name the output needs.
    pub fn people(&self) -> Vec<Who> {
        let mut out: Vec<Who> = self.sender.into_iter().collect();
        if let Some(Origin::Who(who)) = &self.forward {
            out.push(*who);
        }
        if let Body::Service { users, .. } = &self.body {
            out.extend(users.iter().map(|id| Who::User(*id)));
        }
        out
    }
}

fn who_of(sender: &Value) -> Option<Who> {
    match sender["@type"].as_str()? {
        "messageSenderUser" => Some(Who::User(sender["user_id"].as_i64()?)),
        "messageSenderChat" => Some(Who::Chat(sender["chat_id"].as_i64()?)),
        _ => None,
    }
}

fn origin_of(info: &Value) -> Option<Origin> {
    let origin = &info["origin"];
    match origin["@type"].as_str()? {
        "messageOriginUser" => Some(Origin::Who(Who::User(origin["sender_user_id"].as_i64()?))),
        "messageOriginHiddenUser" => Some(Origin::Name(
            origin["sender_name"].as_str().unwrap_or_default().into(),
        )),
        "messageOriginChat" => Some(Origin::Who(Who::Chat(origin["sender_chat_id"].as_i64()?))),
        "messageOriginChannel" => Some(Origin::Who(Who::Chat(origin["chat_id"].as_i64()?))),
        _ => None,
    }
}

fn entity_kind(entity: &Value) -> Option<EntityKind> {
    let kind = &entity["type"];
    Some(match kind["@type"].as_str()? {
        "textEntityTypeBold" => EntityKind::Bold,
        "textEntityTypeItalic" => EntityKind::Italic,
        "textEntityTypeUnderline" => EntityKind::Underline,
        "textEntityTypeStrikethrough" => EntityKind::Strike,
        "textEntityTypeSpoiler" => EntityKind::Spoiler,
        "textEntityTypeCode" => EntityKind::Code,
        "textEntityTypePre" | "textEntityTypePreCode" => EntityKind::Pre,
        "textEntityTypeBlockQuote" | "textEntityTypeExpandableBlockQuote" => EntityKind::Quote,
        "textEntityTypeUrl" => EntityKind::Url,
        "textEntityTypeEmailAddress" => EntityKind::Email,
        "textEntityTypeTextUrl" => EntityKind::Link(kind["url"].as_str()?.to_string()),
        _ => return None,
    })
}

/// (text, entities) of a TDLib `formattedText`.
fn formatted(value: &Value) -> (String, Vec<Entity>) {
    let text = value["text"].as_str().unwrap_or_default().to_string();
    let entities = value["entities"]
        .as_array()
        .map(|list| {
            list.iter()
                .filter_map(|e| {
                    Some(Entity {
                        offset: usize::try_from(e["offset"].as_i64()?).ok()?,
                        length: usize::try_from(e["length"].as_i64()?).ok()?,
                        kind: entity_kind(e)?,
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    (text, entities)
}

/// (id, size) of a TDLib `file`. The size falls back to the expected one
/// while the real size is still unknown.
fn file_of(file: &Value) -> (i32, i64) {
    let id = file["id"]
        .as_i64()
        .and_then(|id| i32::try_from(id).ok())
        .unwrap_or(0);
    let size = file["size"]
        .as_i64()
        .filter(|s| *s > 0)
        .or_else(|| file["expected_size"].as_i64())
        .unwrap_or(0);
    (id, size)
}

fn media(kind: MediaKind, holder: &Value, file_key: &str) -> Body {
    let (file_id, size) = file_of(&holder[file_key]);
    let text = |key: &str| holder[key].as_str().unwrap_or_default().to_string();
    let label = if holder["performer"].is_string() || holder["title"].is_string() {
        [text("performer"), text("title")]
            .iter()
            .filter(|s| !s.is_empty())
            .cloned()
            .collect::<Vec<_>>()
            .join(" - ")
    } else {
        text("emoji")
    };
    Body::Media(MediaRef {
        kind,
        file_id,
        size,
        file_name: text("file_name"),
        mime: text("mime_type"),
        duration: holder["duration"]
            .as_i64()
            .and_then(|d| i32::try_from(d).ok())
            .unwrap_or(0),
        label,
    })
}

fn largest_photo(photo: &Value) -> Body {
    let best = photo["sizes"].as_array().and_then(|sizes| {
        sizes
            .iter()
            .max_by_key(|s| s["width"].as_i64().unwrap_or(0) * s["height"].as_i64().unwrap_or(0))
    });
    let (file_id, size) = best.map_or((0, 0), |s| file_of(&s["photo"]));
    Body::Media(MediaRef {
        kind: MediaKind::Photo,
        file_id,
        size,
        file_name: String::new(),
        mime: "image/jpeg".into(),
        duration: 0,
        label: String::new(),
    })
}

fn service(text: &str, users: Vec<i64>) -> Body {
    Body::Service {
        text: text.into(),
        users,
        names: Vec::new(),
    }
}

fn user_ids(value: &Value) -> Vec<i64> {
    value
        .as_array()
        .map(|list| list.iter().filter_map(Value::as_i64).collect())
        .unwrap_or_default()
}

fn call_line(content: &Value) -> String {
    let kind = if content["is_video"] == true {
        "Video call"
    } else {
        "Call"
    };
    let missed = matches!(
        content["discard_reason"]["@type"].as_str(),
        Some("callDiscardReasonMissed" | "callDiscardReasonDeclined")
    );
    let duration = content["duration"].as_i64().unwrap_or(0);
    if missed {
        format!("{kind} (missed)")
    } else if duration > 0 {
        format!("{kind} ({duration} sec)")
    } else {
        kind.to_string()
    }
}

/// The body, text and entities of a message's `content`.
fn content(content: &Value) -> (Body, String, Vec<Entity>) {
    let caption = |value: &Value| formatted(&value["caption"]);
    let with_caption = |body: Body| {
        let (text, entities) = caption(content);
        (body, text, entities)
    };
    match content["@type"].as_str().unwrap_or_default() {
        "messageText" => {
            let (text, entities) = formatted(&content["text"]);
            (Body::Text, text, entities)
        }
        "messagePhoto" => with_caption(largest_photo(&content["photo"])),
        "messageVideo" => with_caption(media(MediaKind::Video, &content["video"], "video")),
        "messageAnimation" => {
            with_caption(media(MediaKind::Gif, &content["animation"], "animation"))
        }
        "messageVoiceNote" => with_caption(media(
            MediaKind::VoiceMessage,
            &content["voice_note"],
            "voice",
        )),
        "messageVideoNote" => {
            let body = media(MediaKind::VideoMessage, &content["video_note"], "video");
            (body, String::new(), Vec::new())
        }
        "messageSticker" => {
            let body = media(MediaKind::Sticker, &content["sticker"], "sticker");
            (body, String::new(), Vec::new())
        }
        "messageDocument" => with_caption(media(MediaKind::File, &content["document"], "document")),
        "messageAudio" => with_caption(media(MediaKind::File, &content["audio"], "audio")),
        "messageLocation" => (
            Body::Location {
                latitude: content["location"]["latitude"].as_f64().unwrap_or(0.0),
                longitude: content["location"]["longitude"].as_f64().unwrap_or(0.0),
            },
            String::new(),
            Vec::new(),
        ),
        "messageVenue" => {
            let venue = &content["venue"];
            let text = |key: &str| venue[key].as_str().unwrap_or_default().to_string();
            (
                Body::Venue {
                    title: text("title"),
                    address: text("address"),
                    latitude: venue["location"]["latitude"].as_f64().unwrap_or(0.0),
                    longitude: venue["location"]["longitude"].as_f64().unwrap_or(0.0),
                },
                String::new(),
                Vec::new(),
            )
        }
        "messageContact" => {
            let contact = &content["contact"];
            let text = |key: &str| contact[key].as_str().unwrap_or_default().to_string();
            let name = format!("{} {}", text("first_name"), text("last_name"));
            (
                Body::Contact {
                    name: name.trim().to_string(),
                    phone: text("phone_number"),
                },
                String::new(),
                Vec::new(),
            )
        }
        "messagePoll" => {
            let poll = &content["poll"];
            let options = poll["options"]
                .as_array()
                .map(|list| {
                    list.iter()
                        .map(|o| {
                            (
                                o["text"]["text"].as_str().unwrap_or_default().to_string(),
                                o["voter_count"]
                                    .as_i64()
                                    .and_then(|v| i32::try_from(v).ok())
                                    .unwrap_or(0),
                            )
                        })
                        .collect()
                })
                .unwrap_or_default();
            (
                Body::Poll {
                    question: poll["question"]["text"]
                        .as_str()
                        .unwrap_or_default()
                        .to_string(),
                    options,
                    total_voters: poll["total_voter_count"]
                        .as_i64()
                        .and_then(|v| i32::try_from(v).ok())
                        .unwrap_or(0),
                },
                String::new(),
                Vec::new(),
            )
        }
        "messageDice" => (
            Body::Dice {
                emoji: content["emoji"].as_str().unwrap_or_default().to_string(),
                value: content["value"]
                    .as_i64()
                    .and_then(|v| i32::try_from(v).ok())
                    .unwrap_or(0),
            },
            String::new(),
            Vec::new(),
        ),
        "messageBasicGroupChatCreate" => (
            service(
                &format!(
                    "{{from}} created group \"{}\"",
                    content["title"].as_str().unwrap_or_default()
                ),
                Vec::new(),
            ),
            String::new(),
            Vec::new(),
        ),
        "messageSupergroupChatCreate" => (
            service(
                &format!(
                    "{{from}} created \"{}\"",
                    content["title"].as_str().unwrap_or_default()
                ),
                Vec::new(),
            ),
            String::new(),
            Vec::new(),
        ),
        "messageChatChangeTitle" => (
            service(
                &format!(
                    "{{from}} changed group title to \"{}\"",
                    content["title"].as_str().unwrap_or_default()
                ),
                Vec::new(),
            ),
            String::new(),
            Vec::new(),
        ),
        "messageChatChangePhoto" => (
            service("{from} changed group photo", Vec::new()),
            String::new(),
            Vec::new(),
        ),
        "messageChatDeletePhoto" => (
            service("{from} deleted group photo", Vec::new()),
            String::new(),
            Vec::new(),
        ),
        "messageChatAddMembers" => (
            service(
                "{from} invited {users}",
                user_ids(&content["member_user_ids"]),
            ),
            String::new(),
            Vec::new(),
        ),
        "messageChatDeleteMember" => (
            service(
                "{from} removed {users}",
                content["user_id"].as_i64().into_iter().collect(),
            ),
            String::new(),
            Vec::new(),
        ),
        "messageChatJoinByLink" | "messageChatJoinByRequest" => (
            service("{from} joined the group", Vec::new()),
            String::new(),
            Vec::new(),
        ),
        "messagePinMessage" => (
            service("{from} pinned a message", Vec::new()),
            String::new(),
            Vec::new(),
        ),
        "messageScreenshotTaken" => (
            service("{from} took a screenshot", Vec::new()),
            String::new(),
            Vec::new(),
        ),
        "messageContactRegistered" => (
            service("{from} joined Telegram", Vec::new()),
            String::new(),
            Vec::new(),
        ),
        "messageCall" => (
            service(&call_line(content), Vec::new()),
            String::new(),
            Vec::new(),
        ),
        "messageVideoChatStarted" => (
            service("{from} started a video chat", Vec::new()),
            String::new(),
            Vec::new(),
        ),
        "messageVideoChatEnded" => (
            service("Video chat ended", Vec::new()),
            String::new(),
            Vec::new(),
        ),
        _ => (Body::Other, String::new(), Vec::new()),
    }
}

/// Parse one raw TDLib `message`. Returns `None` for messages that must not
/// be exported: those that cannot be saved and self-destructing ones.
pub fn parse_message(message: &Value) -> Option<ExportMessage> {
    let id = message["id"].as_i64()?;
    if message["can_be_saved"] == false || !message["self_destruct_type"].is_null() {
        return None;
    }
    let (body, text, entities) = content(&message["content"]);
    let reply_to = (message["reply_to"]["@type"] == "messageReplyToMessage")
        .then(|| message["reply_to"]["message_id"].as_i64())
        .flatten()
        .filter(|id| *id > 0);
    Some(ExportMessage {
        id,
        date: message["date"]
            .as_i64()
            .and_then(|d| i32::try_from(d).ok())
            .unwrap_or(0),
        edited: message["edit_date"]
            .as_i64()
            .and_then(|d| i32::try_from(d).ok())
            .unwrap_or(0),
        outgoing: message["is_outgoing"] == true,
        sender: who_of(&message["sender_id"]),
        from: None,
        reply_to,
        forward: origin_of(&message["forward_info"]),
        forwarded_from: None,
        text,
        entities,
        body,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn base(content: Value) -> Value {
        json!({"id": 5, "date": 1_700_000_000, "is_outgoing": true,
            "can_be_saved": true, "sender_id": {"@type": "messageSenderUser", "user_id": 7},
            "content": content})
    }

    #[test]
    fn text_message_keeps_entities_and_reply() {
        let mut raw = base(json!({"@type": "messageText", "text": {"text": "hi bold",
            "entities": [{"offset": 3, "length": 4, "type": {"@type": "textEntityTypeBold"}},
                {"offset": 0, "length": 1, "type": {"@type": "textEntityTypeMention"}}]}}));
        raw["reply_to"] = json!({"@type": "messageReplyToMessage", "message_id": 3});
        let m = parse_message(&raw).unwrap();
        assert_eq!(m.body, Body::Text);
        assert_eq!(m.text, "hi bold");
        assert_eq!(m.entities.len(), 1);
        assert_eq!(m.entities[0].kind, EntityKind::Bold);
        assert_eq!(m.reply_to, Some(3));
        assert_eq!(m.sender, Some(Who::User(7)));
        assert!(m.outgoing);
    }

    #[test]
    fn photo_picks_the_largest_size() {
        let raw = base(json!({"@type": "messagePhoto", "caption": {"text": "c"},
            "photo": {"sizes": [
                {"width": 90, "height": 90, "photo": {"id": 1, "size": 10}},
                {"width": 800, "height": 600, "photo": {"id": 2, "size": 500}},
                {"width": 320, "height": 240, "photo": {"id": 3, "size": 50}}]}}));
        let m = parse_message(&raw).unwrap();
        assert_eq!(m.text, "c");
        let Body::Media(media) = m.body else {
            panic!("photo expected");
        };
        assert_eq!(
            (media.kind, media.file_id, media.size),
            (MediaKind::Photo, 2, 500)
        );
    }

    #[test]
    fn media_kinds_map_like_tdesktop() {
        let kind_of = |content: Value| match parse_message(&base(content)).unwrap().body {
            Body::Media(m) => m.kind,
            other => panic!("media expected, got {other:?}"),
        };
        let file = json!({"id": 1, "size": 3});
        assert_eq!(
            kind_of(json!({"@type": "messageVideo", "video": {"video": file}})),
            MediaKind::Video
        );
        assert_eq!(
            kind_of(json!({"@type": "messageAnimation", "animation": {"animation": file}})),
            MediaKind::Gif
        );
        assert_eq!(
            kind_of(json!({"@type": "messageVoiceNote", "voice_note": {"voice": file}})),
            MediaKind::VoiceMessage
        );
        assert_eq!(
            kind_of(json!({"@type": "messageVideoNote", "video_note": {"video": file}})),
            MediaKind::VideoMessage
        );
        assert_eq!(
            kind_of(json!({"@type": "messageSticker", "sticker": {"sticker": file, "emoji": "x"}})),
            MediaKind::Sticker
        );
        // Audio and documents are both "Files", as in tdesktop.
        assert_eq!(
            kind_of(json!({"@type": "messageAudio", "audio": {"audio": file}})),
            MediaKind::File
        );
        assert_eq!(
            kind_of(json!({"@type": "messageDocument", "document": {"document": file}})),
            MediaKind::File
        );
    }

    #[test]
    fn expected_size_stands_in_for_an_unknown_size() {
        let raw = base(json!({"@type": "messageDocument", "document": {
            "file_name": "a.pdf", "document": {"id": 4, "size": 0, "expected_size": 99}}}));
        let Body::Media(media) = parse_message(&raw).unwrap().body else {
            panic!("document expected");
        };
        assert_eq!((media.size, media.file_name.as_str()), (99, "a.pdf"));
    }

    #[test]
    fn protected_and_self_destructing_messages_are_dropped() {
        let mut raw = base(json!({"@type": "messageText", "text": {"text": "x"}}));
        raw["can_be_saved"] = json!(false);
        assert!(parse_message(&raw).is_none());
        let mut raw = base(json!({"@type": "messageText", "text": {"text": "x"}}));
        raw["self_destruct_type"] = json!({"@type": "messageSelfDestructTypeImmediately"});
        assert!(parse_message(&raw).is_none());
    }

    #[test]
    fn forward_origin_and_service_users_are_collected() {
        let mut raw = base(json!({"@type": "messageChatAddMembers", "member_user_ids": [8, 9]}));
        raw["forward_info"] = json!({"origin": {"@type": "messageOriginChannel", "chat_id": -100}});
        let m = parse_message(&raw).unwrap();
        assert!(m.is_service());
        assert_eq!(
            m.people(),
            [Who::User(7), Who::Chat(-100), Who::User(8), Who::User(9)]
        );
        let hidden = parse_message(&{
            let mut raw = base(json!({"@type": "messageText", "text": {"text": "x"}}));
            raw["forward_info"] =
                json!({"origin": {"@type": "messageOriginHiddenUser", "sender_name": "Ann"}});
            raw
        })
        .unwrap();
        assert_eq!(hidden.forward, Some(Origin::Name("Ann".into())));
    }

    #[test]
    fn unknown_content_becomes_other() {
        let m = parse_message(&base(json!({"@type": "messageInvoice"}))).unwrap();
        assert_eq!(m.body, Body::Other);
    }
}
