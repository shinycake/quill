use super::*;

#[test]
fn p1_payment_parses() {
    // `messageInvoice` (schema 1.8.67, line 5270).
    let env = parse_envelope(
            r#"{"@type":"updateNewMessage","message":{"id":401,"chat_id":21,"is_outgoing":false,"content":{"@type":"messageInvoice","product_info":{"@type":"productInfo","title":"Time machine","description":{"@type":"formattedText","text":"Visit your ancestors","entities":[]},"photo":null},"currency":"USD","total_amount":1999,"start_parameter":"buy","is_test":true,"need_shipping_address":true,"receipt_message_id":0,"paid_media":null,"paid_media_caption":{"@type":"formattedText","text":"","entities":[]}}}}"#,
        )
        .unwrap();
    match env.payload {
        EnvelopePayload::UpdateNewMessage(message) => match &message.content {
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
        },
        other => panic!("{other:?}"),
    }
    // `messagePaymentSuccessful` (schema 1.8.67, line 5436).
    let env = parse_envelope(
            r#"{"@type":"updateNewMessage","message":{"id":402,"chat_id":21,"is_outgoing":false,"content":{"@type":"messagePaymentSuccessful","invoice_chat_id":21,"invoice_message_id":401,"currency":"USD","total_amount":1999,"subscription_until_date":0,"is_recurring":false,"is_first_recurring":false,"invoice_name":"Time machine"}}}"#,
        )
        .unwrap();
    match env.payload {
        EnvelopePayload::UpdateNewMessage(message) => match &message.content {
            MessageContent::PaymentSuccessful(success) => {
                assert_eq!(success.invoice_message_id, 401);
                assert_eq!(success.currency, "USD");
                assert_eq!(success.total_amount, 1999);
                assert!(!success.is_recurring);
                assert_eq!(success.invoice_name, "Time machine");
            }
            other => panic!("{other:?}"),
        },
        other => panic!("{other:?}"),
    }
    // `messagePaymentSuccessfulBot` (schema 1.8.67, line 5449) —
    // minimal seller-side parse.
    let env = parse_envelope(
            r#"{"@type":"updateNewMessage","message":{"id":403,"chat_id":21,"is_outgoing":true,"content":{"@type":"messagePaymentSuccessfulBot","currency":"USD","total_amount":1999,"subscription_until_date":0,"is_recurring":true,"is_first_recurring":true,"invoice_payload":"cGF5","shipping_option_id":"","order_info":null,"telegram_payment_charge_id":"1","provider_payment_charge_id":"2"}}}"#,
        )
        .unwrap();
    match env.payload {
        EnvelopePayload::UpdateNewMessage(message) => match &message.content {
            MessageContent::PaymentReceived(received) => {
                assert_eq!(received.currency, "USD");
                assert_eq!(received.total_amount, 1999);
                assert!(received.is_recurring);
            }
            other => panic!("{other:?}"),
        },
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
        EnvelopePayload::PaymentForm(form) => {
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
        EnvelopePayload::PaymentForm(form) => {
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
        EnvelopePayload::ValidatedOrderInfo(validated) => {
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
        EnvelopePayload::PaymentResult(result) => {
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
        EnvelopePayload::PaymentReceipt(receipt) => {
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
        EnvelopePayload::PaymentForm(form) => {
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
