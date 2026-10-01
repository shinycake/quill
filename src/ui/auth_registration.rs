use super::app::QuillApp;
use gpui_kit::component::button::*;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::input::Textarea;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::state::RequestPurpose;
use quill::telegram::envelope::AuthorizationState;
use zeroize::Zeroize;

impl QuillApp {
    pub(super) fn registration_form(&self, cx: &mut Context<Self>) -> AnyElement {
        let AuthorizationState::WaitRegistration { terms } = self.current_auth() else {
            return div().into_any_element();
        };
        let accepted = terms.as_ref() == self.accepted_registration_terms.as_ref();
        let pending = self
            .session()
            .is_some_and(|s| s.requests.has_purpose(RequestPurpose::RegisterUser));
        let mut form = div()
            .flex()
            .flex_col()
            .gap_2()
            .child(Textarea::new(&self.registration_first_input))
            .child(Textarea::new(&self.registration_last_input));
        if let Some(terms) = terms {
            let label = if terms.min_user_age > 0 {
                format!(
                    "I accept these terms and confirm I am at least {} years old",
                    terms.min_user_age
                )
            } else {
                "I accept these terms".into()
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
                        .on_click(cx.listener(move |this, &on, _, cx| {
                            this.accepted_registration_terms = on.then(|| terms.clone());
                            cx.notify();
                        })),
                );
        }
        form.child(
            Checkbox::new("registration-notify-contacts")
                .label("Notify my contacts that I joined Telegram")
                .checked(self.registration_notify_contacts)
                .disabled(pending)
                .on_click(cx.listener(|this, &on, _, cx| {
                    this.registration_notify_contacts = on;
                    cx.notify();
                })),
        )
        .child(
            Button::new("create-telegram-account")
                .label(if pending {
                    "Creating account…"
                } else {
                    "Create account"
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
        let mut first = self.registration_first_input.read(cx).value().to_string();
        let mut last = self.registration_last_input.read(cx).value().to_string();
        let result = live.driver.register_user(
            &first,
            &last,
            self.accepted_registration_terms.as_ref(),
            self.registration_notify_contacts,
        );
        first.zeroize();
        last.zeroize();
        self.status_note = match result {
            Ok(_) => {
                self.registration_first_input
                    .update(cx, |input, cx| input.set_value("", window, cx));
                self.registration_last_input
                    .update(cx, |input, cx| input.set_value("", window, cx));
                self.accepted_registration_terms = None;
                "registration submitted — waiting for Telegram".into()
            }
            Err(_) => "could not register — check your name and accept the current terms".into(),
        };
        cx.notify();
    }
}
