//! Connect driver: translation (`translateMessageText` / `translateText`).
use super::*;
use crate::ids::{ChatId, MessageId};
use crate::state::{RequestPurpose, RequestRollback};
use crate::state::{TranslateTarget, Translation};
use crate::telegram::envelope::ChatKind;
use crate::telegram::requests::{
    toggle_chat_is_translatable, toggle_supergroup_has_automatic_translation,
    translate_message_text, translate_text,
};

impl<S: JsonSender> ConnectDriver<S> {
    /// Translate a whole message into `to_language` (the message menu's
    /// Translate and the translated chat view). Does nothing when the
    /// message already has a pending or finished translation into that
    /// language; a failed one is asked for again. Secret chats are refused
    /// (TDLib: "must not be used in secret chats").
    pub fn translate_message(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        to_language: &str,
    ) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() || message_id.0 <= 0 {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self
            .session
            .chats
            .get(&chat_id.0)
            .is_some_and(|chat| matches!(chat.kind, ChatKind::Secret { .. }))
        {
            return Err(ConnectSendError::InvalidRequest);
        }
        match self
            .session
            .message_translation(chat_id, message_id, to_language)
        {
            Some(Translation::Pending | Translation::Done { .. }) => return Ok(()),
            Some(Translation::Failed(_)) | None => {}
        }
        let (job, extra) = self.session.begin_translation(
            TranslateTarget::Message {
                chat_id: chat_id.0,
                message_id: message_id.0,
            },
            to_language,
            Some(chat_id),
        );
        let json = translate_message_text(extra, chat_id, message_id, to_language);
        if let Err(err) = self.sender.send_json(&json) {
            self.session.abandon_translation(job, extra);
            return Err(err);
        }
        Ok(())
    }

    /// Translate a piece of text (a selection) into `to_language`. The
    /// returned job number reads the answer from
    /// `Session::text_translation`.
    pub fn translate_selection(
        &mut self,
        text: &str,
        to_language: &str,
    ) -> Result<u64, ConnectSendError> {
        if !self.chats_path_active() || text.trim().is_empty() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let (job, extra) = self
            .session
            .begin_translation(TranslateTarget::Text, to_language, None);
        let json = translate_text(extra, text, to_language);
        if let Err(err) = self.sender.send_json(&json) {
            self.session.abandon_translation(job, extra);
            return Err(err);
        }
        Ok(job)
    }
}

impl<S: JsonSender> ConnectDriver<S> {
    /// `toggleChatIsTranslatable`: tdesktop's bar-menu Hide (`false`) and
    /// the toast's Undo (`true`). TDLib answers with
    /// `updateChatIsTranslatable`, which drives the bar; the local flag is
    /// set at once and put back when TDLib refuses.
    pub fn toggle_chat_is_translatable(
        &mut self,
        chat_id: ChatId,
        is_translatable: bool,
    ) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::ToggleChatIsTranslatable, Some(chat_id));
        if let Err(err) = self.sender.send_json(&toggle_chat_is_translatable(
            extra,
            chat_id,
            is_translatable,
        )) {
            self.session.requests.take(extra);
            return Err(err);
        }
        let previous = self.session.chat_is_translatable(chat_id);
        self.session
            .set_chat_translatable(chat_id.0, is_translatable);
        if let Some(pending) = self.session.requests.pending_mut(extra) {
            pending.rollback = Some(RequestRollback::ChatIsTranslatable {
                chat_id: chat_id.0,
                previous,
            });
        }
        Ok(())
    }

    /// `toggleSupergroupHasAutomaticTranslation` for a channel; needs
    /// `can_change_info`. Optimistic; a TDLib error (for example a missing
    /// boost level) restores the flag.
    pub fn toggle_auto_translate(
        &mut self,
        chat_id: ChatId,
        enabled: bool,
    ) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let Some(supergroup_id) = self.session.chat_supergroup(chat_id) else {
            return Err(ConnectSendError::InvalidRequest);
        };
        if !self.session.chat_can_change_info(chat_id) {
            return Err(ConnectSendError::InvalidRequest);
        }
        let purpose = RequestPurpose::ToggleSupergroupAutoTranslate;
        if self.session.requests.has_purpose_for_chat(purpose, chat_id) {
            return Ok(());
        }
        let extra = self.session.request(purpose, Some(chat_id));
        if let Err(err) = self
            .sender
            .send_json(&toggle_supergroup_has_automatic_translation(
                extra,
                supergroup_id,
                enabled,
            ))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        let previous = self.session.chat_auto_translate(chat_id);
        self.session
            .set_supergroup_auto_translate(supergroup_id, enabled);
        if let Some(pending) = self.session.requests.pending_mut(extra) {
            pending.rollback = Some(RequestRollback::AutoTranslate {
                supergroup_id,
                previous,
            });
        }
        Ok(())
    }
}
