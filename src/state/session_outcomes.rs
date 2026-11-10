//! One-shot outcomes the UI drains once (a payment's verification page, a
//! copied message link, a forward's answer, a refused action's note). An
//! answer that lands while its account is in the background has no screen
//! to show it: it waits in the session, and the account's next time on
//! screen would drain it as news (a bank page opening, the clipboard
//! overwritten, an old error minutes late).
use super::*;

impl Session {
    /// The account comes back on screen: what finished while it was away
    /// is not reported on the screen it comes back to.
    pub fn forget_screen_outcomes(&mut self) {
        self.payments.verification_url = None;
        let messages = &mut self.messages;
        messages.message_link_result = None;
        messages.message_link_error = None;
        messages.last_forward = None;
        messages.recognize_speech_error = None;
        messages.ai_error = None;
        messages.resend_error = None;
        messages.message_action_note = None;
        self.groups.community_error = None;
        self.groups.invite_link_error = None;
        self.chats_state.chat_action_error = None;
        self.chats_state.report_chat_outcome = None;
    }
}
