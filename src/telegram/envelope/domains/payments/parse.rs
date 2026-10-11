//! Parses TDLib objects for payments, Premium, Stars and gifts.
use crate::telegram::envelope::*;
use serde_json::Value;

/// The payments domain's TDLib types; `Ok(None)` leaves
/// `type_name` to the other domains.
pub(crate) fn parse_payments_payload(
    type_name: &str,
    value: &Value,
) -> Result<Option<EnvelopePayload>, ParseError> {
    let payload = match type_name {
        // Slice P1: payment answers (TDLib 1.8.67, `schema/td_api.tl:4734` /
        // `:4737` / `:4740` / `:4765`).
        "upgradedGift" => Ok(EnvelopePayload::Payments(PaymentsPayload::MarketplaceGift(
            crate::marketplace::GiftQuote::parse(value),
        ))),
        "optionValueInteger" => Ok(EnvelopePayload::Payments(PaymentsPayload::GiftTextLimit(
            int53_or_zero(value.get("value")),
        ))),
        "giftResaleResultOk" => Ok(EnvelopePayload::Payments(
            PaymentsPayload::GiftPurchaseResult(crate::marketplace::GiftPurchaseResult::Sent(
                json_field_str(value, "received_gift_id"),
            )),
        )),
        "giftResaleResultPriceIncreased" => Ok(EnvelopePayload::Payments(
            PaymentsPayload::GiftPurchaseResult(
                crate::marketplace::GiftPurchaseResult::PriceIncreased(
                    value
                        .get("price")
                        .and_then(crate::marketplace::GiftPrice::parse),
                ),
            ),
        )),
        "paymentForm" => parse_payment_form(value)
            .map(|value| EnvelopePayload::Payments(PaymentsPayload::PaymentForm(value)))
            .ok_or(ParseError::MissingField),
        "validatedOrderInfo" => Ok(EnvelopePayload::Payments(
            PaymentsPayload::ValidatedOrderInfo(ValidatedOrderInfoData {
                order_info_id: json_field_str(value, "order_info_id"),
                shipping_options: value
                    .get("shipping_options")
                    .and_then(Value::as_array)
                    .map(|arr| arr.iter().map(parse_shipping_option).collect())
                    .unwrap_or_default(),
            }),
        )),
        "paymentResult" => Ok(EnvelopePayload::Payments(PaymentsPayload::PaymentResult(
            PaymentResultData {
                success: value
                    .get("success")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                verification_url: json_field_str(value, "verification_url"),
            },
        ))),
        "paymentReceipt" => parse_payment_receipt(value)
            .map(|value| EnvelopePayload::Payments(PaymentsPayload::PaymentReceipt(value)))
            .ok_or(ParseError::MissingField),
        // Slice `parity:bots-payment-recurring`: `getStarSubscriptions`
        // answer (TDLib 1.8.67, `schema/td_api.tl:1269`).
        "starSubscriptions" => parse_star_subscriptions(value)
            .map(|value| EnvelopePayload::Payments(PaymentsPayload::StarSubscriptions(value)))
            .ok_or(ParseError::MissingField),
        "starTransactions" => Ok(EnvelopePayload::Payments(
            PaymentsPayload::StarTransactions(crate::premium_hub::parse_star_transactions(value)),
        )),
        "receivedGifts" => Ok(EnvelopePayload::Payments(PaymentsPayload::ReceivedGifts(
            crate::premium_hub::parse_received_gifts(value),
        ))),
        "premiumFeatures" => Ok(EnvelopePayload::Payments(PaymentsPayload::PremiumFeatures(
            crate::premium_hub::parse_premium_features(value),
        ))),
        "premiumState" => Ok(EnvelopePayload::Payments(PaymentsPayload::PremiumState(
            crate::premium_hub::parse_premium_state(value),
        ))),
        "bankCardInfo" => Ok(EnvelopePayload::Payments(PaymentsPayload::BankCardInfo(
            BankCardInfoData::parse(value),
        ))),
        "updateOwnedStarCount" => Ok(EnvelopePayload::Payments(
            PaymentsPayload::UpdateOwnedStarCount(crate::premium_hub::StarAmount::parse(
                value.get("star_amount"),
            )),
        )),
        _ => return Ok(None),
    };
    payload.map(Some)
}
