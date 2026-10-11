//! The export itself, as a blocking routine that talks to TDLib through the
//! [`Tdlib`] trait. It runs on its own thread (see `handle.rs`), so paging,
//! downloading and writing never touch the UI thread.
//!
//! Two passes, because TDLib pages newest first and the files read oldest
//! first. Pass one pages history, applies the date range and sender
//! choices, resolves names and spools each page to a small file. Pass two
//! reads the spool backwards, copies media within the limits and writes the
//! HTML pages and `result.json` in order.
use super::html::{self, Item, MESSAGES_PER_FILE, Outcome};
use super::json::{self, JsonWriter};
use super::model::{Body, ExportMessage, MediaRef, Origin, Who, parse_message};
use super::names::Namer;
use super::settings::{ExportSettings, Skip};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::fs;
use std::io::{self, BufWriter};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// `getChatHistory` page size (TDLib's maximum).
pub const PAGE_LIMIT: i64 = 100;
const QUERY_TIMEOUT: Duration = Duration::from_secs(300);
/// A synchronous `downloadFile` can legitimately take a long time.
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(6 * 3600);

#[derive(Debug)]
pub enum ExportError {
    Cancelled,
    Io(io::Error),
    Tdlib(String),
}

impl std::fmt::Display for ExportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cancelled => f.write_str("export cancelled"),
            Self::Io(err) => write!(f, "could not write the export: {err}"),
            Self::Tdlib(msg) => write!(f, "Telegram request failed: {msg}"),
        }
    }
}

impl From<io::Error> for ExportError {
    fn from(err: io::Error) -> Self {
        Self::Io(err)
    }
}

impl From<serde_json::Error> for ExportError {
    fn from(err: serde_json::Error) -> Self {
        Self::Io(err.into())
    }
}

/// A blocking TDLib connection. `call` returns the answer object, which
/// may be a TDLib `error`; it fails only on cancel or timeout.
pub trait Tdlib {
    fn call(&mut self, request: Value, timeout: Duration) -> Result<Value, ExportError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Summary {
    pub folder: PathBuf,
    pub messages: usize,
    pub files: usize,
    pub skipped: usize,
    pub pages: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stage {
    Starting,
    Collecting,
    Writing,
    Finished(Summary),
    Failed(String),
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Progress {
    pub stage: Stage,
    pub done: usize,
    /// Known once collecting ends.
    pub total: usize,
    pub files: usize,
    pub skipped: usize,
}

impl Progress {
    pub fn starting() -> Self {
        Self {
            stage: Stage::Starting,
            done: 0,
            total: 0,
            files: 0,
            skipped: 0,
        }
    }

    pub fn is_settled(&self) -> bool {
        matches!(
            self.stage,
            Stage::Finished(_) | Stage::Failed(_) | Stage::Cancelled
        )
    }

    /// One line for the progress box.
    pub fn label(&self) -> String {
        match &self.stage {
            Stage::Starting => "Starting the export…".into(),
            Stage::Collecting => format!("Reading messages… {}", self.done),
            Stage::Writing => format!(
                "Saving messages… {} of {} · {} files",
                self.done, self.total, self.files
            ),
            Stage::Finished(summary) => format!(
                "Exported {} messages and {} files",
                summary.messages, summary.files
            ),
            Stage::Failed(reason) => format!("Export failed: {reason}"),
            Stage::Cancelled => "Export cancelled".into(),
        }
    }

    /// 0.0..=1.0 while writing, `None` while the total is unknown.
    pub fn fraction(&self) -> Option<f32> {
        match self.stage {
            Stage::Writing if self.total > 0 => Some(self.done as f32 / self.total as f32),
            Stage::Finished(_) => Some(1.0),
            _ => None,
        }
    }
}

/// Everything the routine needs besides TDLib.
pub struct Job<'a> {
    pub settings: &'a ExportSettings,
    /// TDLib's files directory; only files under it are copied.
    pub files_root: &'a Path,
    pub offset_at: &'a dyn Fn(i64) -> i32,
    pub report: &'a dyn Fn(Progress),
}

fn query<T: Tdlib>(tdlib: &mut T, request: Value) -> Result<Value, ExportError> {
    let answer = tdlib.call(request, QUERY_TIMEOUT)?;
    if answer["@type"] == "error" {
        return Err(ExportError::Tdlib(
            answer["message"].as_str().unwrap_or("unknown error").into(),
        ));
    }
    Ok(answer)
}

/// Display names, fetched once per user or chat.
#[derive(Default)]
struct Names {
    known: HashMap<Who, String>,
}

impl Names {
    fn name<T: Tdlib>(&mut self, tdlib: &mut T, who: Who) -> Result<String, ExportError> {
        if let Some(name) = self.known.get(&who) {
            return Ok(name.clone());
        }
        let request = match who {
            Who::User(id) => json!({"@type": "getUser", "user_id": id}),
            Who::Chat(id) => json!({"@type": "getChat", "chat_id": id}),
        };
        let name = match tdlib.call(request, QUERY_TIMEOUT)? {
            answer if answer["@type"] == "error" => "Unknown".to_string(),
            answer if matches!(who, Who::Chat(_)) => {
                answer["title"].as_str().unwrap_or("Unknown").to_string()
            }
            answer => {
                let first = answer["first_name"].as_str().unwrap_or_default();
                let last = answer["last_name"].as_str().unwrap_or_default();
                let full = format!("{first} {last}").trim().to_string();
                if full.is_empty() {
                    "Deleted Account".to_string()
                } else {
                    full
                }
            }
        };
        self.known.insert(who, name.clone());
        Ok(name)
    }

    fn fill<T: Tdlib>(
        &mut self,
        tdlib: &mut T,
        msg: &mut ExportMessage,
    ) -> Result<(), ExportError> {
        if let Some(who) = msg.sender {
            msg.from = Some(self.name(tdlib, who)?);
        }
        if let Some(Origin::Who(who)) = msg.forward {
            msg.forwarded_from = Some(self.name(tdlib, who)?);
        }
        if let Body::Service { users, names, .. } = &mut msg.body {
            let list = users.clone();
            for id in list {
                names.push(self.name(tdlib, Who::User(id))?);
            }
        }
        Ok(())
    }
}

fn spool_name(index: usize) -> String {
    format!("{index:08}.json")
}

/// Pass one. Returns the ids of the kept messages, newest first, and the
/// number of spool chunks.
fn collect<T: Tdlib>(
    job: &Job<'_>,
    tdlib: &mut T,
    spool: &Path,
    progress: &mut Progress,
) -> Result<(Vec<i64>, usize), ExportError> {
    let settings = job.settings;
    let mut names = Names::default();
    // Start at the last message before the range's end date.
    let mut from = 0;
    if settings.till > 0 {
        let date = (settings.till - 1).clamp(0, i64::from(i32::MAX));
        match tdlib.call(
            json!({"@type": "getChatMessageByDate", "chat_id": settings.chat_id, "date": date}),
            QUERY_TIMEOUT,
        )? {
            answer if answer["@type"] == "error" => {
                // Nothing at or before that date.
                return Ok((Vec::new(), 0));
            }
            answer => from = answer["id"].as_i64().unwrap_or(0),
        }
    }
    let mut oldest = 0i64;
    let mut ids = Vec::new();
    let mut chunks = 0;
    loop {
        let page = query(
            tdlib,
            json!({"@type": "getChatHistory", "chat_id": settings.chat_id, "from_message_id": from,
                "offset": 0, "limit": PAGE_LIMIT, "only_local": false}),
        )?;
        let raw = page["messages"].as_array().cloned().unwrap_or_default();
        // TDLib may answer with fewer messages than asked; only an empty
        // page (or one with nothing new) ends the history.
        let mut kept = Vec::new();
        let mut progressed = false;
        let mut past_range = false;
        for value in &raw {
            let Some(id) = value["id"].as_i64() else {
                continue;
            };
            if oldest != 0 && id >= oldest {
                continue;
            }
            oldest = id;
            progressed = true;
            let Some(mut msg) = parse_message(value) else {
                continue;
            };
            if settings.before_range(msg.date) {
                past_range = true;
                continue;
            }
            if settings.skips_date(msg.date) || (settings.only_mine && !msg.outgoing) {
                continue;
            }
            names.fill(tdlib, &mut msg)?;
            kept.push(msg);
        }
        if !kept.is_empty() {
            ids.extend(kept.iter().map(|m| m.id));
            fs::write(spool.join(spool_name(chunks)), serde_json::to_vec(&kept)?)?;
            chunks += 1;
            progress.done = ids.len();
            (job.report)(progress.clone());
        }
        if !progressed || past_range {
            break;
        }
        from = oldest;
    }
    Ok((ids, chunks))
}

fn read_chunk(spool: &Path, index: usize) -> Result<Vec<ExportMessage>, ExportError> {
    let bytes = fs::read(spool.join(spool_name(index)))?;
    let mut messages: Vec<ExportMessage> = serde_json::from_slice(&bytes)?;
    messages.reverse();
    Ok(messages)
}

/// Copy one message's file into the export, or say why not.
fn save_file<T: Tdlib>(
    job: &Job<'_>,
    tdlib: &mut T,
    namer: &mut Namer,
    out: &Path,
    media: &MediaRef,
    date: i32,
) -> Result<Outcome, ExportError> {
    if let Some(skip) = job.settings.skip_reason(media.kind, media.size) {
        return Ok(Outcome::Skipped(skip));
    }
    if media.file_id == 0 {
        return Ok(Outcome::Skipped(Skip::Unavailable));
    }
    let answer = tdlib.call(
        json!({"@type": "downloadFile", "file_id": media.file_id, "priority": 1,
            "offset": 0, "limit": 0, "synchronous": true}),
        DOWNLOAD_TIMEOUT,
    )?;
    let local = &answer["local"];
    // The file's real size is known once TDLib has it; honor the limit then
    // too, in case the message reported none.
    let real_size = answer["size"].as_i64().unwrap_or(0);
    if answer["@type"] == "error" || local["is_downloading_completed"] != true {
        return Ok(Outcome::Skipped(Skip::Unavailable));
    }
    if real_size > job.settings.size_limit {
        return Ok(Outcome::Skipped(Skip::TooLarge));
    }
    let Some(source) = local["path"]
        .as_str()
        .and_then(|path| Path::new(path).canonicalize().ok())
    else {
        return Ok(Outcome::Skipped(Skip::Unavailable));
    };
    // Only TDLib's own files: a path outside its directory is never copied.
    let root = job.files_root.canonicalize()?;
    if !source.starts_with(&root) || !source.is_file() {
        return Ok(Outcome::Skipped(Skip::Unavailable));
    }
    let relative = namer.path(
        media,
        date,
        (job.offset_at)(i64::from(date)),
        &source.to_string_lossy(),
    );
    let target = out.join(&relative);
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(&source, &target)?;
    Ok(Outcome::Saved(relative))
}

fn flush_page(
    job: &Job<'_>,
    out: &Path,
    page: usize,
    pages: usize,
    buffer: &[(ExportMessage, Option<Outcome>)],
    ids: &[i64],
) -> Result<(), ExportError> {
    let items: Vec<Item<'_>> = buffer
        .iter()
        .map(|(message, outcome)| Item {
            message,
            outcome: outcome.as_ref(),
        })
        .collect();
    let reply_href = |id: i64| -> Option<String> {
        let position = ids.binary_search(&id).ok()?;
        let target = position / MESSAGES_PER_FILE;
        Some(if target == page {
            format!("#message{id}")
        } else {
            format!("{}#message{id}", html::messages_file(target))
        })
    };
    let page_html = html::render_page(
        &job.settings.title,
        page,
        pages,
        &items,
        job.offset_at,
        &reply_href,
    );
    fs::write(out.join(html::messages_file(page)), page_html)?;
    Ok(())
}

/// Create the export folder inside `parent`, like tdesktop's
/// `ChatExport_2023-11-14`, adding ` (2)` when it exists.
pub fn create_folder(parent: &Path, title: &str, stamp: &str) -> io::Result<PathBuf> {
    let base = {
        let clean = super::names::sanitize(title);
        let clean: String = clean.chars().take(40).collect();
        if clean.is_empty() {
            format!("ChatExport_{stamp}")
        } else {
            format!("ChatExport_{clean}_{stamp}")
        }
    };
    for n in 1.. {
        let name = if n == 1 {
            base.clone()
        } else {
            format!("{base} ({n})")
        };
        let path = parent.join(name);
        match fs::create_dir(&path) {
            Ok(()) => return Ok(path),
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(err) => return Err(err),
        }
    }
    unreachable!("the loop only exits by returning")
}

/// Run the whole export into a fresh folder under `settings.dir`.
pub fn run<T: Tdlib>(job: &Job<'_>, tdlib: &mut T, folder: &Path) -> Result<Summary, ExportError> {
    let spool = folder.join(".spool");
    fs::create_dir_all(&spool)?;
    let mut progress = Progress::starting();
    progress.stage = Stage::Collecting;
    (job.report)(progress.clone());
    let result = write_all(job, tdlib, folder, &spool, &mut progress);
    let _ = fs::remove_dir_all(&spool);
    result
}

fn write_all<T: Tdlib>(
    job: &Job<'_>,
    tdlib: &mut T,
    folder: &Path,
    spool: &Path,
    progress: &mut Progress,
) -> Result<Summary, ExportError> {
    let settings = job.settings;
    let (mut ids, chunks) = collect(job, tdlib, spool, progress)?;
    ids.reverse();
    let total = ids.len();
    let pages = total.div_ceil(MESSAGES_PER_FILE).max(1);
    progress.stage = Stage::Writing;
    progress.total = total;
    progress.done = 0;
    (job.report)(progress.clone());
    if settings.format.wants_html() {
        fs::create_dir_all(folder.join("css"))?;
        fs::write(folder.join("css/style.css"), html::STYLE)?;
    }
    let mut json_writer = if settings.format.wants_json() {
        Some(JsonWriter::start(
            BufWriter::new(fs::File::create(folder.join("result.json"))?),
            &settings.title,
            settings.chat_id,
        )?)
    } else {
        None
    };
    let mut namer = Namer::default();
    let mut buffer: Vec<(ExportMessage, Option<Outcome>)> = Vec::new();
    let mut page = 0;
    let mut files = 0;
    let mut skipped = 0;
    for chunk in (0..chunks).rev() {
        for msg in read_chunk(spool, chunk)? {
            let outcome = match &msg.body {
                Body::Media(media) => {
                    let outcome = save_file(job, tdlib, &mut namer, folder, media, msg.date)?;
                    match outcome {
                        Outcome::Saved(_) => files += 1,
                        Outcome::Skipped(_) => skipped += 1,
                    }
                    Some(outcome)
                }
                _ => None,
            };
            if let Some(writer) = json_writer.as_mut() {
                let offset = (job.offset_at)(i64::from(msg.date));
                writer.push(&json::message_json(&msg, outcome.as_ref(), offset))?;
            }
            buffer.push((msg, outcome));
            progress.done += 1;
            progress.files = files;
            progress.skipped = skipped;
            (job.report)(progress.clone());
            if buffer.len() == MESSAGES_PER_FILE {
                if settings.format.wants_html() {
                    flush_page(job, folder, page, pages, &buffer, &ids)?;
                }
                buffer.clear();
                page += 1;
            }
        }
    }
    if settings.format.wants_html() && (!buffer.is_empty() || total == 0) {
        flush_page(job, folder, page, pages, &buffer, &ids)?;
    }
    if let Some(writer) = json_writer {
        writer.finish()?;
    }
    Ok(Summary {
        folder: folder.to_path_buf(),
        messages: total,
        files,
        skipped,
        pages,
    })
}
