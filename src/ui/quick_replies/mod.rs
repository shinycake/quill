//! Quick replies (Telegram Business shortcuts): the Settings dialog that lists
//! them and edits one shortcut's messages, and the composer `/` rows that
//! send one. Rules and requests: `quill::quick_replies`.
#![deny(clippy::indexing_slicing)]

use crate::ui::status_note::StatusNote;
mod build;
#[cfg(test)]
#[cfg(feature = "demo-capture")]
mod tests;

use super::app::QuillApp;
use gpui_kit::component::input::{InputState, TextareaState};
use gpui_kit::*;
use quill::ids::ChatId;
use quill::lang::{keys, tr};
use quill::quick_replies::{self as qr, NAME_TAKEN_LINE};
use std::rc::Rc;
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

/// What the open shortcut's screen holds beside its address (`View`): the
/// one thing it is doing, and the last refusal. A screen change replaces it
/// whole ([`QuillApp::quick_replies_go`]), so nothing outlives its screen.
#[derive(Clone, Default)]
pub(super) struct ShortcutPane {
    pub(super) stage: PaneStage,
    pub(super) error: Option<String>,
}

/// The poll that settles a request or waits for a shortcut's id (TDLib
/// answers on updates the poll picks up, not as one answer). The stage that
/// waits owns it, so leaving the stage (settled, the screen changed, the box
/// closed) ends the poll; clones share it.
#[derive(Clone)]
pub(super) struct Poll {
    _task: Rc<Task<()>>,
}

impl Poll {
    #[cfg(test)]
    pub(super) fn weak(&self) -> std::rc::Weak<Task<()>> {
        Rc::downgrade(&self._task)
    }
}

/// What the shortcut's screen is doing. Any other action replaces
/// `ConfirmDelete`, so the next press of Delete asks again.
#[derive(Clone)]
pub(super) enum PaneStage {
    /// The field edits the text of message `editing` (a new message when
    /// `None`).
    Editing { editing: Option<i64> },
    /// The text of the field is on its way to the server: the field is
    /// empty meanwhile, and a refusal puts it back. Sent as `request`; only
    /// that request's answer settles it.
    Saving {
        request: quill::ids::RequestId,
        text: String,
        editing: Option<i64>,
        _poll: Poll,
    },
    /// The server accepted the text of a shortcut named here whose id TDLib
    /// has not reported yet: the poll waits for it. Leaves when the id
    /// resolves, the screen changes or the box closes.
    Accepted { _poll: Poll },
    /// "Delete?" was pressed on a shortcut with messages: the next press
    /// deletes. `editing` is the edit still in the field.
    ConfirmDelete { editing: Option<i64> },
    /// The shortcut's delete went out as `request`; its answer leaves for
    /// the list, its refusal comes back to the field.
    Deleting {
        request: quill::ids::RequestId,
        _poll: Poll,
    },
}

impl Default for PaneStage {
    fn default() -> Self {
        Self::Editing { editing: None }
    }
}

impl PaneStage {
    /// The message whose text is in the field.
    pub(super) fn editing(&self) -> Option<i64> {
        match self {
            Self::Editing { editing } | Self::ConfirmDelete { editing } => *editing,
            Self::Saving { .. } | Self::Accepted { .. } | Self::Deleting { .. } => None,
        }
    }

    fn sending(&self) -> bool {
        matches!(self, Self::Saving { .. } | Self::Deleting { .. })
    }
}

#[derive(Clone)]
pub(super) struct QuickRepliesDialog {
    pub(super) view: View,
    pub(super) name_input: Entity<InputState>,
    pub(super) text_input: Entity<TextareaState>,
    pub(super) pane: ShortcutPane,
}

impl QuillApp {
    pub(super) fn open_quick_replies(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(tr(keys::REPLIES_ADD_PLACEHOLDER))
                .submit_on_enter(false)
        });
        let text_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder(tr(keys::REPLIES_MESSAGE_PLACEHOLDER))
                .auto_grow(2, 6)
                .submit_on_enter(false)
        });
        self.dialogs.quick_replies_dialog = Some(QuickRepliesDialog {
            view: View::List,
            name_input,
            text_input,
            pane: ShortcutPane::default(),
        });
        self.open_box(crate::ui::shell::DialogKind::QuickReplies);
        // A new look: the last refusal's line goes, and a shortcut whose
        // load failed is asked for once more.
        if let Some(session) = self.session_mut() {
            session.settings.quick_replies.reopen();
        }
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.ensure_quick_reply_shortcuts();
        }
        cx.notify();
    }

    pub(super) fn close_quick_replies(&mut self, cx: &mut Context<Self>) {
        if let Some(dialog) = self.dialogs.quick_replies_dialog.take() {
            self.forget_quick_reply_stage(dialog.pane.stage);
            cx.notify();
        }
    }

    /// The screen that sent a request went away: nobody waits for its
    /// answer.
    fn forget_quick_reply_stage(&mut self, stage: PaneStage) {
        if let PaneStage::Saving { request, .. } | PaneStage::Deleting { request, .. } = stage
            && let Some(live) = self.live.as_mut()
        {
            live.driver.session.acks.forget(request);
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
        let old = std::mem::take(&mut dialog.pane);
        self.forget_quick_reply_stage(old.stage);
        let Some(dialog) = self.quick_replies_mut() else {
            return;
        };
        let (name_input, text_input) = (dialog.name_input.clone(), dialog.text_input.clone());
        name_input.update(cx, |input, cx| input.set_value(name, window, cx));
        text_input.update(cx, |input, cx| input.set_value("", window, cx));
        self.quick_replies_pump(window, cx);
        cx.notify();
    }

    fn quick_replies_error(&mut self, line: impl Into<String>, cx: &mut Context<Self>) {
        if let Some(dialog) = self.quick_replies_mut() {
            dialog.pane.error = Some(line.into());
        }
        cx.notify();
    }

    /// Anything that needs a live session says so in demo mode.
    fn quick_replies_live(&mut self, cx: &mut Context<Self>) -> bool {
        if self.live.is_some() {
            return true;
        }
        self.quick_replies_error(tr(keys::Q_OPS_QUICK_REPLIES_NEED_A_LIVE), cx);
        false
    }

    /// Settle what the open shortcut still waits for: its id (when it was
    /// just created) and its messages. `true` when nothing is left to do.
    pub(super) fn quick_replies_pump(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let saving = self.settle_quick_reply_save(window, cx);
        let Some(dialog) = self.dialogs.quick_replies_dialog.as_ref() else {
            return true;
        };
        let View::Shortcut { name, id } = dialog.view.clone() else {
            return !saving;
        };
        let resolved = id.or_else(|| {
            self.session()
                .and_then(|s| s.settings.quick_replies.by_name(&name))
                .map(|s| s.id)
        });
        let Some(shortcut_id) = resolved else {
            // The id is awaited only for a text the server accepted (its
            // update is on its way); with nothing sent or the send
            // refused there is nothing to wait for, and neither is there
            // behind the lock screen.
            let waiting = self
                .dialogs
                .quick_replies_dialog
                .as_ref()
                .is_some_and(|dialog| matches!(dialog.pane.stage, PaneStage::Accepted { .. }));
            let locked = self.refused_while_locked();
            if locked && waiting {
                // Nothing resumes the wait behind the lock: it is let go,
                // and its poll with it.
                if let Some(dialog) = self.quick_replies_mut() {
                    dialog.pane.stage = PaneStage::default();
                }
            }
            // A request still out keeps its poll, locked or not: its answer
            // settles here.
            return !saving && (locked || !waiting);
        };
        if let Some(dialog) = self.quick_replies_mut() {
            if let View::Shortcut { id, .. } = &mut dialog.view {
                *id = Some(shortcut_id);
            }
            if matches!(dialog.pane.stage, PaneStage::Accepted { .. }) {
                dialog.pane.stage = PaneStage::default();
            }
        }
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.ensure_quick_reply_messages(shortcut_id);
        }
        // A failed load is settled too: the poll stops, and the next open
        // of the box asks again.
        let settled = self
            .session()
            .is_some_and(|s| s.settings.quick_replies.messages_settled(shortcut_id));
        cx.notify();
        (settled || self.live.is_none()) && !saving
    }

    /// The answer of the request the screen waits for: an accepted text is
    /// done and a refused one gives the text (and its edit) back to the
    /// field; an accepted delete leaves for the list and a refused one
    /// comes back to the field with its line. Returns whether the request
    /// is still out.
    fn settle_quick_reply_save(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(dialog) = self.dialogs.quick_replies_dialog.as_ref() else {
            return false;
        };
        let stage = dialog.pane.stage.clone();
        let (PaneStage::Saving { request, .. } | PaneStage::Deleting { request, .. }) = stage
        else {
            return false;
        };
        let answered = self
            .live
            .as_ref()
            .is_none_or(|live| live.driver.session.requests.get(request).is_none());
        if !answered {
            return true;
        }
        // The answer of this request, not the dialog's last error: a request
        // that left flight with no answer was not accepted either.
        let accepted = self.live.as_mut().is_none_or(|live| {
            live.driver.session.acks.take(request) == Some(quill::state::Answer::Done)
        });
        match stage {
            PaneStage::Deleting { .. } if accepted => {
                self.quick_replies_go(View::List, window, cx);
                return false;
            }
            PaneStage::Deleting { .. } => {
                if let Some(dialog) = self.quick_replies_mut() {
                    dialog.pane.stage = PaneStage::default();
                }
                self.quick_replies_error(tr(keys::Q_OPS_COULD_NOT_DELETE_THE_SHORTCUT), cx);
            }
            PaneStage::Saving {
                text,
                editing,
                _poll: poll,
                ..
            } => {
                if let Some(dialog) = self.quick_replies_mut() {
                    let awaiting_id = matches!(dialog.view, View::Shortcut { id: None, .. });
                    dialog.pane.stage = if accepted && awaiting_id {
                        // The same poll goes on, waiting for the id.
                        PaneStage::Accepted { _poll: poll }
                    } else if accepted {
                        PaneStage::default()
                    } else {
                        PaneStage::Editing { editing }
                    };
                    if !accepted {
                        let input = dialog.text_input.clone();
                        input.update(cx, |input, cx| input.set_value(text, window, cx));
                    }
                }
            }
            _ => {}
        }
        cx.notify();
        false
    }

    /// Poll for the answer of a request and the id and messages of a shortcut
    /// the server is about to report (the updates arrive on the poll, not as
    /// an answer), until both are in, the load fails, or its stage ends. The
    /// stage that waits owns the returned poll.
    fn quick_replies_poll(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Poll {
        let task = self.spawn_screen_in(window, cx, async move |this, cx| {
            let mut seen = quill::wake::APPLIED.epoch();
            loop {
                // Rests on the signal the poll pass pokes after it applied
                // what TDLib sent: the answer and the shortcut's update are
                // in the session by then. (`UPDATES` is poked before the
                // drain, so this pump would find nothing.) The long
                // fallback only covers an answer that arrives without a pass.
                super::wake_wait::sleep_or_wake(
                    cx.background_executor(),
                    &quill::wake::APPLIED,
                    seen,
                    Duration::from_secs(2),
                )
                .await;
                // Taken before the pass, so a poke during it ends the next rest.
                seen = quill::wake::APPLIED.epoch();
                let Ok(done) =
                    this.update_in(cx, |this, window, cx| this.quick_replies_pump(window, cx))
                else {
                    return;
                };
                if done {
                    break;
                }
            }
        });
        Poll {
            _task: Rc::new(task),
        }
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
                    self.quick_replies_error(tr(keys::Q_OPS_COULD_NOT_SEND_THE_NEW), cx);
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
        if dialog.pane.stage.sending() {
            // The text before it is still on its way.
            return;
        }
        let editing = dialog.pane.stage.editing();
        let text = dialog.text_input.read(cx).value().trim().to_string();
        if text.is_empty() {
            self.quick_replies_error(tr(keys::Q_OPS_WRITE_THE_MESSAGE_FIRST), cx);
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
        let Some(Ok(request)) = sent else {
            self.quick_replies_error(tr(keys::Q_OPS_COULD_NOT_SEND_THE_MESSAGE_2), cx);
            return;
        };
        // The same change is already on its way: it answers for this one.
        let Some(request) = request else {
            return;
        };
        if let Some(live) = self.live.as_mut() {
            live.driver.session.acks.expect(request);
        }
        let poll = self.quick_replies_poll(window, cx);
        if let Some(dialog) = self.quick_replies_mut() {
            dialog.pane.stage = PaneStage::Saving {
                request,
                text,
                editing,
                _poll: poll,
            };
            dialog.pane.error = None;
            let input = dialog.text_input.clone();
            input.update(cx, |input, cx| input.set_value("", window, cx));
        }
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
        if let Some(dialog) = self.quick_replies_mut()
            && !dialog.pane.stage.sending()
        {
            dialog.pane.stage = PaneStage::Editing {
                editing: Some(message_id),
            };
            dialog.pane.error = None;
            let input = dialog.text_input.clone();
            input.update(cx, |input, cx| input.set_value(text, window, cx));
        }
        cx.notify();
    }

    pub(super) fn cancel_quick_reply_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(dialog) = self.quick_replies_mut()
            && !dialog.pane.stage.sending()
        {
            dialog.pane.stage = PaneStage::default();
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
        if let Some(dialog) = self.quick_replies_mut()
            && let PaneStage::ConfirmDelete { editing } = dialog.pane.stage
        {
            // Another action: the question is not asked any more.
            dialog.pane.stage = PaneStage::Editing { editing };
        }
        if let Some(live) = self.live.as_mut()
            && live
                .driver
                .delete_quick_reply_message(shortcut_id, message_id)
                .is_err()
        {
            self.quick_replies_error(tr(keys::Q_OPS_COULD_NOT_DELETE_THE_MESSAGE), cx);
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
        if dialog.pane.stage.sending() {
            return;
        }
        let has_messages = self
            .session()
            .is_some_and(|s| !s.settings.quick_replies.messages_of(id).is_empty());
        let confirmed = matches!(dialog.pane.stage, PaneStage::ConfirmDelete { .. });
        if has_messages && !confirmed {
            if let Some(dialog) = self.quick_replies_mut() {
                dialog.pane.stage = PaneStage::ConfirmDelete {
                    editing: dialog.pane.stage.editing(),
                };
            }
            cx.notify();
            return;
        }
        if !self.quick_replies_live(cx) {
            return;
        }
        let sent = self
            .live
            .as_mut()
            .map(|live| live.driver.delete_quick_reply_shortcut(id));
        match sent {
            Some(Ok(Some(request))) => {
                if let Some(live) = self.live.as_mut() {
                    live.driver.session.acks.expect(request);
                }
                let poll = self.quick_replies_poll(window, cx);
                if let Some(dialog) = self.quick_replies_mut() {
                    dialog.pane.stage = PaneStage::Deleting {
                        request,
                        _poll: poll,
                    };
                    dialog.pane.error = None;
                }
            }
            // The same delete is already on its way: it answers for this one.
            Some(Ok(None)) | None => {}
            Some(Err(_)) => {
                self.quick_replies_error(tr(keys::Q_OPS_COULD_NOT_DELETE_THE_SHORTCUT), cx);
                return;
            }
        }
        cx.notify();
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
        let Some(chat_id) = self.viewed_chat() else {
            return;
        };
        let edit = shortcut_id == qr::EDIT_ROW;
        // The `/token` leaves the composer, other text stays (tdesktop).
        let token = self.command_trigger(cx);
        self.close_command_menu(cx);
        if let Some((range, _)) = token {
            self.replace_composer_text(range, "", window, cx);
        }
        self.composer
            .update(cx, |input, cx| input.focus(window, cx));
        if edit {
            self.open_quick_replies(window, cx);
        } else {
            self.send_quick_reply_to(chat_id, shortcut_id, cx);
        }
    }

    fn send_quick_reply_to(&mut self, chat_id: ChatId, shortcut_id: i32, cx: &mut Context<Self>) {
        self.screen.connection.set_note_for(
            &self.screen.windows,
            match self.live.as_mut() {
                Some(live) => match live.driver.send_quick_reply(chat_id, shortcut_id) {
                    Ok(()) => StatusNote::silent(tr(keys::Q_OPS_SENDING_QUICK_REPLY)),
                    Err(_) => StatusNote::error(tr(keys::Q_OPS_COULD_NOT_SEND_THE_QUICK)),
                },
                None => StatusNote::info(tr(keys::Q_OPS_DEMO_MODE_SENDING_A_QUICK)),
            },
        );
        cx.notify();
    }
}
