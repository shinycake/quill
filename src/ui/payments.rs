//! buy buttons, payment dialogs, login-URL confirms.

use super::app::QuillApp;
use super::message_text::format_unix_date_time;
use super::shell::{DialogKind, QuillShell};
use super::*;
use gpui_kit::component::button::*;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::input::{Textarea, TextareaState};
use gpui_kit::component::radio::{Radio, RadioGroup};
use gpui_kit::component::*;
use gpui_kit::*;
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::{ChatId, MessageId};
use quill::state::{LoginUrlRequest, Session};
use quill::telegram::client::copy_and_parse;
use quill::telegram::envelope::{
    LoginUrlInfo, PaymentFormData, PaymentFormTypeData, PaymentProviderKind, StarSubscriptionData,
    StarSubscriptionPricing, StarSubscriptionTypeData, format_payment_price, price_parts_total,
};
use quill::telegram::requests::{input_credentials_new, input_credentials_saved};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
/// `ReadyPoll` fixture (Phase 4.2): open a dedicated "Demo polls" chat
/// (id 15) with two injected `messagePoll` messages through the normal
/// reducer — an open regular poll with a voted option (percentage bars +
/// counts, the chosen option marked) and a closed quiz poll (results only,
/// no voting affordance, correct answer marked).
/// Slice P1 payments demo (injected, no live Telegram): a bot chat with a
/// `messageInvoice` (Buy button), a `messagePaymentSuccessful` row, a paid
/// invoice linking to a receipt, and a seeded `paymentForm` (regular
/// provider, order fields, a saved credential, terms) so the checkout
/// dialog renders open.
pub(super) fn apply_ready_payments(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    use quill::state::RequestPurpose;
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let chat_id = 51;
    let chat_json = format!(
        r#"{{"@type":"updateNewChat","chat":{{"id":{chat_id},"title":"Demo shop bot","type":{{"@type":"chatTypePrivate","user_id":{chat_id}}},"unread_count":0}}}}"#
    );
    let position_json = format!(
        r#"{{"@type":"updateChatPosition","chat_id":{chat_id},"position":{{"@type":"chatPosition","list":{{"@type":"chatListMain"}},"order":"49","is_pinned":false}}}}"#
    );
    for json in [chat_json, position_json] {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
    session.open_chat(ChatId(chat_id));

    let formatted = |text: &str| -> String {
        let text_json = serde_json::to_string(text).unwrap();
        format!(r#"{{"@type":"formattedText","text":{text_json},"entities":[]}}"#)
    };
    let buy_markup = r#"{"@type":"replyMarkupInlineKeyboard","rows":[[{"@type":"inlineKeyboardButton","text":"💳 Buy","type":{"@type":"inlineKeyboardButtonTypeBuy"}}]]}"#;
    let invoice_message = |message_id: i32,
                           title: &str,
                           description: &str,
                           total: i64,
                           is_test: bool,
                           receipt: i64| {
        format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":{message_id},"chat_id":{chat_id},"is_outgoing":false,"content":{{"@type":"messageInvoice","product_info":{{"@type":"productInfo","title":{title_json},"description":{description_json},"photo":null}},"currency":"USD","total_amount":{total},"start_parameter":"buy","is_test":{is_test},"need_shipping_address":false,"receipt_message_id":{receipt},"paid_media":null,"paid_media_caption":{empty_caption}}},"reply_markup":{buy_markup}}}}}"#,
            title_json = serde_json::to_string(title).unwrap(),
            description_json = formatted(description),
            empty_caption = formatted(""),
        )
    };
    let success_message = format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":203,"chat_id":{chat_id},"is_outgoing":false,"content":{{"@type":"messagePaymentSuccessful","invoice_chat_id":{chat_id},"invoice_message_id":201,"currency":"USD","total_amount":1999,"is_recurring":false,"invoice_name":"Time machine"}}}}}}"#
    );
    for json in [
        invoice_message(201, "Time machine", "Visit your ancestors", 1999, true, 0),
        success_message,
        invoice_message(204, "Time machine — paid", "Delivered", 1999, false, 205),
    ] {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }

    // Seed a `paymentForm` answer (regular provider, order fields, one
    // saved credential, terms) through a reserved GetPaymentForm extra so
    // the reducer accepts it exactly like a live answer.
    let extra = session.request(RequestPurpose::GetPaymentForm, Some(ChatId(chat_id)));
    let form_json = format!(
        r#"{{"@type":"paymentForm","@extra":"{}","id":7,"type":{{"@type":"paymentFormTypeRegular","invoice":{{"@type":"invoice","currency":"USD","price_parts":[{{"@type":"labeledPricePart","label":"Machine","amount":1999}}],"subscription_period":0,"max_tip_amount":0,"suggested_tip_amounts":[],"recurring_payment_terms_of_service_url":"","terms_of_service_url":"https://example.com/tos","is_test":true,"need_name":true,"need_phone_number":false,"need_email_address":true,"need_shipping_address":false,"send_phone_number_to_provider":false,"send_email_address_to_provider":false,"is_flexible":false}},"payment_provider_user_id":99,"payment_provider":{{"@type":"paymentProviderOther","url":"https://pay.example.com/x"}},"additional_payment_options":[],"saved_order_info":{{"@type":"orderInfo","name":"Ada","phone_number":"","email_address":"","shipping_address":{{"@type":"address","country_code":"","state":"","city":"","street_line1":"","street_line2":"","postal_code":""}}}},"saved_credentials":[{{"@type":"savedCredentials","id":"cred1","title":"Visa •• 4242"}}],"can_save_credentials":true,"need_password":false}},"seller_bot_user_id":{chat_id},"product_info":{{"@type":"productInfo","title":"Time machine","description":{description_json},"photo":null}}}}"#,
        extra.0,
        description_json = formatted("Visit your ancestors"),
    );
    if let Some(owned) = copy_and_parse(&form_json, seq, &dyn_sink) {
        session.apply(owned);
    }
}

impl QuillApp {
    /// Slice P1: Buy button press — fetch the `paymentForm`
    /// (`getPaymentForm`, schema 1.8.67, line 15262). The dialog opens when
    /// the answer is applied; while it loads, the dialog shows a spinner.
    pub(super) fn press_buy_button(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let sent = self.live.as_mut().map(|live| {
            live.driver.session.payment_form_loading = true;
            live.driver.session.payment_note = None;
            live.driver.send_payment_form_request(chat_id, message_id)
        });
        match sent {
            Some(Ok(_)) => {
                self.open_payment_dialog(window, cx);
            }
            _ => {
                if let Some(live) = self.live.as_mut() {
                    live.driver.session.payment_form_loading = false;
                }
                self.connection.status_note = "could not load the payment form".into();
            }
        }
        cx.notify();
    }

    /// Slice P1: create the checkout dialog inputs, prefilling the order
    /// fields from the form's `saved_order_info` (schema:4720) when present.
    pub(super) fn open_payment_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut dialog = PaymentDialog::new(window, cx);
        if let Some(form) = self.session().and_then(|s| s.payment_form.clone()) {
            dialog.prefill_from_form(&form, window, cx);
        }
        self.payments.dialog = Some(dialog);
    }

    /// Slice P1: close the checkout dialog, discarding the form answers.
    /// The in-flight payment request context is cleared here (and only
    /// here) — the answer reducers keep it so `validateOrderInfo` and
    /// `sendPaymentForm` still correlate after the form answer is applied.
    pub(super) fn close_payment_dialog(&mut self, cx: &mut Context<Self>) {
        self.payments.dialog = None;
        if let Some(live) = self.live.as_mut() {
            live.driver.session.payment_form = None;
            live.driver.session.payment_form_loading = false;
            live.driver.session.payment_note = None;
            live.driver.session.payment_validated = None;
            live.driver.session.payment_shipping_id = None;
            live.driver.session.payment_request = None;
        }
        cx.notify();
    }

    /// Slice P1: close the receipt dialog.
    pub(super) fn close_payment_receipt(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            live.driver.session.payment_receipt = None;
            live.driver.session.payment_receipt_open = false;
        }
        cx.notify();
    }

    /// Slice P1: the regular-form checkout body (split out so
    /// `payment_dialog_overlay` stays readable).
    #[allow(clippy::too_many_arguments)]
    pub(super) fn payment_form_body(
        &self,
        mut body: Div,
        form: &PaymentFormData,
        dialog: &PaymentDialog,
        invoice: &quill::telegram::envelope::InvoiceForm,
        provider: &PaymentProviderKind,
        additional_options: &[quill::telegram::envelope::PaymentOption],
        saved_credentials: &[quill::telegram::envelope::SavedCredential],
        can_save_credentials: bool,
        need_password: bool,
        session: &Session,
        cx: &mut Context<Self>,
    ) -> Div {
        let total = price_parts_total(&invoice.price_parts);
        let mut header = div().flex().flex_col().gap_1().child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .text_sm()
                        .font_semibold()
                        .child(form.product_title.clone()),
                )
                .child(if invoice.is_test {
                    div().text_xs().text_color(warning_text()).child("TEST")
                } else {
                    div()
                }),
        );
        if !form.product_description.is_empty() {
            header = header.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(form.product_description.clone()),
            );
        }
        body = body.child(header);
        let mut prices = div().flex().flex_col().gap_1();
        for part in &invoice.price_parts {
            prices = prices.child(
                div()
                    .flex()
                    .justify_between()
                    .text_sm()
                    .child(div().child(part.label.clone()))
                    .child(div().child(format_payment_price(&invoice.currency, part.amount))),
            );
        }
        prices = prices.child(
            div()
                .flex()
                .justify_between()
                .text_sm()
                .font_semibold()
                .child(div().child("Total"))
                .child(div().child(format_payment_price(&invoice.currency, total))),
        );
        body = body.child(prices);
        // Provider + additional options: Quill has no embedded web view,
        // so web payments open in the OS browser (schema:4689).
        match provider {
            PaymentProviderKind::Web { url } => {
                let url = url.clone();
                body = body.child(
                    Button::new("p1-payment-provider-open")
                        .label("Open provider payment page")
                        .ghost()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.open_message_url(&url, cx);
                        })),
                );
            }
            PaymentProviderKind::Token { name } => {
                body = body.child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(format!(
                            "Card payments via {name}: paste the credential token the provider issued on its site."
                        )),
                );
            }
        }
        for (index, option) in additional_options.iter().enumerate() {
            let url = option.url.clone();
            let title = option.title.clone();
            body = body.child(
                Button::new(format!("p1-payment-option-{index}"))
                    .label(format!("Pay via {title}"))
                    .ghost()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.open_message_url(&url, cx);
                    })),
            );
        }
        // Order info — only the fields the invoice needs (schema:4655).
        let field = |input: &Entity<TextareaState>, label: &'static str| {
            div().child(Textarea::new(input).aria_label(label).h(px(36.)))
        };
        if invoice.need_name {
            body = body.child(field(&dialog.name_input, "Full name"));
        }
        if invoice.need_phone_number {
            body = body.child(field(&dialog.phone_input, "Phone number"));
        }
        if invoice.need_email_address {
            body = body.child(field(&dialog.email_input, "Email address"));
        }
        if invoice.need_shipping_address {
            body = body.child(field(&dialog.street1_input, "Street address"));
            body = body.child(field(&dialog.street2_input, "Apartment or unit"));
            body = body.child(
                div()
                    .flex()
                    .gap_2()
                    .child(div().flex_1().child(field(&dialog.city_input, "City")))
                    .child(
                        div()
                            .flex_1()
                            .child(field(&dialog.state_input, "State or region")),
                    ),
            );
            body = body.child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        div()
                            .flex_1()
                            .child(field(&dialog.country_input, "Country code")),
                    )
                    .child(
                        div()
                            .flex_1()
                            .child(field(&dialog.postal_input, "Postal code")),
                    ),
            );
        }
        let needs_order = invoice.need_name
            || invoice.need_phone_number
            || invoice.need_email_address
            || invoice.need_shipping_address;
        if needs_order {
            let checked = dialog.allow_save_order;
            body = body.child(
                // Phase 6: kit Checkbox (was: ghost button with ☑/☐ label).
                Checkbox::new("p1-payment-save-order")
                    .checked(checked)
                    .label("Remember order info")
                    .on_click(cx.listener(|this, &on, _, cx| {
                        if let Some(dialog) = this.payments.dialog.as_mut() {
                            dialog.allow_save_order = on;
                        }
                        cx.notify();
                    })),
            );
            body = body.child(
                Button::new("p1-payment-validate")
                    .label("Continue")
                    .primary()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.validate_payment_order(cx);
                    })),
            );
        }
        if let Some(validated) = &session.payment_validated
            && !validated.shipping_options.is_empty()
        {
            // Phase 6: kit RadioGroup (was: buttons with a ◉/○ prefix).
            // Controlled: the chosen index writes the value.
            let shipping_ids: Vec<String> = validated
                .shipping_options
                .iter()
                .map(|option| option.id.clone())
                .collect();
            let shipping_selected = validated.shipping_options.iter().position(|option| {
                session.payment_shipping_id.as_deref() == Some(option.id.as_str())
            });
            body = body.child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(div().text_xs().font_semibold().child("Shipping"))
                    .child(
                        RadioGroup::vertical("p1-payment-shipping")
                            .selected_index(shipping_selected)
                            .children(validated.shipping_options.iter().map(|option| {
                                Radio::new(format!("p1-payment-shipping-{}", option.id)).label(
                                    format!(
                                        "{} — {}",
                                        option.title,
                                        format_payment_price(
                                            &invoice.currency,
                                            price_parts_total(&option.price_parts)
                                        )
                                    ),
                                )
                            }))
                            .on_click(cx.listener(move |this, &ix: &usize, _, cx| {
                                if let Some(live) = this.live.as_mut() {
                                    live.driver.session.payment_shipping_id =
                                        Some(shipping_ids[ix].clone());
                                }
                                cx.notify();
                            })),
                    ),
            );
        }
        // Credentials: saved credentials or a fresh provider token.
        // Phase 6: one kit RadioGroup for the whole credential choice
        // (was: buttons with a ◉/○ prefix). Controlled: the chosen index
        // writes the value.
        let cred_ids: Vec<Option<String>> = saved_credentials
            .iter()
            .map(|cred| Some(cred.id.clone()))
            .chain(std::iter::once(None))
            .collect();
        let cred_selected = cred_ids.iter().position(|id| match id {
            Some(id) => dialog.credential_choice == PaymentCredentialChoice::Saved(id.clone()),
            None => dialog.credential_choice == PaymentCredentialChoice::NewToken,
        });
        let mut creds = div()
            .flex()
            .flex_col()
            .gap_1()
            .child(div().text_xs().font_semibold().child("Payment method"))
            .child(
                RadioGroup::vertical("p1-payment-cred")
                    .selected_index(cred_selected)
                    .children(
                        saved_credentials
                            .iter()
                            .map(|cred| {
                                Radio::new(format!("p1-payment-cred-{}", cred.id))
                                    .label(cred.title.clone())
                            })
                            .chain(std::iter::once(
                                Radio::new("p1-payment-cred-new")
                                    .label("New card (provider token)"),
                            )),
                    )
                    .on_click(cx.listener(move |this, &ix: &usize, _, cx| {
                        if let Some(dialog) = this.payments.dialog.as_mut() {
                            dialog.credential_choice = match &cred_ids[ix] {
                                Some(id) => PaymentCredentialChoice::Saved(id.clone()),
                                None => PaymentCredentialChoice::NewToken,
                            };
                        }
                        cx.notify();
                    })),
            );
        let use_token = dialog.credential_choice == PaymentCredentialChoice::NewToken;
        if use_token {
            creds = creds.child(
                div().child(
                    Textarea::new(&dialog.token_input)
                        .aria_label("Payment token")
                        .h(px(36.)),
                ),
            );
            if can_save_credentials {
                let checked = dialog.allow_save_credentials;
                let caption = if need_password {
                    " (needs a 2-step verification password)"
                } else {
                    ""
                };
                creds = creds.child(
                    // Phase 6: kit Checkbox (was: ghost button with ☑/☐ label).
                    Checkbox::new("p1-payment-save-creds")
                        .checked(checked)
                        .label(format!("Save card{caption}"))
                        .on_click(cx.listener(|this, &on, _, cx| {
                            if let Some(dialog) = this.payments.dialog.as_mut() {
                                dialog.allow_save_credentials = on;
                            }
                            cx.notify();
                        })),
                );
            }
        }
        body = body.child(creds);
        if !invoice.terms_url.is_empty() {
            let checked = dialog.terms_accepted;
            let url = invoice.terms_url.clone();
            body = body.child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        // Phase 6: kit Checkbox (was: ghost button with ☑/☐ label).
                        Checkbox::new("p1-payment-terms")
                            .checked(checked)
                            .accessibility_label("I accept the terms of service")
                            .on_click(cx.listener(|this, &on, _, cx| {
                                if let Some(dialog) = this.payments.dialog.as_mut() {
                                    dialog.terms_accepted = on;
                                }
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("p1-payment-terms-open")
                            .label("I accept the terms of service")
                            .ghost()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.open_message_url(&url, cx);
                            })),
                    ),
            );
        }
        if !invoice.recurring_terms_url.is_empty() {
            let url = invoice.recurring_terms_url.clone();
            body = body.child(
                Button::new("p1-payment-recurring-terms")
                    .label("Recurring payment terms")
                    .ghost()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.open_message_url(&url, cx);
                    })),
            );
        }
        let can_pay = {
            let terms_ok = invoice.terms_url.is_empty() || dialog.terms_accepted;
            let order_ok = !needs_order || session.payment_validated.is_some();
            let creds_ok = match &dialog.credential_choice {
                PaymentCredentialChoice::Saved(_) => true,
                PaymentCredentialChoice::NewToken => !dialog.token(cx).trim().is_empty(),
            };
            terms_ok && order_ok && creds_ok
        };
        let pay = Button::new("p1-payment-pay")
            .label(format!(
                "Pay {}",
                format_payment_price(&invoice.currency, total)
            ))
            .primary();
        body = body.child(if session.payment_sending {
            pay.label("Processing…").disabled(true)
        } else if can_pay {
            pay.on_click(cx.listener(|this, _, window, cx| {
                this.submit_payment(cx);
                this.close_kit_dialog_if_done(DialogKind::PaymentForm, window, cx);
            }))
        } else {
            pay.disabled(true)
        });
        body
    }

    /// Slice P1: "View receipt" on a paid invoice — `getPaymentReceipt`
    /// for the invoice's `receipt_message_id` (schema 1.8.67, line 15280).
    pub(super) fn open_payment_receipt(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        cx: &mut Context<Self>,
    ) {
        let sent = self.live.as_mut().map(|live| {
            live.driver.session.payment_receipt_open = false;
            live.driver.fetch_payment_receipt(chat_id, message_id)
        });
        if !matches!(sent, Some(Ok(_))) {
            self.connection.status_note = "could not load the receipt".into();
        }
        cx.notify();
    }

    /// Slice P1: "Continue" in the checkout dialog — `validateOrderInfo`
    /// (schema 1.8.67, line 15268). The answer carries the `order_info_id`
    /// and shipping options.
    pub(super) fn validate_payment_order(&mut self, cx: &mut Context<Self>) {
        let order = self.payments.dialog.as_ref().map(|dialog| dialog.order(cx));
        let request = self
            .session()
            .and_then(|session| session.payment_request.clone());
        let allow_save = self
            .payments
            .dialog
            .as_ref()
            .is_some_and(|dialog| dialog.allow_save_order);
        let (Some(order), Some(request)) = (order, request) else {
            self.connection.status_note = "payment form not loaded".into();
            cx.notify();
            return;
        };
        let sent = self.live.as_mut().map(|live| {
            live.driver.session.payment_note = None;
            live.driver.validate_payment_order_info(
                request.chat_id,
                request.message_id,
                &order,
                allow_save,
            )
        });
        if !matches!(sent, Some(Ok(_))) {
            self.connection.status_note = "could not validate order info".into();
        }
        cx.notify();
    }

    /// Slice P1: "Pay" in the checkout dialog — `sendPaymentForm`
    /// (schema 1.8.67, line 15277). The credential token is read from the
    /// input and never stored; only the submission uses it.
    pub(super) fn submit_payment(&mut self, cx: &mut Context<Self>) {
        let snapshot = self.session().map(|session| {
            (
                session.payment_form.clone(),
                session.payment_request.clone(),
                session.payment_validated.clone(),
                session.payment_shipping_id.clone(),
            )
        });
        let dialog = self.payments.dialog.as_ref();
        let (Some((Some(form), Some(request), validated, shipping_id)), Some(dialog)) =
            (snapshot, dialog)
        else {
            self.connection.status_note = "payment form not loaded".into();
            cx.notify();
            return;
        };
        let PaymentFormTypeData::Regular(regular) = &form.form_type else {
            return;
        };
        let invoice = &regular.invoice;
        let needs_order = invoice.need_name
            || invoice.need_phone_number
            || invoice.need_email_address
            || invoice.need_shipping_address;
        if needs_order && validated.is_none() {
            self.connection.status_note = "validate the order info first".into();
            cx.notify();
            return;
        }
        let credentials = match &dialog.credential_choice {
            PaymentCredentialChoice::Saved(id) => input_credentials_saved(id),
            PaymentCredentialChoice::NewToken => input_credentials_new(
                &dialog.token(cx),
                dialog.allow_save_credentials && regular.can_save_credentials,
            ),
        };
        let order_info_id = validated
            .as_ref()
            .map(|validated| validated.order_info_id.as_str())
            .unwrap_or("");
        let shipping_option_id = shipping_id.as_deref().unwrap_or("");
        let sent = self.live.as_mut().map(|live| {
            live.driver.session.payment_note = None;
            let sent = live.driver.submit_payment_form(
                request.chat_id,
                request.message_id,
                form.id,
                order_info_id,
                shipping_option_id,
                credentials,
                0,
            );
            if sent.is_ok() {
                live.driver.session.payment_sending = true;
            }
            sent
        });
        if !matches!(sent, Some(Ok(_))) {
            self.connection.status_note = "could not submit the payment".into();
        }
        cx.notify();
    }

    /// B1: act on a `loginUrlInfo*` / `httpUrl` answer drained by
    /// `poll_live`. `loginUrlInfoOpen` (and the `httpUrl` from `getLoginUrl`)
    /// opens the authorized URL in the OS browser;
    /// `loginUrlInfoRequestConfirmation` shows the TDLib-reported domain
    /// for consent, then fetches the authorized URL via `getLoginUrl`
    /// (schema 1.8.67, line 12993); a failed resolution opens the button's
    /// raw URL (schema:12993 doc).
    pub(super) fn present_login_url_info(
        &mut self,
        info: LoginUrlInfo,
        request: Option<LoginUrlRequest>,
        cx: &mut Context<Self>,
    ) {
        match info {
            LoginUrlInfo::Open { url } => self.open_message_url(&url, cx),
            LoginUrlInfo::RequestConfirmation {
                domain,
                request_write_access,
            } => match request {
                Some(request) => {
                    self.links.login_url_confirm = Some(LoginUrlConfirm {
                        domain,
                        request_write_access,
                        request,
                    });
                    cx.notify();
                }
                None => self.set_status_note("Login URL unavailable.", cx),
            },
            LoginUrlInfo::Failed { fallback_url } => {
                if fallback_url.is_empty() {
                    self.set_status_note("Login URL unavailable.", cx);
                } else {
                    self.open_message_url(&fallback_url, cx);
                }
            }
        }
    }

    /// B1: confirm a `loginUrlInfoRequestConfirmation` dialog — the user
    /// consented, so fetch the authorized URL via `getLoginUrl` (schema
    /// 1.8.67, line 12993; TGX `TGInlineKeyboard.getLoginCallback` does
    /// exactly this). Without a live connection the button's raw URL opens
    /// instead.
    pub(super) fn confirm_login_url(&mut self, cx: &mut Context<Self>) {
        let Some(confirm) = self.links.login_url_confirm.take() else {
            cx.notify();
            return;
        };
        let sent = self.live.as_mut().map(|live| {
            live.driver
                .send_login_url(&confirm.request, confirm.request_write_access)
        });
        if !matches!(sent, Some(Ok(_))) {
            self.open_message_url(&confirm.request.raw_url, cx);
        }
        cx.notify();
    }

    /// kit Phase 2 (redo): payment checkout hosted in a kit `Dialog` via
    /// `window.open_dialog`. Esc / backdrop / ✕ clear state via `on_close`.
    pub(super) fn build_payment_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::PaymentForm, |this, _, cx| {
                this.close_payment_dialog(cx);
            });
        app.update(cx, |this, cx| {
            let dialog = dialog.overlay(true).title(crate::ui::shell::dialog_title("Checkout"));
            let Some(session) = this.session() else {
                return dialog.on_close(on_close.clone());
            };
            if session.payment_form.is_none() && !session.payment_form_loading {
                return dialog.on_close(on_close.clone());
            }
            let Some(dialog_state) = this.payments.dialog.as_ref() else {
                return dialog.on_close(on_close.clone());
            };
            let mut body = div().flex().flex_col().gap_3();
            if session.payment_form_loading {
                body = body.child(div().text_sm().child("Loading payment form…"));
            }
            if let Some(note) = &session.payment_note {
                body = body.child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(note.clone()),
                );
            }
            let Some(form) = session.payment_form.as_ref() else {
                let body = body.into_any_element();
                return dialog
                    .content(crate::ui::shell::scrollable_dialog_content({
                        // `content` needs an `Fn` closure, but the body is built once
                        // per dialog render — hand it over through a one-shot cell.
                        let body = Rc::new(RefCell::new(Some(body.into_any_element())));
                        move |content, _, _| {
                            let body = body.borrow_mut().take().unwrap_or_else(|| div().into_any_element());
                            content.child(body)
                        }
                    }))
                    .on_close(on_close);
            };
            match &form.form_type {
                PaymentFormTypeData::Stars { star_count } => {
                    body = body.child(div().text_sm().child(format!(
                        "This invoice asks for {star_count} Telegram Stars — Stars checkout is not supported in this slice."
                    )));
                }
                PaymentFormTypeData::StarSubscription => {
                    body = body.child(
                        div()
                            .text_sm()
                            .child("Star subscriptions are not available yet."),
                    );
                }
                PaymentFormTypeData::Unknown => {
                    body =
                        body.child(div().text_sm().child(
                            "This invoice uses a payment form type this client doesn't support.",
                        ));
                }
                PaymentFormTypeData::Regular(regular) => {
                    body = this.payment_form_body(
                        body,
                        form,
                        dialog_state,
                        &regular.invoice,
                        &regular.provider,
                        &regular.additional_options,
                        &regular.saved_credentials,
                        regular.can_save_credentials,
                        regular.need_password,
                        session,
                        cx,
                    );
                }
            }
            let body = body.into_any_element();
            dialog
                .content(crate::ui::shell::scrollable_dialog_content({
                    // `content` needs an `Fn` closure, but the body is built once
                    // per dialog render — hand it over through a one-shot cell.
                    let body = Rc::new(RefCell::new(Some(body.into_any_element())));
                    move |content, _, _| {
                        let body = body.borrow_mut().take().unwrap_or_else(|| div().into_any_element());
                        content.child(body)
                    }
                }))
                .on_close(on_close)
        })
    }

    /// kit Phase 2 (redo): payment receipt hosted in a kit `Dialog` via
    /// `window.open_dialog`. Esc / backdrop / ✕ clear state via `on_close`.
    pub(super) fn build_payment_receipt_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::PaymentReceipt, |this, _, cx| {
                this.close_payment_receipt(cx);
            });
        app.update(cx, |this, cx| {
            let dialog = dialog
                .overlay(true)
                .title(crate::ui::shell::dialog_title("Payment receipt"));
            let session = this.session();
            let receipt = session.as_ref().and_then(|s| s.payment_receipt.as_ref());
            let Some(receipt) = receipt else {
                return dialog.on_close(on_close);
            };
            let total = if receipt.is_stars {
                format!("{} Stars", receipt.star_count)
            } else {
                format_payment_price(&receipt.currency, receipt.total_amount)
            };
            let mut body = div()
                .flex()
                .flex_col()
                .gap_2()
                .child(
                    div()
                        .text_sm()
                        .font_semibold()
                        .child(format!("🧾 {}", receipt.product_title)),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(format_unix_date_time(receipt.date as i64)),
                )
                .child(
                    div()
                        .flex()
                        .justify_between()
                        .text_sm()
                        .child(div().child("Total"))
                        .child(div().font_semibold().child(total)),
                );
            if !receipt.credentials_title.is_empty() {
                body = body.child(
                    div()
                        .flex()
                        .justify_between()
                        .text_sm()
                        .child(div().child("Paid with"))
                        .child(div().child(receipt.credentials_title.clone())),
                );
            }
            if receipt.tip_amount > 0 {
                body = body.child(
                    div()
                        .flex()
                        .justify_between()
                        .text_sm()
                        .child(div().child("Tip"))
                        .child(
                            div()
                                .child(format_payment_price(&receipt.currency, receipt.tip_amount)),
                        ),
                );
            }
            let body = body.into_any_element();
            dialog
                .content(crate::ui::shell::scrollable_dialog_content({
                    // `content` needs an `Fn` closure, but the body is built once
                    // per dialog render — hand it over through a one-shot cell.
                    let body = Rc::new(RefCell::new(Some(body.into_any_element())));
                    move |content, _, _| {
                        let body = body
                            .borrow_mut()
                            .take()
                            .unwrap_or_else(|| div().into_any_element());
                        content.child(body)
                    }
                }))
                .on_close(on_close)
        })
    }

    /// Slice `parity:bots-payment-recurring`: "⭐ Subscriptions" — the
    /// `starSubscriptions` list (`getStarSubscriptions`, schema 1.8.67,
    /// line 16075) hosted in a kit `Dialog` via `window.open_dialog`.
    /// Cancel / re-enable (`editStarSubscription`) and rejoin
    /// (`reuseStarSubscription`) act on the row; the list refetches from
    /// the authoritative `ok` — the rows never flip optimistically.
    pub(super) fn build_subscriptions_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::Subscriptions, |this, _, cx| {
                this.close_subscriptions(cx);
            });
        app.update(cx, |this, cx| {
            let dialog = dialog
                .overlay(true)
                .title(crate::ui::shell::dialog_title("⭐ Subscriptions"));
            let Some(session) = this.session() else {
                return dialog.on_close(on_close);
            };
            let mut body = div().flex().flex_col().gap_3();
            if let Some(subs) = session.star_subscriptions.as_ref() {
                body = body.child(
                    div()
                        .flex()
                        .justify_between()
                        .text_sm()
                        .child(div().child("Star balance"))
                        .child(
                            div()
                                .font_semibold()
                                .child(format!("⭐ {}", subs.star_amount)),
                        ),
                );
                if subs.required_star_count > 0 {
                    body = body.child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(format!(
                                "⭐ {} needed to extend expiring subscriptions",
                                subs.required_star_count
                            )),
                    );
                }
            }
            if session.star_subscriptions_loading && session.star_subscriptions.is_none() {
                body = body.child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child("Loading subscriptions…"),
                );
            }
            if let Some(err) = session.star_subscriptions_error.as_ref() {
                body = body.child(div().text_sm().child(err.clone()));
            }
            if let Some(subs) = session.star_subscriptions.as_ref() {
                if subs.subscriptions.is_empty() && !session.star_subscriptions_loading {
                    body = body.child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child("No active subscriptions."),
                    );
                }
                let mutating = session.star_subscriptions_mutating;
                let confirming = session.subscription_cancel_confirm.clone();
                let chat_titles: std::collections::HashMap<i64, String> = session
                    .chats
                    .iter()
                    .map(|(id, chat)| (*id, chat.title.clone()))
                    .collect();
                for sub in &subs.subscriptions {
                    body = body.child(subscription_row(
                        sub,
                        &chat_titles,
                        mutating,
                        confirming.as_deref(),
                        cx,
                    ));
                }
                if !subs.next_offset.is_empty() {
                    let label = if session.star_subscriptions_loading {
                        "Loading…"
                    } else {
                        "Load more"
                    };
                    body = body.child(
                        Button::new("subs-load-more")
                            .label(label)
                            .ghost()
                            .disabled(session.star_subscriptions_loading)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.load_more_subscriptions(cx);
                            })),
                    );
                }
            }
            let body = body.into_any_element();
            dialog
                .content(crate::ui::shell::scrollable_dialog_content({
                    let body = Rc::new(RefCell::new(Some(body.into_any_element())));
                    move |content, _, _| {
                        let body = body
                            .borrow_mut()
                            .take()
                            .unwrap_or_else(|| div().into_any_element());
                        content.child(body)
                    }
                }))
                .on_close(on_close)
        })
    }

    /// Slice `parity:bots-payment-recurring`: open the Subscriptions
    /// dialog and fetch the list (`getStarSubscriptions`).
    pub(super) fn open_subscriptions(&mut self, cx: &mut Context<Self>) {
        let sent = self.live.as_mut().map(|live| {
            live.driver.session.subscriptions_open = true;
            live.driver.maybe_fetch_star_subscriptions()
        });
        if !matches!(sent, Some(Ok(_))) {
            self.connection.status_note = "could not load subscriptions".into();
        }
        cx.notify();
    }

    /// Slice `parity:bots-payment-recurring`: close the Subscriptions dialog.
    pub(super) fn close_subscriptions(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            live.driver.session.subscriptions_open = false;
            live.driver.session.subscription_cancel_confirm = None;
        }
        cx.notify();
    }

    /// Slice `parity:bots-payment-recurring`: "Load more" — the next
    /// `getStarSubscriptions` page.
    pub(super) fn load_more_subscriptions(&mut self, cx: &mut Context<Self>) {
        let sent = self
            .live
            .as_mut()
            .map(|live| live.driver.fetch_more_star_subscriptions());
        if !matches!(sent, Some(Ok(_))) {
            self.connection.status_note = "could not load more subscriptions".into();
        }
        cx.notify();
    }

    /// Slice `parity:bots-payment-recurring`: "Cancel" on a row — asks
    /// for inline confirmation first (`editStarSubscription` is a real
    /// money action).
    pub(super) fn ask_cancel_subscription(&mut self, id: String, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            live.driver.session.subscription_cancel_confirm = Some(id);
        }
        cx.notify();
    }

    /// Slice `parity:bots-payment-recurring`: "Yes, cancel" —
    /// `editStarSubscription` with `is_canceled: true`.
    pub(super) fn confirm_cancel_subscription(&mut self, cx: &mut Context<Self>) {
        let id = self
            .session()
            .and_then(|s| s.subscription_cancel_confirm.clone());
        let Some(id) = id else {
            return;
        };
        let sent = self.live.as_mut().map(|live| {
            live.driver.session.subscription_cancel_confirm = None;
            live.driver.edit_star_subscription(&id, true)
        });
        if !matches!(sent, Some(Ok(_))) {
            self.connection.status_note = "could not cancel the subscription".into();
        }
        cx.notify();
    }

    /// Slice `parity:bots-payment-recurring`: "Keep" — dismiss the inline
    /// cancel confirmation without touching the subscription.
    pub(super) fn dismiss_cancel_confirm(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            live.driver.session.subscription_cancel_confirm = None;
        }
        cx.notify();
    }

    /// Slice `parity:bots-payment-recurring`: "Re-enable" —
    /// `editStarSubscription` with `is_canceled: false`.
    pub(super) fn reenable_subscription(&mut self, id: String, cx: &mut Context<Self>) {
        let sent = self
            .live
            .as_mut()
            .map(|live| live.driver.edit_star_subscription(&id, false));
        if !matches!(sent, Some(Ok(_))) {
            self.connection.status_note = "could not re-enable the subscription".into();
        }
        cx.notify();
    }

    /// Slice `parity:bots-payment-recurring`: "Rejoin" —
    /// `reuseStarSubscription` on an active channel subscription whose chat
    /// the user needs to rejoin (the type's `can_reuse` is true).
    pub(super) fn rejoin_subscription(&mut self, id: String, cx: &mut Context<Self>) {
        let sent = self
            .live
            .as_mut()
            .map(|live| live.driver.reuse_star_subscription(&id));
        if !matches!(sent, Some(Ok(_))) {
            self.connection.status_note = "could not rejoin the subscription".into();
        }
        cx.notify();
    }
}

/// Slice `parity:bots-payment-recurring`: one `starSubscription` row —
/// title, status, price, and the Cancel / Re-enable / Rejoin / Renew
/// actions.
/// `starSubscription` carries no title (schema 1.8.67, line 1262), so the
/// chat cache supplies it; the type names are the honest fallback.
fn subscription_row(
    sub: &StarSubscriptionData,
    chat_titles: &std::collections::HashMap<i64, String>,
    mutating: bool,
    confirming: Option<&str>,
    cx: &mut Context<QuillApp>,
) -> impl IntoElement {
    let title = match &sub.sub_type {
        // `starSubscriptionTypeBot` carries its own title (schema:1246);
        // channel subscriptions resolve through the chat cache.
        StarSubscriptionTypeData::Bot { title, .. } if !title.is_empty() => title.clone(),
        _ => chat_titles
            .get(&sub.chat_id)
            .cloned()
            .unwrap_or_else(|| match sub.sub_type {
                StarSubscriptionTypeData::Channel { .. } => "Channel subscription".to_string(),
                StarSubscriptionTypeData::Bot { .. } => "Bot subscription".to_string(),
                StarSubscriptionTypeData::Unknown => "Subscription".to_string(),
            }),
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let expired = (sub.expiration_date as i64) <= now;
    let date = format_unix_date_time(sub.expiration_date as i64);
    let status = if expired {
        format!("Expired {}", date)
    } else if sub.is_canceled {
        format!("Canceled — active until {}", date)
    } else if sub.is_expiring {
        format!("Expiring {}", date)
    } else {
        format!("Renews {}", date)
    };
    let mut row = div()
        .flex()
        .justify_between()
        .items_center()
        .gap_3()
        .py_2()
        .child(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(div().text_sm().font_semibold().child(title))
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(format!(
                            "{} · {}",
                            status,
                            subscription_price_label(&sub.pricing)
                        )),
                ),
        );
    if confirming == Some(sub.id.as_str()) {
        let id = sub.id.clone();
        let keep_id = sub.id.clone();
        row = row.child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(div().text_xs().child("Cancel this subscription?"))
                .child(
                    Button::new(format!("subs-confirm-cancel-{id}"))
                        .label("Yes, cancel")
                        .danger()
                        .disabled(mutating)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.confirm_cancel_subscription(cx);
                        })),
                )
                .child(
                    Button::new(format!("subs-keep-{keep_id}"))
                        .label("Keep")
                        .ghost()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.dismiss_cancel_confirm(cx);
                        })),
                ),
        );
    } else if expired {
        // Schema 1.8.67: `reuseStarSubscription` reuses an ACTIVE
        // subscription, so an expired channel sub renews through the
        // type's `invite_link` instead, opened in the OS browser.
        if let StarSubscriptionTypeData::Channel { invite_link, .. } = &sub.sub_type
            && !invite_link.is_empty()
        {
            let link = invite_link.clone();
            let id = sub.id.clone();
            row = row.child(
                Button::new(format!("subs-renew-{id}"))
                    .label("Renew")
                    .disabled(mutating)
                    .on_click(cx.listener(move |_, _, _, cx| {
                        cx.open_url(&link);
                    })),
            );
        }
    } else if sub.is_canceled {
        let id = sub.id.clone();
        row = row.child(
            Button::new(format!("subs-reenable-{id}"))
                .label("Re-enable")
                .disabled(mutating)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.reenable_subscription(id.clone(), cx);
                })),
        );
    } else if matches!(
        sub.sub_type,
        StarSubscriptionTypeData::Channel {
            can_reuse: true,
            ..
        }
    ) {
        let id = sub.id.clone();
        row = row.child(
            Button::new(format!("subs-rejoin-{id}"))
                .label("Rejoin")
                .disabled(mutating)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.rejoin_subscription(id.clone(), cx);
                })),
        );
    } else {
        let id = sub.id.clone();
        row = row.child(
            Button::new(format!("subs-cancel-{id}"))
                .label("Cancel")
                .ghost()
                .disabled(mutating)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.ask_cancel_subscription(id.clone(), cx);
                })),
        );
    }
    row
}

/// Slice `parity:bots-payment-recurring`: "⭐ 100 / month" from a
/// `starSubscriptionPricing` (period is seconds; a month is 2_592_000).
fn subscription_price_label(pricing: &StarSubscriptionPricing) -> String {
    let period = match pricing.period {
        2_592_000 => "month".to_string(),
        604_800 => "week".to_string(),
        p if p > 0 => format!("{}d", p / 86_400),
        _ => "period".to_string(),
    };
    format!("⭐ {} / {}", pricing.star_count, period)
}

crate::ui::shell::register_dialogs! {
    PaymentForm => DialogSpec::new(
        3500,
        |app| {
            app.payments.dialog.is_some()
                && app.session().is_some_and(|s| s.payment_form.is_some() || s.payment_form_loading)
        },
        QuillApp::build_payment_dialog,
    ),

    PaymentReceipt => DialogSpec::new(
        3600,
        |app| app.session().is_some_and(|s| s.payment_receipt_open),
        QuillApp::build_payment_receipt_dialog,
    ),

    // Slice `parity:bots-payment-recurring`: the `starSubscriptions`
    // management dialog.
    Subscriptions => DialogSpec::new(
        3700,
        |app| app.session().is_some_and(|s| s.subscriptions_open),
        QuillApp::build_subscriptions_dialog,
    ),
}
