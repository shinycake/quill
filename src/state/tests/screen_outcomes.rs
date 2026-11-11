//! What finished for an account while it was in the background is not
//! reported when it comes back on screen.
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::ids::{AccountKey, ChatId, MessageId};
use crate::state::{CallbackAnswer, LoginUrlRequest};
use crate::state::{PaymentVerification, Session};
use crate::telegram::envelope::{CallbackQueryAnswer, LoginUrlInfo};
use std::sync::Arc;

#[test]
fn outcomes_that_landed_in_the_background_are_not_drained_later() {
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let mut session = Session::new(AccountKey::primary(), sink);
    session.payments.verification_url = Some(PaymentVerification {
        url: "https://bank.example/3ds".into(),
        closes_checkout: true,
    });
    session.messages.message_link_result = Some("https://t.me/c/1/2".into());
    session.messages.message_link_error = Some("no link".into());
    session.messages.resend_error = Some("resend".into());
    session.messages.ai_error = Some("ai".into());
    session.messages.message_action_note = Some("banned".into());
    session.chats_state.chat_action_error = Some("pin".into());
    session.chats_state.report_chat_outcome = Some("chat reported".into());
    session.groups.invite_link_error = Some("link".into());

    session.payments.receipt_error = Some("receipt".into());
    session.payments.unlocked_notice = Some("unlocked".into());
    session.messages.send_permission_error = Some("no rights".into());
    session.messages.flood_notice = Some("slow down".into());
    session.bots.last_callback_answer = Some(CallbackAnswer {
        answer: CallbackQueryAnswer {
            text: String::new(),
            show_alert: false,
            url: "https://bot.example/game".into(),
        },
        chat: None,
    });
    session.bots.last_login_url_info = Some(LoginUrlInfo::Open {
        url: "https://site.example".into(),
    });
    session.bots.login_url_request = Some(LoginUrlRequest {
        chat_id: ChatId(1),
        message_id: MessageId(2),
        button_id: 3,
        raw_url: "https://site.example".into(),
        from_agent: false,
    });

    session.forget_screen_outcomes();

    assert!(session.payments.receipt_error.is_none());
    assert!(session.payments.unlocked_notice.is_none());
    assert!(session.messages.send_permission_error.is_none());
    assert!(session.messages.flood_notice.is_none());
    assert!(session.bots.last_callback_answer.is_none());
    assert!(session.bots.last_login_url_info.is_none());
    assert!(session.bots.login_url_request.is_none());

    assert!(session.payments.verification_url.is_none());
    assert!(session.messages.message_link_result.is_none());
    assert!(session.messages.message_link_error.is_none());
    assert!(session.messages.resend_error.is_none());
    assert!(session.messages.ai_error.is_none());
    assert!(session.messages.message_action_note.is_none());
    assert!(session.chats_state.chat_action_error.is_none());
    assert!(session.chats_state.report_chat_outcome.is_none());
    assert!(session.groups.invite_link_error.is_none());
}

/// An answer whose request is still in flight keeps its context, so the
/// answer that comes later still degrades to the plain URL.
#[test]
fn a_login_request_in_flight_survives_the_return() {
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let mut session = Session::new(AccountKey::primary(), sink);
    session.bots.login_url_request = Some(LoginUrlRequest {
        chat_id: ChatId(1),
        message_id: MessageId(2),
        button_id: 3,
        raw_url: "https://site.example".into(),
        from_agent: false,
    });
    session.forget_screen_outcomes();
    assert!(session.bots.login_url_request.is_some());
}

/// Every outcome `poll_drains` takes from the session is one the return
/// to the screen forgets (or is listed here with the reason it is not):
/// a new drained outcome that is not forgotten would show late.
#[test]
fn every_drained_outcome_is_forgotten_on_return() {
    let drains = include_str!("../../ui/notifications/poll_drains.rs");
    let forgets = include_str!("../session_outcomes.rs");
    // Not an outcome of the screen: TDLib refused the log-out, which the
    // account's own sign-in state keeps.
    let exempt = ["logout_refused"];
    let mut missing = Vec::new();
    for part in drains.split("driver.session.").skip(1) {
        let path: String = part
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '.')
            .collect();
        if !part
            .get(path.len()..)
            .is_some_and(|rest| rest.starts_with("take()"))
        {
            continue;
        }
        let Some(field) = path.rsplit('.').next() else {
            continue;
        };
        if !exempt.contains(&field) && !forgets.contains(&format!(".{field}")) {
            missing.push(field.to_string());
        }
    }
    assert!(missing.is_empty(), "drained but not forgotten: {missing:?}");
}
