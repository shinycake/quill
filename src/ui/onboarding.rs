//! Sign-in flow: one centered card per authorization step, shown in place
//! of the chat layout until the account is ready.

use super::app::QuillApp;
use gpui_kit::component::button::*;
use gpui_kit::component::input::Textarea;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::auth::{AuthAction, AuthView};
use quill::telegram::envelope::AuthorizationState;

const CARD_WIDTH: f32 = 380.;

impl QuillApp {
    pub(super) fn onboarding(
        &mut self,
        auth: &AuthView,
        show_phone: bool,
        show_code: bool,
        show_password: bool,
        show_qr: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let busy = self.session().is_some_and(|s| s.requests.has_auth_submit());
        let can_qr = quill::auth::can_request_qr_login(&self.current_auth())
            && (self.live.is_some() || self.demo_auth_inputs);
        let live_or_demo = self.live.is_some() || self.demo_auth_inputs;
        let mut card = div()
            .id("onboarding-card")
            .w(px(CARD_WIDTH))
            .max_w_full()
            .flex()
            .flex_col()
            .items_center()
            .gap_4()
            .child(
                div()
                    .size(px(72.))
                    .rounded_full()
                    .bg(cx.theme().primary)
                    .text_color(gpui_kit::white())
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(34.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Q"),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .id("auth-title")
                            .role(Role::Heading)
                            .aria_label(auth.title)
                            .text_2xl()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(auth.title),
                    )
                    .when(!auth.body.is_empty(), |this| {
                        this.child(
                            div()
                                .id("auth-explanation")
                                .role(Role::Label)
                                .aria_label(auth.body.clone())
                                .text_sm()
                                .text_center()
                                .text_color(cx.theme().muted_foreground)
                                .child(auth.body.clone()),
                        )
                    }),
            );
        let mut form = div().w_full().flex().flex_col().gap_3();
        if matches!(auth.action, AuthAction::Register) && self.live.is_some() {
            form = form.child(self.registration_form(cx));
        }
        if matches!(auth.action, AuthAction::EnterEmail) && self.live.is_some() {
            form = form
                .child(
                    Textarea::new(&self.email_input)
                        .aria_label("Email address")
                        .h(px(40.)),
                )
                .child(
                    primary_action("submit-login-email", "Continue", busy)
                        .on_click(cx.listener(|this, _, window, cx| this.submit_email(window, cx))),
                );
        }
        if show_phone {
            form = form
                .child(
                    Textarea::new(&self.phone_input)
                        .aria_label("Phone number")
                        .h(px(40.)),
                )
                .child(
                    primary_action("submit-phone", "Continue", busy)
                        .on_click(cx.listener(|this, _, window, cx| this.submit_phone(window, cx))),
                );
        }
        if show_code {
            form = form
                .child(
                    Textarea::new(&self.code_input)
                        .aria_label("Sign-in code")
                        .h(px(40.)),
                )
                .child(
                    primary_action("submit-code", "Continue", busy)
                        .on_click(cx.listener(|this, _, window, cx| this.submit_code(window, cx))),
                )
                .child(
                    Button::new("resend-code")
                        .label("Send the code again")
                        .ghost()
                        .w_full()
                        .on_click(cx.listener(|this, _, _, cx| this.resend_code(cx))),
                );
        }
        if show_password {
            // Slice A10: password / recovery-code entry (`auth_recovery.rs`).
            form = form.child(self.auth_password_section(cx));
        }
        if show_qr {
            let link = match self.current_auth() {
                AuthorizationState::WaitOtherDeviceConfirmation { link } => link,
                _ => String::new(),
            };
            let qr: AnyElement = match self.qr_login_image(&link) {
                Some(image) => div()
                    .p_3()
                    .rounded_lg()
                    .bg(gpui_kit::white())
                    .child(
                        img(ImageSource::from(image))
                            .size(px(208.))
                            .object_fit(ObjectFit::Contain),
                    )
                    .into_any_element(),
                None => div()
                    .size(px(232.))
                    .rounded_lg()
                    .bg(cx.theme().secondary)
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("Preparing QR code…")
                    .into_any_element(),
            };
            form = form.child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_3()
                    .child(qr)
                    .child(
                        div()
                            .text_sm()
                            .text_center()
                            .text_color(cx.theme().muted_foreground)
                            .child("Telegram → Settings → Devices → Link Desktop Device"),
                    ),
            );
        }
        if can_qr && !show_qr {
            form = form.child(
                Button::new("qr-login")
                    .label("Sign in with QR code")
                    .ghost()
                    .w_full()
                    .disabled(busy)
                    .on_click(cx.listener(|this, _, _, cx| this.request_qr_login(cx))),
            );
        }
        if let AuthAction::UnsupportedHalt { reason } = &auth.action {
            form = form.child(
                div()
                    .text_sm()
                    .text_center()
                    .text_color(cx.theme().danger)
                    .child(reason.to_string()),
            );
        }
        let has_form = live_or_demo
            || show_phone
            || show_code
            || show_password
            || show_qr
            || matches!(auth.action, AuthAction::UnsupportedHalt { .. });
        if has_form {
            card = card.child(form);
        }
        div()
            .id("onboarding")
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .overflow_y_scroll()
            .p_6()
            .bg(cx.theme().background)
            .child(card)
            .into_any_element()
    }
}

/// Full-width primary step action; disabled while a sign-in request is in
/// flight so it can't be submitted twice.
fn primary_action(id: &'static str, label: &'static str, busy: bool) -> Button {
    Button::new(id)
        .label(label)
        .primary()
        .w_full()
        .loading(busy)
        .disabled(busy)
}
