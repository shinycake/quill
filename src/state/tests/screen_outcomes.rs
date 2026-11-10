//! What finished for an account while it was in the background is not
//! reported when it comes back on screen.
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::ids::AccountKey;
use crate::state::{PaymentVerification, Session};
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

    session.forget_screen_outcomes();

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
