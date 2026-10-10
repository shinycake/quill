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
        let state = self.current_auth();
        let auth = &self.signin_view(auth, &state);
        // "Wrong number?" swaps the code form for the phone form.
        let editing_phone = self.signin_phone_visible(&state) && !show_phone;
        let show_phone = show_phone || editing_phone;
        let show_code = show_code && !editing_phone;
        let busy = self.session().is_some_and(|s| s.requests.has_auth_submit());
        let can_qr = quill::auth::can_request_qr_login(&self.current_auth())
            && (self.live.is_some() || self.demo_auth_inputs);
        let live_or_demo = self.live.is_some() || self.demo_auth_inputs;
        let retry_available = self.live.is_some() || self.connection_lost || self.demo_auth_inputs;
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
        let (error_elements, locked) = self.signin_error_elements(cx);
        if show_phone {
            for element in self.signin_phone_section(busy, locked, cx) {
                form = form.child(element);
            }
        }
        if show_code {
            for element in self.signin_code_section(busy, locked, cx) {
                form = form.child(element);
            }
        }
        if show_phone || show_code {
            for element in error_elements {
                form = form.child(element);
            }
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
                            .aspect_square()
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
        if matches!(auth.action, AuthAction::PremiumRequired)
            && let AuthorizationState::WaitPremiumPurchase {
                support_email_address,
                support_email_subject,
                ..
            } = &state
            && let Some(mailto) =
                quill::signin::premium_support_mailto(support_email_address, support_email_subject)
        {
            form = form.child(
                Button::new("premium-support")
                    .label("Email Telegram support")
                    .ghost()
                    .w_full()
                    .on_click(move |_, _, cx| cx.open_url(&mailto)),
            );
        }
        if matches!(auth.action, AuthAction::Closed) && retry_available {
            form = form.child(
                Button::new("retry-connection")
                    .label("Retry")
                    .primary()
                    .w_full()
                    .on_click(cx.listener(|this, _, _, cx| this.restart_live_connection(cx))),
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
            || (matches!(auth.action, AuthAction::Closed) && retry_available)
            || matches!(
                auth.action,
                AuthAction::UnsupportedHalt { .. } | AuthAction::PremiumRequired
            );
        if has_form {
            card = card.child(form);
        }
        // Telegram API terms 2.2: say in the intro that this is a third-party
        // app built on the Telegram API.
        card = card.child(
            div()
                .id("auth-unofficial-notice")
                .role(Role::Label)
                .aria_label(quill::about::INTRO_NOTICE)
                .text_xs()
                .text_center()
                .text_color(cx.theme().muted_foreground)
                .child(quill::about::INTRO_NOTICE),
        );
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
