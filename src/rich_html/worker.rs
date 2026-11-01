//! The export as a blocking routine over [`Tdlib`] (tdesktop
//! `RichMessageHtmlExport::start` … `finalize`): load the message and its
//! full page, make the folder, copy the media, write the page. Any failure
//! removes the folder again, so nothing half-written is left behind.
use super::RichHtmlRequest;
use super::media::{Collector, Saved, name_part};
use super::page::{render, title_from_page};
use crate::export::names::sanitize;
use crate::export::worker::{
    ExportError, Tdlib, create_named_folder, fetch_file, place_into, query,
};
use crate::json_access::JsonExt;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// The `richMessage` of a message: its content, or the ephemeral content
/// the chat shows in its place. Refused when it is not a rich message, or
/// when its content may not be saved: TDLib's `can_be_saved` (per message,
/// so a bot's `protect_content` in an open chat counts too) and
/// self-destructing messages, as the chat export refuses them
/// (`export::model::parse_message`).
fn saveable_rich(message: &Value) -> Result<&Value, ExportError> {
    let refuse = |why: &str| Err(ExportError::Tdlib(why.into()));
    if !message.at("self_destruct_type").is_null() {
        return refuse("the message self-destructs");
    }
    let ephemeral = message.at("ephemeral_content");
    let (content, can_be_saved) = if message.at("content").at("@type") == "messageRichMessage" {
        (message.at("content"), message.at("can_be_saved"))
    } else {
        (ephemeral.at("content"), ephemeral.at("can_be_saved"))
    };
    if content.at("@type") != "messageRichMessage" {
        return refuse("not a rich message");
    }
    if can_be_saved != true {
        return refuse("the message's content can't be saved");
    }
    Ok(content.at("message"))
}

/// The message's page, complete: a partial page is completed with
/// `getFullRichMessage`, and kept as it is if that fails (tdesktop shows
/// `richPage()` when `fullRichPage()` is missing).
fn load_page<T: Tdlib>(tdlib: &mut T, request: &RichHtmlRequest) -> Result<Value, ExportError> {
    let message = query(
        tdlib,
        json!({"@type": "getMessage", "chat_id": request.chat_id.0,
            "message_id": request.message_id.0}),
    )?;
    let rich = saveable_rich(&message)?;
    if rich.at("is_full") != false {
        return Ok(rich.clone());
    }
    match query(
        tdlib,
        json!({"@type": "getFullRichMessage", "chat_id": request.chat_id.0,
            "message_id": request.message_id.0}),
    ) {
        Ok(full) if full.at("@type") == "richMessage" => Ok(full),
        Err(ExportError::Cancelled) => Err(ExportError::Cancelled),
        _ => Ok(rich.clone()),
    }
}

/// `<base>/<title>/` (or `<title> (2)/`, …) and the page file inside it.
fn make_folder(base: &Path, title: &str) -> std::io::Result<(PathBuf, PathBuf)> {
    fs::create_dir_all(base)?;
    let mut name = name_part(&sanitize(title), "");
    if name.is_empty() {
        name = "message".into();
    }
    let folder = create_named_folder(base, &name)?;
    let stem = folder
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("message")
        .to_string();
    let html = folder.join(format!("{stem}.html"));
    Ok((folder, html))
}

/// Run the export; returns the saved page's path.
pub fn run<T: Tdlib>(
    request: &RichHtmlRequest,
    tdlib: &mut T,
    files_root: &Path,
    date: &dyn Fn(i64) -> String,
) -> Result<PathBuf, ExportError> {
    let page = load_page(tdlib, request)?;
    let title = title_from_page(&page).unwrap_or_else(|| request.fallback_title.clone());
    let (folder, html) = make_folder(&request.base_dir, &title)?;
    // Until the page is written, an error or a panic removes the folder
    // (it is this export's own: `create_named_folder` made it).
    let guard = FolderGuard(Some(folder.clone()));
    let env = Env {
        request,
        files_root,
        date,
    };
    write_page(&env, tdlib, &page, &title, &folder, &html)?;
    guard.keep();
    Ok(html)
}

/// Removes a half-written export folder when dropped before [`keep`],
/// which covers both an early return and unwinding from a panic.
///
/// [`keep`]: FolderGuard::keep
struct FolderGuard(Option<PathBuf>);

impl FolderGuard {
    fn keep(mut self) {
        self.0 = None;
    }
}

impl Drop for FolderGuard {
    fn drop(&mut self) {
        if let Some(folder) = self.0.take() {
            let _ = fs::remove_dir_all(folder);
        }
    }
}

/// What [`write_page`] needs besides the page and where it goes.
struct Env<'a> {
    request: &'a RichHtmlRequest,
    files_root: &'a Path,
    date: &'a dyn Fn(i64) -> String,
}

fn write_page<T: Tdlib>(
    env: &Env<'_>,
    tdlib: &mut T,
    page: &Value,
    title: &str,
    folder: &Path,
    html: &Path,
) -> Result<(), ExportError> {
    let labels = &env.request.labels;
    let mut collector = Collector::default();
    render(page, title, labels, env.date, &mut collector);
    let mut saved = HashMap::new();
    for job in collector.jobs {
        // A file that cannot be fetched or copied shows as "Media
        // unavailable"; only a cancel stops the export.
        if let Ok(source) = fetch_file(tdlib, env.files_root, job.file_id, None)? {
            let copied = place_into(folder, &job.relative, &source).is_ok();
            source.release(tdlib)?;
            if copied {
                saved.insert(job.file_id, job.relative);
            }
        }
    }
    let text = render(page, title, labels, env.date, &mut Saved(saved));
    fs::write(html, text)?;
    Ok(())
}
