//! State reducer tests: payments.
use super::common::*;
use super::*;

#[test]
fn payment_form_applies_only_to_own_request() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let form_json = |extra: &str| {
        format!(
            r#"{{"@type":"paymentForm","@extra":"{extra}","id":7,"type":{{"@type":"paymentFormTypeRegular","invoice":{{"@type":"invoice","currency":"USD","price_parts":[{{"@type":"labeledPricePart","label":"Machine","amount":1999}}],"subscription_period":0,"max_tip_amount":0,"suggested_tip_amounts":[],"recurring_payment_terms_of_service_url":"","terms_of_service_url":"","is_test":true,"need_name":true,"need_phone_number":false,"need_email_address":true,"need_shipping_address":false,"send_phone_number_to_provider":false,"send_email_address_to_provider":false,"is_flexible":false}},"payment_provider_user_id":99,"payment_provider":{{"@type":"paymentProviderOther","url":"https://pay.example.com/x"}},"additional_payment_options":[],"saved_order_info":null,"saved_credentials":[],"can_save_credentials":true,"need_password":false}},"seller_bot_user_id":51,"product_info":{{"@type":"productInfo","title":"Time machine","description":{{"@type":"formattedText","text":"","entities":[]}},"photo":null}}}}"#
        )
    };
    apply_json(&mut session, &seq, &sink, &form_json("999"));
    assert!(session.payment_form.is_none());
    let extra = session.request(RequestPurpose::GetPaymentForm, Some(ChatId(51)));
    session.payment_form_loading = true;
    apply_json(&mut session, &seq, &sink, &form_json(&extra.0.to_string()));
    let form = session.payment_form.as_ref().expect("form applies");
    assert_eq!(form.id, 7);
    assert_eq!(form.product_title, "Time machine");
    assert!(!session.payment_form_loading);
}

#[test]
fn payment_request_survives_form_and_validated_answers() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    // Buy press: `getPaymentForm` sent, request context set.
    let extra = session.request(RequestPurpose::GetPaymentForm, Some(ChatId(51)));
    session.payment_request = Some(PaymentRequest {
        chat_id: ChatId(51),
        message_id: MessageId(7),
    });
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"paymentForm","@extra":"{}","id":7,"type":{{"@type":"paymentFormTypeRegular","invoice":{{"@type":"invoice","currency":"USD","price_parts":[{{"@type":"labeledPricePart","label":"Machine","amount":1999}}],"subscription_period":0,"max_tip_amount":0,"suggested_tip_amounts":[],"recurring_payment_terms_of_service_url":"","terms_of_service_url":"","is_test":true,"need_name":true,"need_phone_number":false,"need_email_address":true,"need_shipping_address":false,"send_phone_number_to_provider":false,"send_email_address_to_provider":false,"is_flexible":false}},"payment_provider_user_id":99,"payment_provider":{{"@type":"paymentProviderOther","url":"https://pay.example.com/x"}},"additional_payment_options":[],"saved_order_info":null,"saved_credentials":[],"can_save_credentials":true,"need_password":false}},"seller_bot_user_id":51,"product_info":{{"@type":"productInfo","title":"Time machine","description":{{"@type":"formattedText","text":"","entities":[]}},"photo":null}}}}"#,
            extra.0
        ),
    );
    assert!(session.payment_form.is_some());
    assert!(
        session.payment_request.is_some(),
        "form answer must not clear the payment request"
    );
    // Continue: `validateOrderInfo` sent, request context refreshed.
    let extra = session.request(RequestPurpose::ValidateOrderInfo, Some(ChatId(51)));
    session.payment_request = Some(PaymentRequest {
        chat_id: ChatId(51),
        message_id: MessageId(7),
    });
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"validatedOrderInfo","@extra":"{}","order_info_id":"oid1","shipping_options":[{{"@type":"shippingOption","id":"ship1","title":"Standard","price_parts":[]}}]}}"#,
            extra.0
        ),
    );
    assert!(session.payment_validated.is_some());
    assert_eq!(session.payment_shipping_id.as_deref(), Some("ship1"));
    assert!(
        session.payment_request.is_some(),
        "validated answer must not clear the payment request"
    );
}

#[test]
fn payment_form_error_surfaces_note() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.payment_form_loading = true;
    let extra = session.request(RequestPurpose::GetPaymentForm, Some(ChatId(51)));
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"MESSAGE_NOT_MODIFIED"}}"#,
            extra.0
        ),
    );
    assert!(!session.payment_form_loading);
    assert!(
        session
            .payment_note
            .as_deref()
            .unwrap_or("")
            .starts_with("Payment failed:")
    );
}

#[test]
fn payment_result_notes_and_verification_url() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let result_json = |extra: &str, success: bool, url: &str| {
        format!(
            r#"{{"@type":"paymentResult","@extra":"{extra}","success":{success},"verification_url":"{url}"}}"#
        )
    };
    let extra = session.request(RequestPurpose::SendPaymentForm, Some(ChatId(51)));
    apply_json(
        &mut session,
        &seq,
        &sink,
        &result_json(&extra.0.to_string(), true, ""),
    );
    assert_eq!(
        session.payment_note.as_deref(),
        Some("✅ Payment successful")
    );
    let extra = session.request(RequestPurpose::SendPaymentForm, Some(ChatId(51)));
    apply_json(
        &mut session,
        &seq,
        &sink,
        &result_json(&extra.0.to_string(), false, "https://pay.example.com/3ds"),
    );
    assert_eq!(
        session.payment_verification_url.as_deref(),
        Some("https://pay.example.com/3ds")
    );
    let extra = session.request(RequestPurpose::SendPaymentForm, Some(ChatId(51)));
    apply_json(
        &mut session,
        &seq,
        &sink,
        &result_json(&extra.0.to_string(), false, ""),
    );
    assert_eq!(session.payment_note.as_deref(), Some("Payment failed"));
}

#[test]
fn payment_receipt_opens_dialog() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let extra = session.request(RequestPurpose::GetPaymentReceipt, Some(ChatId(51)));
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"paymentReceipt","@extra":"{}","product_info":{{"@type":"productInfo","title":"Time machine","description":{{"@type":"formattedText","text":"","entities":[]}},"photo":null}},"date":1727400000,"seller_bot_user_id":51,"type":{{"@type":"paymentReceiptTypeRegular","payment_provider_user_id":99,"invoice":{{"@type":"invoice","currency":"USD","price_parts":[{{"@type":"labeledPricePart","label":"Machine","amount":1999}}],"subscription_period":0,"max_tip_amount":0,"suggested_tip_amounts":[],"recurring_payment_terms_of_service_url":"","terms_of_service_url":"","is_test":false,"need_name":false,"need_phone_number":false,"need_email_address":false,"need_shipping_address":false,"send_phone_number_to_provider":false,"send_email_address_to_provider":false,"is_flexible":false}},"order_info":{{"@type":"orderInfo","name":"","phone_number":"","email_address":"","shipping_address":{{"@type":"address","country_code":"","state":"","city":"","street_line1":"","street_line2":"","postal_code":""}}}},"shipping_option":{{"@type":"shippingOption","id":"","title":"","price_parts":[]}},"credentials_title":"Visa •• 4242","tip_amount":0}}}}"#,
            extra.0
        ),
    );
    let receipt = session.payment_receipt.as_ref().expect("receipt");
    assert_eq!(receipt.product_title, "Time machine");
    assert_eq!(receipt.total_amount, 1999);
    assert_eq!(receipt.credentials_title, "Visa •• 4242");
    assert!(session.payment_receipt_open);
}

#[test]
fn star_subscriptions_apply_only_to_own_request() {
    // Slice `parity:bots-payment-recurring`: the `starSubscriptions`
    // answer applies only when `@extra` matches our `getStarSubscriptions`.
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let subs_json = |extra: &str| {
        format!(
            r#"{{"@type":"starSubscriptions","@extra":"{extra}","star_amount":{{"@type":"starAmount","star_count":500,"nanostar_count":0}},"required_star_count":0,"next_offset":"","subscriptions":[{{"@type":"starSubscription","id":"sub1","chat_id":-1001,"expiration_date":1790000000,"is_canceled":false,"is_expiring":false,"pricing":{{"@type":"starSubscriptionPricing","period":2592000,"star_count":100}},"type":{{"@type":"starSubscriptionTypeChannel","invite_link":null}}}}]}}"#
        )
    };
    apply_json(&mut session, &seq, &sink, &subs_json("999"));
    assert!(session.star_subscriptions.is_none());
    let extra = session.request(
        RequestPurpose::Payments(PaymentsPurpose::GetStarSubscriptions { append: false }),
        None,
    );
    session.star_subscriptions_loading = true;
    apply_json(&mut session, &seq, &sink, &subs_json(&extra.0.to_string()));
    let subs = session.star_subscriptions.as_ref().expect("list applies");
    assert_eq!(subs.subscriptions.len(), 1);
    assert_eq!(subs.subscriptions[0].id, "sub1");
    assert_eq!(subs.star_amount, 500);
    assert!(!session.star_subscriptions_loading);
}

#[test]
fn star_subscriptions_append_page_merges() {
    // Slice `parity:bots-payment-recurring`: a follow-up
    // `getStarSubscriptions` page (offset) appends; a fresh fetch
    // replaces.
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let subs_json = |extra: &str, id: &str, next: &str| {
        format!(
            r#"{{"@type":"starSubscriptions","@extra":"{extra}","star_amount":{{"@type":"starAmount","star_count":500,"nanostar_count":0}},"required_star_count":0,"next_offset":"{next}","subscriptions":[{{"@type":"starSubscription","id":"{id}","chat_id":-1001,"expiration_date":1790000000,"is_canceled":false,"is_expiring":false,"pricing":{{"@type":"starSubscriptionPricing","period":2592000,"star_count":100}},"type":{{"@type":"starSubscriptionTypeChannel","invite_link":null}}}}]}}"#
        )
    };
    let extra = session.request(
        RequestPurpose::Payments(PaymentsPurpose::GetStarSubscriptions { append: false }),
        None,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &subs_json(&extra.0.to_string(), "sub1", "50"),
    );
    assert_eq!(session.star_subscriptions_offset, "50");
    let extra = session.request(
        RequestPurpose::Payments(PaymentsPurpose::GetStarSubscriptions { append: true }),
        None,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &subs_json(&extra.0.to_string(), "sub2", ""),
    );
    let subs = session.star_subscriptions.as_ref().expect("list applies");
    assert_eq!(subs.subscriptions.len(), 2);
    assert_eq!(subs.subscriptions[0].id, "sub1");
    assert_eq!(subs.subscriptions[1].id, "sub2");
    assert_eq!(session.star_subscriptions_offset, "");
}

#[test]
fn star_subscription_mutation_ok_marks_stale() {
    // Slice `parity:bots-payment-recurring`: an `editStarSubscription`
    // `ok` marks the list stale (refetch) instead of flipping the row
    // optimistically.
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let extra = session.request(RequestPurpose::EditStarSubscription, None);
    session.star_subscriptions_mutating = true;
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0),
    );
    assert!(session.star_subscriptions_stale);
    assert!(!session.star_subscriptions_mutating);
    assert!(session.star_subscriptions_error.is_none());
}

#[test]
fn star_subscriptions_error_surfaces() {
    // Slice `parity:bots-payment-recurring`: a failed
    // `getStarSubscriptions` surfaces the reason and stops the spinner.
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let extra = session.request(
        RequestPurpose::Payments(PaymentsPurpose::GetStarSubscriptions { append: false }),
        None,
    );
    session.star_subscriptions_loading = true;
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"SUBSCRIPTION_NOT_FOUND"}}"#,
            extra.0
        ),
    );
    assert!(!session.star_subscriptions_loading);
    // `error_reason` never echoes the native TDLib message (it can
    // contain secrets) — the classified reason surfaces instead.
    let err = session
        .star_subscriptions_error
        .as_ref()
        .expect("error surfaces");
    assert_eq!(err, "Couldn't load subscriptions: invalid request");
}
