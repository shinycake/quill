//! "Save Selected": pick a folder and copy the downloaded media of the
//! selection into it.
#![deny(clippy::indexing_slicing)]

use super::unique_destination;
use crate::ui::app::QuillApp;
use crate::ui::status_note::StatusNote;
use gpui_kit::*;
use quill::lang::{keys, tr, trn};

/// Where "Save Selected" stands in a chat: idle, or waiting for the folder
/// the user picks. The picker's task lives in the stage, so a chat change
/// (which replaces the `ChatUi`) drops it with the selection it was for.
#[derive(Default)]
pub(crate) enum SaveSelection {
    #[default]
    Idle,
    Picking(Task<()>),
}

impl QuillApp {
    /// "Save Selected": pick a folder and copy the downloaded media of the
    /// selection into it; media still downloading is requested first.
    pub(in crate::ui) fn save_selection(&mut self, cx: &mut Context<Self>) {
        let Some(chat_id) = self.forward_draft().map(|d| d.from_chat_id) else {
            return;
        };
        if self.refuse_protected_copy(chat_id, cx) {
            return;
        }
        let media = self.selected_media();
        if media.is_empty() {
            return;
        }
        let mut files = Vec::new();
        let mut missing = Vec::new();
        for (id, target) in &media {
            match self.menu_media_local(chat_id, *id) {
                Some((_, path)) => {
                    let local = path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("file")
                        .to_string();
                    files.push((
                        path,
                        quill::message_menu::suggested_save_name(target, &local),
                    ));
                }
                None => missing.push(target.file_id),
            }
        }
        if !missing.is_empty() {
            let sent = self
                .live
                .as_mut()
                .is_none_or(|live| live.driver.ensure_media_files(&missing).is_ok());
            self.set_note(if sent {
                StatusNote::silent(tr(keys::Q_CHAT_DOWNLOADING_CHOOSE_SAVE_AGAIN_WHEN))
            } else {
                StatusNote::error(tr(keys::Q_OPS_REQUEST_NOT_SENT))
            });
            cx.notify();
            return;
        }
        // One picker at a time: another click while it is open does not
        // open a second one.
        if matches!(self.chat_ui.save_selection, SaveSelection::Picking(_)) {
            return;
        }
        let picker = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some(tr(keys::SETTINGS_SAVE).into()),
        });
        let task = self.spawn_scoped(cx, async move |this, cx| {
            let picked = picker.await;
            // The picker is over, whatever it answered.
            let _ = this.update(cx, |this, _| {
                if let SaveSelection::Picking(done) =
                    std::mem::take(&mut this.chat_ui.save_selection)
                {
                    done.detach();
                }
            });
            let Ok(Ok(Some(dirs))) = picked else {
                return;
            };
            let Some(dir) = dirs.into_iter().next() else {
                return;
            };
            let saved = cx
                .background_executor()
                .spawn(async move {
                    let mut used = std::collections::HashSet::new();
                    let mut saved = 0usize;
                    for (path, name) in files {
                        let (dest, claimed) = unique_destination(&dir, &name, &mut used);
                        if std::fs::copy(&path, &dest).is_ok() {
                            saved += 1;
                        } else if claimed {
                            // Drop the empty file `unique_destination` created,
                            // never a path it did not create.
                            let _ = std::fs::remove_file(&dest);
                        }
                    }
                    saved
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.set_note(match saved {
                    1 => StatusNote::silent(tr(keys::Q_CHAT_SAVED_1_FILE)),
                    n => StatusNote::silent(trn(keys::Q_CHAT_SAVED_FILES, n)),
                });
                cx.notify();
            });
        });
        self.chat_ui.save_selection = SaveSelection::Picking(task);
    }
}
