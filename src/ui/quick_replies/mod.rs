//! Quick replies (Telegram Business shortcuts): the Settings dialog that lists
//! them and edits one shortcut's messages, and the composer `/` rows that
//! send one. Rules and requests: `quill::quick_replies`.

mod build;
#[cfg(all(test, feature = "demo-capture"))]
mod tests;

use super::app::QuillApp;
use gpui_kit::component::input::{InputState, TextareaState};
use gpui_kit::*;
use quill::ids::ChatId;
use quill::quick_replies::{self as qr, NAME_TAKEN_LINE};
use std::time::Duration;

/// Which screen of the dialog is showing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum View {
    List,
    /// The name field: a new shortcut, or the rename of `rename_of`.
    Name {
        rename_of: Option<i32>,
    },
    /// One shortcut's messages. `id` is `None` until TDLib reports a
    /// shortcut that was named here and has no message yet.
    Shortcut {
        name: String,
        id: Option<i32>,
    },
}

#[derive(Clone)]
pub(super) struct QuickRepliesDialog {
    pub(super) view: View,
    pub(super) name_input: Entity<InputState>,
    pub(super) text_input: Entity<TextareaState>,
    /// The message whose text is in the field.
    pub(super) editing: Option<i64>,
    pub(super) confirm_delete: bool,
    pub(super) local_error: Option<String>,
}

impl QuillApp {
    pub(super) fn open_quick_replies(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Shortcut")
                .submit_on_enter(false)
        });
        let text_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Add a Quick Reply")
                .auto_grow(2, 6)
                .submit_on_enter(false)
        });
        self.dialogs.quick_replies_dialog = Some(QuickRepliesDialog {
            view: View::List,
            name_input,
            text_input,
            editing: None,
            confirm_delete: false,
            local_error: None,
        });
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.ensure_quick_reply_shortcuts();
        }
        cx.notify();
    }

    pub(super) fn close_quick_replies(&mut self, cx: &mut Context<Self>) {
        if self.dialogs.quick_replies_dialog.take().is_some() {
            cx.notify();
        }
    }

    fn quick_replies_mut(&mut self) -> Option<&mut QuickRepliesDialog> {
        self.dialogs.quick_replies_dialog.as_mut()
    }

    /// Show another screen; the fields and prompts start clean.
    pub(super) fn quick_replies_go(
        &mut self,
        view: View,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let name = match &view {
            View::Name {
                rename_of: Some(id),
            } => self
                .session()
                .and_then(|s| s.settings.quick_replies.shortcuts.get(id))
                .map(|s| s.name.clone())
                .unwrap_or_default(),
            _ => String::new(),
        };
        let Some(dialog) = self.quick_replies_mut() else {
            return;
        };
        dialog.view = view;
        dialog.editing = None;
        dialog.confirm_delete = false;
        dialog.local_error = None;
        let (name_input, text_input) = (dialog.name_input.clone(), dialog.text_input.clone());
        name_input.update(cx, |input, cx| input.set_value(name, window, cx));
        text_input.update(cx, |input, cx| input.set_value("", window, cx));
        self.quick_replies_pump(cx);
        cx.notify();
    }

    fn quick_replies_error(&mut self, line: impl Into<String>, cx: &mut Context<Self>) {
        if let Some(dialog) = self.quick_replies_mut() {
            dialog.local_error = Some(line.into());
        }
        cx.notify();
    }

    /// Anything that needs a live session says so in demo mode.
    fn quick_replies_live(&mut self, cx: &mut Context<Self>) -> bool {
        if self.live.is_some() {
            return true;
        }
        self.quick_replies_error("Quick replies need a live session.", cx);
        false
    }

    /// Settle what the open shortcut still waits for: its id (when it was
    /// just created) and its messages. `true` when nothing is left to do.
    pub(super) fn quick_replies_pump(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(dialog) = self.dialogs.quick_replies_dialog.as_ref() else {
            return true;
        };
        let View::Shortcut { name, id } = dialog.view.clone() else {
            return true;
        };
        let resolved = id.or_else(|| {
            self.session()
                .and_then(|s| s.settings.quick_replies.by_name(&name))
                .map(|s| s.id)
        });
        let Some(shortcut_id) = resolved else {
            return false;
        };
        if let Some(dialog) = self.quick_replies_mut()
            && let View::Shortcut { id, .. } = &mut dialog.view
        {
            *id = Some(shortcut_id);
        }
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.ensure_quick_reply_messages(shortcut_id);
        }
        let loaded = self
            .session()
            .is_some_and(|s| s.settings.quick_replies.messages.contains_key(&shortcut_id));
        cx.notify();
        loaded || self.live.is_none()
    }

    /// Poll for the id and the messages of a shortcut the server is about to
    /// report (the updates arrive on the poll, not as an answer).
    pub(super) fn quick_replies_watch(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        cx.spawn_in(window, async move |this, cx| {
            for _ in 0..40 {
                cx.background_executor()
                    .timer(Duration::from_millis(200))
                    .await;
                let Ok(done) = this.update(cx, |this, cx| this.quick_replies_pump(cx)) else {
                    break;
                };
                if done {
                    break;
                }
            }
        })
        .detach();
    }

    /// Save of the name field: a new shortcut or a rename.
    pub(super) fn submit_quick_reply_name(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(dialog) = self.dialogs.quick_replies_dialog.as_ref() else {
            return;
        };
        let View::Name { rename_of } = dialog.view.clone() else {
            return;
        };
        let raw = dialog.name_input.read(cx).value().to_string();
        let name = match qr::validate_name(&raw) {
            Ok(name) => name,
            Err(problem) => {
                self.quick_replies_error(problem.line(), cx);
                return;
            }
        };
        let existing = self
            .session()
            .and_then(|s| s.settings.quick_replies.by_name(&name))
            .map(|s| (s.id, s.name.clone()));
        match (rename_of, existing) {
            // tdesktop opens the shortcut that already has this name.
            (None, Some((id, name))) => {
                self.quick_replies_go(View::Shortcut { name, id: Some(id) }, window, cx);
            }
            (None, None) => {
                self.quick_replies_go(View::Shortcut { name, id: None }, window, cx);
            }
            (Some(id), Some((other, _))) if other != id => {
                self.quick_replies_error(NAME_TAKEN_LINE, cx);
            }
            (Some(id), _) => {
                if !self.quick_replies_live(cx) {
                    return;
                }
                if let Some(live) = self.live.as_mut()
                    && live.driver.rename_quick_reply(id, &name).is_err()
                {
                    self.quick_replies_error("Could not send the new name.", cx);
                    return;
                }
                self.quick_replies_go(View::Shortcut { name, id: Some(id) }, window, cx);
            }
        }
    }

    /// Add the text in the field as a message, or save the edit of one.
    pub(super) fn submit_quick_reply_text(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(dialog) = self.dialogs.quick_replies_dialog.as_ref() else {
            return;
        };
        let View::Shortcut { name, id } = dialog.view.clone() else {
            return;
        };
        let editing = dialog.editing;
        let text = dialog.text_input.read(cx).value().trim().to_string();
        if text.is_empty() {
            self.quick_replies_error("Write the message first.", cx);
            return;
        }
        if !self.quick_replies_live(cx) {
            return;
        }
        let sent = match (editing, id) {
            (Some(message_id), Some(id)) => self
                .live
                .as_mut()
                .map(|live| live.driver.edit_quick_reply_text(id, message_id, &text)),
            (None, _) => self
                .live
                .as_mut()
                .map(|live| live.driver.add_quick_reply_text(&name, &text)),
            _ => None,
        };
        if !matches!(sent, Some(Ok(()))) {
            self.quick_replies_error("Could not send the message.", cx);
            return;
        }
        if let Some(dialog) = self.quick_replies_mut() {
            dialog.editing = None;
            dialog.local_error = None;
            let input = dialog.text_input.clone();
            input.update(cx, |input, cx| input.set_value("", window, cx));
        }
        self.quick_replies_watch(window, cx);
        cx.notify();
    }

    /// Put a message's text in the field for editing.
    pub(super) fn begin_quick_reply_edit(
        &mut self,
        message_id: i64,
        text: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(dialog) = self.quick_replies_mut() {
            dialog.editing = Some(message_id);
            dialog.local_error = None;
            let input = dialog.text_input.clone();
            input.update(cx, |input, cx| input.set_value(text, window, cx));
        }
        cx.notify();
    }

    pub(super) fn cancel_quick_reply_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(dialog) = self.quick_replies_mut() {
            dialog.editing = None;
            let input = dialog.text_input.clone();
            input.update(cx, |input, cx| input.set_value("", window, cx));
        }
        cx.notify();
    }

    pub(super) fn delete_quick_reply_message(
        &mut self,
        shortcut_id: i32,
        message_id: i64,
        cx: &mut Context<Self>,
    ) {
        if !self.quick_replies_live(cx) {
            return;
        }
        if let Some(live) = self.live.as_mut()
            && live
                .driver
                .delete_quick_reply_message(shortcut_id, message_id)
                .is_err()
        {
            self.quick_replies_error("Could not delete the message.", cx);
        }
        cx.notify();
    }

    /// Delete the shortcut; a shortcut with messages asks first (tdesktop).
    pub(super) fn delete_quick_reply_shortcut(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(dialog) = self.dialogs.quick_replies_dialog.as_ref() else {
            return;
        };
        let View::Shortcut { id, .. } = dialog.view.clone() else {
            return;
        };
        let Some(id) = id else {
            // Nothing exists on the server yet.
            self.quick_replies_go(View::List, window, cx);
            return;
        };
        let has_messages = self
            .session()
            .is_some_and(|s| !s.settings.quick_replies.messages_of(id).is_empty());
        let confirmed = dialog.confirm_delete;
        if has_messages && !confirmed {
            if let Some(dialog) = self.quick_replies_mut() {
                dialog.confirm_delete = true;
            }
            cx.notify();
            return;
        }
        if !self.quick_replies_live(cx) {
            return;
        }
        if let Some(live) = self.live.as_mut()
            && live.driver.delete_quick_reply_shortcut(id).is_err()
        {
            self.quick_replies_error("Could not delete the shortcut.", cx);
            return;
        }
        self.quick_replies_go(View::List, window, cx);
    }

    /// The first `/` in a chat asks for the shortcuts (once per session).
    pub(in crate::ui) fn load_quick_replies_for_menu(&mut self) {
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.ensure_quick_reply_shortcuts();
        }
    }

    /// A `/` row was chosen in a private chat: send the shortcut's messages.
    pub(in crate::ui) fn send_quick_reply_row(
        &mut self,
        shortcut_id: i32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(chat_id) = self.session().and_then(|s| s.open_chat) else {
            return;
        };
        let edit = shortcut_id == qr::EDIT_ROW;
        // The `/token` leaves the composer, other text stays (tdesktop).
        let current = self.composer_markup(cx);
        let rest = quill::composer::strip_command_menu_trigger(&current)
            .unwrap_or(current.as_str())
            .trim_end()
            .to_string();
        self.close_command_menu(cx);
        self.set_composer_markup(&rest, window, cx);
        self.composer
            .update(cx, |input, cx| input.focus(window, cx));
        if edit {
            self.open_quick_replies(window, cx);
        } else {
            self.send_quick_reply_to(chat_id, shortcut_id, cx);
        }
    }

    fn send_quick_reply_to(&mut self, chat_id: ChatId, shortcut_id: i32, cx: &mut Context<Self>) {
        self.connection.status_note = match self.live.as_mut() {
            Some(live) => match live.driver.send_quick_reply(chat_id, shortcut_id) {
                Ok(()) => "Sending quick reply…".into(),
                Err(_) => "Could not send the quick reply.".into(),
            },
            None => "Demo mode: sending a quick reply needs a live session.".into(),
        };
        cx.notify();
    }
}
