//! Connect driver: suggested posts in channel direct messages
//! (`crate::suggested_posts`): accept, decline, counter-offers, offers on
//! plain messages and changed content. The composer's suggest bar lives in
//! the session (`payments.suggested_posts.bars`) and rides the draft.
use super::*;
use crate::ids::{ChatId, MessageId, RequestId};
use crate::state::{PaymentsPurpose, RequestPurpose};
use crate::suggested_posts::{SuggestTerms, SuggestedPostPurpose};
use crate::telegram::requests::{
    add_offer, approve_suggested_post, decline_suggested_post, send_suggested_change,
};

impl<S: JsonSender> ConnectDriver<S> {
    /// `approveSuggestedPost` ("Publish" in the Accept Terms box).
    /// `send_date` is the time picked for a post that had none; a post
    /// whose offer already names a time sends 0, as the schema asks and
    /// tdesktop's `ConfirmApproval` does.
    pub fn approve_suggested_post(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        send_date: i32,
    ) -> Result<RequestId, ConnectSendError> {
        let dated = self
            .session
            .histories
            .get(&chat_id.0)
            .and_then(|history| history.messages.get(&message_id.0))
            .and_then(|message| message.extras.suggested_post)
            .is_some_and(|info| info.send_date > 0);
        let send_date = if dated { 0 } else { send_date };
        let purpose = SuggestedPostPurpose::Approve {
            chat_id: chat_id.0,
            message_id: message_id.0,
        };
        self.send_suggested(purpose, chat_id, message_id, |extra| {
            approve_suggested_post(extra, chat_id, message_id, send_date)
        })
    }

    /// `declineSuggestedPost` with the optional reason.
    pub fn decline_suggested_post(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        comment: &str,
    ) -> Result<RequestId, ConnectSendError> {
        let purpose = SuggestedPostPurpose::Decline {
            chat_id: chat_id.0,
            message_id: message_id.0,
        };
        self.send_suggested(purpose, chat_id, message_id, |extra| {
            decline_suggested_post(extra, chat_id, message_id, comment)
        })
    }

    /// `addOffer`: "Edit Price" / "Edit Time" of a suggestion, or "Add
    /// Offer" on a plain message. `terms` are the complete new terms.
    pub fn add_offer(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        terms: SuggestTerms,
    ) -> Result<RequestId, ConnectSendError> {
        let purpose = SuggestedPostPurpose::AddOffer {
            chat_id: chat_id.0,
            message_id: message_id.0,
            price: terms.price,
        };
        self.send_suggested(purpose, chat_id, message_id, |extra| {
            add_offer(extra, chat_id, message_id, terms)
        })
    }

    /// "Edit Message" of a suggestion saved: a new message replying to it
    /// with the changed text (or caption) and the bar's terms.
    pub fn send_suggested_change(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        text: &str,
        is_media: bool,
        terms: SuggestTerms,
    ) -> Result<RequestId, ConnectSendError> {
        let purpose = SuggestedPostPurpose::SendChange {
            chat_id: chat_id.0,
            message_id: message_id.0,
            price: terms.price,
        };
        let topic_id = self
            .session
            .histories
            .get(&chat_id.0)
            .and_then(|history| history.messages.get(&message_id.0))
            .map_or(0, |message| message.extras.direct_messages_topic_id);
        let result = self.send_suggested(purpose, chat_id, message_id, |extra| {
            send_suggested_change(extra, chat_id, message_id, topic_id, text, is_media, terms)
        });
        if result.is_ok() {
            self.session
                .payments
                .suggested_posts
                .bars
                .remove(&chat_id.0);
        }
        result
    }

    fn send_suggested(
        &mut self,
        purpose: SuggestedPostPurpose,
        chat_id: ChatId,
        message_id: MessageId,
        json: impl FnOnce(RequestId) -> String,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() || message_id.0 <= 0 {
            return Err(ConnectSendError::InvalidRequest);
        }
        let busy = matches!(
            purpose,
            SuggestedPostPurpose::Approve { .. } | SuggestedPostPurpose::Decline { .. }
        );
        if busy
            && self
                .session
                .payments
                .suggested_posts
                .busy
                .contains(&purpose.key())
        {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self.session.request(
            RequestPurpose::Payments(PaymentsPurpose::SuggestedPost(purpose)),
            Some(chat_id),
        );
        match self.sender.send_json(&json(extra)) {
            Ok(()) => {
                if busy {
                    self.session
                        .payments
                        .suggested_posts
                        .busy
                        .insert(purpose.key());
                }
                Ok(extra)
            }
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }
}
