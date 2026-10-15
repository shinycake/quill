#![deny(clippy::indexing_slicing)]
use super::app::QuillApp;
use gpui_kit::component::button::*;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::input::Textarea;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::lang::{keys, tr, tr_with};
use quill::state::RequestPurpose;
use quill::telegram::envelope::AuthorizationState;
use zeroize::Zeroize;

impl QuillApp {
    pub(super) fn registration_form(&self, cx: &mut Context<Self>) -> AnyElement {
        let AuthorizationState::WaitRegistration { terms } = self.current_auth() else {
            return div().into_any_element();
        };
        let accepted = terms.as_ref() == self.auth_ui.accepted_registration_terms.as_ref();
        let pending = self
            .session()
            .is_some_and(|s| s.requests.has_purpose(RequestPurpose::RegisterUser));
        let mut form = div()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                Textarea::new(&self.auth_ui.registration_first_input)
                    .aria_label(tr(keys::Q_MISC_FIRST_NAME)),
            )
            .child(
                Textarea::new(&self.auth_ui.registration_last_input)
                    .aria_label(tr(keys::Q_MISC_LAST_NAME)),
            );
        if let Some(terms) = terms {
            let label = if terms.min_user_age > 0 {
                tr_with(
                    keys::Q_MISC_I_ACCEPT_THESE_TERMS_AND,
                    &[("min_user_age", &terms.min_user_age.to_string())],
                )
            } else {
                tr(keys::Q_MISC_I_ACCEPT_THESE_TERMS).into()
            };
            form = form
                .child(
                    div()
                        .id("registration-terms")
                        .max_h(px(180.))
                        .overflow_y_scroll()
                        .text_sm()
                        .child(terms.text.clone()),
                )
                .child(
                    Checkbox::new("accept-registration-terms")
                        .label(label)
                        .checked(accepted)
                        .disabled(pending)
                        .on_click(cx.listener(move |this, &on: &bool, _, cx| {
                            this.auth_ui.accepted_registration_terms = on.then(|| terms.clone());
                            cx.notify();
                        })),
                );
        }
        form.child(
            Checkbox::new("registration-notify-contacts")
                .label(tr(keys::Q_MISC_NOTIFY_MY_CONTACTS_THAT_I))
                .checked(self.auth_ui.registration_notify_contacts)
                .disabled(pending)
                .on_click(cx.listener(|this, &on, _, cx| {
                    this.auth_ui.registration_notify_contacts = on;
                    cx.notify();
                })),
        )
        .child(
            Button::new("create-telegram-account")
                .label(if pending {
                    tr(keys::Q_MISC_CREATING_ACCOUNT)
                } else {
                    tr(keys::Q_MISC_CREATE_ACCOUNT)
                })
                .disabled(!accepted || pending)
                .on_click(cx.listener(|this, _, window, cx| this.submit_registration(window, cx))),
        )
        .into_any_element()
    }

    fn submit_registration(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(live) = self.live.as_mut() else {
            return;
        };
        let mut first = self
            .auth_ui
            .registration_first_input
            .read(cx)
            .value()
            .to_string();
        let mut last = self
            .auth_ui
            .registration_last_input
            .read(cx)
            .value()
            .to_string();
        let result = live.driver.register_user(
            &first,
            &last,
            self.auth_ui.accepted_registration_terms.as_ref(),
            self.auth_ui.registration_notify_contacts,
        );
        first.zeroize();
        last.zeroize();
        self.connection.status_note = match result {
            Ok(_) => {
                self.auth_ui
                    .registration_first_input
                    .update(cx, |input, cx| input.set_value("", window, cx));
                self.auth_ui
                    .registration_last_input
                    .update(cx, |input, cx| input.set_value("", window, cx));
                self.auth_ui.accepted_registration_terms = None;
                tr(keys::Q_MISC_REGISTRATION_SUBMITTED_WAITING_FOR_TELEGRAM).into()
            }
            Err(_) => tr(keys::Q_MISC_COULD_NOT_REGISTER_CHECK_YOUR).into(),
        };
        cx.notify();
    }
}
