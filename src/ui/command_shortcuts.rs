//! The shortcut commands of tdesktop's `core/shortcuts.cpp` that act on the
//! open chat's composer, its scheduled messages and its recent actions, and
//! the one that brings back a closed chat window: "Just send", "Silent
//! send", "Schedule", "Scheduled messages", "Toggle link preview", "Record
//! round message", "Archive chat", "Group/Channel recent actions",
//! "Apply AI style to text" (`ai_compose::apply_in_place`) and "Reopen
//! closed window". Like tdesktop's, none has a default chord except the
//! last one (Ctrl/Cmd+Shift+T); the others are set in Settings >
//! Appearance > Keyboard shortcuts.
//!
//! Each handler returns `false` when its command does not apply (tdesktop's
//! `request->check` found no handler), so the key reaches whatever else
//! wants it.
#![deny(clippy::indexing_slicing)]

use super::actions::{
    AiComposeApply, ArchiveOpenChat, JustSend, RecordRoundMessage, ReopenClosedWindow,
    ScheduleSend, SendSilently, ShowAdminLog, ShowScheduledMessages, ToggleLinkPreview,
};
use super::app::QuillApp;
use gpui_kit::*;
use quill::ids::ChatId;
use quill::schedule::ScheduleKind;
use quill::state::InfoPanelTarget;
use quill::telegram::envelope::ChatKind;

impl QuillApp {
    /// Whether the composer holds something to send (the send button is in
    /// its "Send" state, `computeSendButtonType() == Send`), so the send
    /// commands of the button's menu apply.
    fn composer_sendable(&self, cx: &App) -> bool {
        self.composer_ui.pending_edit.is_none()
            && !self.recording_here()
            && (!self.composer.read(cx).value().trim().is_empty()
                || !self.pending_attachments().is_empty()
                || self.forward_bar_here())
    }

    /// The open chat when the viewer may write there (`canWriteMessage()`).
    pub(super) fn writable_open_chat(&self) -> Option<quill::ids::ChatId> {
        let session = self.session()?;
        let chat = session.chats.get(&session.open_chat?.0)?;
        (chat.can_post() && !session.is_frozen()).then_some(chat.id)
    }

    /// "Scheduled messages": the open chat's scheduled list
    /// (`HistoryWidget`: `_canSendMessages && request->check(ShowScheduled)`).
    pub(super) fn show_scheduled_by_key(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.shortcut_blocked(window, cx) || self.writable_open_chat().is_none() {
            return false;
        }
        self.composer_ui.scheduled_selected.clear();
        self.open_scheduled_dialog(cx);
        true
    }

    /// "Just send": what a click on the send button does
    /// (`SendMenu::SetupMenuAndShortcuts`, the `JustSendMessage` branch).
    pub(super) fn just_send_by_key(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.shortcut_blocked(window, cx)
            || self.writable_open_chat().is_none()
            || !self.composer_sendable(cx)
        {
            return false;
        }
        self.submit_from_send_button(window, cx);
        true
    }

    /// "Silent send": send now without a notification. Reminders (Saved
    /// Messages) have no silent send (`now != Type::Reminder`).
    pub(super) fn send_silently_by_key(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.shortcut_blocked(window, cx)
            || self.writable_open_chat().is_none()
            || !self.composer_sendable(cx)
            || self.schedule_kind() == ScheduleKind::Reminder
        {
            return false;
        }
        self.composer_ui.send_silent_once = true;
        self.submit_from_send_button(window, cx);
        self.composer_ui.send_silent_once = false;
        true
    }

    /// "Schedule": ask for the time, then send (`ActionType::Schedule`; the
    /// schedule box sends when its button is pressed).
    pub(super) fn schedule_send_by_key(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.shortcut_blocked(window, cx)
            || self.writable_open_chat().is_none()
            || !self.composer_sendable(cx)
        {
            return false;
        }
        self.open_schedule_picker(super::scheduled::ScheduleTarget::Composer, window, cx);
        if let Some(picker) = self.composer_ui.schedule_picker.as_mut() {
            picker.send_after = true;
        }
        true
    }

    /// "Toggle link preview": drop the preview, or bring it back with the
    /// default choices (`_preview->apply({ .removed = true })` and
    /// `apply({}, true)`). Applies while the text has a link or the preview
    /// was removed.
    pub(super) fn toggle_link_preview_by_key(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.shortcut_blocked(window, cx) || self.writable_open_chat().is_none() {
            return false;
        }
        let text = self.composer.read(cx).value().to_string();
        let choice = self.preview_choice();
        if choice.disabled {
            self.set_preview_choice(quill::composer::LinkPreviewChoice::default());
        } else if quill::composer::find_urls(&text).is_empty() {
            return false;
        } else {
            self.set_preview_choice(quill::composer::LinkPreviewChoice {
                disabled: true,
                ..choice
            });
        }
        cx.notify();
        true
    }

    /// "Record round message": start a video message and keep it recording
    /// (`startRecordingAndLock(true)`). Applies where the record button
    /// shows: an empty composer.
    pub(super) fn record_round_by_key(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.shortcut_blocked(window, cx)
            || self.writable_open_chat().is_none()
            || self.composer_ui.pending_edit.is_some()
            || self.recording_active()
            || !self.composer.read(cx).value().trim().is_empty()
            || !self.pending_attachments().is_empty()
        {
            return false;
        }
        self.recording.lock_next = true;
        self.start_video_note_recording(cx);
        true
    }

    /// "Archive chat": archive the open chat, or take it out of the archive
    /// (`Window::ToggleHistoryArchived`), where the chat list is.
    pub(super) fn archive_chat_by_key(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.shortcut_blocked(window, cx) {
            return false;
        }
        let Some(chat_id) = self.session().and_then(|s| s.open_chat) else {
            return false;
        };
        self.toggle_archive(chat_id, cx);
        true
    }

    /// "Group/Channel Recent Actions": for the administrators and the owner
    /// of a channel or supergroup. Quill lists the actions in the chat's
    /// info panel, so that is what opens.
    pub(super) fn admin_log_by_key(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.shortcut_blocked(window, cx) {
            return false;
        }
        let Some(session) = self.session() else {
            return false;
        };
        let Some(chat_id) = session.open_chat else {
            return false;
        };
        let Some(ChatKind::Supergroup { supergroup_id, .. }) =
            session.chats.get(&chat_id.0).map(|chat| chat.kind.clone())
        else {
            return false;
        };
        if !session.chat_can_view_event_log(chat_id) {
            return false;
        }
        self.open_info_panel_target(InfoPanelTarget::Supergroup(supergroup_id), window, cx);
        true
    }

    /// "Reopen closed window": the chat window closed last comes back where
    /// it was (`SavedWindows::reopenLastClosed`).
    pub(super) fn reopen_closed_window_by_key(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.account.passcode.locked {
            return false;
        }
        self.reopen_last_closed_chat_window(window, cx)
    }
}

/// Runs `method` for an action and lets the key go on when the command
/// does not apply.
macro_rules! command {
    ($el:expr, $cx:expr, $action:ty, $method:ident) => {
        $el.on_action($cx.listener(|this, _: &$action, window, cx| {
            if !this.$method(window, cx) {
                cx.propagate();
            }
        }))
    };
}

/// The same for a chat window: its chat's state is put in place first.
macro_rules! window_command {
    ($el:expr, $owner:expr, $chat:expr, $action:ty, $method:ident) => {{
        let owner = $owner.clone();
        let chat_id = $chat;
        $el.on_action(move |_: &$action, window, cx| {
            let handled = owner
                .update(cx, |app, cx| {
                    app.in_window_scope(Some(chat_id), cx, |app, cx| app.$method(window, cx))
                })
                .ok()
                .flatten()
                .unwrap_or(false);
            if !handled {
                cx.propagate();
            }
        })
    }};
}

/// Binds the commands above to a window's root element.
pub(super) trait CommandActions: InteractiveElement + Sized {
    /// The main window: every command.
    fn command_actions(self, cx: &mut Context<QuillApp>) -> Self {
        let el = command!(self, cx, JustSend, just_send_by_key);
        let el = command!(el, cx, SendSilently, send_silently_by_key);
        let el = command!(el, cx, ScheduleSend, schedule_send_by_key);
        let el = command!(el, cx, ShowScheduledMessages, show_scheduled_by_key);
        let el = command!(el, cx, ToggleLinkPreview, toggle_link_preview_by_key);
        let el = command!(el, cx, RecordRoundMessage, record_round_by_key);
        let el = command!(el, cx, ArchiveOpenChat, archive_chat_by_key);
        let el = command!(el, cx, ShowAdminLog, admin_log_by_key);
        let el = command!(el, cx, AiComposeApply, ai_compose_apply_by_key);
        command!(el, cx, ReopenClosedWindow, reopen_closed_window_by_key)
    }

    /// A chat window: no chat list (archiving) and no recording bar.
    fn chat_window_command_actions(self, owner: WeakEntity<QuillApp>, chat_id: ChatId) -> Self {
        let el = window_command!(self, owner, chat_id, JustSend, just_send_by_key);
        let el = window_command!(el, owner, chat_id, SendSilently, send_silently_by_key);
        let el = window_command!(el, owner, chat_id, ScheduleSend, schedule_send_by_key);
        let el = window_command!(
            el,
            owner,
            chat_id,
            ShowScheduledMessages,
            show_scheduled_by_key
        );
        let el = window_command!(
            el,
            owner,
            chat_id,
            ToggleLinkPreview,
            toggle_link_preview_by_key
        );
        let el = window_command!(el, owner, chat_id, ShowAdminLog, admin_log_by_key);
        window_command!(
            el,
            owner,
            chat_id,
            ReopenClosedWindow,
            reopen_closed_window_by_key
        )
    }
}

impl<E: InteractiveElement + Sized> CommandActions for E {}
