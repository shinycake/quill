//! The export itself, as a blocking routine that talks to TDLib through the
//! [`Tdlib`] trait. It runs on its own thread (see `handle.rs`), so paging,
//! downloading and writing never touch the UI thread.
//!
//! One chat is exported in two passes, because TDLib pages newest first and
//! the files read oldest first. Pass one (`collect.rs`) pages history,
//! applies the date range and sender choices, resolves names and spools
//! each page to a small file. Pass two reads the spool backwards, copies
//! media and thumbnails within the limits, and writes the HTML pages and
//! the JSON messages in order. A full account export (`account/`) runs the
//! same [`export_chat`] once per chat.
use super::assets;
use super::collect::{collect, read_chunk};
use super::html::{self, Item, MESSAGES_PER_FILE, Outcome, PageSpec};
use super::json::{self, JsonWriter, MessageSink};
use super::loaded_files::LoadedFiles;
use super::model::{Body, ExportMessage, MediaRef};
use super::names::Namer;
use super::settings::{ExportSettings, Skip};
use crate::json_access::JsonExt;
use serde_json::{Value, json};
use std::fs;
use std::io::{self, BufWriter};
use std::path::{Path, PathBuf};
use std::time::Duration;

pub use super::collect::PAGE_LIMIT;

pub(crate) const QUERY_TIMEOUT: Duration = Duration::from_secs(300);
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

    fn post_unanswered(&mut self, request: Value);
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

/// Everything one chat's export needs besides TDLib.
pub struct Job<'a> {
    pub settings: &'a ExportSettings,
    /// TDLib's files directory; only files under it are copied.
    pub files_root: &'a Path,
    pub offset_at: &'a dyn Fn(i64) -> i32,
    pub report: &'a dyn Fn(Progress),
    /// The viewer's user id, when known: "only my messages" then searches
    /// by sender instead of paging the whole history.
    pub me: Option<i64>,
    /// Files this export already saved, shared by every chat of an
    /// account export.
    pub loaded: &'a LoadedFiles,
}

/// A TDLib answer, or its error as an [`ExportError`].
pub(crate) fn query<T: Tdlib>(tdlib: &mut T, request: Value) -> Result<Value, ExportError> {
    let answer = tdlib.call(request, QUERY_TIMEOUT)?;
    if answer.at("@type") == "error" {
        return Err(ExportError::Tdlib(
            answer
                .at("message")
                .as_str()
                .unwrap_or("unknown error")
                .into(),
        ));
    }
    Ok(answer)
}

pub(crate) struct Fetched {
    file_id: i32,
    path: PathBuf,
    only_for_export: bool,
}

impl Fetched {
    pub(crate) fn release<T: Tdlib>(&self, tdlib: &mut T) -> Result<(), ExportError> {
        if self.only_for_export {
            ask_to_delete_cached(tdlib, self.file_id)?;
        }
        Ok(())
    }
}

fn ask_to_delete_cached<T: Tdlib>(tdlib: &mut T, file_id: i32) -> Result<(), ExportError> {
    let request = json!({"@type": "deleteFile", "file_id": file_id});
    tdlib.call(request, QUERY_TIMEOUT).map(drop)
}

fn nothing_held(file: &Value) -> bool {
    let local = file.at("local");
    file.at("@type") == "file"
        && local.at("is_downloading_completed") == false
        && local.at("is_downloading_active") == false
        && local.at("downloaded_size").as_i64() == Some(0)
}

pub(crate) fn fetch_file<T: Tdlib>(
    tdlib: &mut T,
    files_root: &Path,
    file_id: i32,
    limit: Option<i64>,
) -> Result<Result<Fetched, Skip>, ExportError> {
    if file_id == 0 {
        return Ok(Err(Skip::Unavailable));
    }
    let before = tdlib.call(
        json!({"@type": "getFile", "file_id": file_id}),
        QUERY_TIMEOUT,
    )?;
    let only_for_export = nothing_held(&before);
    let download = tdlib.call(
        json!({"@type": "downloadFile", "file_id": file_id, "priority": 1,
            "offset": 0, "limit": 0, "synchronous": true}),
        DOWNLOAD_TIMEOUT,
    );
    let answer = match download {
        Err(ExportError::Cancelled) if only_for_export => {
            tdlib.post_unanswered(json!({"@type": "cancelDownloadFile", "file_id": file_id,
                "only_if_pending": false}));
            return Err(ExportError::Cancelled);
        }
        answer => answer?,
    };
    let local = &answer.at("local");
    if answer.at("@type") == "error" || local.at("is_downloading_completed") != true {
        return Ok(Err(Skip::Unavailable));
    }
    if let Some(limit) = limit
        && answer.at("size").as_i64().unwrap_or(0) > limit
    {
        if only_for_export {
            ask_to_delete_cached(tdlib, file_id)?;
        }
        return Ok(Err(Skip::TooLarge));
    }
    let Some(path) = local
        .at("path")
        .as_str()
        .and_then(|path| Path::new(path).canonicalize().ok())
    else {
        return Ok(Err(Skip::Unavailable));
    };
    let root = files_root.canonicalize()?;
    if !path.starts_with(&root) || !path.is_file() {
        return Ok(Err(Skip::Unavailable));
    }
    Ok(Ok(Fetched {
        file_id,
        path,
        only_for_export,
    }))
}

pub(crate) fn place_into(out: &Path, relative: &str, file: &Fetched) -> io::Result<()> {
    let target = out.join(relative);
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    if file.only_for_export && fs::hard_link(&file.path, &target).is_ok() {
        return Ok(());
    }
    fs::copy(&file.path, &target)?;
    Ok(())
}

/// Copy one message's file (and its thumbnail) into the chat's folder, or
/// say why not. The thumbnail follows the file's own rules: it is fetched
/// only once the file itself was saved.
fn save_file<T: Tdlib>(
    job: &Job<'_>,
    tdlib: &mut T,
    namer: &mut Namer,
    out: &ChatOut<'_>,
    media: &MediaRef,
    date: i32,
) -> Result<Outcome, ExportError> {
    if let Some(skip) = job.settings.skip_reason(media.kind, media.size) {
        return Ok(Outcome::Skipped(skip));
    }
    let limit = Some(job.settings.size_limit);
    let offset = (job.offset_at)(i64::from(date));
    let path = match save_once(job, tdlib, out, media.file_id, limit, |source| {
        namer.path(media, date, offset, &source.path.to_string_lossy())
    })? {
        Ok(path) => path,
        Err(skip) => return Ok(Outcome::Skipped(skip)),
    };
    let thumb = match media.thumb {
        Some(preview) => save_once(job, tdlib, out, preview.file_id, None, |_| {
            namer.thumb_path(&path)
        })?
        .ok(),
        None => None,
    };
    Ok(Outcome::Saved { path, thumb })
}

/// Save TDLib file `file_id` into `out` under the name `name` picks, or
/// point at the copy this export already saved. The path is relative to
/// the chat's folder.
fn save_once<T: Tdlib>(
    job: &Job<'_>,
    tdlib: &mut T,
    out: &ChatOut<'_>,
    file_id: i32,
    limit: Option<i64>,
    name: impl FnOnce(&Fetched) -> String,
) -> Result<Result<String, Skip>, ExportError> {
    if let Some(saved) = job.loaded.find(file_id) {
        return Ok(Ok(out.from_folder(&saved)));
    }
    let source = match fetch_file(tdlib, job.files_root, file_id, limit)? {
        Ok(source) => source,
        Err(skip) => return Ok(Err(skip)),
    };
    let path = name(&source);
    place_into(out.folder, &path, &source)?;
    source.release(tdlib)?;
    job.loaded.save(file_id, out.from_root(&path));
    Ok(Ok(path))
}

/// Where one chat's files go and how its pages link out.
#[derive(Debug, Clone, Copy)]
pub struct ChatOut<'a> {
    /// The chat's folder: `messages.html` and media folders live here.
    pub folder: &'a Path,
    /// A scratch folder for the spool; emptied when the chat is done.
    pub spool: &'a Path,
    /// From `folder` to the export root (`""` or `"../../"`).
    pub base: &'a str,
    /// The page header's back link, relative to the export root.
    pub back: Option<&'a str>,
    /// Prefix for file paths in the JSON, which are relative to the export
    /// root (`chats/chat_NN/` in an account export, `""` for a chat).
    pub json_prefix: &'a str,
    /// Write a page saying "No exported messages" for an empty chat
    /// (tdesktop does for a chat export, and leaves empty chats of an
    /// account export without pages).
    pub page_when_empty: bool,
}

impl ChatOut<'_> {
    /// `root_relative`, a path from the export root, as a path from this
    /// chat's folder.
    fn from_folder(&self, root_relative: &str) -> String {
        match root_relative.strip_prefix(self.json_prefix) {
            Some(inside) => inside.to_string(),
            None => format!("{}{root_relative}", self.base),
        }
    }

    /// A path from this chat's folder as a path from the export root.
    fn from_root(&self, folder_relative: &str) -> String {
        match folder_relative.strip_prefix(self.base) {
            Some(outside) if !self.base.is_empty() => outside.to_string(),
            _ => format!("{}{folder_relative}", self.json_prefix),
        }
    }
}

/// What one chat's export wrote.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ChatResult {
    pub messages: usize,
    pub files: usize,
    pub skipped: usize,
    pub pages: usize,
}

fn flush_page(
    job: &Job<'_>,
    out: &ChatOut<'_>,
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
            format!("#go_to_message{id}")
        } else {
            format!("{}#go_to_message{id}", html::messages_file(target))
        })
    };
    let spec = PageSpec {
        title: &job.settings.title,
        page,
        pages,
        base: out.base,
        back: out.back,
    };
    let page_html = html::render_page(&spec, &items, job.offset_at, &reply_href);
    fs::create_dir_all(out.folder)?;
    fs::write(out.folder.join(html::messages_file(page)), page_html)?;
    Ok(())
}

/// The outcome with its paths made relative to the export root.
fn rooted_outcome(outcome: Option<&Outcome>, out: &ChatOut<'_>) -> Option<Outcome> {
    match outcome {
        Some(Outcome::Saved { path, thumb }) => Some(Outcome::Saved {
            path: out.from_root(path),
            thumb: thumb.as_deref().map(|t| out.from_root(t)),
        }),
        other => other.cloned(),
    }
}

/// Export one chat into `out.folder`: pages (when the format has HTML),
/// media, and each message pushed to `json` in order.
pub fn export_chat<T: Tdlib>(
    job: &Job<'_>,
    tdlib: &mut T,
    out: &ChatOut<'_>,
    mut json: Option<&mut dyn MessageSink>,
    progress: &mut Progress,
) -> Result<ChatResult, ExportError> {
    fs::create_dir_all(out.spool)?;
    let result = write_chat(job, tdlib, out, &mut json, progress);
    let _ = fs::remove_dir_all(out.spool);
    result
}

fn write_chat<T: Tdlib>(
    job: &Job<'_>,
    tdlib: &mut T,
    out: &ChatOut<'_>,
    json: &mut Option<&mut dyn MessageSink>,
    progress: &mut Progress,
) -> Result<ChatResult, ExportError> {
    let wants_html = job.settings.format.wants_html();
    progress.stage = Stage::Collecting;
    progress.done = 0;
    (job.report)(progress.clone());
    let (mut ids, chunks) = collect(job, tdlib, out.spool, progress)?;
    ids.reverse();
    let total = ids.len();
    let pages = total.div_ceil(MESSAGES_PER_FILE).max(1);
    progress.stage = Stage::Writing;
    progress.total = total;
    progress.done = 0;
    (job.report)(progress.clone());
    let mut namer = Namer::default();
    let mut buffer: Vec<(ExportMessage, Option<Outcome>)> = Vec::new();
    let mut result = ChatResult::default();
    let mut page = 0;
    for chunk in (0..chunks).rev() {
        for msg in read_chunk(out.spool, chunk)? {
            let outcome = match &msg.body {
                Body::Media(media) => {
                    let outcome = save_file(job, tdlib, &mut namer, out, media, msg.date)?;
                    match outcome {
                        Outcome::Saved { .. } => result.files += 1,
                        Outcome::Skipped(_) => result.skipped += 1,
                    }
                    Some(outcome)
                }
                _ => None,
            };
            if let Some(sink) = json.as_mut() {
                let offset = (job.offset_at)(i64::from(msg.date));
                let rooted = rooted_outcome(outcome.as_ref(), out);
                sink.push_message(&json::message_json(&msg, rooted.as_ref(), offset))?;
            }
            buffer.push((msg, outcome));
            progress.done += 1;
            progress.files += usize::from(matches!(
                buffer.last(),
                Some((_, Some(Outcome::Saved { .. })))
            ));
            progress.skipped += usize::from(matches!(
                buffer.last(),
                Some((_, Some(Outcome::Skipped(_))))
            ));
            (job.report)(progress.clone());
            if buffer.len() == MESSAGES_PER_FILE {
                if wants_html {
                    flush_page(job, out, page, pages, &buffer, &ids)?;
                }
                buffer.clear();
                page += 1;
            }
        }
    }
    let write_last = !buffer.is_empty() || (total == 0 && out.page_when_empty);
    if wants_html && write_last {
        flush_page(job, out, page, pages, &buffer, &ids)?;
    }
    result.messages = total;
    result.pages = if total == 0 && !out.page_when_empty {
        0
    } else {
        pages
    };
    Ok(result)
}

/// Create a fresh export folder inside `parent` named `base`, adding
/// ` (2)`, ` (3)`, ... when it exists (tdesktop does the same).
pub fn create_named_folder(parent: &Path, base: &str) -> io::Result<PathBuf> {
    for n in 1.. {
        let name = if n == 1 {
            base.to_string()
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
    // Only reached if every numbered name up to the integer limit exists.
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "no free export folder name",
    ))
}

/// Create the chat export folder inside `parent`, like tdesktop's
/// `ChatExport_2023-11-14` with the chat's name added.
pub fn create_folder(parent: &Path, title: &str, stamp: &str) -> io::Result<PathBuf> {
    let clean = super::names::sanitize(title);
    let clean: String = clean.chars().take(40).collect();
    let base = if clean.is_empty() {
        format!("ChatExport_{stamp}")
    } else {
        format!("ChatExport_{clean}_{stamp}")
    };
    create_named_folder(parent, &base)
}

/// Run a single chat's export into `folder` (made by [`create_folder`]).
pub fn run<T: Tdlib>(job: &Job<'_>, tdlib: &mut T, folder: &Path) -> Result<Summary, ExportError> {
    let settings = job.settings;
    let mut progress = Progress::starting();
    if settings.format.wants_html() {
        assets::write(folder)?;
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
    let spool = folder.join(".spool");
    let out = ChatOut {
        folder,
        spool: &spool,
        base: "",
        back: None,
        json_prefix: "",
        page_when_empty: true,
    };
    let sink = json_writer.as_mut().map(|w| w as &mut dyn MessageSink);
    let chat = export_chat(job, tdlib, &out, sink, &mut progress)?;
    if let Some(writer) = json_writer {
        writer.finish()?;
    }
    Ok(Summary {
        folder: folder.to_path_buf(),
        messages: chat.messages,
        files: chat.files,
        skipped: chat.skipped,
        pages: chat.pages,
    })
}
