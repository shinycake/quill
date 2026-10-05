//! Streaming, cancellable account export. Raw TDLib objects retain fields that
//! the conversation renderer doesn't understand. Disk work stays off the UI thread.
use crate::ids::RequestId;
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub struct AccountExport {
    requests: mpsc::Receiver<Value>,
    replies: mpsc::Sender<Value>,
    pub pending: Option<RequestId>,
    pub status: Arc<Mutex<String>>,
    pub folder: PathBuf,
    pub finished: Arc<AtomicBool>,
    cancelled: Arc<AtomicBool>,
}
impl AccountExport {
    pub fn start(parent: &Path, files: PathBuf, media: bool) -> io::Result<Self> {
        let parent = parent.canonicalize()?;
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let folder = parent.join(format!("Quill-export-{nonce}"));
        let mut builder = std::fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(&folder)?;
        let (request_tx, requests) = mpsc::channel();
        let (replies, reply_rx) = mpsc::channel();
        let status = Arc::new(Mutex::new("Starting account export…".into()));
        let worker_status = status.clone();
        let output = folder.clone();
        let finished = Arc::new(AtomicBool::new(false));
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_finished = finished.clone();
        let worker_cancelled = cancelled.clone();
        std::thread::spawn(move || {
            let mut worker = Worker {
                requests: request_tx,
                replies: reply_rx,
                status: worker_status,
                output,
                files,
                media,
                cancelled: worker_cancelled,
                users: BTreeSet::new(),
                sender_chats: BTreeSet::new(),
                file_ids: BTreeSet::new(),
                messages: 0,
                unavailable: 0,
                protected: 0,
                saved_contacts_exported: false,
            };
            let result = worker.run();
            let finished = result.is_ok();
            let mut limitations =
                vec!["Complete takeout message ranges and left-channel histories are not included"];
            if !worker.saved_contacts_exported {
                limitations
                    .push("Saved phone contacts could not be exported; see saved-contacts.json");
            }
            let manifest = json!({"format":"Quill raw TDLib JSONL", "complete":false,"finished":finished,
                "messages":worker.messages,"unavailable_media":worker.unavailable,
                "include_media":media,"protected_items_skipped":worker.protected,"limitations":limitations,"error":result.err().map(|e|e.to_string())});
            let saved = worker.write("manifest.json", &manifest);
            let note = if finished && saved.is_ok() {
                format!(
                    "Export finished with limitations: {} messages, {} unavailable media files. Some takeout data is not included; see manifest.json.",
                    worker.messages, worker.unavailable
                )
            } else {
                "Export stopped. Partial data is retained; see manifest.json.".into()
            };
            *worker.status.lock().unwrap() = note;
            worker_finished.store(true, Ordering::Release);
        });
        Ok(Self {
            requests,
            replies,
            pending: None,
            status,
            folder,
            finished,
            cancelled,
        })
    }
    pub fn next_request(&self) -> Option<Value> {
        self.requests.try_recv().ok()
    }
    pub fn reply(&mut self, id: RequestId, value: Value) {
        if self.pending == Some(id) {
            self.pending = None;
            let _ = self.replies.send(value);
        }
    }
    pub fn fail_send(&mut self, id: RequestId) {
        self.reply(id, json!({"@type":"error","code":0}));
    }
    pub fn label(&self) -> String {
        self.status.lock().unwrap().clone()
    }
}

impl Drop for AccountExport {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Release);
    }
}

fn private_file(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}
fn line(file: &mut File, value: &Value) -> io::Result<()> {
    serde_json::to_writer(&mut *file, value)?;
    file.write_all(b"\n")
}
struct Worker {
    requests: mpsc::Sender<Value>,
    replies: mpsc::Receiver<Value>,
    status: Arc<Mutex<String>>,
    output: PathBuf,
    files: PathBuf,
    media: bool,
    cancelled: Arc<AtomicBool>,
    users: BTreeSet<i64>,
    sender_chats: BTreeSet<i64>,
    file_ids: BTreeSet<i64>,
    messages: usize,
    unavailable: usize,
    protected: usize,
    saved_contacts_exported: bool,
}
impl Worker {
    fn check_cancel(&self) -> io::Result<()> {
        if self.cancelled.load(Ordering::Acquire) {
            Err(io::Error::other("Export cancelled"))
        } else {
            Ok(())
        }
    }
    fn raw(&self, request: Value) -> io::Result<Value> {
        self.check_cancel()?;
        self.requests
            .send(request)
            .map_err(|_| io::Error::other("Export cancelled"))?;
        let started = Instant::now();
        loop {
            self.check_cancel()?;
            match self.replies.recv_timeout(Duration::from_millis(100)) {
                Ok(mut value) => {
                    if let Some(object) = value.as_object_mut() {
                        object.remove("@extra");
                        object.remove("@client_id");
                    }
                    return Ok(value);
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err(io::Error::other("Export cancelled"));
                }
                Err(_) if started.elapsed() > Duration::from_secs(300) => {
                    return Err(io::Error::other("TDLib export request timed out"));
                }
                Err(_) => {}
            }
        }
    }
    fn query(&self, request: Value) -> io::Result<Value> {
        let value = self.raw(request)?;
        if value["@type"] == "error" {
            return Err(io::Error::other(format!(
                "TDLib export error {}",
                value["code"]
            )));
        }
        Ok(value)
    }
    fn write(&self, name: &str, value: &Value) -> io::Result<()> {
        let mut file = private_file(&self.output.join(name))?;
        line(&mut file, value)?;
        file.sync_all()
    }
    fn collect(&mut self, value: &Value) {
        match value {
            Value::Object(object) => {
                if value["has_protected_content"] == true {
                    return;
                }
                if value["@type"] == "messageSenderChat"
                    && let Some(id) = value["chat_id"].as_i64()
                {
                    self.sender_chats.insert(id);
                }
                if value["@type"] == "file"
                    && let Some(id) = value["id"].as_i64()
                    && id > 0
                {
                    self.file_ids.insert(id);
                }
                if value["@type"] == "messageSenderUser"
                    && let Some(id) = value["user_id"].as_i64()
                {
                    self.users.insert(id);
                }

                for item in object.values() {
                    self.collect(item);
                }
            }
            Value::Array(items) => {
                for item in items {
                    self.collect(item)
                }
            }
            _ => {}
        }
    }
    fn run(&mut self) -> io::Result<()> {
        self.write(
            "manifest-started.json",
            &json!({"complete":false,"include_media":self.media}),
        )?;
        self.write("README.json",&json!({"description":"Raw TDLib JSON records, one object per line. Chats include all accessible main and archived histories, newest first. users.jsonl resolves contacts and message sender users. Saved stories, profile music and server-selected takeout message ranges are separate files. media.jsonl maps TDLib file identifiers to numeric files. Protected, expired and unavailable media is reported rather than bypassed. Partial exports never have complete:true in manifest.json."}))?;
        let me = self.query(json!({"@type":"getMe"}))?;
        let user_id = me["id"]
            .as_i64()
            .ok_or_else(|| io::Error::other("Missing account identity"))?;
        self.users.insert(user_id);
        self.collect(&me);
        self.write("account.json", &me)?;
        let full = self.query(json!({"@type":"getUserFullInfo","user_id":user_id}))?;
        self.collect(&full);
        self.write("profile.json", &full)?;
        for (method, name) in [
            ("getContacts", "contacts.json"),
            ("getImportedContactCount", "imported-contact-count.json"),
            ("getActiveSessions", "sessions.json"),
            ("getConnectedWebsites", "websites.json"),
            ("getAccountTtl", "account-ttl.json"),
        ] {
            let value = self.query(json!({"@type":method}))?;
            if method == "getContacts"
                && let Some(ids) = value["user_ids"].as_array()
            {
                for id in ids.iter().filter_map(Value::as_i64) {
                    self.users.insert(id);
                }
            }
            self.collect(&value);
            self.write(name, &value)?;
        }
        // An unextended TDLib or a server takeout delay leaves a recorded gap;
        // neither is evidence that the account has no saved contacts.
        let saved_contacts = self.raw(json!({"@type":"getQuillSavedContacts"}))?;
        self.saved_contacts_exported = saved_contacts["@type"] == "quillSavedContacts"
            && saved_contacts["contacts"].is_array();
        self.write("saved-contacts.json", &saved_contacts)?;
        // These are Telegram's server-selected partitions, retained separately
        // until the native range-scoped history path is implemented. Errors are
        // retained as errors, never converted into an empty successful range list.
        let ranges = self.raw(json!({"@type":"getQuillTakeoutMessageRanges"}))?;
        self.write("message-ranges.json", &ranges)?;
        for (method, field, name) in [
            ("getUserProfilePhotos", "photos", "profile-photos.jsonl"),
            ("getUserProfileAudios", "audios", "profile-audios.jsonl"),
        ] {
            let mut output = private_file(&self.output.join(name))?;
            let mut offset = 0;
            loop {
                let value = self
                    .query(json!({"@type":method,"user_id":user_id,"offset":offset,"limit":100}))?;
                let count = value[field]
                    .as_array()
                    .ok_or_else(|| io::Error::other("Missing profile media page"))?
                    .len();
                self.collect(&value);
                line(&mut output, &value)?;
                if count == 0 {
                    break;
                }
                offset += count;
            }
            output.sync_all()?;
        }
        let own_chat =
            self.query(json!({"@type":"createPrivateChat","user_id":user_id,"force":false}))?;
        let chat_id = own_chat["id"]
            .as_i64()
            .ok_or_else(|| io::Error::other("Missing account chat identifier"))?;
        let mut stories = private_file(&self.output.join("stories.jsonl"))?;
        let mut from = 0;
        loop {
            let page = self.query(json!({"@type":"getChatArchivedStories","chat_id":chat_id,"from_story_id":from,"limit":100}))?;
            let items = page["stories"]
                .as_array()
                .ok_or_else(|| io::Error::other("Missing story archive page"))?;
            let mut oldest = from;
            for story in items {
                let id = story["id"]
                    .as_i64()
                    .filter(|id| *id > 0)
                    .ok_or_else(|| io::Error::other("Missing story identity"))?;
                if from != 0 && id >= from {
                    continue;
                }
                oldest = if oldest == 0 { id } else { oldest.min(id) };
                if story["can_be_forwarded"] == false {
                    self.protected += 1;
                    continue;
                }
                self.collect(story);
                line(&mut stories, story)?;
            }
            // Short pages aren't an end marker; continue until the cursor stops.
            if oldest == from {
                break;
            }
            from = oldest;
        }
        stories.sync_all()?;
        let mut chats = BTreeSet::new();
        for list in ["chatListMain", "chatListArchive"] {
            loop {
                let value =
                    self.raw(json!({"@type":"loadChats","chat_list":{"@type":list},"limit":100}))?;
                if value["@type"] == "error" {
                    if value["code"] == 404 {
                        break;
                    }
                    return Err(io::Error::other("Could not enumerate account chats"));
                }
            }
            let value = self
                .query(json!({"@type":"getChats","chat_list":{"@type":list},"limit":i32::MAX}))?;
            let ids = value["chat_ids"]
                .as_array()
                .ok_or_else(|| io::Error::other("Missing chat identifiers"))?;
            chats.extend(ids.iter().filter_map(Value::as_i64));
        }
        // TDLib keeps recently inactive channels outside either visible list.
        // Retain its raw answer even when it cannot offer any history, then
        // export every chat it does expose through the normal history path.
        let inactive = self.raw(json!({"@type":"getInactiveSupergroupChats"}))?;
        self.write("inactive-supergroup-chats.json", &inactive)?;
        if inactive["@type"] == "chats"
            && let Some(ids) = inactive["chat_ids"].as_array()
        {
            chats.extend(ids.iter().filter_map(Value::as_i64));
        }
        let mut metadata = private_file(&self.output.join("chats.jsonl"))?;
        for (index, chat_id) in chats.into_iter().enumerate() {
            *self.status.lock().unwrap() =
                format!("Exporting chat {} · {} messages", index + 1, self.messages);
            let chat = self.query(json!({"@type":"getChat","chat_id":chat_id}))?;
            self.collect(&chat);
            line(&mut metadata, &chat)?;
            if chat["has_protected_content"] == true {
                self.protected += 1;
                continue;
            }
            let mut history = private_file(&self.output.join(format!("chat-{chat_id}.jsonl")))?;
            let mut from = 0;
            loop {
                let page=self.query(json!({"@type":"getChatHistory","chat_id":chat_id,"from_message_id":from,"offset":0,"limit":100,"only_local":false}))?;
                let messages = page["messages"]
                    .as_array()
                    .ok_or_else(|| io::Error::other("Missing history page"))?;
                let mut oldest = from;
                for message in messages {
                    let id = message["id"]
                        .as_i64()
                        .ok_or_else(|| io::Error::other("Missing message identity"))?;
                    if from != 0 && id >= from {
                        continue;
                    }
                    oldest = if oldest == 0 { id } else { oldest.min(id) };
                    if message["can_be_saved"] == false
                        || message["has_protected_content"] == true
                        || !message["self_destruct_type"].is_null()
                    {
                        self.protected += 1;
                        continue;
                    }
                    self.collect(message);
                    line(&mut history, message)?;
                    self.messages += 1;
                }
                // TDLib may return a short cache page even when older history exists.
                if oldest == from {
                    break;
                }
                from = oldest;
            }
            history.sync_all()?;
        }
        for id in std::mem::take(&mut self.sender_chats) {
            let value = self.query(json!({"@type":"getChat","chat_id":id}))?;
            self.collect(&value);
            line(&mut metadata, &value)?;
        }
        let mut users = private_file(&self.output.join("users.jsonl"))?;
        for id in std::mem::take(&mut self.users) {
            let value = self.query(json!({"@type":"getUser","user_id":id}))?;
            self.collect(&value);
            line(&mut users, &value)?;
        }
        if self.media {
            let media_folder = self.output.join("media");
            std::fs::create_dir(&media_folder)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&media_folder, std::fs::Permissions::from_mode(0o700))?;
            }
            let root = self.files.canonicalize()?;
            let mut mapping = private_file(&self.output.join("media.jsonl"))?;
            for id in std::mem::take(&mut self.file_ids) {
                *self.status.lock().unwrap() = format!("Exporting media file {id}…");
                let value=self.raw(json!({"@type":"downloadFile","file_id":id,"priority":1,"offset":0,"limit":0,"synchronous":true}))?;
                let source = value["local"]["path"]
                    .as_str()
                    .and_then(|path| Path::new(path).canonicalize().ok());
                if value["local"]["is_downloading_completed"] == true
                    && let Some(source) = source
                    && source.starts_with(&root)
                    && source.is_file()
                {
                    let name = format!("media/{id}");
                    let mut input = File::open(source)?;
                    let mut output = private_file(&self.output.join(&name))?;
                    let mut buffer = [0u8; 65536];
                    loop {
                        self.check_cancel()?;
                        let read = input.read(&mut buffer)?;
                        if read == 0 {
                            break;
                        }
                        output.write_all(&buffer[..read])?;
                    }
                    output.sync_all()?;
                    line(&mut mapping, &json!({"file_id":id,"path":name}))?;
                } else {
                    self.unavailable += 1;
                    line(&mut mapping, &json!({"file_id":id,"unavailable":true}))?;
                }
            }
            mapping.sync_all()?;
        }
        users.sync_all()?;
        metadata.sync_all()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn streamed_export_pages_archives_preserves_unknown_fields_and_cancels() {
        let root = std::env::temp_dir().join(format!(
            "quill-account-export-test-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&root).unwrap();
        let files = root.join("files");
        std::fs::create_dir(&files).unwrap();
        std::fs::write(files.join("fixture"), b"media bytes").unwrap();
        for saved_contacts_available in [true, false] {
            let mut export = AccountExport::start(&root, files.clone(), true).unwrap();
            let mut methods = Vec::new();
            let deadline = Instant::now() + Duration::from_secs(5);
            let mut sequence = 1;
            while !export.finished.load(Ordering::Acquire) {
                assert!(Instant::now() < deadline, "{}", export.label());
                let Some(request) = export.next_request() else {
                    std::thread::sleep(Duration::from_millis(1));
                    continue;
                };
                let method = request["@type"].as_str().unwrap();
                methods.push(method.to_owned());
                let value = match method {
                    "getMe" => json!({"@type":"user","id":1,"future_profile_field":"retained"}),
                    "getContacts" => json!({"@type":"users","user_ids":[2]}),
                    "getUser" => {
                        json!({"@type":"user","id":request["user_id"],"first_name":"Fixture sender"})
                    }
                    "getQuillSavedContacts" => {
                        if saved_contacts_available {
                            json!({"@type":"quillSavedContacts","contacts":[{"phone_number":"fixture-only","future_contact_field":true}]})
                        } else {
                            json!({"@type":"error","code":400,"message":"TAKEOUT_INIT_DELAY_3600"})
                        }
                    }
                    "getQuillTakeoutMessageRanges" => {
                        if saved_contacts_available {
                            json!({"@type":"quillTakeoutMessageRanges","ranges":[{"min_id":1,"max_id":100,"future_range_field":true}]})
                        } else {
                            json!({"@type":"error","code":400,"message":"TAKEOUT_INIT_DELAY_3600"})
                        }
                    }
                    "getUserProfilePhotos" => json!({"@type":"chatPhotos","photos":[]}),
                    "getUserProfileAudios" => {
                        if request["offset"] == 0 {
                            json!({"@type":"audios","audios":[{"@type":"audio","audio":{"@type":"file","id":4}}]})
                        } else {
                            assert_eq!(request["offset"], 1);
                            json!({"@type":"audios","audios":[]})
                        }
                    }
                    "createPrivateChat" => json!({"@type":"chat","id":1}),
                    "getChatArchivedStories" => {
                        let from = request["from_story_id"].as_i64().unwrap();
                        let stories = match from {
                            0 => vec![
                                json!({"@type":"story","id":3,"future_story_field":true,"content":{"file":{"@type":"file","id":4}}}),
                            ],
                            3 => vec![
                                json!({"@type":"story","id":3}),
                                json!({"@type":"story","id":2,"can_be_forwarded":false,"file":{"@type":"file","id":5}}),
                            ],
                            2 => vec![json!({"@type":"story","id":1,"future_story_field":true})],
                            1 => vec![],
                            _ => panic!("Unexpected story cursor"),
                        };
                        json!({"@type":"stories","stories":stories})
                    }
                    "loadChats" => json!({"@type":"error","code":404}),
                    "getChats" => {
                        if request["chat_list"]["@type"] == "chatListArchive" {
                            json!({"@type":"chats","chat_ids":[-2]})
                        } else {
                            json!({"@type":"chats","chat_ids":[1]})
                        }
                    }
                    "getInactiveSupergroupChats" => {
                        json!({"@type":"chats","chat_ids":[-3]})
                    }
                    "getChat" => json!({"@type":"chat","id":request["chat_id"],"title":"Fixture"}),
                    "getChatHistory" => {
                        let from = request["from_message_id"].as_i64().unwrap();
                        let messages = if from == 0 {
                            vec![
                                json!({"@type":"message","id":10,"sender_id":{"@type":"messageSenderUser","user_id":2},"future_content":{"retained":true},"file":{"@type":"file","id":4}}),
                            ]
                        } else if from == 10 {
                            vec![
                                json!({"@type":"message","id":10}),
                                json!({"@type":"message","id":9,"has_protected_content":true,"file":{"@type":"file","id":5}}),
                            ]
                        } else {
                            vec![]
                        };
                        json!({"@type":"messages","messages":messages})
                    }
                    "downloadFile" => {
                        assert_eq!(request["file_id"], 4);
                        json!({"@type":"file","id":4,"local":{"path":files.join("fixture"),"is_downloading_completed":true}})
                    }
                    _ => json!({"@type":"fixture"}),
                };
                let id = RequestId(sequence);
                sequence += 1;
                export.pending = Some(id);
                export.reply(RequestId(id.0 + 10000), json!({"@type":"error","code":500}));
                assert_eq!(export.pending, Some(id));
                let encoded=json!({"@type":value["@type"],"@extra":{"quill_account_export":id.as_extra()},"content":value}).to_string();
                let parsed = crate::telegram::envelope::parse_envelope(&encoded).unwrap();
                assert_eq!(parsed.extra, Some(id));
                let crate::telegram::envelope::EnvelopePayload::AccountExport(raw) = parsed.payload
                else {
                    panic!("Raw response was projected");
                };
                export.reply(id, raw["content"].clone());
            }
            let folder = export.folder.clone();
            let manifest: Value =
                serde_json::from_slice(&std::fs::read(folder.join("manifest.json")).unwrap())
                    .unwrap();
            assert_eq!(manifest["complete"], false);
            assert_eq!(manifest["finished"], true);
            assert_eq!(
                manifest["limitations"].as_array().unwrap().len(),
                if saved_contacts_available { 1 } else { 2 }
            );
            let ranges = std::fs::read_to_string(folder.join("message-ranges.json")).unwrap();
            assert!(ranges.contains(if saved_contacts_available {
                "future_range_field"
            } else {
                "TAKEOUT_INIT_DELAY_3600"
            }));
            assert!(
                methods
                    .iter()
                    .any(|method| method == "getQuillTakeoutMessageRanges")
            );
            let saved = std::fs::read_to_string(folder.join("saved-contacts.json")).unwrap();
            assert!(saved.contains(if saved_contacts_available {
                "future_contact_field"
            } else {
                "TAKEOUT_INIT_DELAY_3600"
            }));
            assert!(
                export
                    .label()
                    .starts_with("Export finished with limitations:")
            );
            assert_eq!(manifest["messages"], 3);
            assert_eq!(manifest["protected_items_skipped"], 4);
            let stories = std::fs::read_to_string(folder.join("stories.jsonl")).unwrap();
            assert_eq!(stories.lines().count(), 2);
            assert!(
                stories
                    .lines()
                    .all(|line| line.contains("future_story_field"))
            );
            assert_eq!(
                methods
                    .iter()
                    .filter(|m| m.as_str() == "getChatArchivedStories")
                    .count(),
                4
            );
            assert_eq!(
                methods
                    .iter()
                    .filter(|m| m.as_str() == "getUserProfileAudios")
                    .count(),
                2
            );
            for name in ["chat-1.jsonl", "chat--2.jsonl", "chat--3.jsonl"] {
                let contents = std::fs::read_to_string(folder.join(name)).unwrap();
                assert_eq!(contents.lines().count(), 1);
                assert!(contents.contains("future_content"));
            }
            assert_eq!(
                methods
                    .iter()
                    .filter(|m| m.as_str() == "getChatHistory")
                    .count(),
                9
            );
            assert!(
                std::fs::read_to_string(folder.join("inactive-supergroup-chats.json"))
                    .unwrap()
                    .contains("-3")
            );
            assert_eq!(
                std::fs::read(folder.join("media/4")).unwrap(),
                b"media bytes"
            );
            assert!(!folder.join("media/5").exists());
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                assert_eq!(
                    std::fs::metadata(&folder).unwrap().permissions().mode() & 0o777,
                    0o700
                );
                assert_eq!(
                    std::fs::metadata(folder.join("account.json"))
                        .unwrap()
                        .permissions()
                        .mode()
                        & 0o777,
                    0o600
                );
            }
            drop(export);
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        let cancelled = AccountExport::start(&root, files, false).unwrap();
        let folder = cancelled.folder.clone();
        let finished = cancelled.finished.clone();
        drop(cancelled);
        while !finished.load(Ordering::Acquire) {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(1));
        }
        let manifest: Value =
            serde_json::from_slice(&std::fs::read(folder.join("manifest.json")).unwrap()).unwrap();
        assert_eq!(manifest["complete"], false);
        std::fs::remove_dir_all(root).unwrap();
    }
}
