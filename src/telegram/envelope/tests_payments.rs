use super::*;

#[test]
fn p1_payment_parses() {
    // `messageInvoice` (schema 1.8.67, line 5270).
    let env = parse_envelope(
            r#"{"@type":"updateNewMessage","message":{"id":401,"chat_id":21,"is_outgoing":false,"content":{"@type":"messageInvoice","product_info":{"@type":"productInfo","title":"Time machine","description":{"@type":"formattedText","text":"Visit your ancestors","entities":[]},"photo":null},"currency":"USD","total_amount":1999,"start_parameter":"buy","is_test":true,"need_shipping_address":true,"receipt_message_id":0,"paid_media":null,"paid_media_caption":{"@type":"formattedText","text":"","entities":[]}}}}"#,
        )
        .unwrap();
    match env.payload {
        EnvelopePayload::Messages(MessagesPayload::UpdateNewMessage(message)) => {
            match &message.content {
                MessageContent::Invoice(invoice) => {
                    assert_eq!(invoice.title, "Time machine");
                    assert_eq!(invoice.description, "Visit your ancestors");
                    assert_eq!(invoice.currency, "USD");
                    assert_eq!(invoice.total_amount, 1999);
                    assert!(invoice.is_test);
                    assert!(invoice.need_shipping_address);
                    assert_eq!(invoice.receipt_message_id, 0);
                }
                other => panic!("{other:?}"),
            }
        }
        other => panic!("{other:?}"),
    }
    // `messagePaymentSuccessful` (schema 1.8.67, line 5436).
    let env = parse_envelope(
            r#"{"@type":"updateNewMessage","message":{"id":402,"chat_id":21,"is_outgoing":false,"content":{"@type":"messagePaymentSuccessful","invoice_chat_id":21,"invoice_message_id":401,"currency":"USD","total_amount":1999,"subscription_until_date":0,"is_recurring":false,"is_first_recurring":false,"invoice_name":"Time machine"}}}"#,
        )
        .unwrap();
    match env.payload {
        EnvelopePayload::Messages(MessagesPayload::UpdateNewMessage(message)) => {
            match &message.content {
                MessageContent::PaymentSuccessful(success) => {
                    assert_eq!(success.invoice_message_id, 401);
                    assert_eq!(success.currency, "USD");
                    assert_eq!(success.total_amount, 1999);
                    assert!(!success.is_recurring);
                    assert_eq!(success.invoice_name, "Time machine");
                }
                other => panic!("{other:?}"),
            }
        }
        other => panic!("{other:?}"),
    }
    // `messagePaymentSuccessfulBot` (schema 1.8.67, line 5449) —
    // minimal seller-side parse.
    let env = parse_envelope(
            r#"{"@type":"updateNewMessage","message":{"id":403,"chat_id":21,"is_outgoing":true,"content":{"@type":"messagePaymentSuccessfulBot","currency":"USD","total_amount":1999,"subscription_until_date":0,"is_recurring":true,"is_first_recurring":true,"invoice_payload":"cGF5","shipping_option_id":"","order_info":null,"telegram_payment_charge_id":"1","provider_payment_charge_id":"2"}}}"#,
        )
        .unwrap();
    match env.payload {
        EnvelopePayload::Messages(MessagesPayload::UpdateNewMessage(message)) => {
            match &message.content {
                MessageContent::PaymentReceived(received) => {
                    assert_eq!(received.currency, "USD");
                    assert_eq!(received.total_amount, 1999);
                    assert!(received.is_recurring);
                }
                other => panic!("{other:?}"),
            }
        }
        other => panic!("{other:?}"),
    }
    // Previews.
    assert_eq!(
        MessageContent::Invoice(InvoiceContent {
            title: "Time machine".into(),
            description: String::new(),
            currency: "USD".into(),
            total_amount: 1999,
            is_test: true,
            need_shipping_address: false,
            receipt_message_id: 0,
        })
        .preview(),
        "🧾 Time machine"
    );
}

#[test]
fn p1_payment_form_parsed() {
    // `paymentForm` with a `paymentFormTypeRegular` (schema:4734/:4720).
    let env = parse_envelope(
            r#"{"@type":"paymentForm","@extra":"21","id":7,"type":{"@type":"paymentFormTypeRegular","invoice":{"@type":"invoice","currency":"USD","price_parts":[{"@type":"labeledPricePart","label":"Machine","amount":1999}],"subscription_period":0,"max_tip_amount":0,"suggested_tip_amounts":[],"recurring_payment_terms_of_service_url":"","terms_of_service_url":"https://example.com/tos","is_test":false,"need_name":true,"need_phone_number":false,"need_email_address":true,"need_shipping_address":false,"send_phone_number_to_provider":false,"send_email_address_to_provider":false,"is_flexible":false},"payment_provider_user_id":99,"payment_provider":{"@type":"paymentProviderOther","url":"https://pay.example.com/x"},"additional_payment_options":[],"saved_order_info":null,"saved_credentials":[{"@type":"savedCredentials","id":"cred1","title":"Visa •• 4242"}],"can_save_credentials":true,"need_password":false},"seller_bot_user_id":21,"product_info":{"@type":"productInfo","title":"Time machine","description":{"@type":"formattedText","text":"Visit your ancestors","entities":[]},"photo":null}}"#,
        )
        .unwrap();
    match env.payload {
        EnvelopePayload::Payments(PaymentsPayload::PaymentForm(form)) => {
            assert_eq!(form.id, 7);
            assert_eq!(form.product_title, "Time machine");
            assert_eq!(form.product_description, "Visit your ancestors");
            match form.form_type {
                PaymentFormTypeData::Regular(regular) => {
                    let invoice = &regular.invoice;
                    let provider = &regular.provider;
                    let saved_credentials = &regular.saved_credentials;
                    assert_eq!(invoice.currency, "USD");
                    assert_eq!(invoice.price_parts.len(), 1);
                    assert_eq!(invoice.price_parts[0].label, "Machine");
                    assert_eq!(invoice.terms_url, "https://example.com/tos");
                    assert!(invoice.need_name);
                    assert!(invoice.need_email_address);
                    assert!(!invoice.need_shipping_address);
                    assert_eq!(
                        *provider,
                        PaymentProviderKind::Web {
                            url: "https://pay.example.com/x".into()
                        }
                    );
                    assert_eq!(saved_credentials.len(), 1);
                    assert_eq!(saved_credentials[0].id, "cred1");
                }
                other => panic!("{other:?}"),
            }
        }
        other => panic!("{other:?}"),
    }
    // Stars form type parses to the honest unsupported variant.
    let env = parse_envelope(
            r#"{"@type":"paymentForm","@extra":"22","id":8,"type":{"@type":"paymentFormTypeStars","star_count":50},"seller_bot_user_id":21,"product_info":{"@type":"productInfo","title":"Stars pack","description":{"@type":"formattedText","text":"","entities":[]},"photo":null}}"#,
        )
        .unwrap();
    match env.payload {
        EnvelopePayload::Payments(PaymentsPayload::PaymentForm(form)) => {
            assert_eq!(
                form.form_type,
                PaymentFormTypeData::Stars { star_count: 50 }
            );
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn p1_validated_order_info_and_result_parsed() {
    // `validatedOrderInfo` (schema:4737).
    let env = parse_envelope(
            r#"{"@type":"validatedOrderInfo","@extra":"23","order_info_id":"oi1","shipping_options":[{"@type":"shippingOption","id":"fast","title":"Express","price_parts":[{"@type":"labeledPricePart","label":"Express","amount":500}]}]}"#,
        )
        .unwrap();
    match env.payload {
        EnvelopePayload::Payments(PaymentsPayload::ValidatedOrderInfo(validated)) => {
            assert_eq!(validated.order_info_id, "oi1");
            assert_eq!(validated.shipping_options.len(), 1);
            assert_eq!(validated.shipping_options[0].id, "fast");
            assert_eq!(validated.shipping_options[0].price_parts[0].amount, 500);
        }
        other => panic!("{other:?}"),
    }
    // `paymentResult` (schema:4740).
    let env = parse_envelope(
        r#"{"@type":"paymentResult","@extra":"24","success":true,"verification_url":""}"#,
    )
    .unwrap();
    match env.payload {
        EnvelopePayload::Payments(PaymentsPayload::PaymentResult(result)) => {
            assert!(result.success);
            assert!(result.verification_url.is_empty());
        }
        other => panic!("{other:?}"),
    }
    // `paymentReceipt` with a regular receipt type (schema:4765/:4752).
    let env = parse_envelope(
            r#"{"@type":"paymentReceipt","@extra":"25","product_info":{"@type":"productInfo","title":"Time machine","description":{"@type":"formattedText","text":"","entities":[]},"photo":null},"date":1790000000,"seller_bot_user_id":21,"type":{"@type":"paymentReceiptTypeRegular","payment_provider_user_id":99,"invoice":{"@type":"invoice","currency":"USD","price_parts":[{"@type":"labeledPricePart","label":"Machine","amount":1999}],"subscription_period":0,"max_tip_amount":0,"suggested_tip_amounts":[],"recurring_payment_terms_of_service_url":"","terms_of_service_url":"","is_test":false,"need_name":false,"need_phone_number":false,"need_email_address":false,"need_shipping_address":false,"send_phone_number_to_provider":false,"send_email_address_to_provider":false,"is_flexible":false},"order_info":null,"shipping_option":null,"credentials_title":"Visa •• 4242","tip_amount":0}}"#,
        )
        .unwrap();
    match env.payload {
        EnvelopePayload::Payments(PaymentsPayload::PaymentReceipt(receipt)) => {
            assert_eq!(receipt.product_title, "Time machine");
            assert_eq!(receipt.currency, "USD");
            assert_eq!(receipt.total_amount, 1999);
            assert_eq!(receipt.credentials_title, "Visa •• 4242");
            assert!(!receipt.is_stars);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn p1_unknown_payment_form_type_parses_honestly() {
    // Slice P1 fix-up: an unknown `paymentFormType*` parses to
    // `Unknown` (never `None`) so the pending request resolves and the
    // dialog declines it instead of spinning forever.
    let env = parse_envelope(
            r#"{"@type":"paymentForm","@extra":"26","id":9,"type":{"@type":"paymentFormTypeFuture"},"seller_bot_user_id":21,"product_info":{"@type":"productInfo","title":"Future thing","description":{"@type":"formattedText","text":"","entities":[]},"photo":null}}"#,
        )
        .unwrap();
    match env.payload {
        EnvelopePayload::Payments(PaymentsPayload::PaymentForm(form)) => {
            assert_eq!(form.form_type, PaymentFormTypeData::Unknown);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn p1_payment_price_uses_currency_exponents() {
    // Slice P1 fix-up: ISO 4217 exponents — zero-decimal (JPY),
    // three-decimal (BHD), two otherwise.
    assert_eq!(format_payment_price("USD", 1999), "USD 19.99");
    assert_eq!(format_payment_price("JPY", 14322), "JPY 14322");
    assert_eq!(format_payment_price("BHD", 1999), "BHD 1.999");
    assert_eq!(format_payment_price("KRW", 1000), "KRW 1000");
    assert_eq!(format_payment_price("EUR", 5), "EUR 0.05");
}

#[test]
fn payment_recurring_star_subscriptions_parse() {
    // Slice `parity:bots-payment-recurring`: `starSubscriptions` with a
    // channel subscription, a bot subscription, and a next page
    // (schema 1.8.67, lines 1239/1246/1262/1269).
    let env = parse_envelope(
            r#"{"@type":"starSubscriptions","star_amount":{"@type":"starAmount","star_count":500,"nanostar_count":0},"required_star_count":100,"next_offset":"50","subscriptions":[{"@type":"starSubscription","id":"sub1","chat_id":-1001,"expiration_date":1790000000,"is_canceled":false,"is_expiring":false,"pricing":{"@type":"starSubscriptionPricing","period":2592000,"star_count":100},"type":{"@type":"starSubscriptionTypeChannel","can_reuse":true,"invite_link":"https://t.me/+abc"}},{"@type":"starSubscription","id":"sub2","chat_id":2,"expiration_date":1700000000,"is_canceled":true,"is_expiring":true,"pricing":{"@type":"starSubscriptionPricing","period":604800,"star_count":25},"type":{"@type":"starSubscriptionTypeBot","is_canceled_by_bot":false,"title":"My Bot","photo":null,"invoice_link":"https://t.me/$botinvoice"}}]}"#,
        )
        .unwrap();
    match env.payload {
        EnvelopePayload::Payments(PaymentsPayload::StarSubscriptions(subs)) => {
            assert_eq!(subs.star_amount, 500);
            assert_eq!(subs.required_star_count, 100);
            assert_eq!(subs.next_offset, "50");
            assert_eq!(subs.subscriptions.len(), 2);
            let ch = &subs.subscriptions[0];
            assert_eq!(ch.id, "sub1");
            assert_eq!(ch.chat_id, -1001);
            assert!(!ch.is_canceled && !ch.is_expiring);
            assert_eq!(ch.pricing.period, 2_592_000);
            assert_eq!(ch.pricing.star_count, 100);
            match &ch.sub_type {
                StarSubscriptionTypeData::Channel {
                    can_reuse,
                    invite_link,
                } => {
                    assert!(can_reuse);
                    assert_eq!(invite_link, "https://t.me/+abc");
                }
                other => panic!("{other:?}"),
            }
            let bot = &subs.subscriptions[1];
            assert_eq!(bot.id, "sub2");
            assert!(bot.is_canceled && bot.is_expiring);
            match &bot.sub_type {
                StarSubscriptionTypeData::Bot {
                    is_canceled_by_bot,
                    title,
                    invoice_link,
                } => {
                    assert!(!is_canceled_by_bot);
                    assert_eq!(title, "My Bot");
                    assert_eq!(invoice_link, "https://t.me/$botinvoice");
                }
                other => panic!("{other:?}"),
            }
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn payment_recurring_star_subscriptions_tolerate_gaps() {
    // Slice `parity:bots-payment-recurring`: an unknown subscription type
    // and a zero-valued pricing block must not fail the whole list — the
    // type parses as `Unknown` and the zero pricing parses fine. A missing
    // pricing block instead drops that subscription (`parse_star_subscription`
    // uses `?` on the pricing parse; pricing is non-optional per the schema).
    let env = parse_envelope(
            r#"{"@type":"starSubscriptions","star_amount":{"@type":"starAmount","star_count":0,"nanostar_count":0},"required_star_count":0,"next_offset":"","subscriptions":[{"@type":"starSubscription","id":"sub9","chat_id":3,"expiration_date":1790000000,"is_canceled":false,"is_expiring":false,"pricing":{"@type":"starSubscriptionPricing","period":0,"star_count":0},"type":{"@type":"starSubscriptionTypeFuture","x":1}}]}"#,
        )
        .unwrap();
    match env.payload {
        EnvelopePayload::Payments(PaymentsPayload::StarSubscriptions(subs)) => {
            assert_eq!(subs.subscriptions.len(), 1);
            let sub = &subs.subscriptions[0];
            assert_eq!(sub.id, "sub9");
            assert!(matches!(sub.sub_type, StarSubscriptionTypeData::Unknown));
            assert_eq!(subs.next_offset, "");
        }
        other => panic!("{other:?}"),
    }
    // A `starSubscriptions` error object parses as `Error`, not as a
    // subscription list.
    let env = parse_envelope(r#"{"@type":"error","code":400,"message":"SUBSCRIPTION_NOT_FOUND"}"#)
        .unwrap();
    assert!(matches!(env.payload, EnvelopePayload::Error(_)));
}

#[test]
fn paid_media_keeps_locked_previews_and_caption() {
    let env = parse_envelope(
        r#"{"@type":"updateNewMessage","message":{"id":402,"chat_id":21,"is_outgoing":false,"content":{"@type":"messagePaidMedia","star_count":25,"media":[{"@type":"paidMediaPreview","width":800,"height":600,"duration":0,"minithumbnail":{"@type":"minithumbnail","width":40,"height":30,"data":"AQID"}},{"@type":"paidMediaPreview","width":400,"height":400,"duration":12,"minithumbnail":null},{"@type":"paidMediaUnsupported"}],"caption":{"@type":"formattedText","text":"Behind the scenes","entities":[]},"show_caption_above_media":false}}}"#,
    )
    .unwrap();
    let EnvelopePayload::Messages(MessagesPayload::UpdateNewMessage(message)) = env.payload else {
        panic!("not a new message");
    };
    let MessageContent::Action(action) = &message.content else {
        panic!("{:?}", message.content);
    };
    let ServiceAction::PaidMedia {
        stars,
        locked,
        caption,
    } = action.as_ref()
    else {
        panic!("{action:?}");
    };
    assert_eq!(*stars, 25);
    assert_eq!(caption, "Behind the scenes");
    assert_eq!(locked.len(), 2);
    assert_eq!((locked[0].width, locked[0].height), (800, 600));
    assert_eq!(
        locked[0].minithumbnail.as_ref().unwrap().data,
        vec![1, 2, 3]
    );
    assert_eq!(locked[1].duration, 12);
    assert!(locked[1].minithumbnail.is_none());
}

#[test]
fn star_subscriptions_balance_uses_schema_star_count() {
    // `starAmount star_count:int53 nanostar_count:int32` (schema:1232).
    let json = r#"{"@type":"starSubscriptions","star_amount":{"@type":"starAmount","star_count":1234,"nanostar_count":500000000},"required_star_count":0,"next_offset":"","subscriptions":[]}"#;
    let value: serde_json::Value = serde_json::from_str(json).unwrap();
    let data = super::payments::parse_star_subscriptions(&value).unwrap();
    assert_eq!(data.star_amount, 1234);
    // A payload using the old, non-schema field name must not be read.
    let wrong = r#"{"@type":"starSubscriptions","star_amount":{"@type":"starAmount","amount":9},"subscriptions":[]}"#;
    let value: serde_json::Value = serde_json::from_str(wrong).unwrap();
    assert_eq!(
        super::payments::parse_star_subscriptions(&value)
            .unwrap()
            .star_amount,
        0
    );
}
