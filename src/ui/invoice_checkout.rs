//! Opens the payment checkout for any invoice: an invoice message, an
//! invoice link, a Stars pack, a Premium gift or a new giveaway.
//!
//! Telegram Desktop starts one `CheckoutProcess` per invoice id
//! (`payments/payments_checkout_process.cpp`). Here the same checkout
//! dialog (`ui/payments`) serves all of them: this module only builds the
//! invoice and fetches its form. Nothing is paid until the user presses Pay
//! in the checkout.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use crate::ui::status_note::StatusNote;
use gpui_base::{Disableable, StyledExt};
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::gift_manage::PremiumGiftOption;
use quill::ids::{ChatId, MessageId};
use quill::lang::{keys, tr, tr_with};
use quill::payment_invoice::{
    InputInvoice, giveaway_invoice, invoice_link_invoice, premium_gift_invoice,
    premium_gift_stars_invoice, stars_pack_invoice,
};
use quill::premium_hub::group_digits;
use quill::state::Session;
use quill::telegram::envelope::{PaymentFormData, PaymentFormTypeData};

impl QuillApp {
    /// The checkout dialog is on screen: it was opened, and its form is
    /// loading, loaded, or failed with a note to show.
    pub(super) fn payment_dialog_visible(&self) -> bool {
        self.payments.dialog.is_some()
            && self.session().is_some_and(|s| {
                s.payments.form.is_some() || s.payments.form_loading || s.payments.note.is_some()
            })
    }

    /// Fetch the `paymentForm` of `invoice` (`getPaymentForm`) and open
    /// the checkout dialog, which shows a spinner until the form lands.
    pub(super) fn start_invoice_checkout(
        &mut self,
        invoice: InputInvoice,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if quill::agent::ui_driver::guard::refuse_hard_if_injecting(
            quill::agent::ui_driver::guard::HardSurface::Payments,
        ) {
            return;
        }
        let sent = self.live.as_mut().map(|live| {
            let payments = &mut live.driver.session.payments;
            payments.form = None;
            payments.validated = None;
            payments.shipping_id = None;
            payments.form_paid = false;
            payments.form_loading = true;
            payments.note = None;
            live.driver.send_payment_form_request(invoice)
        });
        match sent {
            Some(Ok(_)) => self.open_payment_dialog(window, cx),
            _ => {
                if let Some(live) = self.live.as_mut() {
                    live.driver.session.payments.form_loading = false;
                }
                self.connection.set_note(StatusNote::error(tr(
                    keys::Q_MONEY_COULD_NOT_LOAD_THE_PAYMENT,
                )));
            }
        }
        cx.notify();
    }

    /// A Buy button press on an invoice message.
    pub(super) fn press_buy_button(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let invoice = InputInvoice::Message {
            chat_id,
            message_id,
        };
        self.start_invoice_checkout(invoice, window, cx);
    }

    /// An invoice link (`t.me/$slug`, `t.me/invoice/slug`).
    pub(super) fn open_invoice_link(
        &mut self,
        name: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match invoice_link_invoice(name) {
            Some(invoice) => self.start_invoice_checkout(invoice, window, cx),
            None => self.set_status_note(
                StatusNote::info(tr(keys::Q_MONEY_THIS_INVOICE_LINK_IS_BROKEN)),
                cx,
            ),
        }
    }

    /// A pack in the Stars dialog's "Buy more Stars" list.
    pub(super) fn buy_stars_pack(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let pack = self
            .session()
            .and_then(|s| s.payments.star_packs.options.get(index).cloned());
        match pack {
            Some(pack) => self.start_invoice_checkout(stars_pack_invoice(&pack), window, cx),
            None => self.set_status_note(
                StatusNote::silent(tr(keys::Q_MONEY_THAT_STARS_OPTION_IS_NO)),
                cx,
            ),
        }
    }

    /// A Premium option in the send-gift dialog. The message is the gift
    /// dialog's message field.
    pub(super) fn buy_premium_gift(
        &mut self,
        option: PremiumGiftOption,
        by_stars: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (recipient, limit) = match self.session() {
            Some(session) => (
                session.payments.gifts_manage.send.recipient.clone(),
                session.payments.gift_text_length_max.unwrap_or(0),
            ),
            None => (None, 0),
        };
        let text = self
            .payments
            .gift_message_input
            .read(cx)
            .value()
            .to_string();
        let invoice = recipient.and_then(|recipient| {
            if by_stars {
                premium_gift_stars_invoice(&option, &recipient, &text, limit)
            } else {
                premium_gift_invoice(&option, &recipient, &text, limit)
            }
        });
        match invoice {
            Some(invoice) => self.start_invoice_checkout(invoice, window, cx),
            None => self.set_status_note(
                StatusNote::info(tr(keys::Q_MONEY_PREMIUM_CAN_ONLY_BE_GIFTED)),
                cx,
            ),
        }
    }

    /// "Start Giveaway" on a new giveaway: check the form, then open the
    /// checkout for the chosen option. The giveaway itself is created when
    /// the payment goes through.
    pub(super) fn pay_new_giveaway(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(draft) = self.giveaway_draft_now(cx) else {
            return;
        };
        let options = self
            .session()
            .map(|s| s.payments.giveaway.options.clone())
            .unwrap_or_default();
        let note = match draft.validate(quill::local_time::now_unix() as i32) {
            Err(error) => Some(error.text_for(&draft.limits)),
            Ok(()) => match giveaway_invoice(&draft, &options) {
                Some(invoice) => {
                    self.start_invoice_checkout(invoice, window, cx);
                    None
                }
                None => Some(tr(keys::Q_MONEY_PICK_ONE_OF_THE_OFFERED).to_string()),
            },
        };
        if let Some(form) = self.giveaway.form.as_mut() {
            form.note = note;
        }
        cx.notify();
    }

    /// "Pay" on a form paid in Stars (`paymentFormTypeStars`): the Stars
    /// come from the balance, so there are no credentials.
    pub(super) fn submit_stars_payment(&mut self, cx: &mut Context<Self>) {
        let snapshot = self.session().map(|session| {
            (
                session.payments.form.clone(),
                session.payments.request.clone(),
                session.payments.form_paid,
            )
        });
        let Some((Some(form), Some(request), false)) = snapshot else {
            return;
        };
        if !matches!(form.form_type, PaymentFormTypeData::Stars { .. }) {
            return;
        }
        let sent = self.live.as_mut().map(|live| {
            live.driver.session.payments.note = None;
            let sent = live.driver.submit_payment_form(
                &request.invoice,
                form.id,
                "",
                "",
                serde_json::Value::Null,
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

    /// The body of a form paid in Stars (`paymentFormTypeStars`): the
    /// product, the price, the balance and Pay. Pay is the only thing that
    /// sends `sendPaymentForm`.
    pub(super) fn stars_form_body(
        &self,
        mut body: Div,
        form: &PaymentFormData,
        star_count: i64,
        session: &Session,
        cx: &mut Context<Self>,
    ) -> Div {
        let muted = cx.theme().muted_foreground;
        body = body.child(
            div()
                .text_sm()
                .font_semibold()
                .child(form.product_title.clone()),
        );
        if !form.product_description.is_empty() {
            body = body.child(
                div()
                    .text_xs()
                    .text_color(muted)
                    .child(form.product_description.clone()),
            );
        }
        let balance = session.payments.hub.balance.map(|b| b.stars);
        if let Some(balance) = balance {
            body = body.child(div().text_xs().text_color(muted).child(tr_with(
                keys::Q_MONEY_YOUR_BALANCE_3,
                &[("group_digits", &(group_digits(balance)).to_string())],
            )));
        }
        let too_poor = balance.is_some_and(|b| b < star_count);
        if too_poor {
            body = body.child(
                div()
                    .text_sm()
                    .text_color(cx.theme().danger)
                    .child(tr(keys::Q_MONEY_YOUR_BALANCE_IS_TOO_LOW_2)),
            );
        }
        let pay = Button::new("stars-payment-pay")
            .label(tr_with(
                keys::Q_MONEY_PAY,
                &[("group_digits", &(group_digits(star_count)).to_string())],
            ))
            .primary();
        body.child(if session.payments.sending {
            pay.label(tr(keys::Q_MONEY_PROCESSING)).disabled(true)
        } else if session.payments.form_paid {
            pay.label(tr(keys::SR_MESSAGE_INVOICE_PAID)).disabled(true)
        } else if too_poor {
            pay.disabled(true)
        } else {
            pay.on_click(cx.listener(|this, _, _, cx| this.submit_stars_payment(cx)))
        })
    }
}
