//! The account sections other than chats, fetched through TDLib the way
//! tdesktop's `export_api_wrap.cpp` fetches them through MTProto: personal
//! information, profile pictures, the story archive, profile music,
//! contacts (saved phone contacts and frequent contacts) and sessions.
//! Files are copied into `profile_pictures/`, `stories/` and
//! `profile_music/files/`, with tdesktop's names.
use super::settings::AccountSettings;
use crate::export::collect::user_name;
use crate::export::html::Outcome;
use crate::export::loaded_files::LoadedFiles;
use crate::export::model::{Entity, file_of, formatted, int, thumb_of};
use crate::export::names::{Namer, date_part, sanitize};
use crate::export::settings::{MediaKind, Skip};
use crate::export::worker::{ExportError, Tdlib, fetch_file, place_into, query};
use crate::json_access::JsonExt;
use serde_json::{Value, json};
use std::path::Path;

/// Page size for profile photos, stories and music.
const PAGE: i64 = 100;
/// TDLib's limit for `getTopChats`.
const TOP_CHATS: i64 = 30;

/// Where section files are copied, and the rules for them.
pub struct Files<'a> {
    pub root: &'a Path,
    /// Shared with the chats, so a file saved here is not saved again.
    pub loaded: &'a LoadedFiles,
    pub files_root: &'a Path,
    pub settings: &'a AccountSettings,
    pub offset_at: &'a dyn Fn(i64) -> i32,
}

impl Files<'_> {
    /// Copy TDLib file `file_id` to `relative`, or say why not. A file
    /// this export already saved keeps its first path.
    fn save<T: Tdlib>(
        &self,
        tdlib: &mut T,
        file_id: i32,
        relative: &str,
        limit: Option<i64>,
    ) -> Result<Outcome, ExportError> {
        let saved = |path: String| Outcome::Saved { path, thumb: None };
        if let Some(path) = self.loaded.find(file_id) {
            return Ok(saved(path));
        }
        match fetch_file(tdlib, self.files_root, file_id, limit)? {
            Ok(source) => {
                let placed = place_into(self.root, relative, &source);
                source.release(tdlib)?;
                placed?;
                self.loaded.save(file_id, relative.to_string());
                Ok(saved(relative.to_string()))
            }
            Err(skip) => Ok(Outcome::Skipped(skip)),
        }
    }

    fn date_part(&self, date: i32) -> String {
        date_part(date, (self.offset_at)(i64::from(date)))
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PersonalInfo {
    pub user_id: i64,
    pub first_name: String,
    pub last_name: String,
    pub phone: String,
    pub username: String,
    pub bio: String,
}

/// tdesktop's `FormatPhoneNumber` adds the plus sign.
pub fn format_phone(phone: &str) -> String {
    if phone.is_empty() || phone.starts_with('+') {
        phone.to_string()
    } else {
        format!("+{phone}")
    }
}

/// `@name`, tdesktop's `FormatUsername`.
fn first_username(user: &Value) -> String {
    user.at("usernames")
        .at("active_usernames")
        .as_array()
        .and_then(|list| list.first())
        .and_then(Value::as_str)
        .map(|name| format!("@{name}"))
        .unwrap_or_default()
}

pub fn personal_info<T: Tdlib>(tdlib: &mut T, me: &Value) -> Result<PersonalInfo, ExportError> {
    let user_id = me.at("id").as_i64().unwrap_or(0);
    let full = tdlib.call(
        json!({"@type": "getUserFullInfo", "user_id": user_id}),
        crate::export::worker::QUERY_TIMEOUT,
    )?;
    let text = |key: &str| me.at(key).as_str().unwrap_or_default().to_string();
    Ok(PersonalInfo {
        user_id,
        first_name: text("first_name"),
        last_name: text("last_name"),
        phone: format_phone(me.at("phone_number").as_str().unwrap_or_default()),
        username: first_username(me),
        bio: full
            .at("bio")
            .at("text")
            .as_str()
            .unwrap_or_default()
            .to_string(),
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedPhoto {
    pub date: i32,
    pub size: i64,
    pub outcome: Outcome,
}

/// The largest size of a TDLib `photo` / `chatPhoto`: (file id, size).
fn largest(sizes: &Value) -> (i32, i64) {
    sizes
        .as_array()
        .and_then(|list| {
            list.iter()
                .max_by_key(|s| i64::from(int(s.at("width"))) * i64::from(int(s.at("height"))))
        })
        .map_or((0, 0), |s| file_of(s.at("photo")))
}

/// Every profile picture, newest first, as `profile_pictures/photo_N@….jpg`.
/// Like tdesktop, they are never left out by media type; only the size
/// limit applies.
pub fn userpics<T: Tdlib>(
    tdlib: &mut T,
    files: &Files<'_>,
    user_id: i64,
) -> Result<Vec<SavedPhoto>, ExportError> {
    let mut out = Vec::new();
    loop {
        let page = query(
            tdlib,
            json!({"@type": "getUserProfilePhotos", "user_id": user_id,
                "offset": out.len(), "limit": PAGE}),
        )?;
        let photos = page.at("photos").as_array().cloned().unwrap_or_default();
        if photos.is_empty() {
            break;
        }
        for photo in photos {
            let date = int(photo.at("added_date"));
            let (file_id, size) = largest(photo.at("sizes"));
            let relative = format!(
                "profile_pictures/photo_{}{}.jpg",
                out.len() + 1,
                files.date_part(date)
            );
            let outcome = if size > files.settings.size_limit {
                Outcome::Skipped(Skip::TooLarge)
            } else {
                files.save(tdlib, file_id, &relative, Some(files.settings.size_limit))?
            };
            out.push(SavedPhoto {
                date,
                size,
                outcome,
            });
        }
    }
    Ok(out)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Story {
    pub date: i32,
    /// Saved to the profile (tdesktop's "pinned").
    pub pinned: bool,
    pub size: i64,
    pub outcome: Outcome,
    pub caption: String,
    pub entities: Vec<Entity>,
}

/// The story archive, newest first, as `stories/story_N@….jpg|mp4` (and a
/// `_thumb.jpg` next to videos). Stories that can't be forwarded are left
/// out, as Quill never copies protected content.
pub fn stories<T: Tdlib>(
    tdlib: &mut T,
    files: &Files<'_>,
    self_chat: i64,
) -> Result<(Vec<Story>, usize), ExportError> {
    let mut out = Vec::new();
    let mut protected = 0;
    let mut from = 0i64;
    loop {
        let page = query(
            tdlib,
            json!({"@type": "getChatArchivedStories", "chat_id": self_chat,
                "from_story_id": from, "limit": PAGE}),
        )?;
        let mut oldest = from;
        for story in page.at("stories").as_array().cloned().unwrap_or_default() {
            let Some(id) = story.at("id").as_i64().filter(|id| *id > 0) else {
                continue;
            };
            if from != 0 && id >= from {
                continue;
            }
            oldest = if oldest == 0 { id } else { oldest.min(id) };
            if story.at("can_be_forwarded") == false {
                protected += 1;
                continue;
            }
            let date = int(story.at("date"));
            let index = out.len() + 1;
            let content = &story.at("content");
            let (file_id, size, ext, thumb) = match content.at("@type").as_str() {
                Some("storyContentPhoto") => {
                    let (id, size) = largest(content.at("photo").at("sizes"));
                    (id, size, ".jpg", None)
                }
                Some("storyContentVideo") => {
                    let video = &content.at("video");
                    let (id, size) = file_of(video.at("video"));
                    (id, size, ".mp4", thumb_of(video.at("thumbnail")))
                }
                _ => (0, 0, "", None),
            };
            let relative = format!("stories/story_{index}{}{ext}", files.date_part(date));
            let mut outcome = files.save(tdlib, file_id, &relative, None)?;
            if let (Outcome::Saved { path, thumb }, Some(preview)) = (&mut outcome, thumb) {
                let target = format!("{path}_thumb.jpg");
                if let Outcome::Saved { .. } = files.save(tdlib, preview.file_id, &target, None)? {
                    *thumb = Some(target);
                }
            }
            let (caption, entities) = formatted(story.at("caption"));
            out.push(Story {
                date,
                pinned: story.at("is_posted_to_chat_page") == true,
                size,
                outcome,
                caption,
                entities,
            });
        }
        // Short pages aren't an end marker; stop when the cursor stops.
        if oldest == from {
            break;
        }
        from = oldest;
    }
    Ok((out, protected))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Track {
    pub performer: String,
    pub title: String,
    pub file_name: String,
    pub mime: String,
    pub duration: i32,
    pub size: i64,
    pub outcome: Outcome,
}

/// The tracks saved to the profile, as `profile_music/files/<name>`. They
/// follow the media choices like any file in a chat (the Files kind).
pub fn profile_music<T: Tdlib>(
    tdlib: &mut T,
    files: &Files<'_>,
    user_id: i64,
) -> Result<Vec<Track>, ExportError> {
    let mut out = Vec::new();
    let mut namer = Namer::default();
    loop {
        let page = query(
            tdlib,
            json!({"@type": "getUserProfileAudios", "user_id": user_id,
                "offset": out.len(), "limit": PAGE}),
        )?;
        let audios = page.at("audios").as_array().cloned().unwrap_or_default();
        if audios.is_empty() {
            break;
        }
        for audio in audios {
            let text = |key: &str| audio.at(key).as_str().unwrap_or_default().to_string();
            let (file_id, size) = file_of(audio.at("audio"));
            let outcome = match files.settings.skip_reason(MediaKind::File, size) {
                Some(skip) => Outcome::Skipped(skip),
                None => {
                    let named = sanitize(&text("file_name"));
                    let name = if named.is_empty() {
                        format!("audio_{}.mp3", out.len() + 1)
                    } else {
                        named
                    };
                    let relative = namer.unique("profile_music/files", &name);
                    files.save(tdlib, file_id, &relative, Some(files.settings.size_limit))?
                }
            };
            out.push(Track {
                performer: text("performer"),
                title: text("title"),
                file_name: text("file_name"),
                mime: text("mime_type"),
                duration: int(audio.at("duration")),
                size,
                outcome,
            });
        }
    }
    Ok(out)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Contact {
    /// 0 for a phone contact that is not on Telegram, or when unknown.
    pub user_id: i64,
    pub first_name: String,
    pub last_name: String,
    pub phone: String,
    /// When it was added; 0 when TDLib can't say.
    pub date: i32,
}

/// The saved phone contacts, sorted by name like tdesktop's
/// `SortedContactsIndices`. They come from Quill's TDLib extension
/// `getQuillSavedContacts` (Telegram's `contacts.getSaved`, which needs a
/// takeout session). When that is refused (an unextended TDLib, or
/// Telegram's takeout delay) the Telegram contacts stand in, without dates;
/// the second value says so.
pub fn contacts<T: Tdlib>(tdlib: &mut T) -> Result<(Vec<Contact>, bool), ExportError> {
    let saved = tdlib.call(
        json!({"@type": "getQuillSavedContacts"}),
        crate::export::worker::QUERY_TIMEOUT,
    )?;
    let mut list = Vec::new();
    let complete = saved.at("@type") == "quillSavedContacts";
    if complete {
        for contact in saved.at("contacts").as_array().cloned().unwrap_or_default() {
            let text = |key: &str| contact.at(key).as_str().unwrap_or_default().to_string();
            list.push(Contact {
                user_id: 0,
                first_name: text("first_name"),
                last_name: text("last_name"),
                phone: format_phone(&text("phone_number")),
                date: int(contact.at("date")),
            });
        }
    } else {
        let users = query(tdlib, json!({"@type": "getContacts"}))?;
        for id in users
            .at("user_ids")
            .as_array()
            .cloned()
            .unwrap_or_default()
            .iter()
            .filter_map(Value::as_i64)
        {
            let user = tdlib.call(
                json!({"@type": "getUser", "user_id": id}),
                crate::export::worker::QUERY_TIMEOUT,
            )?;
            let text = |key: &str| user.at(key).as_str().unwrap_or_default().to_string();
            list.push(Contact {
                user_id: id,
                first_name: text("first_name"),
                last_name: text("last_name"),
                phone: format_phone(&text("phone_number")),
                date: 0,
            });
        }
    }
    list.sort_by_cached_key(|c| format!("{} {}", c.first_name, c.last_name).to_lowercase());
    Ok((list, complete))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TopPeer {
    pub chat_id: i64,
    pub name: String,
    /// tdesktop's category: "people", "inline bots", "calls".
    pub category: &'static str,
}

/// Frequent contacts in tdesktop's three categories. TDLib keeps the order
/// but not the rating numbers tdesktop prints.
pub fn frequent<T: Tdlib>(tdlib: &mut T, me: i64) -> Result<Vec<TopPeer>, ExportError> {
    let mut out = Vec::new();
    for (category, name) in [
        ("topChatCategoryUsers", "people"),
        ("topChatCategoryInlineBots", "inline bots"),
        ("topChatCategoryCalls", "calls"),
    ] {
        let answer = tdlib.call(
            json!({"@type": "getTopChats", "category": {"@type": category}, "limit": TOP_CHATS}),
            crate::export::worker::QUERY_TIMEOUT,
        )?;
        for chat_id in answer
            .at("chat_ids")
            .as_array()
            .cloned()
            .unwrap_or_default()
            .iter()
            .filter_map(Value::as_i64)
        {
            let chat = query(tdlib, json!({"@type": "getChat", "chat_id": chat_id}))?;
            let saved = chat.at("type").at("user_id").as_i64() == Some(me);
            out.push(TopPeer {
                chat_id,
                name: if saved {
                    "Saved messages".into()
                } else {
                    chat.at("title").as_str().unwrap_or_default().to_string()
                },
                category: name,
            });
        }
    }
    Ok(out)
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Session {
    pub api_id: i64,
    pub application: String,
    pub version: String,
    pub device: String,
    pub platform: String,
    pub system: String,
    pub ip: String,
    pub location: String,
    pub last_active: i32,
    pub created: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WebSession {
    pub domain: String,
    pub bot: String,
    pub browser: String,
    pub platform: String,
    pub ip: String,
    pub location: String,
    pub last_active: i32,
    pub created: i32,
}

/// Active sessions and the websites logged in with Telegram.
pub fn sessions<T: Tdlib>(tdlib: &mut T) -> Result<(Vec<Session>, Vec<WebSession>), ExportError> {
    let answer = query(tdlib, json!({"@type": "getActiveSessions"}))?;
    let text = |v: &Value, key: &str| v.at(key).as_str().unwrap_or_default().to_string();
    let sessions = answer
        .at("sessions")
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(|s| Session {
            api_id: s.at("api_id").as_i64().unwrap_or(0),
            application: text(s, "application_name"),
            version: text(s, "application_version"),
            device: text(s, "device_model"),
            platform: text(s, "platform"),
            system: text(s, "system_version"),
            ip: text(s, "ip_address"),
            location: text(s, "location"),
            last_active: int(s.at("last_active_date")),
            created: int(s.at("log_in_date")),
        })
        .collect();
    let answer = query(tdlib, json!({"@type": "getConnectedWebsites"}))?;
    let mut web = Vec::new();
    for site in answer
        .at("websites")
        .as_array()
        .cloned()
        .unwrap_or_default()
    {
        let bot = match site.at("bot_user_id").as_i64().filter(|id| *id != 0) {
            Some(id) => {
                let user = tdlib.call(
                    json!({"@type": "getUser", "user_id": id}),
                    crate::export::worker::QUERY_TIMEOUT,
                )?;
                user.at("usernames")
                    .at("active_usernames")
                    .nth(0)
                    .as_str()
                    .map(|name| format!("@{name}"))
                    .unwrap_or_else(|| user_name(&user))
            }
            None => String::new(),
        };
        web.push(WebSession {
            domain: text(&site, "domain_name"),
            bot,
            browser: text(&site, "browser"),
            platform: text(&site, "platform"),
            ip: text(&site, "ip_address"),
            location: text(&site, "location"),
            last_active: int(site.at("last_active_date")),
            created: int(site.at("log_in_date")),
        });
    }
    Ok((sessions, web))
}
