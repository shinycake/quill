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
        self.payments.receipt_error = None;
        self.payments.unlocked_notice = None;
        // The earn toasts, a Fragment URL and a setup page's "go back"
        // belong to the screen that asked.
        let earn = &mut self.payments.earn;
        earn.notes.clear();
        earn.open_url = None;
        earn.setup_closed = false;
        self.bots.last_callback_answer = None;
        // The consent box and the raw-URL fallback of a login button
        // answered while away: its request goes with its answer. A request
        // still in flight keeps its context for the answer to come.
        if self.bots.last_login_url_info.take().is_some() {
            self.bots.login_url_request = None;
        }
        let messages = &mut self.messages;
        messages.message_link_result = None;
        messages.message_link_error.clear();
        messages.last_forward = None;
        messages.recognize_speech_error.clear();
        messages.ai_error.clear();
        messages.resend_error.clear();
        messages.message_action_note.clear();
        messages.send_permission_error.clear();
        messages.flood_notice.clear();
        self.groups.community_error = None;
        self.groups.invite_link_error = None;
        self.chats_state.chat_action_error.clear();
        self.chats_state.report_chat_outcome = None;
    }
}

impl RequestRegistry {
    /// The chosen marks the newest `setPollAnswer` still in flight for this
    /// poll row put on screen. An `updatePoll` for an older answer can put
    /// older marks back while it flies, so the next tap builds on these.
    pub fn poll_vote_in_flight(&self, chat_id: ChatId, message_id: MessageId) -> Option<&[bool]> {
        self.pending
            .values()
            .filter_map(|p| match &p.rollback {
                Some(RequestRollback::PollVote {
                    chat_id: chat,
                    message_id: message,
                    shown,
                    ..
                }) if *chat == chat_id.0 && *message == message_id.0 => Some((p.id.0, shown)),
                _ => None,
            })
            .max_by_key(|(id, _)| *id)
            .map(|(_, shown)| shown.as_slice())
    }
}
