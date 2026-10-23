//! The checkout's payment method list and the "New Card" form it opens
//! (tdesktop's `Panel::choosePaymentMethod` and `EditCard::setupContent`).
#![deny(clippy::indexing_slicing)]

use super::card_form::{CardForm, CardInput, placeholder};
use crate::ui::app::QuillApp;
use crate::ui::country_box::CountryHost;
use crate::ui::dialogs::{PaymentCredentialChoice, PaymentDialog};
use gpui_kit::TestSupportExt;
use gpui_kit::component::alert::Alert;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::dialog::{Cancel, Confirm};
use gpui_kit::component::input::{Backspace, Delete, Input};
use gpui_kit::component::radio::{Radio, RadioGroup};
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::lang::{keys, tr};
use quill::payment_card::{CardField, NativeProvider};
use quill::telegram::envelope::{PaymentFormTypeData, PaymentProviderKind, SavedCredential};

/// Where "New Card..." leads for a provider.
enum CardEntry {
    Native,
    Web(String),
}

fn card_entry(provider: &PaymentProviderKind, is_test: bool) -> Option<CardEntry> {
    match provider {
        PaymentProviderKind::Web { url } if !url.is_empty() => Some(CardEntry::Web(url.clone())),
        _ => NativeProvider::for_form(provider, is_test).map(|_| CardEntry::Native),
    }
}

impl QuillApp {
    fn checkout_card_entry(&self) -> Option<CardEntry> {
        let form = self.session()?.payments.form.as_ref()?;
        let PaymentFormTypeData::Regular(regular) = &form.form_type else {
            return None;
        };
        card_entry(&regular.provider, regular.invoice.is_test)
    }

    /// "New Card...": the card form for Stripe and Smart Glocal, the
    /// provider's own page for any other provider.
    pub(in crate::ui) fn enter_new_card(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.checkout_card_entry() {
            Some(CardEntry::Native) => self.open_card_form(window, cx),
            Some(CardEntry::Web(url)) => self.open_message_url(&url, cx),
            None => {}
        }
    }

    /// Pay with no card chosen yet: enter one first, as tdesktop's Pay
    /// opens the payment method when it is missing.
    pub(super) fn needs_card_entry(&self, dialog: &PaymentDialog) -> bool {
        dialog.credential_choice == PaymentCredentialChoice::NewCard && dialog.new_card.is_none()
    }

    pub(super) fn can_enter_new_card(&self) -> bool {
        self.checkout_card_entry().is_some()
    }

    /// "Continue" on the password prompt: Two-Step Verification opens over
    /// the checkout, which keeps the new card and comes back when it closes
    /// (tdesktop's `panelSetPassword`). A password set there lets the card
    /// be saved (see `checkout_can_save_credentials`).
    fn set_password_for_card(&mut self, cx: &mut Context<Self>) {
        if let Some(dialog) = self.payments.dialog.as_mut() {
            dialog.password_prompt = false;
        }
        self.open_twofa(cx);
    }

    /// A method picked from the list. Picking another method drops the
    /// password prompt; picking the new card again asks again when it still
    /// cannot be saved (tdesktop's `Form::chooseSavedMethod`).
    fn choose_payment_method(&mut self, choice: PaymentCredentialChoice, cx: &mut Context<Self>) {
        let ask_password = self.needs_password_to_save();
        if let Some(dialog) = self.payments.dialog.as_mut() {
            let saving_new_card = choice == PaymentCredentialChoice::NewCard
                && dialog.new_card.as_ref().is_some_and(|card| card.save);
            dialog.password_prompt = saving_new_card && ask_password;
            dialog.credential_choice = choice;
        }
        cx.notify();
    }

    fn dismiss_card_password_prompt(&mut self, cx: &mut Context<Self>) {
        if let Some(dialog) = self.payments.dialog.as_mut() {
            dialog.password_prompt = false;
        }
        cx.notify();
    }

    /// The payment method list: "New Card...", the saved cards, then the
    /// card just entered.
    pub(super) fn payment_method_section(
        &self,
        dialog: &PaymentDialog,
        saved: &[SavedCredential],
        cx: &mut Context<Self>,
    ) -> Div {
        let mut choices = vec![None];
        let mut titles = vec![tr(keys::PAYMENTS_NEW_CARD).to_string()];
        for credential in saved {
            choices.push(Some(PaymentCredentialChoice::Saved(credential.id.clone())));
            titles.push(credential.title.clone());
        }
        if let Some(card) = &dialog.new_card {
            choices.push(Some(PaymentCredentialChoice::NewCard));
            titles.push(card.credentials.title().to_string());
        }
        let selected = choices
            .iter()
            .position(|choice| choice.as_ref() == Some(&dialog.credential_choice));
        let can_enter = self.can_enter_new_card();
        let radios = titles.into_iter().enumerate().map(|(index, title)| {
            Radio::new(SharedString::from(format!("payment-method-{index}")))
                .label(title)
                .when(index == 0, |radio| radio.disabled(!can_enter))
        });
        let mut section = div()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .text_xs()
                    .font_semibold()
                    .child(tr(keys::PAYMENTS_PAYMENT_METHOD)),
            )
            .child(
                RadioGroup::vertical("payment-method")
                    .selected_index(selected)
                    .children(radios)
                    .on_click(cx.listener(move |this, &index: &usize, window, cx| {
                        match choices.get(index) {
                            Some(Some(choice)) => this.choose_payment_method(choice.clone(), cx),
                            Some(None) => this.enter_new_card(window, cx),
                            None => {}
                        }
                    })),
            );
        if dialog.password_prompt {
            section = section.child(self.card_password_prompt(cx));
        }
        section
    }

    /// tdesktop's `Panel::askSetPassword`.
    fn card_password_prompt(&self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .id("payment-card-password")
            .flex()
            .flex_col()
            .gap_2()
            .p_3()
            .rounded(cx.theme().radius)
            .bg(cx.theme().muted)
            .child(div().text_sm().child(tr(keys::PAYMENTS_NEED_PASSWORD)))
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap_2()
                    .child(
                        Button::new("payment-card-password-cancel")
                            .label(tr(keys::CANCEL))
                            .ghost()
                            .small()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.dismiss_card_password_prompt(cx);
                            })),
                    )
                    .child(
                        Button::new("payment-card-password-continue")
                            .label(tr(keys::CONTINUE))
                            .primary()
                            .small()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.set_password_for_card(cx);
                            })),
                    ),
            )
            .into_any_element()
    }

    /// The "New Card" form, or its country box while that is open.
    pub(super) fn card_form_view(&self, form: &CardForm, cx: &mut Context<Self>) -> AnyElement {
        if form.country_box.is_some()
            && let Some(view) = self.country_box_view(
                CountryHost::PaymentCard,
                tr(keys::PAYMENTS_BILLING_COUNTRY),
                cx,
            )
        {
            return div()
                .on_action(cx.listener(|this, _: &Cancel, _, cx| {
                    this.close_country_box(CountryHost::PaymentCard, cx);
                }))
                .child(view)
                .into_any_element();
        }
        let mut body = div().flex().flex_col().gap_3();
        if let Some(number) = form.input(CardField::Number) {
            body = body.child(card_field(number, cx));
        }
        body = body.child(
            div()
                .flex()
                .gap_3()
                .children(
                    form.input(CardField::Expiry)
                        .map(|input| card_field(input, cx)),
                )
                .children(
                    form.input(CardField::Cvc)
                        .map(|input| card_field(input, cx)),
                ),
        );
        if let Some(name) = form.input(CardField::Name) {
            body = body.child(card_field(name, cx));
        }
        let needs_country = form.provider.needs(CardField::Country);
        let zip = form.input(CardField::Zip);
        if needs_country || zip.is_some() {
            body = body.child(
                div()
                    .pt_1()
                    .text_sm()
                    .font_semibold()
                    .child(tr(keys::PAYMENTS_BILLING_ADDRESS)),
            );
        }
        if needs_country {
            body = body.child(self.card_country_field(form, cx));
        }
        if let Some(zip) = zip {
            body = body.child(card_field(zip, cx));
        }
        if let Some(save) = form.save {
            body = body.child(
                Checkbox::new("payment-card-save")
                    .checked(save)
                    .label(tr(keys::PAYMENTS_SAVE_INFORMATION))
                    .on_click(cx.listener(|this, &on, _, cx| this.toggle_card_save(on, cx))),
            );
        }
        if let Some(notice) = &form.notice {
            // Inside the checkout, as tdesktop's panel toast: a window
            // toast would paint under the dialog.
            let alert = Alert::error("payment-card-notice-alert", notice.clone())
                .on_close(cx.listener(|this, _: &ClickEvent, _, cx| this.dismiss_card_notice(cx)));
            body = body.child(div().id("payment-card-notice").child(alert).test_support());
        }
        let busy = form.is_tokenizing();
        body.child(
            div()
                .flex()
                .justify_end()
                .gap_2()
                .pt_1()
                .child(
                    Button::new("payment-card-cancel")
                        .label(tr(keys::CANCEL))
                        .ghost()
                        .disabled(busy)
                        .on_click(cx.listener(|this, _, _, cx| this.close_card_form(cx))),
                )
                .child(
                    Button::new("payment-card-done")
                        .label(tr(keys::ABOUT_DONE))
                        .primary()
                        .loading(busy)
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.submit_card_form(window, cx);
                        })),
                ),
        )
        .on_action(cx.listener(|this, _: &Cancel, _, cx| this.close_card_form(cx)))
        .into_any_element()
    }

    /// The billing country: focusable on its own (Enter or Space opens the
    /// box), marked when Done finds it empty.
    fn card_country_field(&self, form: &CardForm, cx: &mut Context<Self>) -> AnyElement {
        let border = if form.country_invalid {
            cx.theme().danger
        } else {
            gpui_kit::transparent_black()
        };
        let ring = cx.theme().ring;
        div()
            .id("payment-card-country")
            .track_focus(&form.country_focus)
            .rounded(cx.theme().radius)
            .border_1()
            .border_color(border)
            .when(!form.country_invalid, |field| {
                field.focus(move |style| style.border_color(ring))
            })
            .on_action(cx.listener(|this, _: &Confirm, window, cx| {
                this.activate_card_field(CardField::Country, window, cx);
            }))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                if event.keystroke.key == "space" {
                    this.activate_card_field(CardField::Country, window, cx);
                }
            }))
            .child(self.country_field_button(
                CountryHost::PaymentCard,
                placeholder(CardField::Country),
                self.country_display_name(&form.country_iso),
                cx,
            ))
            .into_any_element()
    }
}

/// One text field; Backspace and Delete are seen before the input edits so
/// the formatter can step over a separator.
fn card_field(input: &CardInput, cx: &mut Context<QuillApp>) -> AnyElement {
    let field = input.field;
    let danger = cx.theme().danger;
    div()
        .flex_1()
        .capture_action(cx.listener(move |this, _: &Backspace, window, cx| {
            this.card_field_before_delete(field, true, window, cx);
        }))
        .capture_action(cx.listener(move |this, _: &Delete, window, cx| {
            this.card_field_before_delete(field, false, window, cx);
        }))
        .child(
            Input::new(&input.state)
                .aria_label(placeholder(field))
                .when(input.invalid, |input| {
                    input.border_color(danger).focus_bordered(false)
                }),
        )
        .into_any_element()
}
