//! Connect driver: payments.
use super::*;
use crate::ids::{ChatId, MessageId, RequestId};
use crate::payment_invoice::InputInvoice;
use crate::state::BankCardLookup;
use crate::state::PaymentsPurpose;
use crate::state::{PaymentRequest, RequestPurpose};
use crate::telegram::envelope::OrderInfoData;
use crate::telegram::requests::{
    edit_star_subscription, get_payment_form, get_payment_receipt, get_star_subscriptions,
    reuse_star_subscription, send_payment_form as send_payment_form_request,
    validate_order_info as validate_order_info_request,
};
use crate::telegram::{delete_saved_credentials, delete_saved_order_info};

impl<S: JsonSender> ConnectDriver<S> {
    /// The `@extra` for a payment request on `invoice`. An invoice message
    /// must be a real message in a supported chat; any other invoice has
    /// no chat to check.
    fn payment_extra(
        &mut self,
        invoice: &InputInvoice,
        purpose: RequestPurpose,
    ) -> Result<RequestId, ConnectSendError> {
        match invoice.message() {
            Some((chat_id, message_id)) => self.callback_query_extra(chat_id, message_id, purpose),
            None if self.chats_path_active() => Ok(self.session.request(purpose, None)),
            None => Err(ConnectSendError::InvalidRequest),
        }
    }

    /// Slice P1: fetch the `paymentForm` for an invoice (`getPaymentForm`,
    /// schema 1.8.67, line 15262): a Buy button press, an invoice link or a
    /// payment toward Telegram. The dialog opens when the `paymentForm`
    /// answer is applied.
    pub fn send_payment_form_request(
        &mut self,
        invoice: InputInvoice,
    ) -> Result<RequestId, ConnectSendError> {
        let extra = self.payment_extra(&invoice, RequestPurpose::GetPaymentForm)?;
        let json = get_payment_form(extra, &invoice);
        self.session.payments.request = Some(PaymentRequest { invoice });
        self.send_json_request(extra, &json)
    }

    /// Slice P1: `validateOrderInfo` (schema 1.8.67, line 15268) — validate
    /// the order form and fetch the shipping options.
    pub fn validate_payment_order_info(
        &mut self,
        invoice: &InputInvoice,
        order: &OrderInfoData,
        allow_save: bool,
    ) -> Result<RequestId, ConnectSendError> {
        let extra = self.payment_extra(invoice, RequestPurpose::ValidateOrderInfo)?;
        self.session.payments.request = Some(PaymentRequest {
            invoice: invoice.clone(),
        });
        let json = validate_order_info_request(extra, invoice, order, allow_save);
        self.send_json_request(extra, &json)
    }

    /// Slice P1: `sendPaymentForm` (schema 1.8.67, line 15277) — submit the
    /// validated order + credentials from the checkout dialog. `credentials`
    /// is `null` for a payment in Stars.
    #[allow(clippy::too_many_arguments)]
    pub fn submit_payment_form(
        &mut self,
        invoice: &InputInvoice,
        payment_form_id: i64,
        order_info_id: &str,
        shipping_option_id: &str,
        credentials: serde_json::Value,
        tip_amount: i64,
    ) -> Result<RequestId, ConnectSendError> {
        let extra = self.payment_extra(invoice, RequestPurpose::SendPaymentForm)?;
        self.session.payments.request = Some(PaymentRequest {
            invoice: invoice.clone(),
        });
        let json = send_payment_form_request(
            extra,
            invoice,
            payment_form_id,
            order_info_id,
            shipping_option_id,
            credentials,
            tip_amount,
        );
        self.send_json_request(extra, &json)
    }

    /// Slice P1: `getPaymentReceipt` (schema 1.8.67, line 15280) — fetch
    /// the receipt for a paid invoice (its `receipt_message_id`).
    pub fn fetch_payment_receipt(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::GetPaymentReceipt, Some(chat_id));
        self.session.payments.request = Some(PaymentRequest {
            invoice: InputInvoice::Message {
                chat_id,
                message_id,
            },
        });
        let json = get_payment_receipt(extra, chat_id, message_id);
        self.send_json_request(extra, &json)
    }

    /// Slice payments: clear the saved order info (`deleteSavedOrderInfo`,
    /// schema 1.8.67, line 15286) and the saved provider credentials
    /// (`deleteSavedCredentials`, schema line 15289). Both are
    /// parameterless `= Ok` constructors, sent in order without waiting
    /// for the first `ok` (same pattern as `delete_synced_contacts`);
    /// the saved info lives server-side, so there is no local state to
    /// clear. Returns the number of requests sent.
    pub fn clear_saved_payment_info(&mut self) -> Result<usize, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::DeleteSavedOrderInfo, None);
        self.send_json_request(extra, &delete_saved_order_info(extra))?;
        let extra = self
            .session
            .request(RequestPurpose::DeleteSavedCredentials, None);
        self.send_json_request(extra, &delete_saved_credentials(extra))?;
        Ok(2)
    }
}

impl<S: JsonSender> ConnectDriver<S> {
    /// Look up a tapped bank card number (`getBankCardInfo`). The lookup
    /// lives in `payments.bank_card`; a failed send leaves it without info.
    pub fn request_bank_card_info(&mut self, number: &str) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self.session.request(RequestPurpose::GetBankCardInfo, None);
        self.session.payments.bank_card = Some(BankCardLookup {
            number: number.to_string(),
            info: None,
            loading: true,
        });
        let json = crate::telegram::requests::get_bank_card_info(extra, number);
        if let Err(err) = self.send_json_request(extra, &json) {
            if let Some(lookup) = self.session.payments.bank_card.as_mut() {
                lookup.loading = false;
            }
            return Err(err);
        }
        Ok(extra)
    }

    /// Slice `parity:bots-payment-recurring`: fetch the `starSubscriptions`
    /// list (`getStarSubscriptions`, schema 1.8.67, line 16075). Guarded:
    /// once per session unless the list was marked stale by a mutation.
    /// `Ok(None)` = no request needed.
    pub fn maybe_fetch_star_subscriptions(
        &mut self,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if (self.session.payments.star_subscriptions.is_some()
            && !self.session.payments.star_subscriptions_stale)
            || self.session.payments.star_subscriptions_loading
        {
            return Ok(None);
        }
        self.fetch_star_subscriptions_page(false).map(Some)
    }

    /// Slice `parity:bots-payment-recurring`: fetch the next
    /// `getStarSubscriptions` page (`next_offset` from the last answer).
    /// `Ok(None)` = no more pages, or a fetch already in flight.
    pub fn fetch_more_star_subscriptions(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self.session.payments.star_subscriptions_offset.is_empty()
            || self.session.payments.star_subscriptions_loading
        {
            return Ok(None);
        }
        self.fetch_star_subscriptions_page(true).map(Some)
    }

    fn fetch_star_subscriptions_page(
        &mut self,
        append: bool,
    ) -> Result<RequestId, ConnectSendError> {
        let offset = if append {
            self.session.payments.star_subscriptions_offset.clone()
        } else {
            String::new()
        };
        let extra = self.session.request(
            RequestPurpose::Payments(PaymentsPurpose::GetStarSubscriptions { append }),
            None,
        );
        self.session.payments.star_subscriptions_loading = true;
        self.session.payments.star_subscriptions_error = None;
        let json = get_star_subscriptions(extra, false, &offset);
        if let Err(err) = self.send_json_request(extra, &json) {
            self.session.payments.star_subscriptions_loading = false;
            return Err(err);
        }
        Ok(extra)
    }

    /// Slice `parity:bots-payment-recurring`: refetch the subscriptions
    /// list after a mutation marked it stale — the reducer kept the old
    /// cache and marked it stale on the authoritative `ok` (the
    /// `refresh_active_sessions_if_stale` pattern).
    pub fn refresh_star_subscriptions_if_stale(
        &mut self,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.session.payments.star_subscriptions_stale {
            return Ok(None);
        }
        self.session.payments.star_subscriptions_stale = false;
        self.fetch_star_subscriptions_page(false).map(Some)
    }

    /// Slice `parity:bots-payment-recurring`: `editStarSubscription`
    /// (schema 1.8.67, line 16086) — cancel (`is_canceled: true`) or
    /// re-enable a subscription. One mutation at a time; the list is
    /// refetched from the authoritative `ok` — never optimistic.
    pub fn edit_star_subscription(
        &mut self,
        subscription_id: &str,
        is_canceled: bool,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() || self.session.payments.star_subscriptions_mutating {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::EditStarSubscription, None);
        self.session.payments.star_subscriptions_mutating = true;
        self.session.payments.star_subscriptions_error = None;
        let json = edit_star_subscription(extra, subscription_id, is_canceled);
        if let Err(err) = self.send_json_request(extra, &json) {
            self.session.payments.star_subscriptions_mutating = false;
            return Err(err);
        }
        Ok(extra)
    }

    /// Slice `parity:bots-payment-recurring`: `reuseStarSubscription`
    /// (schema 1.8.67, line 16095) — rejoin the chat of an ACTIVE channel
    /// subscription (`can_reuse`). Same one-at-a-time + refetch discipline
    /// as `edit_star_subscription`.
    pub fn reuse_star_subscription(
        &mut self,
        subscription_id: &str,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() || self.session.payments.star_subscriptions_mutating {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::ReuseStarSubscription, None);
        self.session.payments.star_subscriptions_mutating = true;
        self.session.payments.star_subscriptions_error = None;
        let json = reuse_star_subscription(extra, subscription_id);
        if let Err(err) = self.send_json_request(extra, &json) {
            self.session.payments.star_subscriptions_mutating = false;
            return Err(err);
        }
        Ok(extra)
    }
}

impl<S: JsonSender> ConnectDriver<S> {
    /// Look up a tapped collectible username or phone number
    /// (`getCollectibleItemInfo`). The same item is not asked twice while
    /// its lookup is running (tdesktop `resolveCollectible`).
    pub fn request_collectible_info(
        &mut self,
        lookup: crate::collectible_info::CollectibleLookup,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let running = self
            .session
            .payments
            .hub
            .collectible
            .as_ref()
            .is_some_and(|open| open.loading && open.entity == lookup.entity);
        if running {
            return Ok(None);
        }
        let (kind, entity) = (lookup.kind, lookup.entity.clone());
        let extra = self
            .session
            .request(RequestPurpose::GetCollectibleInfo, None);
        self.session.payments.hub.collectible = Some(lookup);
        let json = crate::telegram::requests::get_collectible_item_info(extra, kind, &entity);
        if let Err(err) = self.send_json_request(extra, &json) {
            self.session.payments.hub.collectible = None;
            return Err(err);
        }
        Ok(Some(extra))
    }
}
