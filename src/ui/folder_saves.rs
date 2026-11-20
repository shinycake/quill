//! A document downloaded from a chat row lands in the download folder,
//! not only in TDLib's cache (Telegram Desktop `DocumentSaveClickHandler`,
//! which saves to `Core::App().settings().downloadPath()`). Only when the
//! user set a folder in Settings › Data and Storage, or turned on "Ask
//! where to save each file": with neither, the file stays in the cache
//! as before. Once copied, TDLib's cache copy is deleted, so the file
//! lives only in the folder. The stages are `quill::state::FolderSaves`.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::message_media::DocOrigin;
use crate::ui::status_note::StatusNote;
use gpui_kit::*;
use quill::ids::FileId;
use quill::lang::{keys, tr_with};
use quill::telegram::envelope::MessageContent;
use std::path::PathBuf;

/// Where a finished document goes, read when it finishes (the setting
/// may have changed since the click).
enum Destination {
    Folder(PathBuf),
    Ask(PathBuf),
    Cache,
}

fn destination() -> Destination {
    let prefs = quill::file_prefs::current();
    if prefs.ask_download_path {
        let dir = quill::media_viewer::downloads_dir().unwrap_or_else(|| PathBuf::from("."));
        return Destination::Ask(dir);
    }
    match quill::file_prefs::configured_download_dir() {
        Some(dir) => Destination::Folder(dir),
        None => Destination::Cache,
    }
}

impl QuillApp {
    /// The download disc (or name) of a document row: download it, and
    /// once it finishes copy it to the download folder when one is set.
    pub(super) fn request_document_download(
        &mut self,
        file_id: FileId,
        origin: DocOrigin,
        cx: &mut Context<Self>,
    ) {
        let copies = !matches!(destination(), Destination::Cache);
        if copies {
            let (name, keep_cache) = self.document_to_save(origin);
            if let Some(live) = self.live.as_mut() {
                live.driver
                    .session
                    .media
                    .folder_saves
                    .want(file_id.0, &name, keep_cache);
            }
        }
        let sponsored = origin.sponsored();
        self.request_media_download(file_id, sponsored, cx);
        // A request that did not go out leaves nothing to wait for.
        let out = self.session().is_none_or(|session| {
            session.media.downloads.is_requested(file_id.0)
                || session.requests.has_download(file_id)
                || session
                    .file(file_id)
                    .is_some_and(|file| file.local.is_downloading_active())
        });
        if !out && let Some(live) = self.live.as_mut() {
            live.driver.session.media.folder_saves.cancel(file_id.0);
        }
    }

    /// The document's own name, from its message; empty for a sponsored
    /// one or a message no longer loaded (the copy keeps TDLib's name).
    /// With it, whether TDLib's copy must stay: a picture Quill opens in
    /// its viewer, which shows it from the cache (`open_document`).
    fn document_to_save(&self, origin: DocOrigin) -> (String, bool) {
        let DocOrigin::Message(chat_id, message_id) = origin else {
            return (String::new(), false);
        };
        self.session()
            .and_then(|s| s.histories.get(&chat_id.0)?.messages.get(&message_id.0))
            .and_then(|message| match &message.content {
                MessageContent::Document(doc) => Some((
                    doc.file_name.clone(),
                    quill::media_viewer::opens_as_picture(&doc.file_name, &doc.mime_type),
                )),
                _ => None,
            })
            .unwrap_or_default()
    }

    /// The local file Open and Show in folder use for `file_id`: the copy
    /// in the download folder while it is still there, else TDLib's.
    pub(super) fn document_local_path(&self, file_id: FileId) -> Option<PathBuf> {
        let session = self.session()?;
        if let Some(copy) = session.media.folder_saves.copy_of(file_id.0)
            && copy.is_file()
        {
            return Some(copy.to_path_buf());
        }
        session
            .media
            .files
            .get(&file_id.0)
            .and_then(|file| file.usable_path())
            .map(PathBuf::from)
    }

    /// Copy the documents that finished since the last poll
    /// (`deliver_notifications`, for the account on screen).
    pub(super) fn save_finished_documents(&mut self, cx: &mut Context<Self>) {
        let Some(live) = self.live.as_mut() else {
            return;
        };
        let due = live.driver.session.media.folder_saves.take_due();
        for save in due {
            match destination() {
                Destination::Cache => {
                    if let Some(live) = self.live.as_mut() {
                        live.driver
                            .session
                            .media
                            .folder_saves
                            .note_copy(save.file_id, None);
                    }
                }
                Destination::Folder(dir) => self.copy_document(save, dir, cx),
                Destination::Ask(dir) => self.copy_document_asking(save, dir, cx),
            }
        }
    }

    fn copy_document(
        &mut self,
        save: quill::state::FolderSaveDue,
        dir: PathBuf,
        cx: &mut Context<Self>,
    ) {
        // screen-scope: the copy belongs to the account on screen, and its
        // result to no chat window.
        self.spawn_screen(cx, async move |this, cx| {
            let quill::state::FolderSaveDue {
                file_id,
                source,
                name,
                keep_cache,
            } = save;
            let copied = cx
                .background_executor()
                .spawn(
                    async move { quill::media_viewer::save_file_in_dir_as(&source, &dir, &name) },
                )
                .await;
            let _ = this.update(cx, |this, cx| {
                this.finish_document_copy(file_id, keep_cache, copied, cx)
            });
        })
        .detach();
    }

    fn copy_document_asking(
        &mut self,
        save: quill::state::FolderSaveDue,
        dir: PathBuf,
        cx: &mut Context<Self>,
    ) {
        let name = if save.name.is_empty() {
            save.source
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("file")
                .to_string()
        } else {
            quill::export::names::safe_file_name(&save.name)
        };
        let picker = cx.prompt_for_new_path(&dir, Some(&name));
        // screen-scope: the copy belongs to the account on screen, and its
        // result to no chat window.
        self.spawn_screen(cx, async move |this, cx| {
            let file_id = save.file_id;
            let keep_cache = save.keep_cache;
            let Ok(Ok(Some(dest))) = picker.await else {
                let _ = this.update(cx, |this, _| this.note_document_copy(file_id, None));
                return;
            };
            let source = save.source;
            let copied = cx
                .background_executor()
                .spawn(async move { std::fs::copy(&source, &dest).map(|_| dest) })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.finish_document_copy(file_id, keep_cache, copied, cx)
            });
        })
        .detach();
    }

    fn note_document_copy(&mut self, file_id: i32, dest: Option<PathBuf>) {
        if let Some(live) = self.live.as_mut() {
            live.driver
                .session
                .media
                .folder_saves
                .note_copy(file_id, dest);
        }
    }

    fn finish_document_copy(
        &mut self,
        file_id: i32,
        keep_cache: bool,
        copied: std::io::Result<PathBuf>,
        cx: &mut Context<Self>,
    ) {
        let note = match &copied {
            Ok(dest) => StatusNote::info(quill::media_viewer::saved_note(
                dest,
                quill::media_viewer::downloads_dir().as_deref(),
            )),
            Err(err) => StatusNote::error(tr_with(
                keys::Q_PREFS_COULDNT_SAVE,
                &[("err", &err.to_string())],
            )),
        };
        let saved = copied.is_ok();
        self.note_document_copy(file_id, copied.ok());
        // The copy is the file now: TDLib's cache copy goes, unless the
        // viewer shows the document from it. Saving the document again
        // makes a new copy, de-duplicated with " (n)".
        if saved
            && !keep_cache
            && let Some(live) = self.live.as_mut()
        {
            let _ = live.driver.delete_cached_copy(FileId(file_id));
        }
        self.set_note(note);
        cx.notify();
    }
}
