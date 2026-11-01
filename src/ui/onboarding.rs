//! Sign-in flow: one centered card per authorization step, shown in place
//! of the chat layout until the account is ready.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use gpui_kit::component::button::*;
use gpui_kit::component::input::Textarea;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::auth::{AuthAction, AuthView};
use quill::lang::{keys, tr};
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
        let disk_full = self.disk_full_view();
        let signin_open = disk_full.is_none();
        let auth = &disk_full.unwrap_or_else(|| self.signin_view(auth, &state));
        // "Wrong number?" swaps the code form for the phone form.
        let editing_phone = signin_open && self.signin_phone_visible(&state) && !show_phone;
        let show_phone = show_phone || editing_phone;
        let show_code = show_code && !editing_phone;
        let busy = self.session().is_some_and(|s| s.requests.has_auth_submit());
        let can_qr = signin_open
            && quill::auth::can_request_qr_login(&self.current_auth())
            && (self.live.is_some() || self.auth_ui.demo_inputs);
        let live_or_demo = self.live.is_some() || self.auth_ui.demo_inputs;
        let show_retry = matches!(auth.action, AuthAction::StartupFailed)
            || (matches!(auth.action, AuthAction::Closed)
                && (self.live.is_some() || self.auth_ui.demo_inputs));
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
                    })
                    .children(self.disk_full_reveal()),
            );
        let mut form = div().w_full().flex().flex_col().gap_3();
        if matches!(auth.action, AuthAction::Register) && self.live.is_some() {
            form = form.child(self.registration_form(cx));
        }
        if matches!(auth.action, AuthAction::EnterEmail) && self.live.is_some() {
            form = form
                .child(
                    Textarea::new(&self.auth_ui.email_input)
                        .aria_label(tr(keys::Q_MISC_EMAIL_ADDRESS))
                        .h(px(40.)),
                )
                .child(
                    primary_action("submit-login-email", tr(keys::CONTINUE), busy)
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
                    .child(tr(keys::Q_MISC_PREPARING_QR_CODE))
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
                            .child(tr(keys::Q_MISC_TELEGRAM_SETTINGS_DEVICES_LINK_DESKTOP)),
                    ),
            );
            // tdesktop puts "Log in using passkey" under the QR steps
            // (`intro_qr.cpp` `setupPasskeyLink`).
            if let Some(link) = self.passkey_login_link(cx) {
                form = form.child(link);
            }
        }
        if can_qr && !show_qr {
            form = form.child(
                Button::new("qr-login")
                    .label(tr(keys::Q_MISC_SIGN_IN_WITH_QR_CODE))
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
                    .label(tr(keys::Q_MISC_EMAIL_TELEGRAM_SUPPORT))
                    .ghost()
                    .w_full()
                    .on_click(move |_, _, cx| cx.open_url(&mailto)),
            );
        }
        if show_retry {
            form = form.child(
                Button::new("retry-connection")
                    .label(tr(keys::Q_MISC_RETRY))
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
            || show_retry
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
            .relative()
            .flex()
            .items_center()
            .justify_center()
            .overflow_y_scroll()
            .p_6()
            .bg(cx.theme().background)
            .child(card)
            // The intro says "Test Mode" in its corner while the account on
            // screen belongs to the test environment.
            .when(self.settings.advanced.test_environment(), |this| {
                this.child(
                    div()
                        .id("auth-test-mode-label")
                        .debug_selector(|| "auth-test-mode-label".into())
                        .role(Role::Label)
                        .aria_label(tr(keys::Q_MISC_TEST_MODE))
                        .absolute()
                        .top(px(12.))
                        .right(px(16.))
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(tr(keys::Q_MISC_TEST_MODE)),
                )
            })
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
