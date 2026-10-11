use super::*;
use crate::ids::{ChatId, RequestId};
use crate::marketplace::{GiftPrice, GiftPurchase, valid_gift_name};
use crate::state::RequestPurpose;
use crate::telegram::envelope::{ChatKind, MessageSender};

impl<S: JsonSender> ConnectDriver<S> {
    pub fn fetch_marketplace_gift(
        &mut self,
        chat_id: ChatId,
        name: &str,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active()
            || !valid_gift_name(name)
            || self
                .session
                .requests
                .has_purpose(RequestPurpose::GetMarketplaceGift)
            || self
                .session
                .requests
                .has_purpose(RequestPurpose::SendMarketplaceGift)
        {
            return Err(ConnectSendError::InvalidRequest);
        }
        let chat = self
            .session
            .chats
            .get(&chat_id.0)
            .ok_or(ConnectSendError::InvalidRequest)?;
        let recipient = match chat.kind {
            ChatKind::Private { user_id } if user_id.0 > 0 => {
                MessageSender::User { user_id: user_id.0 }
            }
            ChatKind::Supergroup {
                is_channel: true, ..
            } => MessageSender::Chat { chat_id: chat_id.0 },
            _ => return Err(ConnectSendError::InvalidRequest),
        };
        self.session.payments.marketplace_gift = Some(GiftPurchase {
            chat_id,
            recipient,
            recipient_name: chat.title.clone(),
            requested_name: name.into(),
            quote: None,
            price: None,
            loading: true,
            sending: false,
            completed: false,
            note: None,
        });
        if self.session.payments.gift_text_length_max.is_none()
            && !self
                .session
                .requests
                .has_purpose(RequestPurpose::GetGiftTextLimit)
        {
            let extra = self.session.request(RequestPurpose::GetGiftTextLimit, None);
            let _ = self.send_json_request(extra, &serde_json::json!({"@type":"getOption","@extra":extra.as_extra(),"name":"gift_text_length_max"}).to_string());
        }
        let extra = self
            .session
            .request(RequestPurpose::GetMarketplaceGift, Some(chat_id));
        let result = self.send_json_request(
            extra,
            &crate::telegram::requests_payments::get_marketplace_gift(extra, name),
        );
        if result.is_err()
            && let Some(gift) = self.session.payments.marketplace_gift.as_mut()
        {
            gift.loading = false;
            gift.note = Some("Could not load the gift. Retry.".into());
        }
        result
    }

    /// The supplied price must equal the server quote shown at confirmation.
    pub fn buy_marketplace_gift(
        &mut self,
        price: GiftPrice,
        comment: &str,
        private: bool,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active()
            || !price.valid()
            || self
                .session
                .requests
                .has_purpose(RequestPurpose::SendMarketplaceGift)
        {
            return Err(ConnectSendError::InvalidRequest);
        }
        let max = self.session.payments.gift_text_length_max;
        if !comment.is_empty() && max.is_none_or(|limit| comment.chars().count() > limit) {
            return Err(ConnectSendError::InvalidRequest);
        }
        let gift = self
            .session
            .payments
            .marketplace_gift
            .as_ref()
            .ok_or(ConnectSendError::InvalidRequest)?;
        if gift.loading
            || gift.sending
            || gift.completed
            || gift.price != Some(price)
            || !gift.quote.as_ref().is_some_and(|q| q.allows(price))
        {
            return Err(ConnectSendError::InvalidRequest);
        }
        let json_fields = (gift.requested_name.clone(), gift.recipient, gift.chat_id);
        let extra = self
            .session
            .request(RequestPurpose::SendMarketplaceGift, Some(json_fields.2));
        let result = self.send_json_request(
            extra,
            &crate::telegram::requests_payments::send_marketplace_gift(
                extra,
                &json_fields.0,
                json_fields.1,
                price,
                comment,
                private,
            ),
        );
        if let Some(gift) = self.session.payments.marketplace_gift.as_mut() {
            gift.sending = result.is_ok();
            gift.note = if result.is_ok() {
                Some("Sending the gift…".into())
            } else {
                Some("Could not submit the purchase. Review and retry.".into())
            };
        }
        result
    }
}
