//! "Log in using passkey" on the QR sign-in screen (tdesktop
//! `intro/intro_qr.cpp` `QrWidget::setupPasskeyLink`). The link shows when
//! the app config's `settings_display_passkeys` is on and this build can
//! run a ceremony; a click asks for the request options and opens the
//! ceremony dialog (`passkey_ceremony`) in sign-in mode. A failed step
//! shows the server's error type under the link, as tdesktop's
//! `showError` does; a password-protected account goes on to the password
//! screen through TDLib's authorization state.
#![deny(clippy::indexing_slicing)]

use super::passkey_ceremony::{CeremonyMode, CeremonySheet, CeremonyUi};
use crate::ui::app::QuillApp;
use gpui_kit::TestSupportExt;
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::lang::{keys, tr};
use quill::telegram::envelope::AuthorizationState;

/// The auth states the sign-in ceremony belongs to.
fn signing_in(state: &AuthorizationState) -> bool {
    matches!(
        state,
        AuthorizationState::WaitPhoneNumber
            | AuthorizationState::WaitOtherDeviceConfirmation { .. }
    )
}

impl QuillApp {
    /// Whether the link shows: the server allows passkeys and WebAuthn is
    /// available (demos set it by hand).
    pub(in crate::ui) fn passkey_login_link_shown(&self) -> bool {
        match self.session() {
            Some(session) => session.settings.passkeys.login_link_shown(),
            None => self.privacy.passkeys.demo_login_link,
        }
    }

    /// Whether a sign-in is under way: the options or the server's answer
    /// on their way, or the dialog open in sign-in mode.
    pub(in crate::ui) fn passkey_login_busy(&self) -> bool {
        self.privacy
            .passkeys
            .ceremony
            .as_ref()
            .is_some_and(|ui| ui.mode == CeremonyMode::Login)
            || self
                .live
                .as_ref()
                .is_some_and(|live| live.driver.passkey_login_pending())
    }

    /// The account signed in some other way (the QR code was scanned)
    /// while the dialog was open: the ceremony stops and the dialog
    /// closes, as the intro step it belonged to goes away in tdesktop.
    pub(in crate::ui) fn close_login_ceremony_if_signed_in(&mut self) -> bool {
        let login_open = self
            .privacy
            .passkeys
            .ceremony
            .as_ref()
            .is_some_and(|ui| ui.mode == CeremonyMode::Login);
        if !login_open || self.live.is_none() || signing_in(&self.current_auth()) {
            return false;
        }
        // Dropping the ceremony cancels it.
        self.privacy.passkeys.ceremony = None;
        true
    }

    /// "Log in using passkey": the options (reused while fresh), then the
    /// ceremony.
    pub(in crate::ui) fn start_passkey_login(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let _ = self.passkey_pin_input(window, cx);
        let Some(live) = self.live.as_mut() else {
            return;
        };
        if live.driver.begin_login().is_err() {
            return;
        }
        self.run_passkey_ticker(window, cx);
        cx.notify();
    }

    /// The link and, after a failed step, the server's error under it.
    /// `None` when the link does not show.
    pub(in crate::ui) fn passkey_login_link(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.passkey_login_link_shown() {
            return None;
        }
        let busy = self.passkey_login_busy();
        let error = self
            .session()
            .and_then(|s| s.settings.passkeys.login.error.clone())
            .or_else(|| self.privacy.passkeys.demo_login_error.clone());
        let mut column = div().flex().flex_col().items_center().gap_1().child(
            Button::new("passkey-login")
                .label(tr(keys::INTRO_QR_PASSKEY))
                .ghost()
                .loading(busy)
                .disabled(busy)
                .on_click(cx.listener(|this, _, window, cx| this.start_passkey_login(window, cx))),
        );
        if let Some(error) = error {
            column = column.child(
                div()
                    .id("passkey-login-error")
                    .test_support()
                    .role(Role::Alert)
                    .aria_label(error.clone())
                    .text_sm()
                    .text_center()
                    .text_color(cx.theme().danger)
                    .child(error),
            );
        }
        Some(column.into_any_element())
    }
}

crate::ui::screenshot_demo::register_demos![
    crate::ui::screenshot_demo::DemoSpec::signed_out("wait-qr-passkey", || {
        (
            crate::ui::connect_ui::ConnectUiStatus::DemoWaitPhone,
            "screenshot demo — QR sign-in with the passkey link (injected auth, no live Telegram)"
                .into(),
            AuthorizationState::WaitOtherDeviceConfirmation {
                link: "tg://login?token=AQID".into(),
            },
        )
    })
    .setup(QuillApp::demo_passkey_login),
];

impl QuillApp {
    /// `wait-qr-passkey`: the QR screen with "Log in using passkey", and
    /// with `QUILL_DEMO_PASSKEY_LOGIN` set to `key`, `pin`, `error` or
    /// `server-error`, the sign-in dialog's sheet or the error line. No
    /// worker runs.
    fn demo_passkey_login(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.privacy.passkeys.demo_login_link = true;
        let _ = self.passkey_pin_input(window, cx);
        let sheet = match std::env::var("QUILL_DEMO_PASSKEY_LOGIN")
            .unwrap_or_default()
            .as_str()
        {
            "key" => Some(CeremonySheet::Key),
            "pin" => Some(CeremonySheet::Pin { invalid: false }),
            "error" => Some(CeremonySheet::Error(keys::PASSKEY_ERROR_NO_CREDENTIALS)),
            "server-error" => {
                self.privacy.passkeys.demo_login_error =
                    Some("PASSKEY_CREDENTIAL_NOT_FOUND".into());
                None
            }
            _ => None,
        };
        if let Some(sheet) = sheet {
            self.privacy.passkeys.ceremony =
                Some(CeremonyUi::new(CeremonyMode::Login, sheet, None));
        }
        cx.notify();
    }
}

impl QuillApp {
    /// Tests: open the sign-in dialog on a sheet, without a worker.
    #[cfg(test)]
    #[cfg(feature = "demo-capture")]
    pub(in crate::ui) fn show_login_passkey_sheet(
        &mut self,
        sheet: CeremonySheet,
        cx: &mut Context<Self>,
    ) {
        self.privacy.passkeys.ceremony = Some(CeremonyUi::new(CeremonyMode::Login, sheet, None));
        cx.notify();
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "passkey_login_tests.rs"]
mod tests;
