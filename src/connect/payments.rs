//! Connect driver: payments.
use super::*;
use crate::ids::{ChatId, MessageId, RequestId};
use crate::state::{PaymentRequest, RequestPurpose};
use crate::telegram::envelope::OrderInfoData;
use crate::telegram::requests::{
    get_payment_form, get_payment_receipt, send_payment_form as send_payment_form_request,
    validate_order_info as validate_order_info_request,
};
use crate::telegram::{delete_saved_credentials, delete_saved_order_info};

impl<S: JsonSender> ConnectDriver<S> {
    /// Slice P1: fetch the `paymentForm` for a Buy button press
    /// (`getPaymentForm`, schema 1.8.67, line 15262). The dialog opens when
    /// the `paymentForm` answer is applied.
    pub fn send_payment_form_request(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
    ) -> Result<RequestId, ConnectSendError> {
        let extra =
            self.callback_query_extra(chat_id, message_id, RequestPurpose::GetPaymentForm)?;
        self.session.payment_request = Some(PaymentRequest {
            chat_id,
            message_id,
        });
        let json = get_payment_form(extra, chat_id, message_id);
        self.send_json_request(extra, &json)
    }

    /// Slice P1: `validateOrderInfo` (schema 1.8.67, line 15268) — validate
    /// the order form and fetch the shipping options.
    pub fn validate_payment_order_info(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        order: &OrderInfoData,
        allow_save: bool,
    ) -> Result<RequestId, ConnectSendError> {
        let extra =
            self.callback_query_extra(chat_id, message_id, RequestPurpose::ValidateOrderInfo)?;
        self.session.payment_request = Some(PaymentRequest {
            chat_id,
            message_id,
        });
        let json = validate_order_info_request(extra, chat_id, message_id, order, allow_save);
        self.send_json_request(extra, &json)
    }

    /// Slice P1: `sendPaymentForm` (schema 1.8.67, line 15277) — submit the
    /// validated order + credentials from the checkout dialog.
    #[allow(clippy::too_many_arguments)]
    pub fn submit_payment_form(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        payment_form_id: i64,
        order_info_id: &str,
        shipping_option_id: &str,
        credentials: serde_json::Value,
        tip_amount: i64,
    ) -> Result<RequestId, ConnectSendError> {
        let extra =
            self.callback_query_extra(chat_id, message_id, RequestPurpose::SendPaymentForm)?;
        self.session.payment_request = Some(PaymentRequest {
            chat_id,
            message_id,
        });
        let json = send_payment_form_request(
            extra,
            chat_id,
            message_id,
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
        self.session.payment_request = Some(PaymentRequest {
            chat_id,
            message_id,
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
