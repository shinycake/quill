use super::*;
use serde_json::Value;

/// Slice P1: `messageInvoice` content (TDLib 1.8.67, `schema/td_api.tl:5270`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvoiceContent {
    pub title: String,
    pub description: String,
    pub currency: String,
    pub total_amount: i64,
    pub is_test: bool,
    pub need_shipping_address: bool,
    pub receipt_message_id: i64,
}

/// Slice P1: `messagePaymentSuccessful` content (TDLib 1.8.67,
/// `schema/td_api.tl:5436`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaymentSuccessContent {
    pub invoice_chat_id: i64,
    pub invoice_message_id: i64,
    pub currency: String,
    pub total_amount: i64,
    pub is_recurring: bool,
    pub invoice_name: String,
}

/// Slice P1: `messagePaymentSuccessfulBot` content (TDLib 1.8.67,
/// `schema/td_api.tl:5449`) — minimal, buyer-side render only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaymentReceivedContent {
    pub currency: String,
    pub total_amount: i64,
    pub is_recurring: bool,
}

/// Slice P1: `labeledPricePart` (TDLib 1.8.67, `schema/td_api.tl:4637`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LabeledPrice {
    pub label: String,
    pub amount: i64,
}

/// Slice P1: `invoice` (TDLib 1.8.67, `schema/td_api.tl:4655`) — the full
/// invoice inside a `paymentFormTypeRegular`. Only the fields the checkout
/// dialog needs are kept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvoiceForm {
    pub currency: String,
    pub price_parts: Vec<LabeledPrice>,
    pub max_tip_amount: i64,
    pub suggested_tip_amounts: Vec<i64>,
    pub recurring_terms_url: String,
    pub terms_url: String,
    pub is_test: bool,
    pub need_name: bool,
    pub need_phone_number: bool,
    pub need_email_address: bool,
    pub need_shipping_address: bool,
    pub is_flexible: bool,
}

/// Slice P1: the payment provider on a `paymentFormTypeRegular`
/// (schema:4689–4702). Quill has no in-app web view, so both variants end
/// at the OS browser: `Other` carries the provider's payment-page URL;
/// card-token providers (Stripe/SmartGlocal) need a credential token the
/// user obtains on the provider's site.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PaymentProviderKind {
    /// `paymentProviderOther` — the URL of the provider's payment page.
    Web { url: String },
    /// Stripe / SmartGlocal / anything else — card credentials are a
    /// provider-issued token (`inputCredentialsNew`).
    Token { name: String },
}

/// Slice P1: `paymentOption` (schema:4706) — an additional web payment
/// option, opened in the OS browser.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaymentOption {
    pub title: String,
    pub url: String,
}

/// Slice P1: `savedCredentials` (schema:4671).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedCredential {
    pub id: String,
    pub title: String,
}

/// Slice P1: `address` (schema:4626) / `orderInfo` (schema:4662).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AddressData {
    pub country_code: String,
    pub state: String,
    pub city: String,
    pub street_line1: String,
    pub street_line2: String,
    pub postal_code: String,
}

/// Slice P1: `orderInfo` (schema:4662).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct OrderInfoData {
    pub name: String,
    pub phone_number: String,
    pub email_address: String,
    pub shipping_address: AddressData,
}

/// Slice P1: `paymentFormType*` (schema:4716–4730). Stars payments are
/// parsed so the dialog can decline them honestly — the Stars
/// credentials flow has no verified TDLib path in this slice. Unknown
/// future variants parse to `Unknown` (never `None`) so the pending
/// request resolves and the dialog declines honestly instead of spinning
/// forever. The regular payload is boxed: `Stars`/`StarSubscription` carry
/// almost nothing, which tripped clippy's `large_enum_variant` lint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PaymentFormTypeData {
    Regular(Box<PaymentFormRegular>),
    Stars { star_count: i64 },
    StarSubscription,
    Unknown,
}

/// Slice P1: the regular payment-form payload (`paymentFormTypeRegular`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaymentFormRegular {
    pub invoice: InvoiceForm,
    pub provider: PaymentProviderKind,
    pub additional_options: Vec<PaymentOption>,
    pub saved_order_info: OrderInfoData,
    pub saved_credentials: Vec<SavedCredential>,
    pub can_save_credentials: bool,
    pub need_password: bool,
}

/// Slice P1: `paymentForm` (TDLib 1.8.67, `schema/td_api.tl:4734`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaymentFormData {
    pub id: i64,
    pub form_type: PaymentFormTypeData,
    pub seller_bot_user_id: i64,
    pub product_title: String,
    pub product_description: String,
}

/// Slice P1: `shippingOption` (schema:4668).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShippingOptionData {
    pub id: String,
    pub title: String,
    pub price_parts: Vec<LabeledPrice>,
}

/// Slice P1: `validatedOrderInfo` (schema:4737) — the
/// `validateOrderInfo` answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedOrderInfoData {
    pub order_info_id: String,
    pub shipping_options: Vec<ShippingOptionData>,
}

/// Slice P1: `paymentResult` (schema:4740) — the `sendPaymentForm` answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaymentResultData {
    pub success: bool,
    pub verification_url: String,
}

/// Slice P1: `paymentReceipt` (schema:4765) — the `getPaymentReceipt`
/// answer. Stars receipts keep only the star count.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaymentReceiptData {
    pub product_title: String,
    pub date: i32,
    pub currency: String,
    pub total_amount: i64,
    pub credentials_title: String,
    pub tip_amount: i64,
    pub is_stars: bool,
    pub star_count: i64,
}

/// Slice P1: `messageInvoice` (TDLib 1.8.67, `schema/td_api.tl:5270`).
pub(crate) fn parse_message_invoice(value: &Value) -> (MessageContent, Vec<ParsedFile>) {
    let info = value.get("product_info");
    let description = info
        .and_then(|info| info.get("description"))
        .and_then(|desc| desc.get("text"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    (
        MessageContent::Invoice(InvoiceContent {
            title: info
                .and_then(|info| info.get("title"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            description,
            currency: json_field_str(value, "currency"),
            total_amount: int53(value.get("total_amount")).unwrap_or(0),
            is_test: value
                .get("is_test")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            need_shipping_address: value
                .get("need_shipping_address")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            receipt_message_id: int53(value.get("receipt_message_id")).unwrap_or(0),
        }),
        Vec::new(),
    )
}

/// Slice P1: `messagePaymentSuccessful` (TDLib 1.8.67,
/// `schema/td_api.tl:5436`).
pub(crate) fn parse_message_payment_successful(value: &Value) -> (MessageContent, Vec<ParsedFile>) {
    (
        MessageContent::PaymentSuccessful(PaymentSuccessContent {
            invoice_chat_id: int53(value.get("invoice_chat_id")).unwrap_or(0),
            invoice_message_id: int53(value.get("invoice_message_id")).unwrap_or(0),
            currency: json_field_str(value, "currency"),
            total_amount: int53(value.get("total_amount")).unwrap_or(0),
            is_recurring: value
                .get("is_recurring")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            invoice_name: json_field_str(value, "invoice_name"),
        }),
        Vec::new(),
    )
}

/// Slice P1: `messagePaymentSuccessfulBot` (TDLib 1.8.67,
/// `schema/td_api.tl:5449`) — minimal seller-side parse.
pub(crate) fn parse_message_payment_received(value: &Value) -> (MessageContent, Vec<ParsedFile>) {
    (
        MessageContent::PaymentReceived(PaymentReceivedContent {
            currency: json_field_str(value, "currency"),
            total_amount: int53(value.get("total_amount")).unwrap_or(0),
            is_recurring: value
                .get("is_recurring")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        }),
        Vec::new(),
    )
}

/// Slice P1: `labeledPricePart` list (schema:4637).
pub(crate) fn parse_price_parts(value: Option<&Value>) -> Vec<LabeledPrice> {
    value
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .map(|part| LabeledPrice {
                    label: json_field_str(part, "label"),
                    amount: int53(part.get("amount")).unwrap_or(0),
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Slice P1: `address` (schema:4626); `orderInfo.shipping_address` may be
/// null (schema:4662) — `parse_address(None)` yields the empty address.
pub(crate) fn parse_address(value: Option<&Value>) -> AddressData {
    let empty = || String::new();
    let get = |key: &str| {
        value
            .and_then(|v| v.get(key))
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(empty)
    };
    AddressData {
        country_code: get("country_code"),
        state: get("state"),
        city: get("city"),
        street_line1: get("street_line1"),
        street_line2: get("street_line2"),
        postal_code: get("postal_code"),
    }
}

/// Slice P1: `orderInfo` (schema:4662); `saved_order_info` may be null
/// (schema:4720) — `parse_order_info(None)` yields the empty order.
pub(crate) fn parse_order_info(value: Option<&Value>) -> OrderInfoData {
    OrderInfoData {
        name: value
            .and_then(|v| v.get("name"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        phone_number: value
            .and_then(|v| v.get("phone_number"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        email_address: value
            .and_then(|v| v.get("email_address"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        shipping_address: parse_address(value.and_then(|v| v.get("shipping_address"))),
    }
}

/// Slice P1: `invoice` inside a `paymentFormTypeRegular` (schema:4655).
pub(crate) fn parse_invoice_form(value: &Value) -> InvoiceForm {
    let flag = |key: &str| value.get(key).and_then(Value::as_bool).unwrap_or(false);
    InvoiceForm {
        currency: json_field_str(value, "currency"),
        price_parts: parse_price_parts(value.get("price_parts")),
        max_tip_amount: int53(value.get("max_tip_amount")).unwrap_or(0),
        suggested_tip_amounts: value
            .get("suggested_tip_amounts")
            .and_then(Value::as_array)
            .map(|arr| arr.iter().filter_map(|v| int53(Some(v)).ok()).collect())
            .unwrap_or_default(),
        recurring_terms_url: json_field_str(value, "recurring_payment_terms_of_service_url"),
        terms_url: json_field_str(value, "terms_of_service_url"),
        is_test: flag("is_test"),
        need_name: flag("need_name"),
        need_phone_number: flag("need_phone_number"),
        need_email_address: flag("need_email_address"),
        need_shipping_address: flag("need_shipping_address"),
        is_flexible: flag("is_flexible"),
    }
}

/// Slice P1: `shippingOption` (schema:4668).
pub(crate) fn parse_shipping_option(value: &Value) -> ShippingOptionData {
    ShippingOptionData {
        id: json_field_str(value, "id"),
        title: json_field_str(value, "title"),
        price_parts: parse_price_parts(value.get("price_parts")),
    }
}

/// Slice P1: `paymentForm` (TDLib 1.8.67, `schema/td_api.tl:4734`).
/// `None` when the JSON is malformed; an unknown form *type* parses to
/// `PaymentFormTypeData::Unknown` so the dialog can decline it honestly.
pub(crate) fn parse_payment_form(value: &Value) -> Option<PaymentFormData> {
    let form_type_value = value.get("type")?;
    let form_type = match form_type_value.get("@type").and_then(Value::as_str)? {
        "paymentFormTypeRegular" => {
            let provider_value = form_type_value.get("payment_provider")?;
            let provider = match provider_value.get("@type").and_then(Value::as_str) {
                Some("paymentProviderOther") => PaymentProviderKind::Web {
                    url: json_field_str(provider_value, "url"),
                },
                Some("paymentProviderStripe") => PaymentProviderKind::Token {
                    name: "Stripe".into(),
                },
                Some("paymentProviderSmartGlocal") => PaymentProviderKind::Token {
                    name: "Smart Glocal".into(),
                },
                _ => PaymentProviderKind::Token {
                    name: "card".into(),
                },
            };
            PaymentFormTypeData::Regular(Box::new(PaymentFormRegular {
                invoice: parse_invoice_form(form_type_value.get("invoice")?),
                provider,
                additional_options: form_type_value
                    .get("additional_payment_options")
                    .and_then(Value::as_array)
                    .map(|arr| {
                        arr.iter()
                            .map(|opt| PaymentOption {
                                title: json_field_str(opt, "title"),
                                url: json_field_str(opt, "url"),
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
                saved_order_info: parse_order_info(form_type_value.get("saved_order_info")),
                saved_credentials: form_type_value
                    .get("saved_credentials")
                    .and_then(Value::as_array)
                    .map(|arr| {
                        arr.iter()
                            .map(|cred| SavedCredential {
                                id: json_field_str(cred, "id"),
                                title: json_field_str(cred, "title"),
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
                can_save_credentials: form_type_value
                    .get("can_save_credentials")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                need_password: form_type_value
                    .get("need_password")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            }))
        }
        "paymentFormTypeStars" => PaymentFormTypeData::Stars {
            star_count: int53(form_type_value.get("star_count")).unwrap_or(0),
        },
        "paymentFormTypeStarSubscription" => PaymentFormTypeData::StarSubscription,
        _ => PaymentFormTypeData::Unknown,
    };
    let product_info = value.get("product_info")?;
    Some(PaymentFormData {
        id: int53(value.get("id")).unwrap_or(0),
        form_type,
        seller_bot_user_id: int53(value.get("seller_bot_user_id")).unwrap_or(0),
        product_title: product_info
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        product_description: product_info
            .get("description")
            .and_then(|desc| desc.get("text"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
    })
}

/// Slice P1: `paymentReceipt` (TDLib 1.8.67, `schema/td_api.tl:4765`).
pub(crate) fn parse_payment_receipt(value: &Value) -> Option<PaymentReceiptData> {
    let receipt_type = value.get("type")?;
    let (currency, total_amount, credentials_title, tip_amount, is_stars, star_count) =
        match receipt_type.get("@type").and_then(Value::as_str)? {
            "paymentReceiptTypeRegular" => {
                let invoice = receipt_type.get("invoice")?;
                (
                    json_field_str(invoice, "currency"),
                    price_parts_total(&parse_price_parts(invoice.get("price_parts"))),
                    json_field_str(receipt_type, "credentials_title"),
                    int53(receipt_type.get("tip_amount")).unwrap_or(0),
                    false,
                    0,
                )
            }
            "paymentReceiptTypeStars" => (
                String::new(),
                0,
                String::new(),
                0,
                true,
                int53(receipt_type.get("star_count")).unwrap_or(0),
            ),
            _ => return None,
        };
    let product_info = value.get("product_info")?;
    Some(PaymentReceiptData {
        product_title: product_info
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        date: value.get("date").and_then(Value::as_i64).unwrap_or(0) as i32,
        currency,
        total_amount,
        credentials_title,
        tip_amount,
        is_stars,
        star_count,
    })
}

/// Slice P1: sum of a `labeledPricePart` list — the receipt carries no
/// `total_amount`, only the invoice's price parts (schema:4752).
pub fn price_parts_total(parts: &[LabeledPrice]) -> i64 {
    parts.iter().map(|part| part.amount).sum()
}

/// Slice P1: format a TDLib smallest-unit amount as "USD 19.99", using the
/// ISO 4217 exponent (0 for JPY and friends, 3 for BHD and friends, 2
/// otherwise) — amounts the API returns are always smallest units, so the
/// exponent has to come from the currency, not the payload.
pub fn format_payment_price(currency: &str, amount: i64) -> String {
    let exponent: u32 = match currency {
        "BIF" | "CLP" | "DJF" | "GNF" | "JPY" | "KMF" | "KRW" | "MGA" | "PYG" | "RWF" | "UGX"
        | "UYI" | "VND" | "VUV" | "XAF" | "XOF" | "XPF" => 0,
        "BHD" | "IQD" | "JOD" | "KWD" | "OMR" | "TND" => 3,
        _ => 2,
    };
    let divisor = 10_i64.pow(exponent);
    let (whole, frac) = (amount.div_euclid(divisor), amount.rem_euclid(divisor));
    match exponent {
        0 => format!("{currency} {whole}"),
        3 => format!("{currency} {whole}.{frac:03}"),
        _ => format!("{currency} {whole}.{frac:02}"),
    }
}

/// Slice `parity:bots-payment-recurring`: `starSubscriptionType*` (TDLib
/// 1.8.67, `schema/td_api.tl:1239` / `:1246`). Unknown future variants
/// parse to `Unknown`, never `None`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StarSubscriptionTypeData {
    /// `starSubscriptionTypeChannel` — a subscription to a channel chat.
    Channel {
        can_reuse: bool,
        invite_link: String,
    },
    /// `starSubscriptionTypeBot` — a subscription in a bot.
    Bot {
        is_canceled_by_bot: bool,
        title: String,
        invoice_link: String,
    },
    Unknown,
}

/// Slice `parity:bots-payment-recurring`: `starSubscription` (TDLib 1.8.67,
/// `schema/td_api.tl:1262`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StarSubscriptionData {
    pub id: String,
    pub chat_id: i64,
    pub expiration_date: i32,
    pub is_canceled: bool,
    pub is_expiring: bool,
    pub pricing: StarSubscriptionPricing,
    pub sub_type: StarSubscriptionTypeData,
}

/// Slice `parity:bots-payment-recurring`: `starSubscriptions` (TDLib 1.8.67,
/// `schema/td_api.tl:1269`) — the `getStarSubscriptions` answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StarSubscriptionsData {
    /// The user's current Stars balance (whole stars; nanostars dropped —
    /// the UI shows whole stars only).
    pub star_amount: i64,
    pub subscriptions: Vec<StarSubscriptionData>,
    pub required_star_count: i64,
    pub next_offset: String,
}

/// Slice `parity:bots-payment-recurring`: parse one `starSubscription`.
pub(crate) fn parse_star_subscription(value: &Value) -> Option<StarSubscriptionData> {
    let type_value = value.get("type")?;
    let sub_type = match type_value.get("@type").and_then(Value::as_str)? {
        "starSubscriptionTypeChannel" => StarSubscriptionTypeData::Channel {
            can_reuse: type_value
                .get("can_reuse")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            invite_link: json_field_str(type_value, "invite_link"),
        },
        "starSubscriptionTypeBot" => StarSubscriptionTypeData::Bot {
            is_canceled_by_bot: type_value
                .get("is_canceled_by_bot")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            title: json_field_str(type_value, "title"),
            invoice_link: json_field_str(type_value, "invoice_link"),
        },
        _ => StarSubscriptionTypeData::Unknown,
    };
    Some(StarSubscriptionData {
        id: json_field_str(value, "id"),
        chat_id: int53(value.get("chat_id")).unwrap_or(0),
        expiration_date: value
            .get("expiration_date")
            .and_then(Value::as_i64)
            .unwrap_or(0) as i32,
        is_canceled: value
            .get("is_canceled")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        is_expiring: value
            .get("is_expiring")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        pricing: parse_star_subscription_pricing(value.get("pricing"))?,
        sub_type,
    })
}

/// Slice `parity:bots-payment-recurring`: parse the `getStarSubscriptions`
/// answer (`starSubscriptions`, schema:1269). `None` when the JSON is
/// malformed.
pub(crate) fn parse_star_subscriptions(value: &Value) -> Option<StarSubscriptionsData> {
    let star_amount = value
        .get("star_amount")
        .and_then(|a| a.get("amount"))
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let subscriptions = value
        .get("subscriptions")
        .and_then(Value::as_array)
        .map(|arr| arr.iter().filter_map(parse_star_subscription).collect())
        .unwrap_or_default();
    Some(StarSubscriptionsData {
        star_amount,
        subscriptions,
        required_star_count: value
            .get("required_star_count")
            .and_then(Value::as_i64)
            .unwrap_or(0),
        next_offset: json_field_str(value, "next_offset"),
    })
}
