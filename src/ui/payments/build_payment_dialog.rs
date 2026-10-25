//! Methods moved out of `payments.rs` to keep files under 1000 lines.

use super::*;
use crate::ui::scroll_load::{load_due, loading_row, scrolled_near_end, watch};
use crate::ui::status_note::StatusNote;
use quill::lang::{keys, tr, tr_with};

impl QuillApp {
    /// kit Phase 2 (redo): payment checkout hosted in a kit `Dialog` via
    /// `window.open_dialog`. Esc / backdrop / ✕ clear state via `on_close`.
    pub(in crate::ui) fn build_payment_dialog(
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
            let dialog = dialog
                .overlay(true)
                .title(crate::ui::shell::dialog_title(this.checkout_title()));
            let Some(session) = this.session() else {
                return dialog.on_close(on_close.clone());
            };
            if session.payments.form.is_none()
                && !session.payments.form_loading
                && session.payments.note.is_none()
            {
                return dialog.on_close(on_close.clone());
            }
            let Some(dialog_state) = this.payments.dialog.as_ref() else {
                return dialog.on_close(on_close.clone());
            };
            let mut body = div().flex().flex_col().gap_3();
            if session.payments.form_loading {
                body = body.child(
                    div()
                        .text_sm()
                        .child(tr(keys::Q_MONEY_LOADING_PAYMENT_FORM)),
                );
            }
            if let Some(note) = &session.payments.note {
                body = body.child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(note.clone()),
                );
            }
            let Some(form) = session.payments.form.as_ref() else {
                let body = body.into_any_element();
                return dialog
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
                    .on_close(on_close);
            };
            match &form.form_type {
                PaymentFormTypeData::Stars { star_count } => {
                    body = this.stars_form_body(body, form, *star_count, session, cx);
                }
                PaymentFormTypeData::StarSubscription => {
                    body = body.child(
                        div()
                            .text_sm()
                            .child(tr(keys::Q_MONEY_STAR_SUBSCRIPTIONS_ARE_NOT_AVAILABLE)),
                    );
                }
                PaymentFormTypeData::Unknown => {
                    body = body.child(
                        div()
                            .text_sm()
                            .child(tr(keys::Q_MONEY_THIS_INVOICE_USES_A_PAYMENT)),
                    );
                }
                PaymentFormTypeData::Regular(regular) => {
                    body = this.payment_form_body(
                        body,
                        form,
                        dialog_state,
                        &regular.invoice,
                        &regular.additional_options,
                        &regular.saved_credentials,
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

    /// The checkout's title, "New Card" while the card form is open, with
    /// " (Test)" on a test invoice (tdesktop's `Panel::setTitle`).
    fn checkout_title(&self) -> String {
        let title = if self.card_form().is_some() {
            tr(keys::PAYMENTS_CARD_TITLE)
        } else {
            tr(keys::PAYMENTS_CHECKOUT_TITLE)
        };
        let is_test = self
            .session()
            .and_then(|session| session.payments.form.as_ref())
            .is_some_and(|form| match &form.form_type {
                PaymentFormTypeData::Regular(regular) => regular.invoice.is_test,
                _ => false,
            });
        if is_test {
            tr_with(keys::Q_PAYMENT_TEST_TITLE, &[("title", title)])
        } else {
            title.to_string()
        }
    }

    /// kit Phase 2 (redo): payment receipt hosted in a kit `Dialog` via
    /// `window.open_dialog`. Esc / backdrop / ✕ clear state via `on_close`.
    pub(in crate::ui) fn build_payment_receipt_dialog(
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
                .title(crate::ui::shell::dialog_title(tr(
                    keys::Q_MONEY_PAYMENT_RECEIPT,
                )));
            let session = this.session();
            let receipt = session.as_ref().and_then(|s| s.payments.receipt.as_ref());
            let Some(receipt) = receipt else {
                return dialog.on_close(on_close);
            };
            let total = if receipt.is_stars {
                tr_with(
                    keys::Q_MONEY_STARS_3,
                    &[("star_count", &receipt.star_count.to_string())],
                )
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
                        .child(div().child(tr(keys::PAYMENTS_TOTAL_LABEL)))
                        .child(div().font_semibold().child(total)),
                );
            if !receipt.credentials_title.is_empty() {
                body = body.child(
                    div()
                        .flex()
                        .justify_between()
                        .text_sm()
                        .child(div().child(tr(keys::Q_MONEY_PAID_WITH)))
                        .child(div().child(receipt.credentials_title.clone())),
                );
            }
            if receipt.tip_amount > 0 {
                body = body.child(
                    div()
                        .flex()
                        .justify_between()
                        .text_sm()
                        .child(div().child(tr(keys::Q_MONEY_TIP)))
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
    pub(in crate::ui) fn build_subscriptions_dialog(
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
                .title(crate::ui::shell::dialog_title(tr(
                    keys::Q_MONEY_SUBSCRIPTIONS,
                )));
            let Some(session) = this.session() else {
                return dialog.on_close(on_close);
            };
            let mut body = div().flex().flex_col().gap_3();
            if let Some(subs) = session.payments.star_subscriptions.as_ref() {
                body = body.child(
                    div()
                        .flex()
                        .justify_between()
                        .text_sm()
                        .child(div().child(tr(keys::Q_MONEY_STAR_BALANCE)))
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
            if session.payments.star_subscriptions_loading
                && session.payments.star_subscriptions.is_none()
            {
                body = body.child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(tr(keys::Q_MONEY_LOADING_SUBSCRIPTIONS)),
                );
            }
            if let Some(err) = session.payments.star_subscriptions_error.as_ref() {
                body = body.child(div().text_sm().child(err.clone()));
            }
            if let Some(subs) = session.payments.star_subscriptions.as_ref() {
                if subs.subscriptions.is_empty() && !session.payments.star_subscriptions_loading {
                    body = body.child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(tr(keys::Q_MONEY_NO_ACTIVE_SUBSCRIPTIONS)),
                    );
                }
                let mutating = session.payments.star_subscriptions_mutating;
                let confirming = session.payments.subscription_cancel_confirm.clone();
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
                if !subs.next_offset.is_empty() && session.payments.star_subscriptions_loading {
                    body = body.child(loading_row("subs-loading", cx));
                }
            }
            let body = watch(body, cx, Self::subscriptions_more).into_any_element();
            let scroll = this.payments.subs_scroll.clone();
            dialog
                .content(crate::ui::shell::scrollable_dialog_content_tracked(
                    Some(scroll),
                    {
                        let body = Rc::new(RefCell::new(Some(body.into_any_element())));
                        move |content, _, _| {
                            let body = body
                                .borrow_mut()
                                .take()
                                .unwrap_or_else(|| div().into_any_element());
                            content.child(body)
                        }
                    },
                ))
                .on_close(on_close)
        })
    }

    /// Slice `parity:bots-payment-recurring`: open the Subscriptions
    /// dialog and fetch the list (`getStarSubscriptions`).
    pub(in crate::ui) fn open_subscriptions(&mut self, cx: &mut Context<Self>) {
        if quill::agent::ui_driver::guard::refuse_hard_if_injecting(
            quill::agent::ui_driver::guard::HardSurface::Payments,
        ) {
            return;
        }
        let sent = self.live.as_mut().map(|live| {
            live.driver.session.payments.subscriptions_open = true;
            live.driver.maybe_fetch_star_subscriptions()
        });
        if !matches!(sent, Some(Ok(_))) {
            self.connection.set_note(StatusNote::error(tr(
                keys::Q_MONEY_COULD_NOT_LOAD_SUBSCRIPTIONS,
            )));
        }
        cx.notify();
    }

    /// Slice `parity:bots-payment-recurring`: close the Subscriptions dialog.
    pub(in crate::ui) fn close_subscriptions(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            live.driver.session.payments.subscriptions_open = false;
            live.driver.session.payments.subscription_cancel_confirm = None;
        }
        cx.notify();
    }

    /// Slice `parity:bots-payment-recurring`: "Load more" — the next
    /// `getStarSubscriptions` page.
    fn subscriptions_more(&mut self, cx: &mut Context<Self>, scrolled: bool) {
        // Paging needs a live connection; a demo session has nothing to ask.
        if self.live.is_none() {
            return;
        }
        let Some(payments) = self.session().map(|s| &s.payments) else {
            return;
        };
        if !payments.subscriptions_open
            || (payments.star_subscriptions_error.is_some() && !scrolled)
            || !load_due(
                scrolled_near_end(&self.payments.subs_scroll),
                !payments.star_subscriptions_offset.is_empty(),
                payments.star_subscriptions_loading,
            )
            || !self.payments.subs_asked.allow(
                payments
                    .star_subscriptions
                    .as_ref()
                    .map_or(0, |subs| subs.subscriptions.len()),
                scrolled,
            )
        {
            return;
        }
        let sent = self
            .live
            .as_mut()
            .map(|live| live.driver.fetch_more_star_subscriptions());
        if !matches!(sent, Some(Ok(_))) {
            self.connection.set_note(StatusNote::error(tr(
                keys::Q_MONEY_COULD_NOT_LOAD_MORE_SUBSCRIPTIONS,
            )));
        }
        cx.notify();
    }

    /// Slice `parity:bots-payment-recurring`: "Cancel" on a row — asks
    /// for inline confirmation first (`editStarSubscription` is a real
    /// money action).
    pub(in crate::ui) fn ask_cancel_subscription(&mut self, id: String, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            live.driver.session.payments.subscription_cancel_confirm = Some(id);
        }
        cx.notify();
    }

    /// Slice `parity:bots-payment-recurring`: "Yes, cancel" —
    /// `editStarSubscription` with `is_canceled: true`.
    pub(in crate::ui) fn confirm_cancel_subscription(&mut self, cx: &mut Context<Self>) {
        let id = self
            .session()
            .and_then(|s| s.payments.subscription_cancel_confirm.clone());
        let Some(id) = id else {
            return;
        };
        let sent = self.live.as_mut().map(|live| {
            live.driver.session.payments.subscription_cancel_confirm = None;
            live.driver.edit_star_subscription(&id, true)
        });
        if !matches!(sent, Some(Ok(_))) {
            self.connection.set_note(StatusNote::error(tr(
                keys::Q_MONEY_COULD_NOT_CANCEL_THE_SUBSCRIPTION,
            )));
        }
        cx.notify();
    }

    /// Slice `parity:bots-payment-recurring`: "Keep" — dismiss the inline
    /// cancel confirmation without touching the subscription.
    pub(in crate::ui) fn dismiss_cancel_confirm(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            live.driver.session.payments.subscription_cancel_confirm = None;
        }
        cx.notify();
    }

    /// Slice `parity:bots-payment-recurring`: "Re-enable" —
    /// `editStarSubscription` with `is_canceled: false`.
    pub(in crate::ui) fn reenable_subscription(&mut self, id: String, cx: &mut Context<Self>) {
        let sent = self
            .live
            .as_mut()
            .map(|live| live.driver.edit_star_subscription(&id, false));
        if !matches!(sent, Some(Ok(_))) {
            self.connection
                .set_note(StatusNote::error(tr(keys::Q_MONEY_COULD_NOT_RE_ENABLE_THE)));
        }
        cx.notify();
    }

    /// Slice `parity:bots-payment-recurring`: "Rejoin" —
    /// `reuseStarSubscription` on an active channel subscription whose chat
    /// the user needs to rejoin (the type's `can_reuse` is true).
    pub(in crate::ui) fn rejoin_subscription(&mut self, id: String, cx: &mut Context<Self>) {
        let sent = self
            .live
            .as_mut()
            .map(|live| live.driver.reuse_star_subscription(&id));
        if !matches!(sent, Some(Ok(_))) {
            self.connection.set_note(StatusNote::error(tr(
                keys::Q_MONEY_COULD_NOT_REJOIN_THE_SUBSCRIPTION,
            )));
        }
        cx.notify();
    }
}
