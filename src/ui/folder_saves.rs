//! A document downloaded from a chat row also lands where the user saves
//! files (Telegram Desktop `DocumentSaveClickHandler`, which saves to
//! `Core::App().settings().downloadPath()`): the folder set in Settings ›
//! Data and Storage, or the path "Ask where to save each file" asks for at
//! the click. With neither, the file stays in TDLib's cache as before.
//! TDLib's copy always stays, so every surface that reads the file map
//! keeps working; Open and Show in folder prefer the saved copy.
//!
//! The target is chosen at the click and travels with the download intent
//! (`quill::state::SaveTarget`, `DownloadIntents::save_to`). This module
//! owns the two stages that run in the UI: the save dialog and the copy
//! (`FolderCopies`), each holding its task, so a newer one for the same
//! file or the account leaving the screen cancels it.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::message_media::DocOrigin;
use crate::ui::status_note::StatusNote;
use gpui_kit::*;
use quill::ids::FileId;
use quill::lang::{keys, tr_with};
use quill::state::{FolderSaveDue, SaveTarget};
use quill::telegram::envelope::MessageContent;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// One document's save work that runs in the UI.
enum FolderCopy {
    /// The save dialog is open; the download starts once it answers.
    Asking { _task: Task<()> },
    /// The finished file is being copied off the UI thread.
    Copying { _task: Task<()> },
}

/// The save dialogs and copies of the account on screen, by file id. An
/// `OnScreen` field: they go with the screen, and a dropped task never
/// answers.
#[derive(Default)]
pub(super) struct FolderCopies {
    by_file: HashMap<i32, FolderCopy>,
}

impl FolderCopies {
    fn is_asking(&self, file_id: i32) -> bool {
        matches!(self.by_file.get(&file_id), Some(FolderCopy::Asking { .. }))
    }

    /// The stage of `file_id` answered: its task is the one running this
    /// answer, so it is let finish instead of dropped mid-poll.
    fn finish(&mut self, file_id: i32) {
        match self.by_file.remove(&file_id) {
            Some(FolderCopy::Asking { _task: task } | FolderCopy::Copying { _task: task }) => {
                task.detach();
            }
            None => {}
        }
    }
}

/// The copies end with the screen: nothing to say through the client.
impl crate::ui::on_screen::Farewell for FolderCopies {}

/// How a copy ended, worked out off the UI thread.
enum CopyOutcome {
    /// Written to `dest`; `downloads` is the folder the note names.
    Saved {
        dest: PathBuf,
        downloads: Option<PathBuf>,
    },
    /// The download folder cannot be reached: it counts as unset, and the
    /// file stays in TDLib's cache only.
    NoFolder,
    Failed(std::io::Error),
}

fn run_copy(source: &Path, target: SaveTarget) -> CopyOutcome {
    let copied = match target {
        SaveTarget::Folder { dir, name } => {
            if !dir.is_dir() {
                return CopyOutcome::NoFolder;
            }
            quill::media_viewer::save_file_in_dir_as(source, &dir, &name)
        }
        SaveTarget::Exact(dest) => std::fs::copy(source, &dest).map(|_| dest),
    };
    match copied {
        Ok(dest) => CopyOutcome::Saved {
            dest,
            downloads: quill::media_viewer::downloads_dir(),
        },
        Err(err) => CopyOutcome::Failed(err),
    }
}

impl QuillApp {
    /// The download disc (or name) of a document row: download it, and
    /// copy it where the user saves files once it finishes.
    pub(super) fn request_document_download(
        &mut self,
        file_id: FileId,
        origin: DocOrigin,
        cx: &mut Context<Self>,
    ) {
        let prefs = quill::file_prefs::current();
        let wants_copy = prefs.ask_download_path || prefs.download_dir.is_some();
        let name = if wants_copy {
            self.savable_document_name(origin)
        } else {
            None
        };
        let sponsored = origin.sponsored();
        match (name, prefs.download_dir) {
            (Some(name), dir) if prefs.ask_download_path => {
                self.ask_where_to_save(file_id, origin, &name, dir, cx);
            }
            (Some(name), Some(dir)) => {
                let target = SaveTarget::Folder { dir, name };
                self.request_media_download_to(file_id, sponsored, Some(target), cx);
            }
            _ => self.request_media_download(file_id, sponsored, cx),
        }
    }

    /// The name to save the document under (empty: TDLib's cache name), or
    /// `None` when it may not be copied out: a chat with protected
    /// content, a self-destructing or one-time message (the checks Save
    /// As and the viewer's Save make), or a message no longer loaded.
    fn savable_document_name(&self, origin: DocOrigin) -> Option<String> {
        let DocOrigin::Message(chat_id, message_id) = origin else {
            return Some(String::new());
        };
        let session = self.session()?;
        if session.chat_has_protected_content(chat_id) {
            return None;
        }
        let message = session
            .histories
            .get(&chat_id.0)?
            .messages
            .get(&message_id.0)?;
        if message.self_destruct.is_some() || message.ephemeral.is_some() {
            return None;
        }
        match &message.content {
            MessageContent::Document(doc) => Some(doc.file_name.clone()),
            _ => Some(String::new()),
        }
    }

    /// "Ask where to save each file": the dialog opens at the click, as in
    /// Telegram Desktop; the download starts once a path is chosen, and a
    /// closed dialog downloads nothing.
    fn ask_where_to_save(
        &mut self,
        file_id: FileId,
        origin: DocOrigin,
        name: &str,
        dir: Option<PathBuf>,
        cx: &mut Context<Self>,
    ) {
        if self.folder_copies.is_asking(file_id.0) {
            return;
        }
        // No stat here: the dialog itself copes with a folder that is gone.
        let dir = dir
            .or_else(quill::media_viewer::os_downloads_dir)
            .unwrap_or_else(|| PathBuf::from("."));
        let suggested = if name.is_empty() {
            "file".to_string()
        } else {
            quill::export::names::safe_file_name(name)
        };
        let picker = cx.prompt_for_new_path(&dir, Some(&suggested));
        // screen-scope: the dialog belongs to the account on screen, and
        // its answer to no chat window.
        let task = self.spawn_screen(cx, async move |this, cx| {
            let dest = picker.await.ok().and_then(Result::ok).flatten();
            let _ = this.update(cx, |this, cx| {
                this.answer_save_dialog(file_id, origin, dest, cx)
            });
        });
        self.folder_copies
            .by_file
            .insert(file_id.0, FolderCopy::Asking { _task: task });
    }

    fn answer_save_dialog(
        &mut self,
        file_id: FileId,
        origin: DocOrigin,
        dest: Option<PathBuf>,
        cx: &mut Context<Self>,
    ) {
        self.folder_copies.finish(file_id.0);
        let Some(dest) = dest else {
            return;
        };
        // Finished while the dialog was open (another surface asked for
        // it): copy it now.
        let cached = self
            .session()
            .and_then(|s| s.file(file_id))
            .and_then(|file| file.usable_path())
            .map(PathBuf::from);
        match cached {
            Some(source) => self.start_copy(
                FolderSaveDue {
                    file_id: file_id.0,
                    source,
                    target: SaveTarget::Exact(dest),
                },
                cx,
            ),
            None => self.request_media_download_to(
                file_id,
                origin.sponsored(),
                Some(SaveTarget::Exact(dest)),
                cx,
            ),
        }
    }

    /// The local file Open and Show in folder use for `file_id`: the saved
    /// copy while it is still there, else TDLib's.
    pub(super) fn document_local_path(&self, file_id: FileId) -> Option<PathBuf> {
        let session = self.session()?;
        if let Some(copy) = session.media.folder_saves.copy_of(file_id.0)
            && copy.is_file()
        {
            return Some(copy.to_path_buf());
        }
        session
            .file(file_id)
            .and_then(|file| file.usable_path())
            .map(PathBuf::from)
    }

    /// Copy the documents that finished since the last poll.
    pub(super) fn save_finished_documents(&mut self, cx: &mut Context<Self>) {
        let due = match self.live.as_mut() {
            Some(live) if live.driver.session.media.folder_saves.has_due() => {
                live.driver.session.media.folder_saves.take_due()
            }
            _ => return,
        };
        for save in due {
            self.start_copy(save, cx);
        }
    }

    /// A newer copy of the same file replaces (and cancels) an older one.
    fn start_copy(&mut self, save: FolderSaveDue, cx: &mut Context<Self>) {
        let FolderSaveDue {
            file_id,
            source,
            target,
        } = save;
        // screen-scope: the copy belongs to the account on screen, and its
        // result to no chat window.
        let task = self.spawn_screen(cx, async move |this, cx| {
            let outcome = cx
                .background_executor()
                .spawn(async move { run_copy(&source, target) })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.finish_document_copy(file_id, outcome, cx)
            });
        });
        self.folder_copies
            .by_file
            .insert(file_id, FolderCopy::Copying { _task: task });
    }

    fn finish_document_copy(&mut self, file_id: i32, outcome: CopyOutcome, cx: &mut Context<Self>) {
        self.folder_copies.finish(file_id);
        let note = match outcome {
            CopyOutcome::Saved { dest, downloads } => {
                let note = quill::media_viewer::saved_note(&dest, downloads.as_deref());
                if let Some(live) = self.live.as_mut() {
                    live.driver
                        .session
                        .media
                        .folder_saves
                        .note_saved(file_id, dest);
                }
                StatusNote::info(note)
            }
            CopyOutcome::NoFolder => return,
            CopyOutcome::Failed(err) => {
                log::warn!("could not copy document {file_id} to the download folder: {err}");
                StatusNote::error(tr_with(
                    keys::Q_PREFS_COULDNT_SAVE,
                    &[("err", &err.to_string())],
                ))
            }
        };
        self.set_note(note);
        cx.notify();
    }
}
