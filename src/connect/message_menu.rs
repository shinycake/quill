//! Connect driver: the message context menu's Report flow, its seen /
//! reacted lists, and the admin moderation actions.
use super::*;
use crate::ids::{ChatId, MessageId, RequestId};
use crate::state::RequestPurpose;
use crate::telegram::envelope::{ChatKind, MessageActions};
use crate::telegram::requests::{
    delete_chat_messages_by_sender, get_message_added_reactions, get_message_read_date,
    get_message_viewers, report_chat_messages, report_supergroup_spam,
};

/// One page of reactors the menu list asks for (Telegram Desktop loads 50
/// at a time).
pub const ADDED_REACTIONS_PAGE: i32 = 50;

/// What the delete box's admin checkboxes ask for on top of the delete
/// itself (`boxes/moderate_messages_box.cpp`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ModerationChoice {
    pub report_spam: bool,
    pub delete_all: bool,
    pub ban: bool,
}

impl ModerationChoice {
    pub fn any(self) -> bool {
        self.report_spam || self.delete_all || self.ban
    }
}

impl<S: JsonSender> ConnectDriver<S> {
    /// Send one step of the Report flow. The first call (empty
    /// `option_id`) opens the flow; later calls echo the option the user
    /// picked, or the details text. `chosen` is `(option text, current
    /// title, current options)` so the dialog can go back one level.
    pub fn report_messages(
        &mut self,
        chat_id: ChatId,
        message_ids: &[MessageId],
        option_id: &str,
        text: &str,
        chosen: Option<(String, String, Vec<crate::telegram::envelope::ReportOption>)>,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() || message_ids.is_empty() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !self.session.chats.contains_key(&chat_id.0) {
            return Err(ConnectSendError::InvalidRequest);
        }
        if option_id.is_empty() && text.is_empty() {
            self.session
                .begin_message_report(chat_id, message_ids.to_vec());
        } else {
            self.session.message_report_sending(chosen);
        }
        let extra = self
            .session
            .request(RequestPurpose::ReportMessages, Some(chat_id));
        let json = report_chat_messages(extra, chat_id, option_id, message_ids, text);
        match self.sender.send_json(&json) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                self.session
                    .fail_message_report(chat_id, "Could not send the report.".into());
                Err(err)
            }
        }
    }

    /// Ask who saw / reacted to the message the menu is open on, as far as
    /// `getMessageProperties` allows it. Each list is its own request and
    /// fills in independently.
    pub fn fetch_message_audience(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        actions: MessageActions,
    ) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.begin_message_audience(chat_id, message_id);
        let private = matches!(
            self.session.chats.get(&chat_id.0).map(|c| &c.kind),
            Some(ChatKind::Private { .. } | ChatKind::Secret { .. })
        );
        let can_react_list = self
            .session
            .histories
            .get(&chat_id.0)
            .and_then(|h| h.messages.get(&message_id.0))
            .and_then(|m| m.interaction_info.as_ref())
            .and_then(|info| info.reactions.as_ref())
            .is_some_and(|r| r.can_get_added_reactions && !r.reactions.is_empty());
        if actions.can_get_viewers && !private {
            let extra = self.session.request(
                RequestPurpose::GetMessageViewers {
                    chat_id,
                    message_id,
                },
                Some(chat_id),
            );
            self.send_audience(extra, &get_message_viewers(extra, chat_id, message_id))?;
            self.session.audience_viewers_loading(chat_id, message_id);
        }
        if actions.can_get_read_date && private {
            let extra = self.session.request(
                RequestPurpose::GetMessageReadDate {
                    chat_id,
                    message_id,
                },
                Some(chat_id),
            );
            self.send_audience(extra, &get_message_read_date(extra, chat_id, message_id))?;
            self.session.audience_read_date_loading(chat_id, message_id);
        }
        if can_react_list {
            let extra = self.session.request(
                RequestPurpose::GetMessageAddedReactions {
                    chat_id,
                    message_id,
                },
                Some(chat_id),
            );
            self.send_audience(
                extra,
                &get_message_added_reactions(
                    extra,
                    chat_id,
                    message_id,
                    None,
                    "",
                    ADDED_REACTIONS_PAGE,
                ),
            )?;
            self.session.audience_reactions_loading(chat_id, message_id);
        }
        Ok(())
    }

    fn send_audience(&mut self, extra: RequestId, json: &str) -> Result<(), ConnectSendError> {
        self.sender.send_json(json).inspect_err(|_| {
            self.session.requests.take(extra);
        })
    }

    /// The admin checkboxes of the delete box: report the message as spam,
    /// delete everything the sender wrote, ban them. Each is its own
    /// request gated on what the group allows (the creator or an admin
    /// with the right); nothing here deletes the message itself.
    pub fn moderate_message(
        &mut self,
        chat_id: ChatId,
        message_ids: &[MessageId],
        user_id: i64,
        choice: ModerationChoice,
    ) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let supergroup_id = match self.session.chats.get(&chat_id.0).map(|c| &c.kind) {
            Some(ChatKind::Supergroup { supergroup_id, .. }) => *supergroup_id,
            _ => return Err(ConnectSendError::InvalidRequest),
        };
        if choice.report_spam {
            let extra = self
                .session
                .request(RequestPurpose::ReportSupergroupSpam, Some(chat_id));
            let json = report_supergroup_spam(extra, supergroup_id, message_ids);
            self.send_audience(extra, &json)?;
        }
        if choice.delete_all {
            let extra = self
                .session
                .request(RequestPurpose::DeleteChatMessagesBySender, Some(chat_id));
            let json = delete_chat_messages_by_sender(extra, chat_id, user_id);
            self.send_audience(extra, &json)?;
        }
        if choice.ban {
            // Reports and deletes name the user's messages, so the ban goes
            // last; it is gated on `can_restrict_members` inside.
            self.ban_chat_member(chat_id, user_id, 0)?;
        }
        Ok(())
    }
}
