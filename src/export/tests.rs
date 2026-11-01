//! End-to-end tests of the export routine against a fake TDLib. Fixtures
//! only; no real chat is ever read.
use super::settings::{ExportFormat, ExportSettings, MediaKind, MediaTypes};
use super::worker::{ExportError, Job, Progress, Stage, Tdlib, create_folder, run};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn scratch(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "quill-export-test-{label}-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn text_message(id: i64, date: i64, outgoing: bool, text: &str) -> Value {
    json!({"id": id, "date": date, "is_outgoing": outgoing, "can_be_saved": true,
        "sender_id": {"@type": "messageSenderUser", "user_id": if outgoing { 1 } else { 2 }},
        "content": {"@type": "messageText", "text": {"text": text, "entities": []}}})
}

fn photo_message(id: i64, date: i64, file_id: i64, size: i64) -> Value {
    json!({"id": id, "date": date, "is_outgoing": false, "can_be_saved": true,
        "sender_id": {"@type": "messageSenderUser", "user_id": 2},
        "content": {"@type": "messagePhoto", "caption": {"text": ""},
            "photo": {"sizes": [{"width": 10, "height": 10,
                "photo": {"id": file_id, "size": size}}]}}})
}

fn video_message(id: i64, date: i64, file_id: i64, size: i64) -> Value {
    json!({"id": id, "date": date, "is_outgoing": false, "can_be_saved": true,
        "sender_id": {"@type": "messageSenderUser", "user_id": 2},
        "content": {"@type": "messageVideo", "caption": {"text": ""},
            "video": {"file_name": "clip.mp4", "mime_type": "video/mp4",
                "video": {"id": file_id, "size": size}}}})
}

/// A chat whose history is a list of raw messages, newest first.
struct Fake {
    messages: Vec<Value>,
    /// file id -> local path TDLib reports after "downloading".
    files: HashMap<i64, PathBuf>,
    local: HashMap<i64, Value>,
    /// 0 pages whole; n returns at most n messages per page.
    page_size: usize,
    calls: Vec<String>,
    /// Cancel once this many requests were made.
    cancel_after: Option<usize>,
    /// What `getFile` reports once a download ends, when something else
    /// started using the file meanwhile.
    touched_after_download: Option<Value>,
    fail_history: bool,
    leftovers: Option<super::leftovers::Leftovers>,
}

impl Fake {
    fn new(messages: Vec<Value>) -> Self {
        Self {
            messages,
            files: HashMap::new(),
            local: HashMap::new(),
            page_size: 0,
            calls: Vec::new(),
            cancel_after: None,
            touched_after_download: None,
            fail_history: false,
            leftovers: None,
        }
    }

    fn count(&self, method: &str) -> usize {
        self.calls.iter().filter(|c| *c == method).count()
    }
}

impl Tdlib for Fake {
    fn post_unanswered(&mut self, request: Value) {
        let _ = self.call(request, Duration::ZERO);
    }

    fn is_cancelled(&self) -> bool {
        self.cancel_after.is_some_and(|n| self.calls.len() >= n)
    }

    fn leftovers(&self) -> Option<&super::leftovers::Leftovers> {
        self.leftovers.as_ref()
    }

    fn call(&mut self, request: Value, _timeout: Duration) -> Result<Value, ExportError> {
        let method = request["@type"].as_str().unwrap().to_string();
        self.calls.push(method.clone());
        if self.cancel_after.is_some_and(|n| self.calls.len() > n) {
            return Err(ExportError::Cancelled);
        }
        Ok(match method.as_str() {
            "getChatHistory" => {
                if self.fail_history {
                    return Ok(json!({"@type": "error", "code": 500, "message": "boom"}));
                }
                let from = request["from_message_id"].as_i64().unwrap();
                let limit = if self.page_size == 0 {
                    100
                } else {
                    self.page_size
                };
                // `from_message_id` is part of the answer, as in TDLib.
                let take = if from == 0 { limit } else { limit + 1 };
                let page: Vec<Value> = self
                    .messages
                    .iter()
                    .filter(|m| from == 0 || m["id"].as_i64().unwrap() <= from)
                    .take(take)
                    .cloned()
                    .collect();
                json!({"@type": "messages", "messages": page})
            }
            "getChatMessageByDate" => {
                let date = request["date"].as_i64().unwrap();
                match self
                    .messages
                    .iter()
                    .find(|m| m["date"].as_i64().unwrap() <= date)
                {
                    Some(m) => m.clone(),
                    None => json!({"@type": "error", "code": 404, "message": "Message not found"}),
                }
            }
            "getUser" => {
                let id = request["user_id"].as_i64().unwrap();
                json!({"@type": "user", "id": id, "first_name": format!("User{id}"), "last_name": "<x>"})
            }
            "getChat" => json!({"@type": "chat", "title": "Some Channel"}),
            "getFile" => {
                let id = request["file_id"].as_i64().unwrap();
                let cached = json!({"is_downloading_completed": true});
                let local = self.local.get(&id).cloned().unwrap_or(cached);
                json!({"@type": "file", "id": id, "local": local,
                    "remote": {"id": format!("remote-{id}")}})
            }
            "deleteFile" => {
                let id = request["file_id"].as_i64().unwrap();
                if let Some(path) = self.files.get(&id) {
                    std::fs::remove_file(path).unwrap();
                }
                self.local.insert(id, not_local());
                json!({"@type": "ok"})
            }
            "downloadFile" => {
                let id = request["file_id"].as_i64().unwrap();
                match self.files.get(&id) {
                    Some(path) => {
                        let local = json!({"path": path.to_string_lossy(),
                            "is_downloading_completed": true, "is_downloading_active": false});
                        let now = self.touched_after_download.clone();
                        self.local.insert(id, now.unwrap_or_else(|| local.clone()));
                        json!({"@type": "file", "id": id,
                            "size": std::fs::metadata(path).map(|m| m.len()).unwrap_or(0),
                            "local": local})
                    }
                    None => json!({"@type": "file", "id": id, "size": 1,
                        "local": {"path": "", "is_downloading_completed": false}}),
                }
            }
            other => panic!("unexpected request {other}"),
        })
    }
}

struct Run {
    out: PathBuf,
    result: Result<super::worker::Summary, ExportError>,
    last: Progress,
}

fn export(fake: &mut Fake, settings: &ExportSettings, files_root: &Path) -> Run {
    let out = create_folder(&settings.dir, &settings.title, "2023-11-14").unwrap();
    export_into(fake, settings, files_root, out)
}

fn export_into(fake: &mut Fake, settings: &ExportSettings, files_root: &Path, out: PathBuf) -> Run {
    let last = std::cell::RefCell::new(Progress::starting());
    let report = |p: Progress| *last.borrow_mut() = p;
    let loaded = super::loaded_files::LoadedFiles::default();
    let job = Job {
        settings,
        files_root,
        offset_at: &|_| 0,
        report: &report,
        me: None,
        loaded: &loaded,
    };
    let result = run(&job, fake, &out);
    Run {
        out,
        result,
        last: last.into_inner(),
    }
}

fn settings(dir: &Path) -> ExportSettings {
    ExportSettings::new(7, "Test Chat".into(), dir.to_path_buf())
}

/// `n` text messages, ids 1..=n, one per minute, newest first.
fn history(n: i64) -> Vec<Value> {
    (1..=n)
        .rev()
        .map(|id| text_message(id, 1_700_000_000 + id * 60, id % 2 == 0, &format!("m{id}")))
        .collect()
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap()
}

#[test]
fn long_chats_split_into_numbered_files_oldest_first() {
    let dir = scratch("paging");
    let mut messages = history(2300);
    // A reply from the last page to a message on the first page.
    messages[0]["reply_to"] = json!({"@type": "messageReplyToMessage", "message_id": 5});
    let mut fake = Fake::new(messages);
    let mut s = settings(&dir);
    s.format = ExportFormat::HtmlAndJson;
    let run = export(&mut fake, &s, &dir);
    let summary = run.result.unwrap();
    assert_eq!(summary.messages, 2300);
    assert_eq!(summary.pages, 3);
    assert!(run.out.join("messages.html").exists());
    assert!(run.out.join("messages2.html").exists());
    assert!(run.out.join("messages3.html").exists());
    assert!(run.out.join("css/style.css").exists());
    let first = read(&run.out.join("messages.html"));
    assert!(first.contains("id=\"message1\""));
    assert!(first.contains("id=\"message1000\""));
    assert!(!first.contains("id=\"message1001\""));
    assert!(first.contains("href=\"messages2.html\">Next messages"));
    // Oldest first within a file.
    assert!(first.find("id=\"message1\"").unwrap() < first.find("id=\"message2\"").unwrap());
    let last = read(&run.out.join("messages3.html"));
    assert!(last.contains("id=\"message2300\""));
    assert!(last.contains("href=\"messages.html#go_to_message5\">this message"));
    assert!(last.contains("href=\"messages2.html\">Previous messages"));
    let result: Value = serde_json::from_str(&read(&run.out.join("result.json"))).unwrap();
    assert_eq!(result["messages"].as_array().unwrap().len(), 2300);
    assert_eq!(result["messages"][0]["id"], 1);
    assert_eq!(result["messages"][2299]["id"], 2300);
    // The spool is gone.
    assert!(!run.out.join(".spool").exists());
    // The last report is the final write; the thread's wrapper marks it done.
    assert_eq!(run.last.stage, Stage::Writing);
    assert_eq!(
        (run.last.done, run.last.total),
        (summary.messages, summary.messages)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn json_only_writes_no_html() {
    let dir = scratch("json-only");
    let mut fake = Fake::new(history(3));
    let mut s = settings(&dir);
    s.format = ExportFormat::Json;
    let run = export(&mut fake, &s, &dir);
    run.result.unwrap();
    assert!(run.out.join("result.json").exists());
    assert!(!run.out.join("messages.html").exists());
    assert!(!run.out.join("css").exists());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn date_range_keeps_from_inclusive_till_exclusive_and_stops_paging() {
    let dir = scratch("range");
    // Dates 1_700_000_060 .. 1_700_006_000 at one per minute (ids 1..=100).
    let mut fake = Fake::new(history(100));
    fake.page_size = 10;
    let mut s = settings(&dir);
    s.format = ExportFormat::Json;
    s.from = 1_700_000_000 + 20 * 60;
    s.till = 1_700_000_000 + 30 * 60;
    let run = export(&mut fake, &s, &dir);
    run.result.unwrap();
    let result: Value = serde_json::from_str(&read(&run.out.join("result.json"))).unwrap();
    let ids: Vec<i64> = result["messages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["id"].as_i64().unwrap())
        .collect();
    assert_eq!(ids, (20..30).collect::<Vec<_>>());
    // It jumped to the end date instead of paging from the newest message,
    // and stopped once a page passed the start.
    assert_eq!(fake.count("getChatMessageByDate"), 1);
    assert!(fake.count("getChatHistory") <= 3, "{:?}", fake.calls);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_range_with_nothing_before_it_exports_an_empty_chat() {
    let dir = scratch("range-empty");
    let mut fake = Fake::new(history(5));
    let mut s = settings(&dir);
    s.till = 1_000;
    let run = export(&mut fake, &s, &dir);
    assert_eq!(run.result.unwrap().messages, 0);
    assert!(run.out.join("messages.html").exists());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn short_pages_do_not_end_the_export_early() {
    let dir = scratch("short-pages");
    let mut fake = Fake::new(history(25));
    fake.page_size = 1;
    let mut s = settings(&dir);
    s.format = ExportFormat::Json;
    let run = export(&mut fake, &s, &dir);
    assert_eq!(run.result.unwrap().messages, 25);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn only_my_messages_filters_other_senders() {
    let dir = scratch("mine");
    let mut fake = Fake::new(history(10));
    let mut s = settings(&dir);
    s.format = ExportFormat::Json;
    s.only_mine = true;
    let run = export(&mut fake, &s, &dir);
    assert_eq!(run.result.unwrap().messages, 5);
    let result: Value = serde_json::from_str(&read(&run.out.join("result.json"))).unwrap();
    assert!(
        result["messages"]
            .as_array()
            .unwrap()
            .iter()
            .all(|m| m["outgoing"] == true)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn sender_names_come_from_tdlib_once_per_person() {
    let dir = scratch("names");
    let mut fake = Fake::new(history(6));
    let s = settings(&dir);
    let run = export(&mut fake, &s, &dir);
    run.result.unwrap();
    assert_eq!(fake.count("getUser"), 2);
    let html = read(&run.out.join("messages.html"));
    assert!(html.contains("User2 &lt;x&gt;"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn protected_and_self_destructing_messages_never_reach_the_files() {
    let dir = scratch("protected");
    let mut messages = history(3);
    messages[0]["can_be_saved"] = json!(false);
    messages[1]["self_destruct_type"] = json!({"@type": "messageSelfDestructTypeImmediately"});
    let mut fake = Fake::new(messages);
    let s = settings(&dir);
    let run = export(&mut fake, &s, &dir);
    assert_eq!(run.result.unwrap().messages, 1);
    let html = read(&run.out.join("messages.html"));
    assert!(html.contains(">m1<") && !html.contains(">m2<") && !html.contains(">m3<"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn media_follows_kind_and_size_limits() {
    let dir = scratch("media");
    let tdlib_files = dir.join("td");
    std::fs::create_dir_all(&tdlib_files).unwrap();
    std::fs::write(tdlib_files.join("p.jpg"), b"jpeg").unwrap();
    std::fs::write(tdlib_files.join("v.mp4"), b"video").unwrap();
    let outside = scratch("outside");
    std::fs::write(outside.join("secret.jpg"), b"nope").unwrap();

    let mut sticker = photo_message(5, 1_700_000_500, 50, 5);
    sticker["content"] = json!({"@type": "messageSticker",
        "sticker": {"emoji": "x", "sticker": {"id": 50, "size": 5}}});
    let messages = vec![
        sticker,                                         // kind off
        photo_message(4, 1_700_000_400, 40, 500),        // outside TDLib's folder
        video_message(3, 1_700_000_300, 30, 50_000_000), // too large
        video_message(2, 1_700_000_200, 20, 5),
        photo_message(1, 1_700_000_100, 10, 4),
    ];
    let mut fake = Fake::new(messages);
    fake.files.insert(10, tdlib_files.join("p.jpg"));
    fake.files.insert(20, tdlib_files.join("v.mp4"));
    fake.files.insert(40, outside.join("secret.jpg"));

    let mut s = settings(&dir);
    s.format = ExportFormat::HtmlAndJson;
    s.media = MediaTypes::none()
        .with(MediaKind::Photo, true)
        .with(MediaKind::Video, true);
    s.size_limit = 1024 * 1024;
    let run = export(&mut fake, &s, &tdlib_files);
    let summary = run.result.unwrap();
    assert_eq!((summary.files, summary.skipped), (2, 3));
    assert_eq!(
        std::fs::read(run.out.join("photos/photo_1@14-11-2023_22-15-00.jpg")).unwrap(),
        b"jpeg"
    );
    assert_eq!(
        std::fs::read(run.out.join("video_files/clip.mp4")).unwrap(),
        b"video"
    );
    // Only one photo was copied; the one outside TDLib's folder never was.
    assert_eq!(
        std::fs::read_dir(run.out.join("photos")).unwrap().count(),
        1
    );
    assert!(!run.out.join("stickers").exists());
    // The too-large video and the sticker were never asked for.
    assert_eq!(fake.count("downloadFile"), 3);
    let html = read(&run.out.join("messages.html"));
    assert!(html.contains("src=\"photos/photo_1@14-11-2023_22-15-00.jpg\""));
    assert!(html.contains("src=\"video_files/clip.mp4\""));
    assert!(html.contains("(Video exceeds maximum size."));
    assert!(html.contains("(Sticker not included."));
    assert!(html.contains("(Photo unavailable, please try again later)"));
    let result: Value = serde_json::from_str(&read(&run.out.join("result.json"))).unwrap();
    assert_eq!(
        result["messages"][0]["photo"],
        "photos/photo_1@14-11-2023_22-15-00.jpg"
    );
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&outside);
}

#[test]
fn cancelling_stops_the_export_and_removes_the_spool() {
    let dir = scratch("cancel");
    let mut fake = Fake::new(history(300));
    fake.cancel_after = Some(4);
    let s = settings(&dir);
    let run = export(&mut fake, &s, &dir);
    assert!(matches!(run.result, Err(ExportError::Cancelled)));
    assert!(!run.out.join(".spool").exists());
    assert!(!run.out.join("messages.html").exists());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_tdlib_error_fails_the_export_with_its_message() {
    let dir = scratch("error");
    let mut fake = Fake::new(history(3));
    fake.fail_history = true;
    let s = settings(&dir);
    let run = export(&mut fake, &s, &dir);
    match run.result {
        Err(ExportError::Tdlib(message)) => assert_eq!(message, "boom"),
        other => panic!("expected a TDLib error, got {other:?}"),
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn export_folders_never_overwrite_each_other() {
    let dir = scratch("folders");
    let first = create_folder(&dir, "My/Chat", "2023-11-14").unwrap();
    let second = create_folder(&dir, "My/Chat", "2023-11-14").unwrap();
    assert_ne!(first, second);
    assert_eq!(first.file_name().unwrap(), "ChatExport_My_Chat_2023-11-14");
    assert_eq!(
        second.file_name().unwrap(),
        "ChatExport_My_Chat_2023-11-14 (2)"
    );
    let untitled = create_folder(&dir, "", "2023-11-14").unwrap();
    assert_eq!(untitled.file_name().unwrap(), "ChatExport_2023-11-14");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn progress_labels_and_fraction() {
    let mut p = Progress::starting();
    assert_eq!(p.label(), "Starting the export…");
    assert_eq!(p.fraction(), None);
    p.stage = Stage::Collecting;
    p.done = 120;
    assert_eq!(p.label(), "Reading messages… 120");
    p.stage = Stage::Writing;
    p.total = 200;
    p.done = 50;
    p.files = 3;
    assert_eq!(p.label(), "Saving messages… 50 of 200 · 3 files");
    assert_eq!(p.fraction(), Some(0.25));
    assert!(!p.is_settled());
    p.stage = Stage::Cancelled;
    assert!(p.is_settled());
}

#[test]
fn thumbnails_are_saved_next_to_their_file_and_used_as_posters() {
    let dir = scratch("thumbs");
    let td = dir.join("td");
    std::fs::create_dir_all(&td).unwrap();
    std::fs::write(td.join("v.mp4"), b"video").unwrap();
    std::fs::write(td.join("t.jpg"), b"thumb").unwrap();
    std::fs::write(td.join("d.pdf"), b"doc").unwrap();
    let mut video = video_message(2, 1_700_000_200, 20, 5);
    video["content"]["video"]["thumbnail"] = json!({"format": {"@type": "thumbnailFormatJpeg"},
        "width": 90, "height": 50, "file": {"id": 21, "size": 5}});
    video["content"]["video"]["width"] = json!(1280);
    video["content"]["video"]["height"] = json!(720);
    // A big document: left out, so its thumbnail is never fetched either.
    let doc = json!({"id": 1, "date": 1_700_000_100, "is_outgoing": false, "can_be_saved": true,
        "sender_id": {"@type": "messageSenderUser", "user_id": 2},
        "content": {"@type": "messageDocument", "caption": {"text": ""},
            "document": {"file_name": "d.pdf", "mime_type": "application/pdf",
                "thumbnail": {"format": {"@type": "thumbnailFormatJpeg"}, "file": {"id": 31, "size": 5}},
                "document": {"id": 30, "size": 99_000_000}}}});
    let mut fake = Fake::new(vec![video, doc]);
    fake.files.insert(20, td.join("v.mp4"));
    fake.files.insert(21, td.join("t.jpg"));
    fake.files.insert(30, td.join("d.pdf"));
    fake.files.insert(31, td.join("t.jpg"));
    let mut s = settings(&dir);
    s.format = ExportFormat::HtmlAndJson;
    s.media = MediaTypes::all();
    let run = export(&mut fake, &s, &td);
    let summary = run.result.unwrap();
    assert_eq!((summary.files, summary.skipped), (1, 1));
    assert_eq!(
        std::fs::read(run.out.join("video_files/clip.mp4_thumb.jpg")).unwrap(),
        b"thumb"
    );
    assert!(!run.out.join("files").exists());
    // One download for the video, one for its thumbnail; none for the doc.
    assert_eq!(fake.count("downloadFile"), 2);
    let html = read(&run.out.join("messages.html"));
    assert!(html.contains("poster=\"video_files/clip.mp4_thumb.jpg\""));
    assert!(html.contains("width=\"520\" height=\"293\""));
    assert!(run.out.join("js/script.js").exists());
    let result: Value = serde_json::from_str(&read(&run.out.join("result.json"))).unwrap();
    assert_eq!(
        result["messages"][1]["thumbnail"],
        "video_files/clip.mp4_thumb.jpg"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_empty_chat_says_so_on_its_page() {
    let dir = scratch("empty");
    let mut fake = Fake::new(Vec::new());
    let s = settings(&dir);
    let run = export(&mut fake, &s, &dir);
    assert_eq!(run.result.unwrap().messages, 0);
    assert!(read(&run.out.join("messages.html")).contains("No exported messages"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn replies_on_the_same_page_jump_with_the_script() {
    let dir = scratch("reply-same-page");
    let mut messages = history(3);
    messages[0]["reply_to"] = json!({"@type": "messageReplyToMessage", "message_id": 1});
    let mut fake = Fake::new(messages);
    let s = settings(&dir);
    let run = export(&mut fake, &s, &dir);
    run.result.unwrap();
    let html = read(&run.out.join("messages.html"));
    assert!(html.contains(
        "<a href=\"#go_to_message1\" onclick=\"return GoToMessage(1)\">this message</a>"
    ));
    let _ = std::fs::remove_dir_all(&dir);
}

fn not_local() -> Value {
    json!({"is_downloading_completed": false, "is_downloading_active": false,
        "downloaded_size": 0})
}

fn one_photo(dir: &Path) -> (Fake, ExportSettings, PathBuf) {
    let td = dir.join("td");
    std::fs::create_dir_all(&td).unwrap();
    std::fs::write(td.join("p.jpg"), b"jpeg").unwrap();
    let mut fake = Fake::new(vec![photo_message(1, 1_700_000_100, 10, 4)]);
    fake.files.insert(10, td.join("p.jpg"));
    let mut s = settings(dir);
    s.media = MediaTypes::none().with(MediaKind::Photo, true);
    (fake, s, td)
}

const PHOTO: &str = "photos/photo_1@14-11-2023_22-15-00.jpg";

#[test]
fn a_file_only_the_export_wanted_leaves_the_cache_and_stays_in_the_export() {
    let dir = scratch("moved");
    let (mut fake, s, td) = one_photo(&dir);
    fake.local.insert(10, not_local());
    let run = export(&mut fake, &s, &td);
    assert_eq!(run.result.unwrap().files, 1);
    assert_eq!(fake.count("deleteFile"), 1);
    assert!(!td.join("p.jpg").exists());
    assert_eq!(std::fs::read(run.out.join(PHOTO)).unwrap(), b"jpeg");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_file_cached_before_the_export_stays_cached_and_apart_from_the_export() {
    let dir = scratch("kept");
    let (mut fake, s, td) = one_photo(&dir);
    let run = export(&mut fake, &s, &td);
    assert_eq!(run.result.unwrap().files, 1);
    assert_eq!(fake.count("deleteFile"), 0);
    std::fs::write(run.out.join(PHOTO), b"edited").unwrap();
    assert_eq!(std::fs::read(td.join("p.jpg")).unwrap(), b"jpeg");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_file_left_in_the_cache_after_all_stays_apart_from_the_export() {
    let dir = scratch("left");
    let (mut fake, s, td) = one_photo(&dir);
    fake.local.insert(10, not_local());
    fake.touched_after_download = Some(json!({"is_downloading_completed": false,
        "is_downloading_active": true, "downloaded_size": 1}));
    let run = export(&mut fake, &s, &td);
    assert_eq!(run.result.unwrap().files, 1);
    assert_eq!(fake.count("deleteFile"), 0);
    std::fs::write(run.out.join(PHOTO), b"edited").unwrap();
    assert_eq!(std::fs::read(td.join("p.jpg")).unwrap(), b"jpeg");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_stop_during_the_copy_leaves_no_file_and_still_releases_the_cache() {
    let dir = scratch("stopped-copy");
    let (mut fake, s, td) = one_photo(&dir);
    fake.local.insert(10, not_local());
    let probe_dir = scratch("stopped-copy-probe");
    let (mut probe, probe_settings, probe_td) = one_photo(&probe_dir);
    probe.local.insert(10, not_local());
    let _ = export(&mut probe, &probe_settings, &probe_td);
    let downloaded = probe
        .calls
        .iter()
        .position(|c| c == "downloadFile")
        .unwrap();
    let _ = std::fs::remove_dir_all(&probe_dir);
    fake.cancel_after = Some(downloaded + 1);
    let leftovers = super::leftovers::Leftovers::new(dir.join("leftovers"), 1);
    fake.leftovers = Some(leftovers.clone());

    let run = export(&mut fake, &s, &td);

    assert!(matches!(run.result, Err(ExportError::Cancelled)));
    let photos = run.out.join("photos");
    let left: Vec<_> = std::fs::read_dir(&photos).map_or(Vec::new(), |dir| dir.collect());
    assert!(
        left.is_empty(),
        "nothing sits under the photo's name: {left:?}"
    );
    assert_eq!(fake.count("deleteFile"), 1, "the delete still went out");
    let next_run = super::leftovers::Leftovers::new(dir.join("leftovers"), 2);
    let listed: Vec<_> = next_run.earlier().into_iter().map(|e| e.entry).collect();
    assert_eq!(
        listed,
        [super::leftovers::Entry {
            remote_id: "remote-10".into(),
            path: Some(td.join("p.jpg").canonicalize().unwrap()),
        }],
        "listed for the next start in case the delete never lands"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_file_that_could_not_be_saved_still_leaves_the_cache() {
    let dir = scratch("unsaved");
    let (mut fake, s, td) = one_photo(&dir);
    fake.local.insert(10, not_local());
    let out = create_folder(&s.dir, &s.title, "2023-11-14").unwrap();
    std::fs::write(out.join("photos"), b"a file where the folder goes").unwrap();
    let run = export_into(&mut fake, &s, &td, out);
    assert!(matches!(run.result, Err(ExportError::Io(_))));
    assert_eq!(fake.count("deleteFile"), 1);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_file_something_else_is_fetching_stays_cached() {
    for local in [
        json!({"is_downloading_completed": false, "is_downloading_active": true,
            "downloaded_size": 0}),
        json!({"is_downloading_completed": false, "is_downloading_active": false,
            "downloaded_size": 1024}),
    ] {
        let dir = scratch("shared");
        let (mut fake, s, td) = one_photo(&dir);
        fake.local.insert(10, local);
        let run = export(&mut fake, &s, &td);
        assert_eq!(run.result.unwrap().files, 1);
        assert_eq!(fake.count("deleteFile"), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[test]
fn a_file_found_too_large_after_its_download_leaves_the_cache() {
    let dir = scratch("too-large");
    let (mut fake, mut s, td) = one_photo(&dir);
    fake.messages = vec![photo_message(1, 1_700_000_100, 10, 0)];
    fake.local.insert(10, not_local());
    s.size_limit = 1;
    let run = export(&mut fake, &s, &td);
    assert_eq!(run.result.unwrap().skipped, 1);
    assert_eq!(fake.count("deleteFile"), 1);
    let _ = std::fs::remove_dir_all(&dir);
}

fn cancelled_download(fake: &mut Fake, td: &Path) {
    fake.cancel_after = Some(1);
    assert!(matches!(
        super::worker::fetch_file(fake, td, super::worker::FileOf { id: 10, chat: None }, None),
        Err(ExportError::Cancelled)
    ));
}

#[test]
fn cancelling_during_a_download_stops_the_download() {
    let dir = scratch("cancel-download");
    let (mut fake, _, td) = one_photo(&dir);
    fake.local.insert(10, not_local());
    cancelled_download(&mut fake, &td);
    assert_eq!(fake.count("cancelDownloadFile"), 1);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn cancelling_leaves_a_download_something_else_asked_for() {
    let dir = scratch("cancel-shared");
    let (mut fake, _, td) = one_photo(&dir);
    fake.local.insert(
        10,
        json!({"is_downloading_completed": false, "is_downloading_active": true,
            "downloaded_size": 0}),
    );
    cancelled_download(&mut fake, &td);
    assert_eq!(fake.count("cancelDownloadFile"), 0);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn what_the_worker_publishes_wakes_the_poll_pass() {
    let wake = crate::wake::Signal::new();
    let slot = std::sync::Mutex::new(Progress::starting());
    super::handle::publish(&slot, &wake, |progress| progress.stage = Stage::Cancelled);
    assert_eq!(wake.epoch(), 1);
    assert!(
        slot.lock()
            .is_ok_and(|p| matches!(p.stage, Stage::Cancelled))
    );
}

#[test]
fn a_file_that_occurs_again_is_saved_once_and_linked_from_both_messages() {
    let dir = scratch("repeat");
    let (mut fake, s, td) = one_photo(&dir);
    fake.messages = vec![
        photo_message(2, 1_700_000_160, 10, 4),
        photo_message(1, 1_700_000_100, 10, 4),
    ];
    fake.local.insert(10, not_local());
    let run = export(&mut fake, &s, &td);
    assert_eq!(run.result.unwrap().files, 2);
    assert_eq!(fake.count("downloadFile"), 1);
    assert_eq!(
        std::fs::read_dir(run.out.join("photos")).unwrap().count(),
        1
    );
    let html = read(&run.out.join("messages.html"));
    assert!(html.matches(PHOTO).count() >= 2, "both messages show it");
    assert!(!html.contains("photo_2@"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_file_something_else_started_using_during_the_export_stays_cached() {
    let dir = scratch("touched");
    let (mut fake, s, td) = one_photo(&dir);
    fake.local.insert(10, not_local());
    fake.touched_after_download = Some(json!({"path": td.join("p.jpg").to_string_lossy(),
        "is_downloading_completed": true, "is_downloading_active": true}));
    let run = export(&mut fake, &s, &td);
    assert_eq!(run.result.unwrap().files, 1);
    assert_eq!(fake.count("deleteFile"), 0);
    assert_eq!(std::fs::read(td.join("p.jpg")).unwrap(), b"jpeg");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn cancelling_after_a_download_still_releases_the_cache_copy() {
    let dir = scratch("cancel-release");
    let (mut fake, _, td) = one_photo(&dir);
    fake.local.insert(10, not_local());
    let fetched = super::worker::fetch_file(
        &mut fake,
        &td,
        super::worker::FileOf { id: 10, chat: None },
        None,
    )
    .unwrap()
    .unwrap();
    fake.cancel_after = Some(fake.calls.len());
    assert!(matches!(
        fetched.release(&mut fake),
        Err(ExportError::Cancelled)
    ));
    assert_eq!(fake.count("deleteFile"), 1);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_repeated_file_with_another_thumbnail_keeps_both_thumbnails() {
    let dir = scratch("repeat-thumb");
    let td = dir.join("td");
    std::fs::create_dir_all(&td).unwrap();
    std::fs::write(td.join("v.mp4"), b"video").unwrap();
    std::fs::write(td.join("t1.jpg"), b"first").unwrap();
    std::fs::write(td.join("t2.jpg"), b"second").unwrap();
    let with_thumb = |id, date, thumb| {
        let mut message = video_message(id, date, 20, 5);
        message["content"]["video"]["thumbnail"] = json!({"format": {"@type": "thumbnailFormatJpeg"},
            "width": 90, "height": 50, "file": {"id": thumb, "size": 5}});
        message
    };
    let mut fake = Fake::new(vec![
        with_thumb(2, 1_700_000_200, 22),
        with_thumb(1, 1_700_000_100, 21),
    ]);
    fake.files.insert(20, td.join("v.mp4"));
    fake.files.insert(21, td.join("t1.jpg"));
    fake.files.insert(22, td.join("t2.jpg"));
    let mut s = settings(&dir);
    s.format = ExportFormat::Json;
    s.media = MediaTypes::all();
    let run = export(&mut fake, &s, &td);
    run.result.unwrap();
    let result: Value = serde_json::from_str(&read(&run.out.join("result.json"))).unwrap();
    let first = result["messages"][0]["thumbnail"].as_str().unwrap();
    let second = result["messages"][1]["thumbnail"].as_str().unwrap();
    assert_eq!(result["messages"][1]["file"], result["messages"][0]["file"]);
    assert_eq!(std::fs::read(run.out.join(first)).unwrap(), b"first");
    assert_eq!(std::fs::read(run.out.join(second)).unwrap(), b"second");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_link_hears_its_worker_end_at_once() {
    let (export, worker) = super::ChatExport::scripted();
    assert!(!export.link.worker_ended());
    drop(worker);
    assert!(export.link.worker_ended());
    let far_off = std::time::Instant::now() + Duration::from_secs(3600);
    assert!(export.link.wait_ended(far_off), "no wait once it ended");
}
