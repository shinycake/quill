//! Methods moved out of `payments.rs` to keep files under 1000 lines.

use super::*;

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
            let dialog = dialog.overlay(true).title(crate::ui::shell::dialog_title("Checkout"));
            let Some(session) = this.session() else {
                return dialog.on_close(on_close.clone());
            };
            if session.payments.form.is_none() && !session.payments.form_loading {
                return dialog.on_close(on_close.clone());
            }
            let Some(dialog_state) = this.payments.dialog.as_ref() else {
                return dialog.on_close(on_close.clone());
            };
            let mut body = div().flex().flex_col().gap_3();
            if session.payments.form_loading {
                body = body.child(div().text_sm().child("Loading payment form…"));
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
                .title(crate::ui::shell::dialog_title("Payment receipt"));
            let session = this.session();
            let receipt = session.as_ref().and_then(|s| s.payments.receipt.as_ref());
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
                .title(crate::ui::shell::dialog_title("⭐ Subscriptions"));
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
            if session.payments.star_subscriptions_loading
                && session.payments.star_subscriptions.is_none()
            {
                body = body.child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child("Loading subscriptions…"),
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
                            .child("No active subscriptions."),
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
                if !subs.next_offset.is_empty() {
                    let label = if session.payments.star_subscriptions_loading {
                        "Loading…"
                    } else {
                        "Load more"
                    };
                    body = body.child(
                        Button::new("subs-load-more")
                            .label(label)
                            .ghost()
                            .disabled(session.payments.star_subscriptions_loading)
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
    pub(in crate::ui) fn open_subscriptions(&mut self, cx: &mut Context<Self>) {
        let sent = self.live.as_mut().map(|live| {
            live.driver.session.payments.subscriptions_open = true;
            live.driver.maybe_fetch_star_subscriptions()
        });
        if !matches!(sent, Some(Ok(_))) {
            self.connection.status_note = "could not load subscriptions".into();
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
    pub(in crate::ui) fn load_more_subscriptions(&mut self, cx: &mut Context<Self>) {
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
            self.connection.status_note = "could not cancel the subscription".into();
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
            self.connection.status_note = "could not re-enable the subscription".into();
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
            self.connection.status_note = "could not rejoin the subscription".into();
        }
        cx.notify();
    }
}
