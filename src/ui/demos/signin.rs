//! Screenshot demos: sign-in and connection screens (injected auth states,
//! no live Telegram).

use crate::ui::app::QuillApp;
use crate::ui::connect_ui::ConnectUiStatus;
use crate::ui::screenshot_demo::{DemoSpec, register_demos};
use gpui_kit::*;
use quill::connect::ConnectBlocker;
use quill::telegram::envelope::{AuthorizationState, CodeDelivery, CodeDetail, CodeKind};
use std::time::Duration;

register_demos![
    // Unexpected `authorizationStateClosed`: the Retry card.
    DemoSpec::signed_out("connection-closed", || {
        (
            ConnectUiStatus::DemoWaitPhone,
            "screenshot demo — Closed (injected auth, no live Telegram)".into(),
            AuthorizationState::Closed,
        )
    }),
    DemoSpec::signed_out("need-tdjson", || {
        (
            ConnectUiStatus::NeedTdjson,
            ConnectBlocker::MissingTdjson.user_message().into(),
            AuthorizationState::WaitPhoneNumber,
        )
    })
    .auth_inputs(false),
    DemoSpec::signed_out("wait-code", || {
        (
            ConnectUiStatus::DemoWaitCode,
            "screenshot demo — WaitCode (injected auth, no live Telegram)".into(),
            AuthorizationState::WaitCode {
                code_length: Some(5),
                delivery: Default::default(),
            },
        )
    }),
    // Code step for `authenticationCodeTypeFirebase*` (official apps only).
    DemoSpec::signed_out("wait-code-firebase", || typed_code_start(
        TypedCode::Firebase
    ))
    .setup(|app, _, _| app.demo_typed_code(TypedCode::Firebase)),
    // Code step for `authenticationCodeTypeFlashCall`.
    DemoSpec::signed_out("wait-code-flash", || typed_code_start(TypedCode::Flash))
        .setup(|app, _, _| app.demo_typed_code(TypedCode::Flash)),
    // Code step for `authenticationCodeTypeFragment` with "Open Fragment".
    DemoSpec::signed_out("wait-code-fragment", || typed_code_start(
        TypedCode::Fragment
    ))
    .setup(|app, _, _| app.demo_typed_code(TypedCode::Fragment)),
    // Code step for `authenticationCodeTypeMissedCall`.
    DemoSpec::signed_out("wait-code-missed", || typed_code_start(TypedCode::Missed))
        .setup(|app, _, _| app.demo_typed_code(TypedCode::Missed)),
    // Sign-in polish: code step with the resend countdown and "Wrong number?".
    DemoSpec::signed_out("wait-code-resend", || {
        (
            ConnectUiStatus::DemoWaitCode,
            "screenshot demo — WaitCode with resend countdown (injected auth)".into(),
            AuthorizationState::WaitCode {
                code_length: Some(5),
                delivery: resend_delivery(),
            },
        )
    })
    .setup(QuillApp::demo_wait_code_resend),
    DemoSpec::signed_out("wait-password", || {
        (
            ConnectUiStatus::DemoWaitPassword,
            "screenshot demo — WaitPassword (injected auth, no live Telegram)".into(),
            AuthorizationState::WaitPassword {
                has_recovery_email: true,
            },
        )
    }),
    DemoSpec::signed_out("wait-phone", || {
        (
            ConnectUiStatus::DemoWaitPhone,
            "screenshot demo — WaitPhoneNumber (injected auth, no live Telegram)".into(),
            AuthorizationState::WaitPhoneNumber,
        )
    })
    .setup(|app, _, _| app.demo_phone_countries()),
    // Sign-in polish: tdesktop's banned-number box (with Help).
    DemoSpec::signed_out("wait-phone-banned", phone_step_start)
        .setup(QuillApp::demo_wait_phone_banned),
    // Sign-in polish: the country picker open with a search query.
    DemoSpec::signed_out("wait-phone-country", phone_step_start)
        .setup(QuillApp::demo_wait_phone_country),
    // Sign-in polish: a pasted international number, grouped as typed.
    DemoSpec::signed_out("wait-phone-formatted", phone_step_start)
        .setup(QuillApp::demo_wait_phone_formatted),
    DemoSpec::signed_out("wait-premium", || {
        (
            ConnectUiStatus::DemoWaitPhone,
            "screenshot demo — WaitPremiumPurchase (injected auth, no live Telegram)".into(),
            AuthorizationState::WaitPremiumPurchase {
                premium_day_count: 365,
                support_email_address: "premium-support@example.invalid".into(),
                support_email_subject: "Premium sign-in".into(),
            },
        )
    }),
    // Slice A1: injected `authorizationStateWaitOtherDeviceConfirmation`
    // with a fake link, rendered as a real QR (no live Telegram).
    DemoSpec::signed_out("wait-qr", || {
        (
            ConnectUiStatus::DemoWaitQr,
            "screenshot demo — WaitOtherDeviceConfirmation (injected auth, no live Telegram)"
                .into(),
            // Fake link for the demo QR; never touches the network.,
            AuthorizationState::WaitOtherDeviceConfirmation {
                link: "tg://login/?token=demo_qr_login_token_not_for_network".into(),
            },
        )
    }),
];

fn phone_step_start() -> (ConnectUiStatus, String, AuthorizationState) {
    (
        ConnectUiStatus::DemoWaitPhone,
        "screenshot demo — sign-in phone step (injected auth, no live Telegram)".into(),
        AuthorizationState::WaitPhoneNumber,
    )
}

/// The code-delivery types without a typed code field of their own.
#[derive(Clone, Copy, PartialEq, Eq)]
enum TypedCode {
    Firebase,
    Flash,
    Fragment,
    Missed,
}

fn typed_code_start(code: TypedCode) -> (ConnectUiStatus, String, AuthorizationState) {
    (
        ConnectUiStatus::DemoWaitCode,
        "screenshot demo — WaitCode by call/Fragment/Firebase (injected auth)".into(),
        AuthorizationState::WaitCode {
            code_length: match code {
                TypedCode::Flash => None,
                TypedCode::Missed => Some(6),
                _ => Some(5),
            },
            delivery: typed_code_delivery(code),
        },
    )
}

/// Fake `authenticationCodeInfo` for the typed-delivery code demos: a fake
/// +1 555 number and example.invalid-style hosts, never contacting anyone.
fn typed_code_delivery(code: TypedCode) -> CodeDelivery {
    let (kind, detail) = match code {
        TypedCode::Flash => (
            CodeKind::FlashCall,
            CodeDetail {
                call_number: "+155501*****".into(),
                ..CodeDetail::default()
            },
        ),
        TypedCode::Missed => (
            CodeKind::MissedCall,
            CodeDetail {
                call_number: "+155501".into(),
                missed_digits: Some(6),
                ..CodeDetail::default()
            },
        ),
        TypedCode::Fragment => (
            CodeKind::Fragment,
            CodeDetail {
                url: "https://fragment.example.invalid/login".into(),
                ..CodeDetail::default()
            },
        ),
        TypedCode::Firebase => (CodeKind::Firebase, CodeDetail::default()),
    };
    CodeDelivery {
        kind,
        next: Some(CodeKind::Sms),
        timeout_secs: 60,
        detail,
    }
}

fn resend_delivery() -> CodeDelivery {
    CodeDelivery {
        kind: CodeKind::TelegramMessage,
        next: Some(CodeKind::Sms),
        timeout_secs: 60,
        detail: Default::default(),
    }
}

/// 18 seconds into the code step's countdown.
fn code_step_started() -> std::time::Instant {
    std::time::Instant::now()
        .checked_sub(Duration::from_secs(18))
        .unwrap_or_else(std::time::Instant::now)
}

impl QuillApp {
    /// The phone step's country list and IP guess, as fixtures.
    fn demo_phone_countries(&mut self) {
        self.auth_ui.signin.demo_countries = crate::ui::signin_ui::demo_country_fixtures();
        self.auth_ui.signin.demo_guess = Some("US".into());
    }

    fn demo_wait_phone_country(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.demo_phone_countries();
        self.auth_ui.signin.picker_open = true;
        self.auth_ui
            .signin
            .search
            .update(cx, |input, cx| input.set_value("uni", window, cx));
    }

    fn demo_wait_phone_formatted(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.demo_phone_countries();
        self.auth_ui.signin.touched = true;
        self.auth_ui.signin.phone_text = "+1 555 010 0199".into();
        self.auth_ui.phone_input.update(cx, |input, cx| {
            input.set_value("+1 555 010 0199", window, cx)
        });
    }

    fn demo_wait_phone_banned(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.demo_phone_countries();
        self.auth_ui.signin.touched = true;
        self.auth_ui.signin.phone_text = "+1 555 010 0199".into();
        self.auth_ui.signin.submitted_phone = "+1 555 010 0199".into();
        self.auth_ui.phone_input.update(cx, |input, cx| {
            input.set_value("+1 555 010 0199", window, cx)
        });
        self.auth_ui.signin.demo_error = Some(quill::state::AuthRequestError {
            purpose: quill::state::RequestPurpose::SetPhoneNumber,
            class: quill::telegram::envelope::ErrorClass::PhoneBanned,
            flood_wait_secs: None,
        });
    }

    fn demo_typed_code(&mut self, code: TypedCode) {
        self.auth_ui.signin.submitted_phone = "+1 555 010 0199".into();
        self.auth_ui.signin.code_clock = Some((typed_code_delivery(code), code_step_started()));
    }

    fn demo_wait_code_resend(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        self.auth_ui.signin.submitted_phone = "+1 555 010 0199".into();
        // 18 seconds into the 60-second wait.
        self.auth_ui.signin.code_clock = Some((resend_delivery(), code_step_started()));
    }
}
