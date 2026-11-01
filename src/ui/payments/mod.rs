//! buy buttons, payment dialogs, login-URL confirms.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::country_box::CountryHost;
use super::message_text::format_unix_date_time;
use super::shell::{DialogKind, QuillShell};
use super::*;
use crate::ui::status_note::StatusNote;
use gpui_kit::component::button::*;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::input::{Textarea, TextareaState};
use gpui_kit::component::radio::{Radio, RadioGroup};
use gpui_kit::component::*;
use gpui_kit::*;
use quill::ids::{ChatId, MessageId};
use quill::lang::{keys, tr, tr_with};
use quill::state::{LoginUrlRequest, Session};
use quill::telegram::envelope::{
    LoginUrlInfo, PaymentFormData, PaymentFormTypeData, StarSubscriptionData,
    StarSubscriptionPricing, StarSubscriptionTypeData, format_payment_price, price_parts_total,
};
use quill::telegram::requests::{input_credentials_new, input_credentials_saved};
use std::cell::RefCell;
use std::rc::Rc;

impl QuillApp {
    /// Slice P1: create the checkout dialog inputs, prefilling the order
    /// fields from the form's `saved_order_info` (schema:4720) when present.
    pub(super) fn open_payment_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut dialog = PaymentDialog::new(window, cx);
        if let Some(form) = self.session().and_then(|s| s.payments.form.clone()) {
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
            live.driver.session.payments.form = None;
            live.driver.session.payments.form_loading = false;
            live.driver.session.payments.note = None;
            live.driver.session.payments.validated = None;
            live.driver.session.payments.shipping_id = None;
            live.driver.session.payments.request = None;
            live.driver.session.payments.form_paid = false;
        }
        cx.notify();
    }

    /// The provider's 3-D Secure page opens in the OS browser and the
    /// checkout closes, as tdesktop does when it cannot show a web view.
    pub(super) fn open_payment_verification(&mut self, url: &str, cx: &mut Context<Self>) {
        self.open_message_url(url, cx);
        self.close_payment_dialog(cx);
    }

    /// Slice P1: close the receipt dialog.
    pub(super) fn close_payment_receipt(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            live.driver.session.payments.receipt = None;
            live.driver.session.payments.receipt_open = false;
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
        additional_options: &[quill::telegram::envelope::PaymentOption],
        saved_credentials: &[quill::telegram::envelope::SavedCredential],
        session: &Session,
        cx: &mut Context<Self>,
    ) -> Div {
        if dialog.country_box.is_some()
            && let Some(view) =
                self.country_box_view(CountryHost::Payment, tr(keys::PAYMENTS_ADDRESS_COUNTRY), cx)
        {
            return body.child(view);
        }
        if let Some(card_form) = &dialog.card_form {
            return body.child(self.card_form_view(card_form, cx));
        }
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
                .child(div().child(tr(keys::PAYMENTS_TOTAL_LABEL)))
                .child(div().child(format_payment_price(&invoice.currency, total))),
        );
        body = body.child(prices);
        for (index, option) in additional_options.iter().enumerate() {
            let url = option.url.clone();
            let title = option.title.clone();
            body = body.child(
                Button::new(format!("p1-payment-option-{index}"))
                    .label(tr_with(
                        keys::Q_MONEY_PAY_VIA,
                        &[("title", &title.to_string())],
                    ))
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
            body = body.child(field(&dialog.name_input, tr(keys::Q_MONEY_FULL_NAME)));
        }
        if invoice.need_phone_number {
            body = body.child(field(&dialog.phone_input, tr(keys::Q_MONEY_PHONE_NUMBER)));
        }
        if invoice.need_email_address {
            body = body.child(field(&dialog.email_input, tr(keys::Q_MONEY_EMAIL_ADDRESS)));
        }
        if invoice.need_shipping_address {
            body = body.child(field(
                &dialog.street1_input,
                tr(keys::Q_MONEY_STREET_ADDRESS),
            ));
            body = body.child(field(
                &dialog.street2_input,
                tr(keys::Q_MONEY_APARTMENT_OR_UNIT),
            ));
            body = body.child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        div()
                            .flex_1()
                            .child(field(&dialog.city_input, tr(keys::PAYMENTS_ADDRESS_CITY))),
                    )
                    .child(div().flex_1().child(field(
                        &dialog.state_input,
                        tr(keys::Q_MONEY_STATE_OR_REGION),
                    ))),
            );
            body = body.child(
                div()
                    .flex()
                    .gap_2()
                    .child(div().flex_1().child(self.country_field_button(
                        CountryHost::Payment,
                        tr(keys::PAYMENTS_ADDRESS_COUNTRY),
                        self.country_display_name(&dialog.country_iso),
                        cx,
                    )))
                    .child(
                        div()
                            .flex_1()
                            .child(field(&dialog.postal_input, tr(keys::Q_MONEY_POSTAL_CODE))),
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
                    .label(tr(keys::Q_MONEY_REMEMBER_ORDER_INFO))
                    .on_click(cx.listener(|this, &on, _, cx| {
                        if let Some(dialog) = this.payments.dialog.as_mut() {
                            dialog.allow_save_order = on;
                        }
                        cx.notify();
                    })),
            );
            body = body.child(
                Button::new("p1-payment-validate")
                    .label(tr(keys::CONTINUE))
                    .primary()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.validate_payment_order(cx);
                    })),
            );
        }
        if let Some(validated) = &session.payments.validated
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
                session.payments.shipping_id.as_deref() == Some(option.id.as_str())
            });
            body = body.child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .text_xs()
                            .font_semibold()
                            .child(tr(keys::Q_MONEY_SHIPPING)),
                    )
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
                                if let Some(live) = this.live.as_mut()
                                    && let Some(id) = shipping_ids.get(ix)
                                {
                                    live.driver.session.payments.shipping_id = Some(id.clone());
                                }
                                cx.notify();
                            })),
                    ),
            );
        }
        body = body.child(self.payment_method_section(dialog, saved_credentials, cx));
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
                            .accessibility_label(tr(keys::Q_MONEY_I_ACCEPT_THE_TERMS_OF))
                            .on_click(cx.listener(|this, &on, _, cx| {
                                if let Some(dialog) = this.payments.dialog.as_mut() {
                                    dialog.terms_accepted = on;
                                }
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("p1-payment-terms-open")
                            .label(tr(keys::Q_MONEY_I_ACCEPT_THE_TERMS_OF))
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
                    .label(tr(keys::Q_MONEY_RECURRING_PAYMENT_TERMS))
                    .ghost()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.open_message_url(&url, cx);
                    })),
            );
        }
        let can_pay = {
            let terms_ok = invoice.terms_url.is_empty() || dialog.terms_accepted;
            let order_ok = !needs_order || session.payments.validated.is_some();
            let creds_ok = match &dialog.credential_choice {
                PaymentCredentialChoice::Saved(_) => true,
                PaymentCredentialChoice::NewCard => {
                    dialog.new_card.is_some() || self.can_enter_new_card()
                }
            };
            // The password prompt is tdesktop's modal box: answer it first.
            terms_ok && order_ok && creds_ok && !dialog.password_prompt
        };
        let pay = Button::new("p1-payment-pay")
            .label(tr_with(
                keys::PAYMENTS_PAY_AMOUNT,
                &[(
                    "amount",
                    &(format_payment_price(&invoice.currency, total)).to_string(),
                )],
            ))
            .primary();
        body = body.child(if session.payments.sending {
            pay.label(tr(keys::Q_MONEY_PROCESSING)).disabled(true)
        } else if session.payments.form_paid {
            pay.label(tr(keys::PAYMENTS_DATE_LABEL)).disabled(true)
        } else if can_pay {
            pay.on_click(cx.listener(|this, _, window, cx| {
                if this
                    .payments
                    .dialog
                    .as_ref()
                    .is_some_and(|dialog| this.needs_card_entry(dialog))
                {
                    this.enter_new_card(window, cx);
                    return;
                }
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
            live.driver.session.payments.receipt_open = false;
            live.driver.fetch_payment_receipt(chat_id, message_id)
        });
        if !matches!(sent, Some(Ok(_))) {
            self.connection.set_note(StatusNote::error(tr(
                keys::Q_MONEY_COULD_NOT_LOAD_THE_RECEIPT,
            )));
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
            .and_then(|session| session.payments.request.clone());
        let allow_save = self
            .payments
            .dialog
            .as_ref()
            .is_some_and(|dialog| dialog.allow_save_order);
        let (Some(order), Some(request)) = (order, request) else {
            self.connection.set_note(StatusNote::silent(tr(
                keys::Q_MONEY_PAYMENT_FORM_NOT_LOADED,
            )));
            cx.notify();
            return;
        };
        let sent = self.live.as_mut().map(|live| {
            live.driver.session.payments.note = None;
            live.driver
                .validate_payment_order_info(&request.invoice, &order, allow_save)
        });
        if !matches!(sent, Some(Ok(_))) {
            self.connection.set_note(StatusNote::error(tr(
                keys::Q_MONEY_COULD_NOT_VALIDATE_ORDER_INFO,
            )));
        }
        cx.notify();
    }

    /// Slice P1: "Pay" in the checkout dialog — `sendPaymentForm`
    /// (schema 1.8.67, line 15277). A new card is sent as the provider's
    /// token, saved only when the form allows saving, counting a password
    /// set since the form loaded (tdesktop's `send`).
    pub(super) fn submit_payment(&mut self, cx: &mut Context<Self>) {
        let snapshot = self.session().map(|session| {
            (
                session.payments.form.clone(),
                session.payments.request.clone(),
                session.payments.validated.clone(),
                session.payments.shipping_id.clone(),
                session.payments.form_paid,
            )
        });
        let dialog = self.payments.dialog.as_ref();
        let (Some((Some(form), Some(request), validated, shipping_id, false)), Some(dialog)) =
            (snapshot, dialog)
        else {
            self.connection.set_note(StatusNote::silent(tr(
                keys::Q_MONEY_PAYMENT_FORM_NOT_LOADED,
            )));
            cx.notify();
            return;
        };
        let PaymentFormTypeData::Regular(regular) = &form.form_type else {
            return;
        };
        if dialog.password_prompt {
            return;
        }
        let invoice = &regular.invoice;
        let needs_order = invoice.need_name
            || invoice.need_phone_number
            || invoice.need_email_address
            || invoice.need_shipping_address;
        if needs_order && validated.is_none() {
            self.connection.set_note(StatusNote::silent(tr(
                keys::Q_MONEY_VALIDATE_THE_ORDER_INFO_FIRST,
            )));
            cx.notify();
            return;
        }
        let credentials = match (&dialog.credential_choice, &dialog.new_card) {
            (PaymentCredentialChoice::Saved(id), _) => input_credentials_saved(id),
            (PaymentCredentialChoice::NewCard, Some(card)) => input_credentials_new(
                card.credentials.data(),
                card.save && self.checkout_can_save_credentials(),
            ),
            (PaymentCredentialChoice::NewCard, None) => return,
        };
        let order_info_id = validated
            .as_ref()
            .map(|validated| validated.order_info_id.as_str())
            .unwrap_or("");
        let shipping_option_id = shipping_id.as_deref().unwrap_or("");
        let sent = self.live.as_mut().map(|live| {
            live.driver.session.payments.note = None;
            let sent = live.driver.submit_payment_form(
                &request.invoice,
                form.id,
                order_info_id,
                shipping_option_id,
                credentials,
                0,
            );
            if sent.is_ok() {
                live.driver.session.payments.sending = true;
            }
            sent
        });
        if !matches!(sent, Some(Ok(_))) {
            self.connection.set_note(StatusNote::error(tr(
                keys::Q_MONEY_COULD_NOT_SUBMIT_THE_PAYMENT,
            )));
        }
        cx.notify();
    }

    /// B1: act on a `loginUrlInfo*` / `httpUrl` answer drained by
    /// `poll_live`. `loginUrlInfoOpen` (and the `httpUrl` from `getLoginUrl`)
    /// opens the authorized URL in the OS browser;
    /// `loginUrlInfoRequestConfirmation` shows the TDLib-reported domain
    /// for consent, then fetches the authorized URL via `getLoginUrl`
    /// (schema 1.8.67, line 12993); a failed resolution opens the button's
    /// raw URL (schema:12993 doc). An answer to a press an agent's step
    /// made is refused as part of that step: no site, no consent box.
    pub(super) fn present_login_url_info(
        &mut self,
        info: LoginUrlInfo,
        request: Option<LoginUrlRequest>,
        cx: &mut Context<Self>,
    ) {
        use quill::agent::ui_driver::guard;
        let from_agent = request.as_ref().is_some_and(|request| request.from_agent);
        if guard::with_injecting(from_agent, || {
            guard::refuse_if_injecting("sign in to sites with a bot's login button")
        }) {
            return;
        }
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
                None => self.set_status_note(
                    StatusNote::info(tr(keys::Q_MONEY_LOGIN_URL_UNAVAILABLE)),
                    cx,
                ),
            },
            LoginUrlInfo::Failed { fallback_url } => {
                if fallback_url.is_empty() {
                    self.set_status_note(
                        StatusNote::info(tr(keys::Q_MONEY_LOGIN_URL_UNAVAILABLE)),
                        cx,
                    );
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
                StarSubscriptionTypeData::Channel { .. } => {
                    tr(keys::Q_MONEY_CHANNEL_SUBSCRIPTION).to_string()
                }
                StarSubscriptionTypeData::Bot { .. } => {
                    tr(keys::Q_MONEY_BOT_SUBSCRIPTION).to_string()
                }
                StarSubscriptionTypeData::Unknown => tr(keys::Q_MONEY_SUBSCRIPTION).to_string(),
            }),
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let expired = (sub.expiration_date as i64) <= now;
    let date = format_unix_date_time(sub.expiration_date as i64);
    let status = if expired {
        tr_with(keys::Q_MONEY_EXPIRED, &[("date", &date.to_string())])
    } else if sub.is_canceled {
        tr_with(
            keys::Q_MONEY_CANCELED_ACTIVE_UNTIL,
            &[("date", &date.to_string())],
        )
    } else if sub.is_expiring {
        tr_with(keys::Q_MONEY_EXPIRING, &[("date", &date.to_string())])
    } else {
        tr_with(keys::Q_MONEY_RENEWS, &[("date", &date.to_string())])
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
                .child(
                    div()
                        .text_xs()
                        .child(tr(keys::Q_MONEY_CANCEL_THIS_SUBSCRIPTION)),
                )
                .child(
                    Button::new(format!("subs-confirm-cancel-{id}"))
                        .label(tr(keys::Q_MONEY_YES_CANCEL))
                        .danger()
                        .disabled(mutating)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.confirm_cancel_subscription(cx);
                        })),
                )
                .child(
                    Button::new(format!("subs-keep-{keep_id}"))
                        .label(tr(keys::Q_MONEY_KEEP))
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
                    .label(tr(keys::Q_MONEY_RENEW))
                    .disabled(mutating)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.open_link(&link, cx);
                    })),
            );
        }
    } else if sub.is_canceled {
        let id = sub.id.clone();
        row = row.child(
            Button::new(format!("subs-reenable-{id}"))
                .label(tr(keys::Q_MONEY_RE_ENABLE))
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
                .label(tr(keys::Q_MONEY_REJOIN))
                .disabled(mutating)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.rejoin_subscription(id.clone(), cx);
                })),
        );
    } else {
        let id = sub.id.clone();
        row = row.child(
            Button::new(format!("subs-cancel-{id}"))
                .label(tr(keys::CANCEL))
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
        |app| app.payment_dialog_visible(),
        QuillApp::build_payment_dialog,
    )
    .closes(|app, _, cx| app.close_payment_dialog(cx)),

    PaymentReceipt => DialogSpec::new(
        3600,
        |app| app.session().is_some_and(|s| s.payments.receipt_open),
        QuillApp::build_payment_receipt_dialog,
    )

    .closes(|app, _, cx| app.close_payment_receipt(cx)),

    // Slice `parity:bots-payment-recurring`: the `starSubscriptions`
    // management dialog.
    Subscriptions => DialogSpec::new(
        3700,
        |app| app.session().is_some_and(|s| s.payments.subscriptions_open),
        QuillApp::build_subscriptions_dialog,
    )
    .closes(|app, _, cx| app.close_subscriptions(cx)),
}

mod build_payment_dialog;
pub(crate) mod card_form;
mod card_form_demo;
#[cfg(test)]
#[cfg(feature = "demo-capture")]
mod card_form_tests;
mod card_form_view;
