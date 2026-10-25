use super::*;
use crate::ids::{ChatId, MessageId, RequestId};
use crate::local_time::now_unix;
use crate::state::{EnableSharing, GroupsPurpose, MessagesPurpose, RequestPurpose};
use crate::telegram::requests::{
    get_message_properties, process_chat_has_protected_content_disable_request,
};

impl<S: JsonSender> ConnectDriver<S> {
    /// The profile menu's Enable Sharing. Within a day of disabling it
    /// here, or with no message to ask about, it goes straight to
    /// `toggleChatHasProtectedContent`. Otherwise `getMessageProperties`
    /// on the last message says whether the other person protects the
    /// chat, and [`Self::apply_resolved_enable_sharing`] or the request box
    /// carries on.
    pub fn enable_private_sharing(
        &mut self,
        chat_id: ChatId,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self.session.private_sharing_disabled(chat_id) != Some(true) {
            return Ok(None);
        }
        let purpose = RequestPurpose::Messages(MessagesPurpose::GetSharingProtection { chat_id });
        if self.session.requests.has_purpose(purpose) {
            return Ok(None);
        }
        let recent = self
            .session
            .groups
            .private_sharing
            .recently_disabled_by_me(chat_id, now_unix());
        let last = self
            .session
            .chats
            .get(&chat_id.0)
            .and_then(|chat| chat.last_message.as_ref())
            .map(|message| message.id)
            .filter(|id| id.0 > 0 && !recent);
        let Some(message_id) = last else {
            return self.set_chat_protected_content(chat_id, false);
        };
        let extra = self.session.request(purpose, Some(chat_id));
        self.send_json_request(extra, &get_message_properties(extra, chat_id, message_id))
            .map(Some)
    }

    pub(crate) fn apply_resolved_enable_sharing(&mut self) -> Result<(), ConnectSendError> {
        let Some(EnableSharing::TurnOff(chat_id)) = self.session.groups.private_sharing.enable
        else {
            return Ok(());
        };
        self.session.groups.private_sharing.enable = None;
        self.set_chat_protected_content(chat_id, false).map(|_| ())
    }

    /// The request box's Send Request: TDLib asks the other person.
    pub fn send_sharing_request(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        let Some(chat_id) = self.session.groups.private_sharing.request_box_chat() else {
            return Ok(None);
        };
        self.session.groups.private_sharing.enable = None;
        self.set_chat_protected_content(chat_id, false)
    }

    pub fn close_sharing_request_box(&mut self) {
        self.session.groups.private_sharing.enable = None;
    }

    /// Accept or Reject under an incoming request; a second press while
    /// the first is on its way does nothing.
    pub fn answer_sharing_request(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        approve: bool,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let answerable = self
            .session
            .histories
            .get(&chat_id.0)
            .and_then(|history| history.messages.get(&message_id.0))
            .and_then(|message| self.session.sharing_request_view(message))
            .is_some_and(|view| view.buttons);
        if !answerable || self.session.sharing_request_answering(chat_id, message_id) {
            return Ok(None);
        }
        let extra = self.session.request(
            RequestPurpose::Groups(GroupsPurpose::AnswerSharingRequest {
                chat_id,
                message_id,
            }),
            Some(chat_id),
        );
        self.send_json_request(
            extra,
            &process_chat_has_protected_content_disable_request(
                extra,
                chat_id.0,
                message_id.0,
                approve,
            ),
        )
        .map(Some)
    }
}
