//! Connect driver: suggested posts in channel direct messages
//! (`crate::suggested_posts`): accept, decline, counter-offers, offers on
//! plain messages, changed content, and the composer's suggest bar.
use super::*;
use crate::ids::{ChatId, MessageId, RequestId};
use crate::state::{PaymentsPurpose, RequestPurpose};
use crate::suggested_posts::{SuggestBar, SuggestMode, SuggestTerms, SuggestedPostPurpose};
use crate::telegram::requests::{
    add_offer, approve_suggested_post, decline_suggested_post, send_suggested_change,
};

impl<S: JsonSender> ConnectDriver<S> {
    /// `approveSuggestedPost` ("Publish" in the Accept Terms box).
    pub fn approve_suggested_post(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        send_date: i32,
    ) -> Result<RequestId, ConnectSendError> {
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
        };
        let result = self.send_suggested(purpose, chat_id, message_id, |extra| {
            send_suggested_change(extra, chat_id, message_id, text, is_media, terms)
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

    /// Puts the suggest bar up (or changes its terms) and saves the draft
    /// right away, as tdesktop's `applySuggestOptions` does. Returns
    /// whether a draft request went out.
    pub fn set_suggest_bar(
        &mut self,
        chat_id: ChatId,
        bar: Option<SuggestBar>,
        text: &str,
        reply_to: Option<crate::telegram::requests::SendReply>,
        now_ms: u64,
    ) -> Result<DraftSaveOutcome, ConnectSendError> {
        let bars = &mut self.session.payments.suggested_posts.bars;
        match bar {
            Some(bar) => {
                bars.insert(chat_id.0, bar);
            }
            None => {
                bars.remove(&chat_id.0);
            }
        }
        if bar.is_some_and(|bar| bar.mode != SuggestMode::New || bar.edit_of.is_some()) {
            return Ok(DraftSaveOutcome::Skipped);
        }
        self.note_composer_draft_in(
            chat_id,
            crate::composer::DraftTopic::Chat,
            text,
            reply_to,
            now_ms,
            false,
        )
    }
}
