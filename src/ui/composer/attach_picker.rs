use super::super::app::QuillApp;
use super::super::composer_sends::SendSlot;
use super::super::demo::demo_media_allowlist;
use gpui_kit::*;
use quill::composer::ComposerAttachment;
use quill::lang::{keys, tr};
use std::path::PathBuf;

impl QuillApp {
    /// The attach menu's "Photo or video" / "File": Telegram Desktop's
    /// "Choose Files" dialog. Picked photos and videos join the album;
    /// anything else, or everything when `as_files`, goes as files.
    /// `QUILL_ATTACH_PHOTO` / `QUILL_ATTACH_FILE` skip the dialog for live
    /// testing; the offline demo attaches its fixtures instead.
    pub(crate) fn pick_attachments(&mut self, as_files: bool, cx: &mut Context<Self>) {
        let env_key = if as_files {
            "QUILL_ATTACH_FILE"
        } else {
            "QUILL_ATTACH_PHOTO"
        };
        let preset = std::env::var_os(env_key).map(PathBuf::from).or_else(|| {
            (self.live.is_none() && self.demo_session.is_some()).then(|| {
                demo_media_allowlist().join(if as_files {
                    "demo-notes.txt"
                } else {
                    "demo-thumb.png"
                })
            })
        });
        if let Some(path) = preset {
            self.attach_picked(vec![path], as_files, cx);
            return;
        }
        let picker = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some(tr(keys::Q_CHAT_CHOOSE_FILES).into()),
        });
        // The picker answers later (the Linux portal's is not modal):
        // the files belong to the chat and account it was opened in, not
        // to whatever the window shows by then.
        let window_chat = self.windows.scope;
        let asked_in = self.send_slot();
        let asked_edit = self.chat_ui.edit_id();
        self.spawn_screen(cx, async move |this, cx| {
            if let (Ok(Ok(Some(paths))), Some(asked_in)) = (picker.await, asked_in) {
                let _ = this.update(cx, |this, cx| {
                    this.attach_answer(window_chat, asked_in, asked_edit, paths, as_files, cx)
                });
            }
        })
        .detach();
    }

    /// The portal's answer. A locked Quill takes no files into a tray (the
    /// video probes would read them): the answer waits for the unlock.
    fn attach_answer(
        &mut self,
        window_chat: Option<quill::ids::ChatId>,
        asked_in: SendSlot,
        asked_edit: Option<crate::ui::chat_ui_state::EditId>,
        paths: Vec<PathBuf>,
        as_files: bool,
        cx: &mut Context<Self>,
    ) {
        if self.refused_while_locked() {
            self.wait_for_unlock(move |this, cx| {
                this.attach_answer(window_chat, asked_in, asked_edit, paths, as_files, cx)
            });
            return;
        }
        self.in_window_scope(window_chat, cx, |this, cx| {
            // Asked in an edit that is over, or answered in one
            // started since: the files are not the tray's.
            let stale = match asked_edit {
                Some(_) => this.chat_ui.edit_id() != asked_edit,
                None => this.chat_ui.is_editing() && this.send_slot().as_ref() == Some(&asked_in),
            };
            if !stale {
                this.attach_picked_in(&asked_in, paths, as_files, cx)
            }
        });
    }

    /// A picker answer for `asked_in`. In the slot on screen it is the
    /// ordinary path; in another one it joins that tray without touching
    /// the open chat, and a slot whose account or chat is gone drops it.
    pub(crate) fn attach_picked_in(
        &mut self,
        asked_in: &SendSlot,
        paths: Vec<PathBuf>,
        as_files: bool,
        cx: &mut Context<Self>,
    ) {
        if self.send_slot().as_ref() == Some(asked_in) {
            self.attach_picked(paths, as_files, cx);
            return;
        }
        if !self.send_slot_exists(asked_in) {
            return;
        }
        let appended = self.composer_sends.update(asked_in, |send| {
            let appended = ComposerAttachment::append_dropped_files(&mut send.attachments, &paths);
            if as_files {
                ComposerAttachment::set_send_as_files(&mut send.attachments, true);
            }
            appended
        });
        // A limit note is about a chat that is not on screen: show nothing.
        let _ = appended;
        self.check_tray_videos(cx);
        cx.notify();
    }

    fn attach_picked(&mut self, paths: Vec<PathBuf>, as_files: bool, cx: &mut Context<Self>) {
        self.attach_dropped_files(&paths, cx);
        if as_files {
            self.update_pending_attachments(|tray| {
                ComposerAttachment::set_send_as_files(tray, true)
            });
        }
    }
}
