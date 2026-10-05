//! auth screens (phone/code/password/QR).

use super::app::QuillApp;
use gpui_kit::*;
use quill::telegram::envelope::AuthorizationState;
use smallvec::SmallVec;
use std::sync::Arc;
use zeroize::Zeroize;
impl QuillApp {
    pub(super) fn cycle_auth(&mut self, cx: &mut Context<Self>) {
        if self.live.is_some() {
            return;
        }
        self.auth_demo = match &self.auth_demo {
            AuthorizationState::WaitPhoneNumber => AuthorizationState::WaitCode {
                code_length: Some(5),
            },
            AuthorizationState::WaitCode { .. } => AuthorizationState::WaitPassword {
                has_recovery_email: true,
            },
            AuthorizationState::WaitPassword { .. } => AuthorizationState::WaitPremiumPurchase,
            AuthorizationState::WaitPremiumPurchase => AuthorizationState::Ready,
            AuthorizationState::Ready => AuthorizationState::WaitPhoneNumber,
            other => other.clone(),
        };
        cx.notify();
    }

    pub(super) fn submit_email(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(live) = self.live.as_mut() else {
            return;
        };
        let mut email = self.email_input.read(cx).value().to_string();
        let result = live.driver.submit_email(&email);
        email.zeroize();
        self.status_note = match result {
            Ok(_) => {
                self.email_input
                    .update(cx, |input, cx| input.set_value("", window, cx));
                "email submitted — waiting for Telegram".into()
            }
            Err(_) => "could not submit email".into(),
        };
        cx.notify();
    }

    pub(super) fn submit_phone(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(live) = self.live.as_mut() else {
            return;
        };
        if !matches!(
            live.driver.session.auth,
            AuthorizationState::WaitPhoneNumber
        ) {
            return;
        }
        let phone = self.phone_input.read(cx).value().to_string();
        match live.driver.submit_phone(&phone) {
            Ok(_) => {
                self.phone_input
                    .update(cx, |input, cx| input.set_value("", window, cx));
                self.status_note = "phone submitted — waiting for Telegram".into();
            }
            Err(_) => {
                self.status_note = "could not submit phone".into();
            }
        }
        cx.notify();
    }

    pub(super) fn submit_code(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(live) = self.live.as_mut() else {
            return;
        };
        if !matches!(
            live.driver.session.auth,
            AuthorizationState::WaitCode { .. } | AuthorizationState::WaitEmailCode { .. }
        ) {
            return;
        }
        let mut code = self.code_input.read(cx).value().to_string();
        let result = live.driver.submit_code(&code);
        code.zeroize();
        match result {
            Ok(_) => {
                self.code_input
                    .update(cx, |input, cx| input.set_value("", window, cx));
                self.status_note = "code submitted — waiting for Telegram".into();
            }
            Err(_) => {
                self.status_note = "could not submit code".into();
            }
        }
        cx.notify();
    }

    pub(super) fn submit_password(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(live) = self.live.as_mut() else {
            return;
        };
        if !matches!(
            live.driver.session.auth,
            AuthorizationState::WaitPassword { .. }
        ) {
            return;
        }
        let mut password = self.password_input.read(cx).value().to_string();
        let result = live.driver.submit_password(&password);
        password.zeroize();
        match result {
            Ok(_) => {
                self.password_input
                    .update(cx, |input, cx| input.set_value("", window, cx));
                self.status_note = "password submitted — waiting for Telegram".into();
            }
            Err(_) => {
                self.status_note = "could not submit password".into();
            }
        }
        cx.notify();
    }

    /// Slice A1: resend the login code. TDLib enforces the server-side
    /// cooldown (429 surfaces via `last_auth_error`); no local countdown.
    pub(super) fn resend_code(&mut self, cx: &mut Context<Self>) {
        let Some(live) = self.live.as_mut() else {
            return;
        };
        if !matches!(
            live.driver.session.auth,
            AuthorizationState::WaitCode { .. } | AuthorizationState::WaitEmailCode { .. }
        ) {
            return;
        }
        match live.driver.resend_code() {
            Ok(_) => {
                self.status_note = "code resent — waiting for Telegram".into();
            }
            Err(_) => {
                self.status_note = "could not resend code".into();
            }
        }
        cx.notify();
    }

    /// Slice A1: start QR-code login. TDLib moves auth to
    /// `WaitOtherDeviceConfirmation` carrying the QR link.
    pub(super) fn request_qr_login(&mut self, cx: &mut Context<Self>) {
        let Some(live) = self.live.as_mut() else {
            return;
        };
        match live.driver.request_qr_login() {
            Ok(_) => {
                self.status_note = "QR login requested — scan with a logged-in Telegram app".into();
            }
            Err(_) => {
                self.status_note = "could not start QR login".into();
            }
        }
        cx.notify();
    }

    /// Slice A1: render the QR-login link as a bitmap, cached by link and
    /// rebuilt only when the link changes. `None` for an empty or
    /// unencodable link (the caller shows honest fallback text). The link
    /// is rendered, never logged.
    pub(super) fn qr_login_image(&mut self, link: &str) -> Option<Arc<RenderImage>> {
        if link.is_empty() {
            return None;
        }
        if let Some((cached_link, image)) = &self.qr_login_cache {
            if cached_link == link {
                return Some(image.clone());
            }
        }
        let code = qrcode::QrCode::new(link.as_bytes()).ok()?;
        // Grayscale is unchanged by the R<->B swap `video_render_image`
        // applies to color frames, so no channel fixup is needed.
        let luma = code
            .render::<image::Luma<u8>>()
            .quiet_zone(true)
            .module_dimensions(6, 6)
            .build();
        let rgba = image::DynamicImage::ImageLuma8(luma).into_rgba8();
        let rendered = Arc::new(RenderImage::new(SmallVec::from_buf([image::Frame::new(
            rgba,
        )])));
        self.qr_login_cache = Some((link.to_string(), rendered.clone()));
        Some(rendered)
    }
}
