//! Connect driver: quick reply shortcuts (`crate::quick_replies`).
use super::*;
use crate::ids::{ChatId, RequestId};
use crate::quick_replies as qr;
use crate::state::fetch::Fetch;
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
        subject: Option<String>,
        build: impl Fn(&str) -> String,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        self.quick_replies_ready()?;
        // The same change is not sent twice while the first is on its way:
        // a double click on Add must not create the message twice. The
        // answer (or error) ends it.
        let is_change = purpose == RequestPurpose::Settings(SettingsPurpose::MutateQuickReplies);
        let subject = if is_change { Some(build("")) } else { subject };
        // The request registry is the one record of what is on its way: the
        // request carries what it is for, so nothing here outlives it.
        if let Some(subject) = &subject
            && self.session.requests.pending.values().any(|p| {
                p.purpose == purpose
                    && p.chat_id == chat_id
                    && p.subject_id() == Some(subject.as_str())
            })
        {
            return Ok(None);
        }
        let extra = match &subject {
            Some(subject) => self
                .session
                .request_for_subject_in(purpose, chat_id, subject),
            None => self.session.request(purpose, chat_id),
        };
        let json = build(&extra.as_extra());
        self.send_json_request(extra, &json).map(Some)
    }

    /// `loadQuickReplyShortcuts`, once per session unless a load failed.
    pub fn ensure_quick_reply_shortcuts(&mut self) -> Result<(), ConnectSendError> {
        if !self.session.settings.quick_replies.shortcuts_load.is_idle() {
            return Ok(());
        }
        let sent = self.send_quick_reply_request(
            RequestPurpose::Settings(SettingsPurpose::LoadQuickReplyShortcuts),
            None,
            None,
            qr::load_shortcuts_request,
        );
        // A send that did not leave stays unasked: the next look asks again.
        if let Ok(Some(request)) = sent {
            let load = &mut self.session.settings.quick_replies.shortcuts_load;
            load.begin();
            load.sent(request);
        }
        sent.map(|_| ())
    }

    /// `loadQuickReplyShortcutMessages`, once per shortcut unless a load
    /// failed.
    pub fn ensure_quick_reply_messages(
        &mut self,
        shortcut_id: i32,
    ) -> Result<(), ConnectSendError> {
        if self
            .session
            .settings
            .quick_replies
            .messages
            .contains_key(&shortcut_id)
        {
            return Ok(());
        }
        let sent = self.send_quick_reply_request(
            RequestPurpose::Settings(SettingsPurpose::LoadQuickReplyMessages { shortcut_id }),
            None,
            None,
            |extra| qr::load_messages_request(extra, shortcut_id),
        )?;
        if let Some(request) = sent {
            let mut load = Fetch::Idle;
            load.begin();
            load.sent(request);
            self.session
                .settings
                .quick_replies
                .messages
                .insert(shortcut_id, load);
        }
        Ok(())
    }

    /// Add a text message; the shortcut `name` is created when new.
    /// The request, or `None` when the same change is already on its way.
    pub fn add_quick_reply_text(
        &mut self,
        name: &str,
        text: &str,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        let name = qr::validate_name(name).map_err(|_| ConnectSendError::InvalidRequest)?;
        if text.trim().is_empty() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.settings.quick_replies.error = None;
        self.send_quick_reply_request(
            RequestPurpose::Settings(SettingsPurpose::MutateQuickReplies),
            None,
            None,
            |extra| qr::add_message_request(extra, &name, text),
        )
    }

    pub fn edit_quick_reply_text(
        &mut self,
        shortcut_id: i32,
        message_id: i64,
        text: &str,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if text.trim().is_empty() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.settings.quick_replies.error = None;
        self.send_quick_reply_request(
            RequestPurpose::Settings(SettingsPurpose::MutateQuickReplies),
            None,
            None,
            |extra| qr::edit_message_request(extra, shortcut_id, message_id, text),
        )
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
            None,
            |extra| qr::rename_request(extra, shortcut_id, &name),
        )
        .map(|_| ())
    }

    /// The request, or `None` when the same delete is already on its way.
    pub fn delete_quick_reply_shortcut(
        &mut self,
        shortcut_id: i32,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        self.session.settings.quick_replies.error = None;
        self.send_quick_reply_request(
            RequestPurpose::Settings(SettingsPurpose::MutateQuickReplies),
            None,
            None,
            |extra| qr::delete_shortcut_request(extra, shortcut_id),
        )
    }

    /// Send every message of the shortcut to a private chat with a user. The
    /// sent messages come back as an ordinary `messages` answer.
    pub fn send_quick_reply(
        &mut self,
        chat_id: ChatId,
        shortcut_id: i32,
    ) -> Result<(), ConnectSendError> {
        // A shortcut being sent to a chat is not sent there again until its
        // answer (or error) ends that request: a double tap would deliver
        // every message twice.
        let state = &mut self.session.settings.quick_replies;
        state.next_sending_id = state.next_sending_id.wrapping_add(1).max(1);
        let sending_id = state.next_sending_id;
        self.send_quick_reply_request(
            // The `messages` answer is stored like an album send's, and a
            // refusal gets the same send-permission notice.
            RequestPurpose::SendMessageAlbum,
            Some(chat_id),
            Some(format!("quick-reply:{shortcut_id}")),
            |extra| qr::send_request(extra, chat_id.0, shortcut_id, sending_id),
        )
        .map(|_| ())
    }
}
