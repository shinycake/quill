//! Screenshot demos of the first-run onboarding: the intro clip, the
//! experimental notice and the highlights.
#![deny(clippy::indexing_slicing)]

use super::connect_ui::ConnectUiStatus;
use super::first_run_state::{CARD_IN, FirstRun};
use super::screenshot_demo::{DemoSpec, register_demos};
use quill::telegram::envelope::AuthorizationState;
use std::time::Instant;

fn signed_out() -> (ConnectUiStatus, String, AuthorizationState) {
    (
        ConnectUiStatus::DemoWaitPhone,
        "screenshot demo — first run (no live Telegram)".into(),
        AuthorizationState::WaitPhoneNumber,
    )
}

register_demos![
    DemoSpec::signed_out("first-run-intro", signed_out).setup(|app, _, cx| {
        app.first_run = super::first_run_ui::start_intro(cx);
    }),
    DemoSpec::signed_out("first-run-notice", signed_out).setup(|app, _, _| {
        app.first_run = FirstRun::welcome(None, Instant::now());
    }),
    DemoSpec::signed_out("first-run-highlights", signed_out).setup(|app, _, _| {
        let now = Instant::now();
        app.first_run = FirstRun::Highlights {
            since: now.checked_sub(CARD_IN).unwrap_or(now),
        };
    }),
];
