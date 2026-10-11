//! Connect driver: quick reply shortcuts (`crate::quick_replies`).
use super::*;
use crate::ids::{ChatId, RequestId};
use crate::quick_replies as qr;
use crate::state::{RequestPurpose, SettingsPurpose};

impl<S: JsonSender> ConnectDriver<S> {
    fn quick_replies_ready(&self) -> Result<(), ConnectSendError> {
        if self.chats_path_active() {
            Ok(())
        } else {
            Err(ConnectSendError::InvalidRequest)
        }
    }

    fn send_quick_reply_request(
        &mut self,
        purpose: RequestPurpose,
        chat_id: Option<ChatId>,
        build: impl FnOnce(&str) -> String,
    ) -> Result<RequestId, ConnectSendError> {
        self.quick_replies_ready()?;
        let extra = self.session.request(purpose, chat_id);
        let json = build(&extra.as_extra());
        self.send_json_request(extra, &json)
    }

    /// `loadQuickReplyShortcuts`, once per session unless a load failed.
    pub fn ensure_quick_reply_shortcuts(&mut self) -> Result<(), ConnectSendError> {
        if self.session.settings.quick_replies.shortcuts_requested {
            return Ok(());
        }
        self.session.settings.quick_replies.shortcuts_requested = true;
        let sent = self.send_quick_reply_request(
            RequestPurpose::Settings(SettingsPurpose::LoadQuickReplyShortcuts),
            None,
            qr::load_shortcuts_request,
        );
        if sent.is_err() {
            self.session.settings.quick_replies.shortcuts_requested = false;
        }
        sent.map(|_| ())
    }

    /// `loadQuickReplyShortcutMessages`, once per shortcut unless a load
    /// failed.
    pub fn ensure_quick_reply_messages(
        &mut self,
        shortcut_id: i32,
    ) -> Result<(), ConnectSendError> {
        if !self
            .session
            .settings
            .quick_replies
            .messages_requested
            .insert(shortcut_id)
        {
            return Ok(());
        }
        let sent = self.send_quick_reply_request(
            RequestPurpose::Settings(SettingsPurpose::LoadQuickReplyMessages { shortcut_id }),
            None,
            |extra| qr::load_messages_request(extra, shortcut_id),
        );
        if sent.is_err() {
            self.session
                .settings
                .quick_replies
                .messages_requested
                .remove(&shortcut_id);
        }
        sent.map(|_| ())
    }

    /// Add a text message; the shortcut `name` is created when new.
    pub fn add_quick_reply_text(&mut self, name: &str, text: &str) -> Result<(), ConnectSendError> {
        let name = qr::validate_name(name).map_err(|_| ConnectSendError::InvalidRequest)?;
        if text.trim().is_empty() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.settings.quick_replies.error = None;
        self.send_quick_reply_request(
            RequestPurpose::Settings(SettingsPurpose::MutateQuickReplies),
            None,
            |extra| qr::add_message_request(extra, &name, text),
        )
        .map(|_| ())
    }

    pub fn edit_quick_reply_text(
        &mut self,
        shortcut_id: i32,
        message_id: i64,
        text: &str,
    ) -> Result<(), ConnectSendError> {
        if text.trim().is_empty() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.settings.quick_replies.error = None;
        self.send_quick_reply_request(
            RequestPurpose::Settings(SettingsPurpose::MutateQuickReplies),
            None,
            |extra| qr::edit_message_request(extra, shortcut_id, message_id, text),
        )
        .map(|_| ())
    }

    /// Delete a message; the list updates when TDLib reports it. Deleting a
    /// shortcut's last message removes the shortcut on the server too.
    pub fn delete_quick_reply_message(
        &mut self,
        shortcut_id: i32,
        message_id: i64,
    ) -> Result<(), ConnectSendError> {
        self.session.settings.quick_replies.error = None;
        self.send_quick_reply_request(
            RequestPurpose::Settings(SettingsPurpose::MutateQuickReplies),
            None,
            |extra| qr::delete_messages_request(extra, shortcut_id, &[message_id]),
        )
        .map(|_| ())
    }

    pub fn rename_quick_reply(
        &mut self,
        shortcut_id: i32,
        name: &str,
    ) -> Result<(), ConnectSendError> {
        let name = qr::validate_name(name).map_err(|_| ConnectSendError::InvalidRequest)?;
        self.session.settings.quick_replies.error = None;
        self.send_quick_reply_request(
            RequestPurpose::Settings(SettingsPurpose::MutateQuickReplies),
            None,
            |extra| qr::rename_request(extra, shortcut_id, &name),
        )
        .map(|_| ())
    }

    pub fn delete_quick_reply_shortcut(
        &mut self,
        shortcut_id: i32,
    ) -> Result<(), ConnectSendError> {
        self.session.settings.quick_replies.error = None;
        self.send_quick_reply_request(
            RequestPurpose::Settings(SettingsPurpose::MutateQuickReplies),
            None,
            |extra| qr::delete_shortcut_request(extra, shortcut_id),
        )
        .map(|_| ())
    }

    /// Send every message of the shortcut to a private chat with a user. The
    /// sent messages come back as an ordinary `messages` answer.
    pub fn send_quick_reply(
        &mut self,
        chat_id: ChatId,
        shortcut_id: i32,
    ) -> Result<(), ConnectSendError> {
        let state = &mut self.session.settings.quick_replies;
        state.next_sending_id = state.next_sending_id.wrapping_add(1).max(1);
        let sending_id = state.next_sending_id;
        self.send_quick_reply_request(
            // The `messages` answer is stored like an album send's, and a
            // refusal gets the same send-permission notice.
            RequestPurpose::SendMessageAlbum,
            Some(chat_id),
            |extra| qr::send_request(extra, chat_id.0, shortcut_id, sending_id),
        )
        .map(|_| ())
    }
}
