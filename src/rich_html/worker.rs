//! The export as a blocking routine over [`Tdlib`] (tdesktop
//! `RichMessageHtmlExport::start` … `finalize`): load the message and its
//! full page, make the folder, copy the media, write the page. Any failure
//! removes the folder again, so nothing half-written is left behind.
use super::RichHtmlRequest;
use super::media::{Collector, Saved, name_part};
use super::page::{render, title_from_page};
use crate::export::names::sanitize;
use crate::export::worker::{
    ExportError, Tdlib, copy_into, create_named_folder, fetch_file, query,
};
use crate::json_access::JsonExt;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// The `richMessage` of a message: its content, or the ephemeral content
/// the chat shows in its place.
fn rich_of(message: &Value) -> Option<&Value> {
    [
        message.at("content"),
        message.at("ephemeral_content").at("content"),
    ]
    .into_iter()
    .find(|content| content.at("@type") == "messageRichMessage")
    .map(|content| content.at("message"))
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
    let Some(rich) = rich_of(&message) else {
        return Err(ExportError::Tdlib("not a rich message".into()));
    };
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
    let env = Env {
        request,
        files_root,
        date,
    };
    let written = write_page(&env, tdlib, &page, &title, &folder, &html);
    if written.is_err() {
        // The folder is this export's own (`create_named_folder` made it).
        let _ = fs::remove_dir_all(&folder);
    }
    written.map(|()| html)
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
        if let Ok(source) = fetch_file(tdlib, env.files_root, job.file_id, None)?
            && copy_into(folder, &job.relative, &source).is_ok()
        {
            saved.insert(job.file_id, job.relative);
        }
    }
    let text = render(page, title, labels, env.date, &mut Saved(saved));
    fs::write(html, text)?;
    Ok(())
}
