//! The error notes a request dropped by logout must not leave behind.
use super::*;

/// The cross-cutting note lines the failure handlers of user actions write.
/// A request lost to logout is settled through those handlers so its busy
/// marker clears, but nobody asked it to fail: its note would show while the
/// account signs out, and again after the next login in this Session.
pub(super) struct UserNotes([Option<String>; 12]);

impl Session {
    pub(super) fn take_user_notes(&self) -> UserNotes {
        UserNotes([
            self.chats_state.chat_action_error.clone(),
            self.messages.message_action_note.clone(),
            self.messages.message_link_error.clone(),
            self.messages.resend_error.clone(),
            self.messages.send_permission_error.clone(),
            self.messages.flood_notice.clone(),
            self.payments.note.clone(),
            self.users_state.contacts_notice.clone(),
            self.groups.invite_link_error.clone(),
            self.calls.group_call_error.clone(),
            self.calls.error.clone(),
            self.settings.account_error.clone(),
        ])
    }

    pub(super) fn restore_user_notes(&mut self, notes: UserNotes) {
        let [
            chat_action,
            message_action,
            message_link,
            resend,
            send_permission,
            flood,
            payments,
            contacts,
            invite_link,
            group_call,
            calls,
            account,
        ] = notes.0;
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
