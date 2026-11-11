//! Settling the requests of an account that is going away, without the
//! error notes they would leave behind.
use super::*;

/// The cross-cutting note lines the failure handlers of user actions write.
/// A request lost to logout is settled through those handlers so its busy
/// marker clears, but nobody asked it to fail: its note would show while the
/// account signs out, and again after the next login in this Session.
pub(super) struct UserNotes([Option<String>; 7], [Notes; 5]);

impl Session {
    /// The account is going away (logout, close): no answer can land any
    /// more, so settle every pending lookup the way a failure would and
    /// forget the stale-window marks. The settle is a plain failure of kind
    /// 408, which is no flood, so no flood notice follows.
    pub(crate) fn settle_account_requests(&mut self) {
        // Settling clears the busy markers; the failure notes it would
        // write are not the user's to see (see `logout_notes`).
        let notes = self.take_user_notes();
        for pending in self.requests.drain_all() {
            // Every request but a login submit settles: a Set / Create /
            // Toggle that was in flight holds a busy marker or a spinner
            // the Session keeps when the account signs in again.
            if pending.purpose.kind() != super::request_kind::RequestKind::Auth {
                self.settle_lost_request(pending);
            }
        }
        self.restore_user_notes(notes);
        self.stale_history_requests.clear();
    }

    pub(super) fn take_user_notes(&self) -> UserNotes {
        UserNotes(
            [
                self.chats_state.chat_action_error.clone(),
                self.payments.note.clone(),
            self.users_state.contacts_notice.clone(),
            self.groups.invite_link_error.clone(),
            self.calls.group_call_error.clone(),
            self.calls.error.clone(),
                self.settings.account_error.clone(),
            ],
            [
                self.messages.message_action_note.clone(),
                self.messages.message_link_error.clone(),
                self.messages.resend_error.clone(),
                self.messages.send_permission_error.clone(),
                self.messages.flood_notice.clone(),
            ],
        )
    }

    pub(super) fn restore_user_notes(&mut self, notes: UserNotes) {
        let UserNotes(
            [
                chat_action,
                payments,
                contacts,
                invite_link,
                group_call,
                calls,
                account,
            ],
            [message_action, message_link, resend, send_permission, flood],
        ) = notes;
        self.chats_state.chat_action_error = chat_action;
        self.messages.message_action_note = message_action;
        self.messages.message_link_error = message_link;
        self.messages.resend_error = resend;
        self.messages.send_permission_error = send_permission;
        self.messages.flood_notice = flood;
        self.payments.note = payments;
        self.users_state.contacts_notice = contacts;
        self.groups.invite_link_error = invite_link;
        self.calls.group_call_error = group_call;
        self.calls.error = calls;
        self.settings.account_error = account;
    }
}
