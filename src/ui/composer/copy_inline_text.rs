//! Methods moved out of `composer.rs` to keep files under 1000 lines.

use super::*;
use crate::ui::status_note::StatusNote;
use quill::lang::{keys, tr, tr_with};

impl QuillApp {
    /// Phase 3.2: `inlineKeyboardButtonTypeCopyText` — copy to the clipboard.
    pub(in crate::ui) fn copy_inline_text(&mut self, text: &str, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(text.to_string()));
        self.connection
            .set_note(StatusNote::info(tr(keys::TEXT_COPIED)));
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
            prompt: Some(tr(keys::Q_CHAT_CHOOSE_FILE).into()),
        });
        self.spawn_scoped(cx, async move |this, cx| {
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
                self.connection.set_note(StatusNote::silent(tr_with(
                    keys::Q_CHAT_REPLACING_ATTACHMENT_WITH,
                    &[("file_name", &replacement.file_name.to_string())],
                )));
                // A different file starts without the video editor's edits.
                if edit
                    .media_edit
                    .replacement
                    .as_ref()
                    .is_none_or(|old| old.path != replacement.path)
                {
                    edit.media_edit.video_extras = Default::default();
                }
                edit.media_edit.replacement = Some(replacement);
            }
            Err(note) => self.connection.set_note(StatusNote::error(note)),
        }
        cx.notify();
    }

    /// B5: drop the staged replacement and keep the original media.
    pub(in crate::ui) fn clear_edit_replacement(&mut self, cx: &mut Context<Self>) {
        if let Some(edit) = self.composer_ui.pending_edit.as_mut() {
            edit.media_edit.replacement = None;
        }
        self.composer_ui.edit_replace_as_file = false;
        self.connection
            .set_note(StatusNote::silent(tr(keys::Q_CHAT_REPLACEMENT_CLEARED)));
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
        self.composer_ui.set_reply(Some(reply));
        self.note_open_draft(true, cx);
        self.composer
            .update(cx, |input, cx| input.focus(window, cx));
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
            // tdesktop: `_replyTo` keeps Up for scrolling.
            || self.composer_ui.has_reply()
            || !self.pending_attachments().is_empty()
            || self.composer_ui.command_menu_open
            || !self.mention_menu_items(cx).is_empty()
        {
            return false;
        }
        // tdesktop `canWriteMessage()`.
        let Some(edit) = self.session().and_then(|s| {
            let chat = s.open_chat?;
            s.chats.get(&chat.0)?.can_post().then_some(())?;
            s.last_editable_message(chat)
        }) else {
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
        // The field starts from the live message. Not loaded yet: refuse, so
        // a submit can never blank the message with an empty field, and say
        // so with tdesktop's `lng_edit_error` toast.
        let Some(original) = self
            .session()
            .and_then(|session| session.edit_target_text(&edit))
        else {
            self.set_status_note(StatusNote::error(tr(keys::EDIT_ERROR)), cx);
            return;
        };
        self.clear_pending_attachments();
        self.composer_ui.edit_replace_as_file = false;
        let current = self.composer_markup(cx);
        // Flush while the reply is still set so a reply-only draft is not wiped.
        self.note_open_draft(false, cx);
        let reply = self.composer_ui.take_reply();
        let (edit, field, saved, stashed) =
            begin_edit_keeping_reply(current, edit, original, reply);
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
        // The `/` menu is off while a message is edited.
        self.sync_command_menu(cx);
        cx.notify();
    }

    pub(in crate::ui) fn clear_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // tdesktop cancelEditMessage → applyDraft(): restore normal draft.
        let saved = std::mem::take(&mut self.composer_ui.saved_edit_draft);
        let stashed = self.composer_ui.saved_edit_reply.take();
        let (_, restored) = cancel_edit_draft(self.composer_ui.pending_edit.take(), saved);
        let (restored, reply) = cancel_edit_keeping_reply(restored, stashed);
        self.composer_ui.set_reply(reply);
        self.set_composer_markup(&restored, window, cx);
        let restored = self.composer.read(cx).value().to_string();
        self.sync_composer_typing(&restored);
        self.sync_command_menu(cx);
        self.connection
            .set_note(StatusNote::silent(tr(keys::Q_CHAT_EDIT_CANCELLED)));
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
        self.composer_ui.set_reply(reply);
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
        if self
            .session()
            .is_some_and(|session| session.edit_target_gone(&edit))
        {
            self.clear_edit(window, cx);
            return;
        }
        // A media message still being sent keeps the new caption until
        // it has its real id (tdesktop edits the local item).
        let pending = self.session().is_some_and(|s| {
            s.histories
                .get(&edit.chat_id.0)
                .and_then(|history| history.messages.get(&edit.message_id.0))
                .is_some_and(|m| m.pending)
        });
        if let Some(live) = self.live.as_mut() {
            let driver = &mut live.driver;
            let result = if pending {
                driver.edit_pending_caption(&edit, &text)
            } else {
                driver.edit_snapshot(&edit, &text).map(|_| ())
            };
            match result {
                Ok(_) => {
                    self.finish_edit_restore_draft(window, cx);
                    self.connection.set_note(
                        self.send_started_note(
                            ComposerScheduling::None,
                            tr(keys::Q_CHAT_SAVING_EDIT),
                        ),
                    );
                }
                Err(quill::connect::ConnectSendError::TextTooLong { limit }) => {
                    // R8: tdesktop `lng_edit_limit_reached` — the edit stays
                    // open so the text can be shortened.
                    let over = quill::text_split::units_over_limit(&text, limit);
                    self.connection.set_note(StatusNote::info(tr_with(
                        keys::Q_CHAT_MESSAGE_TOO_LONG_MAX_CHARACTERS,
                        &[("limit", &limit.to_string()), ("over", &over.to_string())],
                    )));
                }
                Err(_) => {
                    self.connection
                        .set_note(StatusNote::error(tr(keys::Q_CHAT_COULD_NOT_EDIT_MESSAGE)));
                }
            }
            cx.notify();
            return;
        }
        if self.demo_session.is_some() {
            self.apply_demo_edit(&edit, text.trim());
            self.finish_edit_restore_draft(window, cx);
            self.connection.set_note(StatusNote::silent(tr(
                keys::Q_CHAT_DEMO_EDIT_APPLIED_LOCALLY_NO,
            )));
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
        self.connection
            .set_note(StatusNote::silent(tr(keys::Q_CHAT_DELETE_CANCELLED)));
        cx.notify();
    }

    pub(in crate::ui) fn confirm_delete(&mut self, cx: &mut Context<Self>) {
        let Some(confirm) = self.message_ui.pending_delete.take() else {
            return;
        };
        self.begin_vanish(confirm.chat_id, &[confirm.message_id]);
        if let Some(live) = self.live.as_mut() {
            let result = live.driver.delete_confirmed(&confirm);
            self.connection.set_note(match result {
                Ok(_) => StatusNote::silent(tr(keys::Q_CHAT_DELETING)),
                Err(_) => StatusNote::error(tr(keys::Q_CHAT_COULD_NOT_DELETE_MESSAGE)),
            });
        } else if self.demo_session.is_some() {
            self.apply_demo_delete(confirm.chat_id, confirm.message_id);
            self.connection.set_note(StatusNote::silent(tr(
                keys::Q_CHAT_DEMO_DELETE_APPLIED_LOCALLY_NO,
            )));
        }
        cx.notify();
    }

    pub(in crate::ui) fn clear_reply(&mut self, cx: &mut Context<Self>) {
        // tdesktop FieldHeader Escape / replyCancelled: header only — keep typed text.
        let kept = cancel_reply_draft(self.composer_ui.take_reply(), String::new()).0;
        self.composer_ui.set_reply(kept);
        self.share.reply_elsewhere_open = false;
        self.share.reply_quote_open = false;
        self.note_open_draft(true, cx);
        self.connection
            .set_note(StatusNote::silent(tr(keys::Q_CHAT_REPLY_CANCELLED)));
        cx.notify();
    }

    /// Sticker/GIF/voice sends consume the composer reply. Drop it from the UI
    /// and from the stored draft, and keep any unsent text.
    pub(in crate::ui) fn consume_sent_reply(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        self.composer_ui.set_reply(None);
        if self.composer_ui.pending_edit.is_some() {
            self.composer_ui.saved_edit_reply = None;
            return;
        }
        let (content, _) = self.composer_draft_parts(chat_id, cx);
        let topic = self.open_draft_topic(chat_id);
        self.save_chat_draft(chat_id, topic, content, None, false, cx);
    }
}
