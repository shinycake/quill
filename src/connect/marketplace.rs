use super::*;
use crate::ids::{ChatId, RequestId};
use crate::marketplace::{GiftPrice, GiftPurchase, GiftPurchaseStage, valid_gift_name};
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
                .payments
                .marketplace_gift
                .as_ref()
                .is_some_and(|gift| gift.is_busy())
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
        let recipient_name = chat.title.clone();
        self.ask_gift_text_limit();
        let extra = self
            .session
            .request(RequestPurpose::GetMarketplaceGift, Some(chat_id));
        self.session.payments.marketplace_gift = Some(GiftPurchase::loading(
            chat_id,
            recipient,
            recipient_name,
            name.into(),
            extra,
        ));
        let result = self.send_json_request(
            extra,
            &crate::telegram::requests_payments::get_marketplace_gift(extra, name),
        );
        if let Err(err) = &result
            && let Some(gift) = self.session.payments.marketplace_gift.as_mut()
        {
            gift.fail(err.stage_line());
        }
        result
    }

    /// `getOption gift_text_length_max` unless it is known or on its way.
    pub(super) fn ask_gift_text_limit(&mut self) {
        let purpose = RequestPurpose::GetGiftTextLimit;
        if self.session.payments.gift_text_length_max.is_some()
            || self.asked.is_out(&self.session.requests, purpose)
        {
            return;
        }
        let extra = self.session.request(purpose, None);
        self.asked.sent(&self.session.requests, purpose, extra);
        let _ = self.send_json_request(extra, &serde_json::json!({"@type":"getOption","@extra":extra.as_extra(),"name":"gift_text_length_max"}).to_string());
    }

    /// The supplied price must equal the server quote shown at confirmation.
    pub fn buy_marketplace_gift(
        &mut self,
        price: GiftPrice,
        comment: &str,
        private: bool,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() || !price.valid() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let max = self.session.payments.gift_text_length_max;
        if !comment.is_empty() && max.is_none_or(|limit| comment.chars().count() > limit) {
            // Say why the Buy button did nothing, and ask for the limit
            // again when it never arrived (the first lookup failed).
            if max.is_none() {
                self.ask_gift_text_limit();
            }
            if let Some(gift) = self.session.payments.marketplace_gift.as_mut() {
                gift.set_note(
                    crate::lang::tr(if max.is_none() {
                        crate::lang::keys::Q_MARKETPLACE_COMMENT_UNCHECKED
                    } else {
                        crate::lang::keys::Q_MARKETPLACE_COMMENT_TOO_LONG
                    })
                    .to_string(),
                );
            }
            return Err(ConnectSendError::InvalidRequest);
        }
        let gift = self
            .session
            .payments
            .marketplace_gift
            .as_ref()
            .ok_or(ConnectSendError::InvalidRequest)?;
        let GiftPurchaseStage::Quoted { quote, .. } = &gift.stage else {
            return Err(ConnectSendError::InvalidRequest);
        };
        if gift.price() != Some(price) || !quote.allows(price) {
            return Err(ConnectSendError::InvalidRequest);
        }
        let quote = quote.clone();
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
            match result {
                Ok(request) => {
                    gift.stage = GiftPurchaseStage::Sending {
                        quote,
                        price,
                        request,
                    };
                }
                Err(err) => gift.set_note(err.stage_line()),
            }
        }
        result
    }
}
