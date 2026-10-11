//! The account export's `result.json`, streamed: small sections are
//! written whole as they finish, chats message by message. The top level
//! has tdesktop's keys: `about`, `personal_information`,
//! `profile_pictures`, `stories`, `profile_music`, `contacts`,
//! `frequent_contacts`, `sessions`, `web_sessions` and `chats`.
use super::dialogs::Dialog;
use super::html::{
    ABOUT_CHATS, ABOUT_CONTACTS, ABOUT_CONTACTS_PARTIAL, ABOUT_EXPORT, ABOUT_FREQUENT,
    ABOUT_SESSIONS, ABOUT_WEB_SESSIONS,
};
use super::sections::{
    Contact, PersonalInfo, SavedPhoto, Session, Story, TopPeer, Track, WebSession,
};
use crate::export::html::{Outcome, noun_skip_note};
use crate::export::json::{MessageSink, iso_date};
use crate::export::userpic::bare_chat_id;
use serde_json::{Map, Value, json};
use std::io::{self, Write};

/// `date` and `date_unixtime` style pair, skipped for a zero date.
fn put_date(object: &mut Map<String, Value>, key: &str, date: i32, offset_at: &dyn Fn(i64) -> i32) {
    if date > 0 {
        object.insert(
            key.into(),
            iso_date(date, offset_at(i64::from(date))).into(),
        );
        object.insert(format!("{key}_unixtime"), date.to_string().into());
    }
}

fn file_value(noun: &str, outcome: &Outcome) -> Value {
    match outcome {
        Outcome::Saved { path, .. } => path.clone().into(),
        Outcome::Skipped(skip) => noun_skip_note(noun, *skip).into(),
    }
}

pub fn personal_json(info: &PersonalInfo) -> Value {
    let mut object = Map::new();
    object.insert("user_id".into(), info.user_id.into());
    object.insert("first_name".into(), info.first_name.clone().into());
    object.insert("last_name".into(), info.last_name.clone().into());
    object.insert("phone_number".into(), info.phone.clone().into());
    if !info.username.is_empty() {
        object.insert("username".into(), info.username.clone().into());
    }
    if !info.bio.is_empty() {
        object.insert("bio".into(), info.bio.clone().into());
    }
    object.into()
}

pub fn userpics_json(list: &[SavedPhoto], offset_at: &dyn Fn(i64) -> i32) -> Value {
    list.iter()
        .map(|photo| {
            let mut object = Map::new();
            put_date(&mut object, "date", photo.date, offset_at);
            object.insert("photo".into(), file_value("Photo", &photo.outcome));
            Value::Object(object)
        })
        .collect()
}

pub fn stories_json(list: &[Story], offset_at: &dyn Fn(i64) -> i32) -> Value {
    list.iter()
        .map(|story| {
            let mut object = Map::new();
            put_date(&mut object, "date", story.date, offset_at);
            object.insert("pinned".into(), story.pinned.into());
            object.insert("media".into(), file_value("Story", &story.outcome));
            if let Outcome::Saved {
                thumb: Some(thumb), ..
            } = &story.outcome
            {
                object.insert("thumbnail".into(), thumb.clone().into());
            }
            if !story.caption.is_empty() {
                object.insert("text".into(), story.caption.clone().into());
            }
            Value::Object(object)
        })
        .collect()
}

pub fn music_json(list: &[Track]) -> Value {
    list.iter()
        .map(|track| {
            let mut object = Map::new();
            object.insert("file".into(), file_value("File", &track.outcome));
            if !track.file_name.is_empty() {
                object.insert("file_name".into(), track.file_name.clone().into());
            }
            object.insert("file_size".into(), track.size.into());
            object.insert("media_type".into(), "audio_file".into());
            if !track.performer.is_empty() {
                object.insert("performer".into(), track.performer.clone().into());
            }
            if !track.title.is_empty() {
                object.insert("title".into(), track.title.clone().into());
            }
            if !track.mime.is_empty() {
                object.insert("mime_type".into(), track.mime.clone().into());
            }
            if track.duration > 0 {
                object.insert("duration_seconds".into(), track.duration.into());
            }
            Value::Object(object)
        })
        .collect()
}

pub fn contacts_json(list: &[Contact], complete: bool, offset_at: &dyn Fn(i64) -> i32) -> Value {
    let entries: Vec<Value> = list
        .iter()
        .map(|contact| {
            let mut object = Map::new();
            if contact.user_id != 0 {
                object.insert("user_id".into(), contact.user_id.into());
            }
            object.insert("first_name".into(), contact.first_name.clone().into());
            object.insert("last_name".into(), contact.last_name.clone().into());
            object.insert("phone_number".into(), contact.phone.clone().into());
            put_date(&mut object, "date", contact.date, offset_at);
            Value::Object(object)
        })
        .collect();
    let about = if complete {
        ABOUT_CONTACTS
    } else {
        ABOUT_CONTACTS_PARTIAL
    };
    json!({"about": about, "list": entries})
}

pub fn frequent_json(list: &[TopPeer]) -> Value {
    let entries: Vec<Value> = list
        .iter()
        .map(|peer| {
            json!({"id": bare_chat_id(peer.chat_id), "category": peer.category.replace(' ', "_"),
                "type": "user", "name": peer.name})
        })
        .collect();
    json!({"about": ABOUT_FREQUENT, "list": entries})
}

pub fn sessions_json(list: &[Session], offset_at: &dyn Fn(i64) -> i32) -> Value {
    let entries: Vec<Value> = list
        .iter()
        .map(|s| {
            let mut object = Map::new();
            put_date(&mut object, "last_active", s.last_active, offset_at);
            object.insert("last_ip".into(), s.ip.clone().into());
            object.insert("last_location".into(), s.location.clone().into());
            object.insert("application_name".into(), s.application.clone().into());
            object.insert("application_version".into(), s.version.clone().into());
            object.insert("device_model".into(), s.device.clone().into());
            object.insert("platform".into(), s.platform.clone().into());
            object.insert("system_version".into(), s.system.clone().into());
            put_date(&mut object, "created", s.created, offset_at);
            Value::Object(object)
        })
        .collect();
    json!({"about": ABOUT_SESSIONS, "list": entries})
}

pub fn web_sessions_json(list: &[WebSession], offset_at: &dyn Fn(i64) -> i32) -> Value {
    let entries: Vec<Value> = list
        .iter()
        .map(|s| {
            let mut object = Map::new();
            put_date(&mut object, "last_active", s.last_active, offset_at);
            object.insert("last_ip".into(), s.ip.clone().into());
            object.insert("last_location".into(), s.location.clone().into());
            if !s.bot.is_empty() {
                object.insert("bot_username".into(), s.bot.clone().into());
            }
            object.insert("domain_name".into(), s.domain.clone().into());
            object.insert("browser".into(), s.browser.clone().into());
            object.insert("platform".into(), s.platform.clone().into());
            put_date(&mut object, "created", s.created, offset_at);
            Value::Object(object)
        })
        .collect();
    json!({"about": ABOUT_WEB_SESSIONS, "list": entries})
}

/// Where the writer is inside the chats list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ChatState {
    Outside,
    /// In `chats.list`; `bool` is whether a chat was written yet.
    List(bool),
    /// In one chat's `messages`; `bool` is whether a message was written.
    Messages(bool),
}

/// Streams the account export's `result.json`.
pub struct ResultJson<W: Write> {
    out: W,
    state: ChatState,
}

impl<W: Write> ResultJson<W> {
    pub fn start(mut out: W) -> io::Result<Self> {
        write!(
            out,
            "{{\n \"about\": {}",
            serde_json::to_string(ABOUT_EXPORT)?
        )?;
        Ok(Self {
            out,
            state: ChatState::Outside,
        })
    }

    /// A whole top-level section.
    pub fn field(&mut self, key: &str, value: &Value) -> io::Result<()> {
        debug_assert_eq!(self.state, ChatState::Outside);
        write!(self.out, ",\n {}: ", serde_json::to_string(key)?)?;
        serde_json::to_writer(&mut self.out, value)?;
        Ok(())
    }

    pub fn chats_start(&mut self) -> io::Result<()> {
        write!(
            self.out,
            ",\n \"chats\": {{\n  \"about\": {},\n  \"list\": [",
            serde_json::to_string(ABOUT_CHATS)?
        )?;
        self.state = ChatState::List(false);
        Ok(())
    }

    pub fn chat_start(&mut self, dialog: &Dialog) -> io::Result<()> {
        let ChatState::List(any) = self.state else {
            return Err(io::Error::other("chat outside the chats list"));
        };
        if any {
            self.out.write_all(b",")?;
        }
        let mut head = Map::new();
        if dialog.kind != super::dialogs::ChatKind::SavedMessages {
            head.insert("name".into(), dialog.name.clone().into());
        }
        head.insert("type".into(), dialog.kind.json_type().into());
        head.insert("id".into(), bare_chat_id(dialog.chat_id).into());
        let head = serde_json::to_string(&Value::Object(head))?;
        // Reopen the object to append the streamed `messages` array.
        let head = head.strip_suffix('}').unwrap_or(&head);
        write!(self.out, "\n   {head},\"messages\": [")?;
        self.state = ChatState::Messages(false);
        Ok(())
    }

    pub fn chat_end(&mut self) -> io::Result<()> {
        if let ChatState::Messages(_) = self.state {
            self.out.write_all(b"\n   ]}")?;
            self.state = ChatState::List(true);
        }
        Ok(())
    }

    pub fn chats_end(&mut self) -> io::Result<()> {
        self.chat_end()?;
        if let ChatState::List(_) = self.state {
            self.out.write_all(b"\n  ]\n }")?;
            self.state = ChatState::Outside;
        }
        Ok(())
    }

    pub fn finish(mut self) -> io::Result<W> {
        self.chats_end()?;
        self.out.write_all(b"\n}\n")?;
        self.out.flush()?;
        Ok(self.out)
    }
}

impl<W: Write> MessageSink for ResultJson<W> {
    fn push_message(&mut self, message: &Value) -> io::Result<()> {
        let ChatState::Messages(any) = self.state else {
            return Err(io::Error::other("message outside a chat"));
        };
        if any {
            self.out.write_all(b",")?;
        }
        self.out.write_all(b"\n    ")?;
        serde_json::to_writer(&mut self.out, message)?;
        self.state = ChatState::Messages(true);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::export::account::dialogs::ChatKind;

    fn dialog(kind: ChatKind, id: i64, name: &str) -> Dialog {
        Dialog {
            chat_id: id,
            kind,
            name: name.into(),
            only_my: false,
            folder: "chats/chat_1/".into(),
        }
    }

    #[test]
    fn the_streamed_file_is_valid_json_with_tdesktops_keys() {
        let mut writer = ResultJson::start(Vec::new()).unwrap();
        writer
            .field(
                "personal_information",
                &personal_json(&PersonalInfo {
                    user_id: 5,
                    first_name: "Ann".into(),
                    phone: "+1".into(),
                    ..Default::default()
                }),
            )
            .unwrap();
        writer.chats_start().unwrap();
        writer
            .chat_start(&dialog(ChatKind::SavedMessages, 5, "Saved messages"))
            .unwrap();
        writer.chat_end().unwrap();
        writer
            .chat_start(&dialog(
                ChatKind::PublicChannel,
                -1_000_000_000_042,
                "News \"x\"",
            ))
            .unwrap();
        writer.push_message(&json!({"id": 1})).unwrap();
        writer.push_message(&json!({"id": 2})).unwrap();
        let bytes = writer.finish().unwrap();
        let parsed: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(parsed["about"], ABOUT_EXPORT);
        assert_eq!(parsed["personal_information"]["first_name"], "Ann");
        assert!(parsed["personal_information"].get("bio").is_none());
        let chats = parsed["chats"]["list"].as_array().unwrap();
        assert_eq!(chats.len(), 2);
        assert!(chats[0].get("name").is_none());
        assert_eq!(chats[0]["type"], "saved_messages");
        assert_eq!(chats[0]["messages"].as_array().unwrap().len(), 0);
        assert_eq!(chats[1]["name"], "News \"x\"");
        assert_eq!(chats[1]["id"], 42);
        assert_eq!(chats[1]["type"], "public_channel");
        assert_eq!(chats[1]["messages"][1]["id"], 2);
    }

    #[test]
    fn a_file_without_chats_is_still_valid() {
        let mut writer = ResultJson::start(Vec::new()).unwrap();
        writer
            .field("sessions", &sessions_json(&[Session::default()], &|_| 0))
            .unwrap();
        let parsed: Value = serde_json::from_slice(&writer.finish().unwrap()).unwrap();
        assert_eq!(parsed["sessions"]["list"].as_array().unwrap().len(), 1);
        assert!(parsed.get("chats").is_none());
        // Messages outside a chat are refused.
        let mut writer = ResultJson::start(Vec::new()).unwrap();
        assert!(writer.push_message(&json!({})).is_err());
    }

    #[test]
    fn sections_use_tdesktops_field_names() {
        let photos = userpics_json(
            &[
                SavedPhoto {
                    date: 1_700_000_000,
                    size: 5,
                    outcome: Outcome::Saved {
                        path: "profile_pictures/photo_1@14-11-2023_22-13-20.jpg".into(),
                        thumb: None,
                    },
                },
                SavedPhoto {
                    date: 0,
                    size: 5,
                    outcome: Outcome::Skipped(crate::export::settings::Skip::TooLarge),
                },
            ],
            &|_| 0,
        );
        assert_eq!(photos[0]["date"], "2023-11-14T22:13:20");
        assert_eq!(photos[0]["date_unixtime"], "1700000000");
        assert!(photos[1].get("date").is_none());
        assert!(
            photos[1]["photo"]
                .as_str()
                .unwrap()
                .contains("exceeds maximum size")
        );
        let contacts = contacts_json(
            &[Contact {
                user_id: 0,
                first_name: "Bo".into(),
                last_name: String::new(),
                phone: "+12".into(),
                date: 1_700_000_000,
            }],
            true,
            &|_| 0,
        );
        assert_eq!(contacts["about"], ABOUT_CONTACTS);
        assert!(contacts["list"][0].get("user_id").is_none());
        assert_eq!(contacts["list"][0]["phone_number"], "+12");
        let partial = contacts_json(&[], false, &|_| 0);
        assert_eq!(partial["about"], ABOUT_CONTACTS_PARTIAL);
        let top = frequent_json(&[TopPeer {
            chat_id: 77,
            name: "Cy".into(),
            category: "inline bots",
        }]);
        assert_eq!(top["list"][0]["category"], "inline_bots");
        assert_eq!(top["list"][0]["id"], 77);
    }
}
