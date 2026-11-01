//! End-to-end tests of the account export against a fake TDLib. Fixtures
//! only; no real account is ever read.
use super::settings::{AccountSettings, Section, Sections};
use super::worker::{
    AccountJob, AccountProgress, AccountStage, AccountSummary, create_folder, run,
};
use crate::export::settings::{ExportFormat, MediaKind, MediaTypes};
use crate::export::worker::{ExportError, Tdlib};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn scratch(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "quill-account-export-{label}-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

const ME: i64 = 1;
const SAVED: i64 = 1;
const PERSONAL: i64 = 10;
const BOT: i64 = 11;
const ARCHIVED: i64 = 12;
const GROUP: i64 = -20;
const PRIVATE_SUPERGROUP: i64 = -1_000_000_000_030;
const PUBLIC_CHANNEL: i64 = -1_000_000_000_040;
const PROTECTED: i64 = -1_000_000_000_050;
const SECRET: i64 = -2_000_000_000_000_060;

fn text(id: i64, date: i64, from: i64, body: &str) -> Value {
    json!({"id": id, "date": date, "is_outgoing": from == ME, "can_be_saved": true,
        "sender_id": {"@type": "messageSenderUser", "user_id": from},
        "content": {"@type": "messageText", "text": {"text": body, "entities": []}}})
}

fn photo(id: i64, date: i64, file_id: i64) -> Value {
    json!({"id": id, "date": date, "is_outgoing": false, "can_be_saved": true,
        "sender_id": {"@type": "messageSenderUser", "user_id": 2},
        "content": {"@type": "messagePhoto", "caption": {"text": ""},
            "photo": {"sizes": [{"width": 640, "height": 480,
                "photo": {"id": file_id, "size": 5}}]}}})
}

fn chat(id: i64, kind: Value, title: &str) -> Value {
    json!({"@type": "chat", "id": id, "type": kind, "title": title,
        "has_protected_content": id == PROTECTED})
}

/// A whole account: chats with histories (newest first), profile, files.
struct Account {
    chats: HashMap<i64, Value>,
    main: Vec<i64>,
    archive: Vec<i64>,
    histories: HashMap<i64, Vec<Value>>,
    files: HashMap<i64, PathBuf>,
    saved_contacts: bool,
    fail_history: Option<i64>,
    cancel_after: Option<usize>,
    calls: Vec<String>,
}

impl Account {
    fn new(files_root: &Path) -> Self {
        let private = |user: i64| json!({"@type": "chatTypePrivate", "user_id": user});
        let supergroup = |id: i64, channel: bool| json!({"@type": "chatTypeSupergroup", "supergroup_id": id, "is_channel": channel});
        let chats = HashMap::from([
            (SAVED, chat(SAVED, private(ME), "Ann Lee")),
            (PERSONAL, chat(PERSONAL, private(2), "Bo Park")),
            (BOT, chat(BOT, private(3), "Helper Bot")),
            (ARCHIVED, chat(ARCHIVED, private(4), "Cy Old")),
            (
                GROUP,
                chat(
                    GROUP,
                    json!({"@type": "chatTypeBasicGroup", "basic_group_id": 20}),
                    "Family",
                ),
            ),
            (
                PRIVATE_SUPERGROUP,
                chat(PRIVATE_SUPERGROUP, supergroup(30, false), "Team"),
            ),
            (
                PUBLIC_CHANNEL,
                chat(PUBLIC_CHANNEL, supergroup(40, true), "News"),
            ),
            (
                PROTECTED,
                chat(PROTECTED, supergroup(50, false), "Secret Club"),
            ),
            (
                SECRET,
                chat(
                    SECRET,
                    json!({"@type": "chatTypeSecret", "secret_chat_id": 6, "user_id": 2}),
                    "Bo Park",
                ),
            ),
        ]);
        let histories = HashMap::from([
            (
                PERSONAL,
                vec![
                    photo(3, 1_700_000_300, 100),
                    text(2, 1_700_000_200, ME, "hi Bo"),
                    text(1, 1_700_000_100, 2, "hello"),
                ],
            ),
            (SAVED, vec![text(1, 1_700_000_050, ME, "note to self")]),
            (BOT, vec![text(1, 1_700_000_000, 3, "/start")]),
            (ARCHIVED, vec![]),
            (
                GROUP,
                vec![
                    text(3, 1_700_000_300, ME, "mine"),
                    text(2, 1_700_000_200, 2, "theirs"),
                    text(1, 1_700_000_100, ME, "mine too"),
                ],
            ),
            (
                PRIVATE_SUPERGROUP,
                vec![
                    text(2, 1_700_000_200, 2, "x"),
                    text(1, 1_700_000_100, ME, "y"),
                ],
            ),
            (PUBLIC_CHANNEL, vec![text(1, 1_700_000_100, 9, "news")]),
            (PROTECTED, vec![text(1, 1_700_000_100, 2, "hidden")]),
        ]);
        let mut files = HashMap::new();
        for (id, name, bytes) in [
            (100, "chat_photo.jpg", &b"chat photo"[..]),
            (200, "pic_new.jpg", b"newest picture"),
            (201, "pic_old.jpg", b"older picture"),
            (300, "story.jpg", b"story photo"),
            (301, "story.mp4", b"story video"),
            (302, "story_thumb.jpg", b"story thumb"),
            (400, "song.mp3", b"song"),
        ] {
            let path = files_root.join(name);
            std::fs::write(&path, bytes).unwrap();
            files.insert(id, path);
        }
        Self {
            chats,
            main: vec![
                PERSONAL,
                SAVED,
                BOT,
                GROUP,
                PRIVATE_SUPERGROUP,
                PUBLIC_CHANNEL,
                PROTECTED,
                SECRET,
            ],
            archive: vec![ARCHIVED, PERSONAL],
            histories,
            files,
            saved_contacts: true,
            fail_history: None,
            cancel_after: None,
            calls: Vec::new(),
        }
    }

    fn count(&self, method: &str) -> usize {
        self.calls.iter().filter(|c| *c == method).count()
    }

    fn history(&self, chat_id: i64, from: i64) -> Vec<Value> {
        self.histories
            .get(&chat_id)
            .map(|list| {
                list.iter()
                    .filter(|m| from == 0 || m["id"].as_i64().unwrap() <= from)
                    .take(if from == 0 { 100 } else { 101 })
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }
}

fn user(id: i64) -> Value {
    let (first, last, bot) = match id {
        ME => ("Ann", "Lee", false),
        2 => ("Bo", "Park", false),
        3 => ("Helper", "Bot", true),
        4 => ("Cy", "Old", false),
        _ => ("User", "", false),
    };
    let kind = if bot {
        json!({"@type": "userTypeBot"})
    } else {
        json!({"@type": "userTypeRegular"})
    };
    json!({"@type": "user", "id": id, "first_name": first, "last_name": last,
        "phone_number": format!("1555000{id}"), "type": kind,
        "usernames": {"active_usernames": if id == ME { json!(["ann"]) } else { json!([]) }}})
}

impl Tdlib for Account {
    fn call(&mut self, request: Value, _timeout: Duration) -> Result<Value, ExportError> {
        let method = request["@type"].as_str().unwrap().to_string();
        self.calls.push(method.clone());
        if self.cancel_after.is_some_and(|n| self.calls.len() > n) {
            return Err(ExportError::Cancelled);
        }
        let error =
            |code: i64, message: &str| json!({"@type": "error", "code": code, "message": message});
        Ok(match method.as_str() {
            "getMe" => user(ME),
            "getUser" => user(request["user_id"].as_i64().unwrap()),
            "getUserFullInfo" => json!({"@type": "userFullInfo", "bio": {"text": "Bio <here>"}}),
            "loadChats" => error(404, "Not Found"),
            "getChats" => {
                let ids = if request["chat_list"]["@type"] == "chatListArchive" {
                    &self.archive
                } else {
                    &self.main
                };
                json!({"@type": "chats", "chat_ids": ids})
            }
            "getChat" => self
                .chats
                .get(&request["chat_id"].as_i64().unwrap())
                .cloned()
                .unwrap_or_else(|| error(400, "Chat not found")),
            "getSupergroup" => {
                let public = request["supergroup_id"] == 40;
                json!({"@type": "supergroup", "usernames":
                    {"active_usernames": if public { json!(["news"]) } else { json!([]) }}})
            }
            "getChatHistory" => {
                let chat_id = request["chat_id"].as_i64().unwrap();
                if self.fail_history == Some(chat_id) {
                    return Ok(error(400, "CHANNEL_PRIVATE"));
                }
                let from = request["from_message_id"].as_i64().unwrap();
                json!({"@type": "messages", "messages": self.history(chat_id, from)})
            }
            "searchChatMessages" => {
                let chat_id = request["chat_id"].as_i64().unwrap();
                let sender = request["sender_id"]["user_id"].as_i64().unwrap();
                let from = request["from_message_id"].as_i64().unwrap();
                // One message per page, to exercise the cursor.
                let found: Vec<Value> = self
                    .histories
                    .get(&chat_id)
                    .cloned()
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|m| m["sender_id"]["user_id"] == sender)
                    .filter(|m| from == 0 || m["id"].as_i64().unwrap() < from)
                    .take(1)
                    .collect();
                let next = found.last().map_or(0, |m| m["id"].as_i64().unwrap());
                json!({"@type": "foundChatMessages", "messages": found, "next_from_message_id": next})
            }
            "getUserProfilePhotos" => {
                let photos = if request["offset"] == 0 {
                    json!([
                        {"id": 1, "added_date": 1_700_000_500, "sizes": [
                            {"width": 160, "height": 160, "photo": {"id": 299, "size": 1}},
                            {"width": 640, "height": 640, "photo": {"id": 200, "size": 9}}]},
                        {"id": 2, "added_date": 1_600_000_000, "sizes": [
                            {"width": 640, "height": 640, "photo": {"id": 201, "size": 9}}]}
                    ])
                } else {
                    json!([])
                };
                json!({"@type": "chatPhotos", "photos": photos})
            }
            "createPrivateChat" => chat(
                SAVED,
                json!({"@type": "chatTypePrivate", "user_id": ME}),
                "Ann",
            ),
            "getChatArchivedStories" => {
                let stories = match request["from_story_id"].as_i64().unwrap() {
                    0 => json!([
                        {"id": 7, "date": 1_700_000_700, "can_be_forwarded": true,
                            "is_posted_to_chat_page": true, "caption": {"text": "Sunset", "entities": []},
                            "content": {"@type": "storyContentPhoto", "photo": {"sizes": [
                                {"width": 720, "height": 1280, "photo": {"id": 300, "size": 11}}]}}},
                        {"id": 6, "date": 1_700_000_600, "can_be_forwarded": false,
                            "content": {"@type": "storyContentPhoto", "photo": {"sizes": []}}}
                    ]),
                    6 => json!([
                        {"id": 5, "date": 1_700_000_500, "can_be_forwarded": true,
                            "caption": {"text": ""},
                            "content": {"@type": "storyContentVideo", "video": {
                                "thumbnail": {"format": {"@type": "thumbnailFormatJpeg"},
                                    "file": {"id": 302, "size": 3}},
                                "video": {"id": 301, "size": 11}}}}
                    ]),
                    _ => json!([]),
                };
                json!({"@type": "stories", "stories": stories})
            }
            "getUserProfileAudios" => {
                let audios = if request["offset"] == 0 {
                    json!([{"duration": 185, "title": "Song", "performer": "Band",
                        "file_name": "song.mp3", "mime_type": "audio/mpeg",
                        "audio": {"id": 400, "size": 4}}])
                } else {
                    json!([])
                };
                json!({"@type": "audios", "audios": audios})
            }
            "getQuillSavedContacts" if self.saved_contacts => {
                json!({"@type": "quillSavedContacts", "contacts": [
                    {"phone_number": "15550009", "first_name": "zed", "last_name": "", "date": 1_650_000_000},
                    {"phone_number": "15550008", "first_name": "Amy", "last_name": "Z", "date": 1_650_000_001}]})
            }
            "getQuillSavedContacts" => error(400, "TAKEOUT_INIT_DELAY_3600"),
            "getContacts" => json!({"@type": "users", "user_ids": [2, 4]}),
            "getTopChats" => {
                let ids = if request["category"]["@type"] == "topChatCategoryUsers" {
                    json!([PERSONAL, SAVED])
                } else {
                    json!([])
                };
                json!({"@type": "chats", "chat_ids": ids})
            }
            "getActiveSessions" => json!({"@type": "sessions", "sessions": [
                {"api_id": 2040, "application_name": "Quill", "application_version": "2026.1",
                    "device_model": "MacBook", "platform": "macOS", "system_version": "27",
                    "ip_address": "203.0.113.5", "location": "Testland",
                    "log_in_date": 1_690_000_000, "last_active_date": 1_700_000_000}]}),
            "getConnectedWebsites" => json!({"@type": "connectedWebsites", "websites": [
                {"domain_name": "example.org", "bot_user_id": 3, "browser": "Firefox",
                    "platform": "Linux", "ip_address": "198.51.100.7", "location": "Elsewhere",
                    "log_in_date": 1_680_000_000, "last_active_date": 1_690_000_000}]}),
            "getFile" => json!({"@type": "file", "id": request["file_id"],
                "local": {"is_downloading_completed": true}}),
            "downloadFile" => {
                let id = request["file_id"].as_i64().unwrap();
                match self.files.get(&id) {
                    Some(path) => json!({"@type": "file", "id": id,
                        "size": std::fs::metadata(path).map(|m| m.len()).unwrap_or(0),
                        "local": {"path": path.to_string_lossy(), "is_downloading_completed": true}}),
                    None => json!({"@type": "file", "id": id, "size": 1,
                        "local": {"path": "", "is_downloading_completed": false}}),
                }
            }
            other => panic!("unexpected request {other}"),
        })
    }
}

struct Run {
    root: PathBuf,
    result: Result<AccountSummary, ExportError>,
    reports: Vec<AccountProgress>,
}

fn export(account: &mut Account, settings: &AccountSettings, files_root: &Path) -> Run {
    let root = create_folder(&settings.dir, "2023-11-14").unwrap();
    let reports = std::cell::RefCell::new(Vec::new());
    let report = |p: AccountProgress| reports.borrow_mut().push(p);
    let job = AccountJob {
        settings,
        files_root,
        offset_at: &|_| 0,
        report: &report,
    };
    let result = run(&job, account, &root);
    Run {
        root,
        result,
        reports: reports.into_inner(),
    }
}

fn everything(dir: &Path) -> AccountSettings {
    let mut s = AccountSettings::new(dir.to_path_buf());
    for section in [
        Section::Sessions,
        Section::BotChats,
        Section::PrivateChannels,
        Section::PublicGroups,
        Section::PublicChannels,
    ] {
        s.set(section, true);
    }
    s.format = ExportFormat::HtmlAndJson;
    s.media = MediaTypes::all();
    s
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn setup(label: &str) -> (PathBuf, PathBuf, Account) {
    let dir = scratch(label);
    let td = dir.join("td");
    std::fs::create_dir_all(&td).unwrap();
    let account = Account::new(&td);
    (dir, td, account)
}

#[test]
fn a_full_export_follows_tdesktops_layout() {
    let (dir, td, mut account) = setup("full");
    let settings = everything(&dir);
    let run = export(&mut account, &settings, &td);
    let summary = run.result.unwrap();
    let root = &run.root;
    assert_eq!(root.file_name().unwrap(), "DataExport_2023-11-14"); // Saved, personal, bot, group, private supergroup, public channel and
    // the archived chat; the protected and secret chats are left out.
    assert_eq!(summary.chats, 7);
    assert_eq!(summary.protected_chats, 1);
    assert!(!summary.contacts_partial);
    for path in [
        "export_results.html",
        "result.json",
        "css/style.css",
        "js/script.js",
        "lists/chats.html",
        "lists/contacts.html",
        "lists/frequent.html",
        "lists/profile_pictures.html",
        "lists/stories.html",
        "lists/profile_music.html",
        "lists/sessions.html",
        "lists/web_sessions.html",
        "profile_pictures/photo_1@14-11-2023_22-21-40.jpg",
        "profile_pictures/photo_2@13-09-2020_12-26-40.jpg",
        "stories/story_1@14-11-2023_22-25-00.jpg",
        "stories/story_2@14-11-2023_22-21-40.mp4",
        "stories/story_2@14-11-2023_22-21-40.mp4_thumb.jpg",
        "profile_music/files/song.mp3",
    ] {
        assert!(root.join(path).exists(), "{path} missing");
    }
    // Chats in list order: main list first, then the archive.
    let chat = |n: usize| root.join(format!("chats/chat_{n}"));
    let personal = read(&chat(1).join("messages.html"));
    assert!(personal.contains("href=\"../../css/style.css\""));
    assert!(personal.contains("href=\"../../lists/chats.html\" onclick=\"return GoBack(this)\""));
    assert!(personal.contains(">hello<") && personal.contains(">hi Bo<"));
    assert!(personal.contains("src=\"photos/photo_1@14-11-2023_22-18-20.jpg\""));
    assert!(
        chat(1)
            .join("photos/photo_1@14-11-2023_22-18-20.jpg")
            .exists()
    );
    // The archived chat has no messages: a row, but no folder.
    assert!(!chat(7).exists());
    // The private group keeps only my messages (the default), found by a
    // sender search instead of paging the whole history.
    let group = read(&chat(4).join("messages.html"));
    assert!(group.contains(">mine<") && group.contains(">mine too<"));
    assert!(!group.contains(">theirs<"));
    assert!(account.count("searchChatMessages") >= 3);
    let list = read(&root.join("lists/chats.html"));
    assert!(list.contains("Saved messages"));
    assert!(list.contains("2 outgoing messages"));
    assert!(list.contains("No messages"));
    assert!(list.contains("href=\"../chats/chat_1/messages.html#allow_back\""));
    assert!(!list.contains("chat_7/messages.html"));
    assert!(!list.contains("Secret Club"));
    // The index: personal information, then sections in tdesktop's order.
    let index = read(&root.join("export_results.html"));
    assert!(index.contains("Bio &lt;here&gt;"));
    assert!(index.contains("+15550001") && index.contains("@ann"));
    assert!(index.contains("src=\"profile_pictures/photo_1@14-11-2023_22-21-40.jpg\""));
    let order: Vec<usize> = [
        "lists/chats.html",
        "lists/contacts.html",
        "lists/frequent.html",
        "lists/profile_pictures.html",
        "lists/stories.html",
        "lists/profile_music.html",
        "lists/sessions.html",
        "lists/web_sessions.html",
    ]
    .iter()
    .map(|p| index.find(&format!("href=\"{p}#allow_back\"")).unwrap())
    .collect();
    assert!(order.windows(2).all(|w| w[0] < w[1]), "{order:?}");
    // Stories: the protected one is left out, the video has its thumbnail.
    let stories = read(&root.join("lists/stories.html"));
    assert!(stories.contains("Saved to Profile") && stories.contains(">Sunset<"));
    assert!(stories.contains("src=\"../stories/story_2@14-11-2023_22-21-40.mp4_thumb.jpg\""));
    // result.json holds every section and every chat.
    let result: Value = serde_json::from_str(&read(&root.join("result.json"))).unwrap();
    assert_eq!(result["personal_information"]["username"], "@ann");
    assert_eq!(result["profile_pictures"].as_array().unwrap().len(), 2);
    assert_eq!(result["stories"].as_array().unwrap().len(), 2);
    assert_eq!(
        result["profile_music"][0]["file"],
        "profile_music/files/song.mp3"
    );
    assert_eq!(result["contacts"]["list"][0]["first_name"], "Amy");
    assert_eq!(
        result["frequent_contacts"]["list"][1]["name"],
        "Saved messages"
    );
    assert_eq!(result["sessions"]["list"][0]["application_name"], "Quill");
    assert_eq!(
        result["web_sessions"]["list"][0]["bot_username"],
        "Helper Bot"
    );
    let chats = result["chats"]["list"].as_array().unwrap();
    let types: Vec<&str> = chats.iter().map(|c| c["type"].as_str().unwrap()).collect();
    assert_eq!(
        types,
        [
            "personal_chat",
            "saved_messages",
            "bot_chat",
            "private_group",
            "private_supergroup",
            "public_channel",
            "personal_chat"
        ]
    );
    assert_eq!(chats[0]["messages"].as_array().unwrap().len(), 3);
    // JSON paths are relative to the export root, as in tdesktop.
    assert_eq!(
        chats[0]["messages"][2]["photo"],
        "chats/chat_1/photos/photo_1@14-11-2023_22-18-20.jpg"
    );
    // A public channel only ever keeps my messages: here, none.
    assert_eq!(chats[5]["messages"].as_array().unwrap().len(), 0);
    assert_eq!(summary.messages, 3 + 1 + 1 + 2 + 1);
    assert!(!root.join(".spool").exists());
    // Progress walked through every step and ended on the last chat.
    let stages: Vec<&AccountStage> = run.reports.iter().map(|p| &p.stage).collect();
    for wanted in [
        AccountStage::ChatsList,
        AccountStage::PersonalInfo,
        AccountStage::Userpics,
        AccountStage::Stories,
        AccountStage::ProfileMusic,
        AccountStage::Contacts,
        AccountStage::Sessions,
        AccountStage::Chats,
    ] {
        assert!(stages.contains(&&wanted), "{wanted:?}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn only_the_chosen_sections_are_fetched() {
    let (dir, td, mut account) = setup("sections");
    let mut settings = AccountSettings::new(dir.clone());
    settings.sections = Sections::of(&[Section::Sessions]);
    let run = export(&mut account, &settings, &td);
    let summary = run.result.unwrap();
    assert_eq!(summary.chats, 0);
    for method in [
        "loadChats",
        "getChats",
        "getUserProfilePhotos",
        "getChatArchivedStories",
        "getUserProfileAudios",
        "getQuillSavedContacts",
        "downloadFile",
    ] {
        assert_eq!(account.count(method), 0, "{method}");
    }
    let root = &run.root;
    assert!(root.join("lists/sessions.html").exists());
    assert!(!root.join("lists/chats.html").exists());
    assert!(!root.join("chats").exists());
    assert!(!root.join("result.json").exists(), "HTML only");
    let index = read(&root.join("export_results.html"));
    assert!(index.contains("lists/sessions.html"));
    assert!(!index.contains("personal_info"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn json_only_writes_no_pages() {
    let (dir, td, mut account) = setup("json");
    let mut settings = everything(&dir);
    settings.format = ExportFormat::Json;
    let run = export(&mut account, &settings, &td);
    run.result.unwrap();
    let root = &run.root;
    assert!(root.join("result.json").exists());
    assert!(!root.join("export_results.html").exists());
    assert!(!root.join("lists").exists());
    assert!(!root.join("css").exists());
    assert!(!root.join("chats/chat_1/messages.html").exists());
    // Media still lands in each chat's folder.
    assert!(
        root.join("chats/chat_1/photos/photo_1@14-11-2023_22-18-20.jpg")
            .exists()
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn media_choices_and_size_limit_apply_to_chats_and_music() {
    let (dir, td, mut account) = setup("media");
    let mut settings = everything(&dir);
    settings.media = MediaTypes::none().with(MediaKind::Video, true);
    let run = export(&mut account, &settings, &td);
    let summary = run.result.unwrap();
    let root = &run.root;
    assert!(!root.join("chats/chat_1/photos").exists());
    assert!(!root.join("profile_music").exists());
    assert!(read(&root.join("chats/chat_1/messages.html")).contains("(Photo not included."));
    assert!(read(&root.join("lists/profile_music.html")).contains("(File not included."));
    // Profile pictures and stories ignore the media kinds, like tdesktop.
    assert!(root.join("profile_pictures").exists());
    assert!(root.join("stories").exists());
    assert_eq!(summary.skipped, 1);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn telegram_contacts_stand_in_when_saved_contacts_are_refused() {
    let (dir, td, mut account) = setup("contacts");
    account.saved_contacts = false;
    let mut settings = AccountSettings::new(dir.clone());
    settings.sections = Sections::of(&[Section::Contacts]);
    settings.format = ExportFormat::HtmlAndJson;
    let run = export(&mut account, &settings, &td);
    assert!(run.result.unwrap().contacts_partial);
    let page = read(&run.root.join("lists/contacts.html"));
    assert!(page.contains("Bo Park") && page.contains("Cy Old"));
    assert!(page.contains("takeout session"));
    let result: Value = serde_json::from_str(&read(&run.root.join("result.json"))).unwrap();
    assert_eq!(result["contacts"]["list"][0]["user_id"], 2);
    assert!(result["contacts"]["list"][0].get("date").is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_chat_telegram_refuses_does_not_end_the_export() {
    let (dir, td, mut account) = setup("refused");
    account.fail_history = Some(BOT);
    let settings = everything(&dir);
    let run = export(&mut account, &settings, &td);
    let summary = run.result.unwrap();
    assert_eq!(summary.failed_chats, 1);
    assert!(run.root.join("chats/chat_4/messages.html").exists());
    let result: Value = serde_json::from_str(&read(&run.root.join("result.json"))).unwrap();
    assert_eq!(result["chats"]["list"].as_array().unwrap().len(), 7);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn cancelling_stops_the_export_and_removes_the_spool() {
    let (dir, td, mut account) = setup("cancel");
    // Past the sections, inside the chats.
    account.cancel_after = Some(60);
    let settings = everything(&dir);
    let run = export(&mut account, &settings, &td);
    assert!(matches!(run.result, Err(ExportError::Cancelled)));
    assert!(!run.root.join(".spool").exists());
    assert!(!run.root.join("export_results.html").exists());
    assert_eq!(account.calls.len(), 61);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn progress_labels_and_fraction() {
    let mut p = AccountProgress::starting();
    assert_eq!(p.label(), "Initializing…");
    assert_eq!(p.fraction(), None);
    p.stage = AccountStage::ChatsList;
    p.found = 12;
    assert_eq!(p.label(), "Processing chats… 12");
    p.stage = AccountStage::Chats;
    p.chat_index = 1;
    p.chat_count = 4;
    p.chat_name = "Team".into();
    assert_eq!(p.label(), "Chats: 2 of 4 · Team");
    assert_eq!(p.fraction(), Some(0.25));
    p.chat.stage = crate::export::Stage::Writing;
    p.chat.total = 10;
    p.chat.done = 5;
    assert_eq!(p.fraction(), Some(0.375));
    assert_eq!(p.detail().unwrap(), "Saving messages… 5 of 10 · 0 files");
    assert!(!p.is_settled());
    p.stage = AccountStage::Cancelled;
    assert!(p.is_settled());
}

#[test]
fn export_folders_are_private_and_never_overwrite_each_other() {
    let dir = scratch("folders");
    let first = create_folder(&dir, "2023-11-14").unwrap();
    let second = create_folder(&dir, "2023-11-14").unwrap();
    assert_eq!(first.file_name().unwrap(), "DataExport_2023-11-14");
    assert_eq!(second.file_name().unwrap(), "DataExport_2023-11-14 (2)");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&first).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o700);
    }
    let _ = std::fs::remove_dir_all(&dir);
}
