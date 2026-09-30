use crate::ids::{ChatId, MessageId, RequestId};
use crate::telegram::envelope::OrderInfoData;
use serde_json::{Value, json};

/// Slice P1: minimal `themeParameters` (TDLib 1.8.67, `schema/td_api.tl:1111`)
/// for `getPaymentForm`. Quill's UI is dark; these approximate the app
/// palette. The theme only styles a provider page Quill would show in a
/// web view — Quill opens provider URLs in the OS browser instead, so
/// this is a formality the schema requires.
fn payment_theme_parameters() -> Value {
    json!({
        "@type": "themeParameters",
        "background_color": 0x17212b,
        "secondary_background_color": 0x0e1621,
        "header_background_color": 0x17212b,
        "bottom_bar_background_color": 0x17212b,
        "section_background_color": 0x17212b,
        "section_separator_color": 0x0e1621,
        "text_color": 0xffffff,
        "accent_text_color": 0x6ab2f2,
        "section_header_text_color": 0x6ab2f2,
        "subtitle_text_color": 0x8a97a3,
        "destructive_text_color": 0xe06c75,
        "hint_color": 0x8a97a3,
        "link_color": 0x6ab2f2,
        "button_color": 0x5288c1,
        "button_text_color": 0xffffff,
    })
}

/// Slice P1: `getPaymentForm` (TDLib 1.8.67, `schema/td_api.tl:15262`) —
/// the Buy button flow. Response is `paymentForm`.
pub fn get_payment_form(extra: RequestId, chat_id: ChatId, message_id: MessageId) -> String {
    json!({
        "@type": "getPaymentForm",
        "@extra": extra.as_extra(),
        "input_invoice": {
            "@type": "inputInvoiceMessage",
            "chat_id": chat_id.0,
            "message_id": message_id.0,
        },
        "theme": payment_theme_parameters(),
    })
    .to_string()
}

/// Slice P1: `orderInfo` JSON (schema:4662) for `validateOrderInfo` /
/// `sendPaymentForm`.
pub fn order_info_json(order: &OrderInfoData) -> Value {
    json!({
        "@type": "orderInfo",
        "name": order.name,
        "phone_number": order.phone_number,
        "email_address": order.email_address,
        "shipping_address": {
            "@type": "address",
            "country_code": order.shipping_address.country_code,
            "state": order.shipping_address.state,
            "city": order.shipping_address.city,
            "street_line1": order.shipping_address.street_line1,
            "street_line2": order.shipping_address.street_line2,
            "postal_code": order.shipping_address.postal_code,
        },
    })
}

/// Slice P1: `validateOrderInfo` (TDLib 1.8.67, `schema/td_api.tl:15268`).
/// Response is `validatedOrderInfo` with the shipping options.
pub fn validate_order_info(
    extra: RequestId,
    chat_id: ChatId,
    message_id: MessageId,
    order: &OrderInfoData,
    allow_save: bool,
) -> String {
    json!({
        "@type": "validateOrderInfo",
        "@extra": extra.as_extra(),
        "input_invoice": {
            "@type": "inputInvoiceMessage",
            "chat_id": chat_id.0,
            "message_id": message_id.0,
        },
        "order_info": order_info_json(order),
        "allow_save": allow_save,
    })
    .to_string()
}

/// Slice P1: `inputCredentialsNew` (schema:4680) — a credential token the
/// user obtained on the provider's site (e.g. a Stripe `tok_*`).
pub fn input_credentials_new(data: &str, allow_save: bool) -> Value {
    json!({
        "@type": "inputCredentialsNew",
        "data": data,
        "allow_save": allow_save,
    })
}

/// Slice P1: `inputCredentialsSaved` (schema:4677).
pub fn input_credentials_saved(saved_credentials_id: &str) -> Value {
    json!({
        "@type": "inputCredentialsSaved",
        "saved_credentials_id": saved_credentials_id,
    })
}

/// Slice P1: `sendPaymentForm` (TDLib 1.8.67, `schema/td_api.tl:15277`).
/// Response is `paymentResult`.
#[allow(clippy::too_many_arguments)]
pub fn send_payment_form(
    extra: RequestId,
    chat_id: ChatId,
    message_id: MessageId,
    payment_form_id: i64,
    order_info_id: &str,
    shipping_option_id: &str,
    credentials: Value,
    tip_amount: i64,
) -> String {
    json!({
        "@type": "sendPaymentForm",
        "@extra": extra.as_extra(),
        "input_invoice": {
            "@type": "inputInvoiceMessage",
            "chat_id": chat_id.0,
            "message_id": message_id.0,
        },
        "payment_form_id": payment_form_id,
        "order_info_id": order_info_id,
        "shipping_option_id": shipping_option_id,
        "credentials": credentials,
        "tip_amount": tip_amount,
    })
    .to_string()
}

/// Slice P1: `getPaymentReceipt` (TDLib 1.8.67, `schema/td_api.tl:15280`).
/// Response is `paymentReceipt`.
pub fn get_payment_receipt(extra: RequestId, chat_id: ChatId, message_id: MessageId) -> String {
    json!({
        "@type": "getPaymentReceipt",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "message_id": message_id.0,
    })
    .to_string()
}

/// Slice `parity:bots-payment-recurring`: `getStarSubscriptions` (TDLib
/// 1.8.67, `schema/td_api.tl:16075`). Response is `starSubscriptions`.
pub fn get_star_subscriptions(extra: RequestId, only_expiring: bool, offset: &str) -> String {
    json!({
        "@type": "getStarSubscriptions",
        "@extra": extra.as_extra(),
        "only_expiring": only_expiring,
        "offset": offset,
    })
    .to_string()
}

/// Slice `parity:bots-payment-recurring`: `editStarSubscription` (TDLib
/// 1.8.67, `schema/td_api.tl:16086`) — cancel (`is_canceled: true`) or
/// re-enable (`is_canceled: false`) a subscription. Response is `ok`.
pub fn edit_star_subscription(
    extra: RequestId,
    subscription_id: &str,
    is_canceled: bool,
) -> String {
    json!({
        "@type": "editStarSubscription",
        "@extra": extra.as_extra(),
        "subscription_id": subscription_id,
        "is_canceled": is_canceled,
    })
    .to_string()
}

/// Slice `parity:bots-payment-recurring`: `reuseStarSubscription` (TDLib
/// 1.8.67, `schema/td_api.tl:16095`) — rejoin an expired channel
/// subscription via its invite link. Response is `ok`.
pub fn reuse_star_subscription(extra: RequestId, subscription_id: &str) -> String {
    json!({
        "@type": "reuseStarSubscription",
        "@extra": extra.as_extra(),
        "subscription_id": subscription_id,
    })
    .to_string()
}
