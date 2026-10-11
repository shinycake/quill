//! Methods moved out of `composer.rs` to keep files under 1000 lines.

use super::*;

impl QuillApp {
    /// Phase 3.2: `inlineKeyboardButtonTypeCopyText` — copy to the clipboard.
    pub(in crate::ui) fn copy_inline_text(&mut self, text: &str, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(text.to_string()));
        self.connection.status_note = "copied".into();
        cx.notify();
    }

    /// B5: tdesktop `EditCaptionBox` "Replace attachment": open the file
    /// dialog (all platforms) for one replacement file. `QUILL_EDIT_REPLACE`
    /// skips the dialog for live testing; the offline demo uses its fixture.
    pub(in crate::ui) fn pick_edit_replacement(&mut self, cx: &mut Context<Self>) {
        let preset = std::env::var_os("QUILL_EDIT_REPLACE")
            .map(PathBuf::from)
            .or_else(|| {
                (self.live.is_none() && self.demo_session.is_some())
                    .then(|| demo_media_allowlist().join("demo-thumb.png"))
            });
        if let Some(path) = preset {
            self.set_edit_replacement(&path, cx);
            return;
        }
        let picker = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Choose File".into()),
        });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = picker.await
                && let Some(path) = paths.first().cloned()
            {
                let _ = this.update(cx, |this, cx| this.set_edit_replacement(&path, cx));
            }
        })
        .detach();
    }

    /// B5: validate a file against tdesktop's replacement rules and stage it.
    pub(in crate::ui) fn set_edit_replacement(
        &mut self,
        path: &std::path::Path,
        cx: &mut Context<Self>,
    ) {
        let as_file = self.composer_ui.edit_replace_as_file;
        let Some(edit) = self.composer_ui.pending_edit.as_mut() else {
            return;
        };
        match edit.replacement_for(path, as_file) {
            Ok(replacement) => {
                self.connection.status_note =
                    format!("Replacing attachment with {}", replacement.file_name);
                edit.media_edit.replacement = Some(replacement);
            }
            Err(note) => self.connection.status_note = note.into(),
        }
        cx.notify();
    }

    /// B5: drop the staged replacement and keep the original media.
    pub(in crate::ui) fn clear_edit_replacement(&mut self, cx: &mut Context<Self>) {
        if let Some(edit) = self.composer_ui.pending_edit.as_mut() {
            edit.media_edit.replacement = None;
        }
        self.composer_ui.edit_replace_as_file = false;
        self.connection.status_note = "replacement cleared".into();
        cx.notify();
    }

    /// B5: tdesktop's "Send as a document" checkbox on the replacement.
    pub(in crate::ui) fn toggle_edit_replace_as_file(&mut self, cx: &mut Context<Self>) {
        self.composer_ui.edit_replace_as_file = !self.composer_ui.edit_replace_as_file;
        let path = self
            .composer_ui
            .pending_edit
            .as_ref()
            .and_then(|e| e.media_edit.replacement.as_ref())
            .map(|r| r.path.clone());
        if let Some(path) = path {
            self.set_edit_replacement(&path, cx);
        }
    }

    pub(in crate::ui) fn toggle_edit_replace_spoiler(&mut self, cx: &mut Context<Self>) {
        if let Some(replacement) = self
            .composer_ui
            .pending_edit
            .as_mut()
            .and_then(|e| e.media_edit.replacement.as_mut())
        {
            replacement.spoiler = !replacement.spoiler;
            cx.notify();
        }
    }

    pub(in crate::ui) fn begin_reply_to(
        &mut self,
        reply: ComposerReplyTo,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.composer_ui.pending_edit.is_some() {
            self.clear_edit(window, cx);
        }
        self.composer_ui.pending_reply = Some(reply);
        self.note_open_draft(true, cx);
        self.composer
            .update(cx, |input, cx| input.focus(window, cx));
        self.connection.status_note = "replying".into();
        cx.notify();
    }

    /// Up in the composer: start editing the last own message when the
    /// composer is focused and empty and nothing else uses the key.
    pub(in crate::ui) fn try_edit_last_message(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.composer.read(cx).focus_handle(cx).is_focused(window)
            || !self.composer.read(cx).value().is_empty()
            || self.composer_ui.pending_edit.is_some()
            || !self.composer_ui.pending_attachments.is_empty()
            || self.composer_ui.command_menu_open
            || !self.mention_menu_items(cx).is_empty()
        {
            return false;
        }
        let Some(edit) = self
            .session()
            .and_then(|s| s.open_chat.and_then(|chat| s.last_editable_message(chat)))
        else {
            return false;
        };
        self.begin_edit(edit, window, cx);
        true
    }

    pub(in crate::ui) fn begin_edit(
        &mut self,
        edit: ComposerEdit,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.composer_ui.pending_attachments.clear();
        self.composer_ui.edit_replace_as_file = false;
        let current = self.composer_markup(cx);
        // Flush while the reply is still set so a reply-only draft is not wiped.
        self.note_open_draft(false, cx);
        let reply = self.composer_ui.pending_reply.take();
        let (edit, field, saved, stashed) = begin_edit_keeping_reply(current, edit, reply);
        self.composer_ui.pending_edit = Some(edit);
        self.composer_ui.saved_edit_draft = saved;
        self.composer_ui.saved_edit_reply = stashed;
        self.set_composer_markup(&field, window, cx);
        self.composer.update(cx, |input, cx| {
            let end = input.value().len();
            input.set_selected_range(end..end, cx);
            input.focus(window, cx);
        });
        let field = self.composer.read(cx).value().to_string();
        self.sync_composer_typing(&field);
        self.connection.status_note = "editing".into();
        cx.notify();
    }

    pub(in crate::ui) fn clear_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // tdesktop cancelEditMessage → applyDraft(): restore normal draft.
        let saved = std::mem::take(&mut self.composer_ui.saved_edit_draft);
        let stashed = self.composer_ui.saved_edit_reply.take();
        let (_, restored) = cancel_edit_draft(self.composer_ui.pending_edit.take(), saved);
        let (restored, reply) = cancel_edit_keeping_reply(restored, stashed);
        self.composer_ui.pending_reply = reply;
        self.set_composer_markup(&restored, window, cx);
        let restored = self.composer.read(cx).value().to_string();
        self.sync_composer_typing(&restored);
        self.connection.status_note = "edit cancelled".into();
        cx.notify();
    }

    pub(in crate::ui) fn finish_edit_restore_draft(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let saved = std::mem::take(&mut self.composer_ui.saved_edit_draft);
        let reply = self.composer_ui.saved_edit_reply.take();
        self.composer_ui.pending_edit = None;
        self.composer_ui.pending_reply = reply;
        self.set_composer_markup(&saved, window, cx);
    }

    pub(in crate::ui) fn submit_edit(
        &mut self,
        text: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(edit) = self.composer_ui.pending_edit.clone() else {
            return;
        };
        if self.live.is_some() {
            let result = self
                .live
                .as_mut()
                .expect("live")
                .driver
                .edit_snapshot(&edit, &text);
            match result {
                Ok(_) => {
                    self.finish_edit_restore_draft(window, cx);
                    self.connection.status_note =
                        self.send_started_note(ComposerScheduling::None, "saving edit…");
                }
                Err(quill::connect::ConnectSendError::TextTooLong { limit }) => {
                    // R8: tdesktop `lng_edit_limit_reached` — the edit stays
                    // open so the text can be shortened.
                    let over = quill::text_split::units_over_limit(&text, limit);
                    self.connection.status_note =
                        format!("message too long (max {limit} characters, remove {over})");
                }
                Err(_) => {
                    self.connection.status_note = "could not edit message".into();
                }
            }
            cx.notify();
            return;
        }
        if self.demo_session.is_some() {
            self.apply_demo_edit(&edit, text.trim());
            self.finish_edit_restore_draft(window, cx);
            self.connection.status_note = "demo edit applied locally (no live Telegram)".into();
            cx.notify();
        }
    }

    /// R8: the runtime `message_text_length_max` (TDLib's compiled default
    /// before the first `updateOption`, and in demo mode).
    pub(in crate::ui) fn text_length_limit(&self) -> i32 {
        self.live
            .as_ref()
            .map(|live| live.driver.session.messages.message_text_length_max)
            .unwrap_or(4096)
    }

    pub(in crate::ui) fn cancel_delete(&mut self, cx: &mut Context<Self>) {
        self.message_ui.pending_delete = None;
        self.connection.status_note = "delete cancelled".into();
        cx.notify();
    }

    pub(in crate::ui) fn confirm_delete(&mut self, cx: &mut Context<Self>) {
        let Some(confirm) = self.message_ui.pending_delete.take() else {
            return;
        };
        self.begin_vanish(confirm.chat_id, &[confirm.message_id]);
        if self.live.is_some() {
            let result = self
                .live
                .as_mut()
                .expect("live")
                .driver
                .delete_confirmed(&confirm);
            self.connection.status_note = match result {
                Ok(_) => "deleting…".into(),
                Err(_) => "could not delete message".into(),
            };
        } else if self.demo_session.is_some() {
            self.apply_demo_delete(confirm.chat_id, confirm.message_id);
            self.connection.status_note = "demo delete applied locally (no live Telegram)".into();
        }
        cx.notify();
    }

    pub(in crate::ui) fn clear_reply(&mut self, cx: &mut Context<Self>) {
        // tdesktop FieldHeader Escape / replyCancelled: header only — keep typed text.
        self.composer_ui.pending_reply =
            cancel_reply_draft(self.composer_ui.pending_reply.take(), String::new()).0;
        self.share.reply_elsewhere_open = false;
        self.share.reply_quote_open = false;
        self.note_open_draft(true, cx);
        self.connection.status_note = "reply cancelled".into();
        cx.notify();
    }

    /// Sticker/GIF/voice sends consume the composer reply. Drop it from the UI
    /// and from the stored draft, and keep any unsent text.
    pub(in crate::ui) fn consume_sent_reply(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        self.composer_ui.pending_reply = None;
        if self.composer_ui.pending_edit.is_some() {
            self.composer_ui.saved_edit_reply = None;
            return;
        }
        let text = self.composer_markup(cx);
        self.save_chat_draft(chat_id, &text, None, false, cx);
    }
}
