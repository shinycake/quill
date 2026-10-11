//! `result.json`: the machine-readable export, written message by message
//! so a long chat never sits in memory (tdesktop's `result.json` for a
//! single chat has the same top-level shape).
use super::html::{Outcome, service_text, skip_note};
use super::model::{Body, EntityKind, ExportMessage, Origin, Who};
use super::settings::MediaKind;
use crate::local_time::civil_at;
use serde_json::{Value, json};
use std::io::{self, Write};

fn who_id(who: Who) -> String {
    match who {
        Who::User(id) => format!("user{id}"),
        Who::Chat(id) => format!("chat{id}"),
    }
}

fn media_type(kind: MediaKind) -> Option<&'static str> {
    match kind {
        MediaKind::Photo | MediaKind::File => None,
        MediaKind::Video => Some("video_file"),
        MediaKind::VoiceMessage => Some("voice_message"),
        MediaKind::VideoMessage => Some("video_message"),
        MediaKind::Sticker => Some("sticker"),
        MediaKind::Gif => Some("animation"),
    }
}

fn entity_name(kind: &EntityKind) -> &'static str {
    match kind {
        EntityKind::Bold => "bold",
        EntityKind::Italic => "italic",
        EntityKind::Underline => "underline",
        EntityKind::Strike => "strikethrough",
        EntityKind::Spoiler => "spoiler",
        EntityKind::Code => "code",
        EntityKind::Pre => "pre",
        EntityKind::Quote => "blockquote",
        EntityKind::Url => "link",
        EntityKind::Email => "email",
        EntityKind::Link(_) => "text_link",
    }
}

fn iso_date(date: i32, offset: i32) -> String {
    let unix = i64::from(date);
    let time = civil_at(unix, offset);
    let second = (unix + i64::from(offset)).rem_euclid(60);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
        time.year, time.month, time.day, time.hour, time.minute, second
    )
}

/// One message as a JSON object.
pub fn message_json(msg: &ExportMessage, outcome: Option<&Outcome>, offset: i32) -> Value {
    let mut out = json!({
        "id": msg.id,
        "type": if msg.is_service() { "service" } else { "message" },
        "date": iso_date(msg.date, offset),
        "date_unixtime": msg.date.to_string(),
    });
    let object = out.as_object_mut().expect("json! object");
    if msg.edited > 0 {
        object.insert("edited".into(), iso_date(msg.edited, offset).into());
        object.insert("edited_unixtime".into(), msg.edited.to_string().into());
    }
    if msg.is_service() {
        object.insert("actor".into(), msg.from.clone().into());
        object.insert("action_text".into(), service_text(msg).into());
    } else if let Some(from) = &msg.from {
        object.insert("from".into(), from.clone().into());
    }
    if let Some(who) = msg.sender {
        object.insert("from_id".into(), who_id(who).into());
    }
    if msg.outgoing {
        object.insert("outgoing".into(), true.into());
    }
    if let Some(reply) = msg.reply_to {
        object.insert("reply_to_message_id".into(), reply.into());
    }
    match (&msg.forward, &msg.forwarded_from) {
        (Some(Origin::Name(name)), _) => {
            object.insert("forwarded_from".into(), name.clone().into());
        }
        (Some(Origin::Who(who)), name) => {
            object.insert(
                "forwarded_from".into(),
                name.clone().unwrap_or_else(|| who_id(*who)).into(),
            );
            object.insert("forwarded_from_id".into(), who_id(*who).into());
        }
        (None, _) => {}
    }
    object.insert("text".into(), msg.text.clone().into());
    if !msg.entities.is_empty() {
        let entities: Vec<Value> = msg
            .entities
            .iter()
            .map(|e| {
                let mut entity = json!({
                    "type": entity_name(&e.kind),
                    "offset": e.offset,
                    "length": e.length,
                });
                if let EntityKind::Link(url) = &e.kind {
                    entity["href"] = url.clone().into();
                }
                entity
            })
            .collect();
        object.insert("text_entities".into(), entities.into());
    }
    match &msg.body {
        Body::Media(media) => {
            let value: Value = match outcome {
                Some(Outcome::Saved(path)) => path.clone().into(),
                Some(Outcome::Skipped(skip)) => skip_note(media.kind, *skip).into(),
                None => Value::Null,
            };
            let key = if media.kind == MediaKind::Photo {
                "photo"
            } else {
                "file"
            };
            object.insert(key.into(), value);
            if let Some(kind) = media_type(media.kind) {
                object.insert("media_type".into(), kind.into());
            }
            if !media.mime.is_empty() {
                object.insert("mime_type".into(), media.mime.clone().into());
            }
            if media.size > 0 {
                object.insert("file_size".into(), media.size.into());
            }
            if media.duration > 0 {
                object.insert("duration_seconds".into(), media.duration.into());
            }
            if !media.label.is_empty() {
                object.insert("label".into(), media.label.clone().into());
            }
        }
        Body::Location {
            latitude,
            longitude,
        } => {
            object.insert(
                "location_information".into(),
                json!({"latitude": latitude, "longitude": longitude}),
            );
        }
        Body::Venue {
            title,
            address,
            latitude,
            longitude,
        } => {
            object.insert(
                "location_information".into(),
                json!({"latitude": latitude, "longitude": longitude}),
            );
            object.insert("place_name".into(), title.clone().into());
            object.insert("address".into(), address.clone().into());
        }
        Body::Contact { name, phone } => {
            object.insert(
                "contact_information".into(),
                json!({"name": name, "phone_number": phone}),
            );
        }
        Body::Poll {
            question,
            options,
            total_voters,
        } => {
            let answers: Vec<Value> = options
                .iter()
                .map(|(text, votes)| json!({"text": text, "voters": votes}))
                .collect();
            object.insert(
                "poll".into(),
                json!({"question": question, "closed": false, "total_voters": total_voters, "answers": answers}),
            );
        }
        Body::Dice { emoji, value } => {
            object.insert("dice".into(), json!({"emoji": emoji, "value": value}));
        }
        Body::Text | Body::Service { .. } | Body::Other => {}
    }
    out
}

/// Streams `result.json`.
pub struct JsonWriter<W: Write> {
    out: W,
    first: bool,
}

impl<W: Write> JsonWriter<W> {
    pub fn start(mut out: W, name: &str, chat_id: i64) -> io::Result<Self> {
        write!(
            out,
            "{{\n \"name\": {},\n \"id\": {chat_id},\n \"messages\": [",
            serde_json::to_string(name)?
        )?;
        Ok(Self { out, first: true })
    }

    pub fn push(&mut self, message: &Value) -> io::Result<()> {
        if !self.first {
            self.out.write_all(b",")?;
        }
        self.first = false;
        self.out.write_all(b"\n  ")?;
        serde_json::to_writer(&mut self.out, message)?;
        Ok(())
    }

    pub fn finish(mut self) -> io::Result<W> {
        self.out.write_all(b"\n ]\n}\n")?;
        self.out.flush()?;
        Ok(self.out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::export::model::MediaRef;
    use crate::export::settings::Skip;

    fn msg(body: Body) -> ExportMessage {
        ExportMessage {
            id: 4,
            date: 1_700_000_000,
            edited: 0,
            outgoing: true,
            sender: Some(Who::User(9)),
            from: Some("Ann".into()),
            reply_to: Some(2),
            forward: None,
            forwarded_from: None,
            text: "hi \"you\"".into(),
            entities: Vec::new(),
            body,
        }
    }

    #[test]
    fn plain_message_fields() {
        let v = message_json(&msg(Body::Text), None, 3600);
        assert_eq!(v["id"], 4);
        assert_eq!(v["type"], "message");
        assert_eq!(v["date"], "2023-11-14T23:13:20");
        assert_eq!(v["date_unixtime"], "1700000000");
        assert_eq!(v["from"], "Ann");
        assert_eq!(v["from_id"], "user9");
        assert_eq!(v["reply_to_message_id"], 2);
        assert_eq!(v["text"], "hi \"you\"");
        assert_eq!(v["outgoing"], true);
    }

    #[test]
    fn media_paths_and_skip_notes() {
        let media = Body::Media(MediaRef {
            kind: MediaKind::VoiceMessage,
            file_id: 1,
            size: 99,
            file_name: String::new(),
            mime: "audio/ogg".into(),
            duration: 4,
            label: String::new(),
        });
        let saved = message_json(
            &msg(media.clone()),
            Some(&Outcome::Saved("voice_messages/audio_1.ogg".into())),
            0,
        );
        assert_eq!(saved["file"], "voice_messages/audio_1.ogg");
        assert_eq!(saved["media_type"], "voice_message");
        assert_eq!(saved["duration_seconds"], 4);
        let skipped = message_json(&msg(media), Some(&Outcome::Skipped(Skip::TooLarge)), 0);
        assert!(
            skipped["file"]
                .as_str()
                .unwrap()
                .contains("exceeds maximum size")
        );
        let photo = Body::Media(MediaRef {
            kind: MediaKind::Photo,
            file_id: 1,
            size: 0,
            file_name: String::new(),
            mime: String::new(),
            duration: 0,
            label: String::new(),
        });
        let v = message_json(&msg(photo), Some(&Outcome::Saved("photos/p.jpg".into())), 0);
        assert_eq!(v["photo"], "photos/p.jpg");
    }

    #[test]
    fn service_messages_have_actor_and_action() {
        let v = message_json(
            &msg(Body::Service {
                text: "{from} invited {users}".into(),
                users: vec![1],
                names: vec!["Bo".into()],
            }),
            None,
            0,
        );
        assert_eq!(v["type"], "service");
        assert_eq!(v["actor"], "Ann");
        assert_eq!(v["action_text"], "Ann invited Bo");
    }

    #[test]
    fn writer_produces_valid_json_for_zero_and_many_messages() {
        let empty = JsonWriter::start(Vec::new(), "A \"chat\"", 5)
            .unwrap()
            .finish()
            .unwrap();
        let parsed: Value = serde_json::from_slice(&empty).unwrap();
        assert_eq!(parsed["name"], "A \"chat\"");
        assert_eq!(parsed["messages"].as_array().unwrap().len(), 0);

        let mut writer = JsonWriter::start(Vec::new(), "c", 5).unwrap();
        for i in 0..3 {
            writer.push(&json!({"id": i})).unwrap();
        }
        let parsed: Value = serde_json::from_slice(&writer.finish().unwrap()).unwrap();
        assert_eq!(parsed["id"], 5);
        assert_eq!(parsed["messages"][2]["id"], 2);
    }
}
