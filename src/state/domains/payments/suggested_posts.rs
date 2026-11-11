//! Reducer for suggested posts in channel direct messages
//! (`crate::suggested_posts`): the viewer's side in each direct messages
//! group, offer state updates, the Grams balance and refused requests.
use crate::state::*;
use crate::suggested_posts::{
    BarSend, DirectMessagesRole, SuggestedPostPayload, SuggestedPostPurpose, error_note,
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
                // `busy` is the approve or decline's own: only its answer
                // (or error) clears it, not an offer update that lands first.
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
            self.payments.suggested_posts.settle_busy(purpose);
        }
    }

    /// The `message` answer of an edit's send: the bar's terms rode it, so
    /// the bar goes (when it is the bar that sent).
    pub(crate) fn settle_suggested_send(&mut self, pending: Option<&PendingRequest>) {
        let Some(SuggestedPostPurpose::SendChange { chat_id, .. }) = suggested_purpose(pending)
        else {
            return;
        };
        let bars = &mut self.payments.suggested_posts.bars;
        if bars
            .get(&chat_id)
            .is_some_and(|bar| bar.send.waits_for(pending.map(|p| p.id)))
        {
            bars.remove(&chat_id);
        }
    }

    pub(crate) fn apply_suggested_post_error(
        &mut self,
        err: &TdError,
        purpose: SuggestedPostPurpose,
        request: Option<RequestId>,
    ) {
        let posts = &mut self.payments.suggested_posts;
        posts.settle_busy(purpose);
        if let SuggestedPostPurpose::SendChange { chat_id, .. } = purpose
            && let Some(bar) = posts.bars.get_mut(&chat_id)
            && bar.send.waits_for(request)
        {
            bar.send = BarSend::Idle;
        }
        if err.earn_hint == Some(crate::earn::EarnErrorHint::BalanceTooLow) {
            posts.balance_low.push(purpose);
        } else {
            posts.notes.push(error_note(purpose, &error_reason(err)));
        }
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

    /// Takes the requests refused for a low balance.
    pub fn take_suggested_post_balance_low(&mut self) -> Vec<SuggestedPostPurpose> {
        std::mem::take(&mut self.payments.suggested_posts.balance_low)
    }

    /// Takes the one-shot error notes for the UI's toasts.
    pub fn take_suggested_post_notes(&mut self) -> Vec<String> {
        std::mem::take(&mut self.payments.suggested_posts.notes)
    }
}
