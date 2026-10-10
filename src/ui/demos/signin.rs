//! Screenshot demos: sign-in and connection screens (injected auth states,
//! no live Telegram).

use crate::ui::app::QuillApp;
use crate::ui::connect_ui::ConnectUiStatus;
use crate::ui::screenshot_demo::{DemoSpec, register_demos};
use quill::connect::ConnectBlocker;
use quill::telegram::envelope::AuthorizationState;

register_demos![
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
];

impl QuillApp {
    /// The phone step's country list and IP guess, as fixtures.
    fn demo_phone_countries(&mut self) {
        self.auth_ui.signin.demo_countries = crate::ui::signin_ui::demo_country_fixtures();
        self.auth_ui.signin.demo_guess = Some("US".into());
    }
}
