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
        cx.spawn(async move |this, cx| {
            if let (Ok(Ok(Some(paths))), Some(asked_in)) = (picker.await, asked_in) {
                let _ = this.update(cx, |this, cx| {
                    this.in_window_scope(window_chat, cx, |this, cx| {
                        this.attach_picked_in(&asked_in, paths, as_files, cx)
                    })
                });
            }
        })
        .detach();
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
