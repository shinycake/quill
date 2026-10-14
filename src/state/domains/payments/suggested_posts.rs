//! Reducer for suggested posts in channel direct messages
//! (`crate::suggested_posts`): the viewer's side in each direct messages
//! group, offer state updates, the Grams balance and refused requests.
use crate::state::*;
use crate::suggested_posts::{
    DirectMessagesRole, SuggestedPostPayload, SuggestedPostPurpose, error_note,
};

fn suggested_purpose(pending: Option<&PendingRequest>) -> Option<SuggestedPostPurpose> {
    match pending.map(|p| p.purpose) {
        Some(RequestPurpose::Payments(PaymentsPurpose::SuggestedPost(purpose))) => Some(purpose),
        _ => None,
    }
}

impl Session {
    pub(crate) fn apply_suggested_post(&mut self, payload: SuggestedPostPayload) {
        match payload {
            SuggestedPostPayload::InfoUpdated {
                chat_id,
                message_id,
                info,
            } => {
                self.edit_loaded_message(chat_id, message_id, |message| {
                    message.extras.suggested_post = info;
                });
                self.payments
                    .suggested_posts
                    .busy
                    .remove(&(chat_id.0, message_id.0));
            }
            SuggestedPostPayload::GramBalance(nanos) => {
                self.payments.suggested_posts.gram_balance = Some(nanos);
            }
        }
    }

    /// `ok` for an approve or decline: the state update follows, so the
    /// buttons only stop waiting.
    pub(crate) fn apply_suggested_post_ok(&mut self, pending: Option<&PendingRequest>) {
        if let Some(purpose) = suggested_purpose(pending) {
            self.payments.suggested_posts.busy.remove(&purpose.key());
        }
    }

    pub(crate) fn apply_suggested_post_error(
        &mut self,
        err: &TdError,
        purpose: SuggestedPostPurpose,
    ) {
        let posts = &mut self.payments.suggested_posts;
        posts.busy.remove(&purpose.key());
        posts.notes.push(error_note(purpose, &error_reason(err)));
    }

    pub(crate) fn set_direct_messages_role(
        &mut self,
        supergroup_id: i64,
        role: Option<DirectMessagesRole>,
    ) {
        let roles = &mut self.groups.supergroup_direct_messages;
        match role {
            Some(role) => {
                roles.insert(supergroup_id, role);
            }
            None => {
                roles.remove(&supergroup_id);
            }
        }
    }

    /// The viewer's side when `chat_id` is a channel's direct messages
    /// group; `None` for any other chat.
    pub fn direct_messages_role(&self, chat_id: ChatId) -> Option<DirectMessagesRole> {
        let supergroup_id = self.chat_supergroup(chat_id)?;
        self.groups
            .supergroup_direct_messages
            .get(&supergroup_id)
            .copied()
    }

    /// The suggest bar of `chat_id`, if one is up.
    pub fn suggest_bar(&self, chat_id: ChatId) -> Option<crate::suggested_posts::SuggestBar> {
        self.payments.suggested_posts.bar(chat_id).copied()
    }

    /// Changes one message's offer in every loaded copy (the demo, which
    /// has no TDLib to answer a decision).
    pub fn edit_suggested_post(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        mut edit: impl FnMut(&mut Option<crate::suggested_posts::SuggestedPostInfo>),
    ) {
        self.edit_loaded_message(chat_id, message_id, |message| {
            edit(&mut message.extras.suggested_post);
        });
    }

    /// Takes the one-shot error notes for the UI's toasts.
    pub fn take_suggested_post_notes(&mut self) -> Vec<String> {
        std::mem::take(&mut self.payments.suggested_posts.notes)
    }
}
