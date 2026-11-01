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
    /// 0 pages whole; n returns at most n messages per page.
    page_size: usize,
    calls: Vec<String>,
    /// Cancel once this many requests were made.
    cancel_after: Option<usize>,
    fail_history: bool,
}

impl Fake {
    fn new(messages: Vec<Value>) -> Self {
        Self {
            messages,
            files: HashMap::new(),
            page_size: 0,
            calls: Vec::new(),
            cancel_after: None,
            fail_history: false,
        }
    }

    fn count(&self, method: &str) -> usize {
        self.calls.iter().filter(|c| *c == method).count()
    }
}

impl Tdlib for Fake {
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
    out: PathBuf,
    result: Result<super::worker::Summary, ExportError>,
    last: Progress,
}

fn export(fake: &mut Fake, settings: &ExportSettings, files_root: &Path) -> Run {
    let out = create_folder(&settings.dir, &settings.title, "2023-11-14").unwrap();
    let last = std::cell::RefCell::new(Progress::starting());
    let report = |p: Progress| *last.borrow_mut() = p;
    let job = Job {
        settings,
        files_root,
        offset_at: &|_| 0,
        report: &report,
        me: None,
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

#[test]
fn a_worker_ending_wakes_the_poll_loop_after_it_published() {
    let progress = std::sync::Mutex::new(Stage::Starting);
    let wake = crate::wake::Signal::new();
    let seen = wake.epoch();
    crate::export::handle::publish_end(&progress, |stage| *stage = Stage::Cancelled, &wake);
    assert_ne!(wake.epoch(), seen);
    assert_eq!(*progress.lock().unwrap(), Stage::Cancelled);
}
