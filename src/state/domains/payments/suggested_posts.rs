//! Reducer for suggested posts in channel direct messages
//! (`crate::suggested_posts`): the viewer's side in each direct messages
//! group, offer state updates, the Grams balance and refused requests.
use crate::state::*;
use crate::suggested_posts::{
    BarSend, DirectMessagesRole, SuggestedPostFailure, SuggestedPostPayload, SuggestedPostPurpose,
    error_note,
};

fn suggested_purpose(pending: Option<&PendingRequest>) -> Option<SuggestedPostPurpose> {
    match pending.map(|p| p.purpose) {
        Some(RequestPurpose::Payments(PaymentsPurpose::SuggestedPost(purpose))) => Some(purpose),
        _ => None,
    }
}

impl Session {
    /// The approve or decline of this post was sent and its answer has not
    /// come: the pending request is the only record of it, so an answer, an
    /// error or a lost request frees the post.
    pub fn suggested_post_busy(&self, chat_id: i64, message_id: i64) -> bool {
        self.payments
            .suggested_posts
            .deciding
            .iter()
            .any(|decision| decision.chat_id == chat_id && decision.message_id == message_id)
    }

    /// An approve or decline of the post went out as `request`.
    pub fn decision_sent(&mut self, chat_id: i64, message_id: i64, request: RequestId) {
        self.payments
            .suggested_posts
            .deciding
            .push(crate::suggested_posts::PostDecision {
                chat_id,
                message_id,
                request,
            });
    }

    /// `request` was answered, refused or lost: its post is free.
    pub fn decision_settled(&mut self, request: RequestId) {
        self.payments
            .suggested_posts
            .deciding
            .retain(|decision| decision.request != request);
    }

    /// An `addOffer` for this message is on its way (at any price): a second
    /// submit of the box must not send another offer message.
    pub fn suggested_offer_busy(&self, chat_id: i64, message_id: i64) -> bool {
        self.requests.pending.values().any(|pending| {
            matches!(
                pending.purpose,
                RequestPurpose::Payments(PaymentsPurpose::SuggestedPost(
                    SuggestedPostPurpose::AddOffer { chat_id: c, message_id: m, .. }
                )) if c == chat_id && m == message_id
            )
        })
    }

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
            }
            SuggestedPostPayload::GramBalance(nanos) => {
                self.payments.suggested_posts.gram_balance = Some(nanos);
            }
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
            .is_some_and(|bar| bar.send().waits_for(pending.map(|p| p.id)))
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
        if let Some(request) = request {
            self.decision_settled(request);
        }
        let posts = &mut self.payments.suggested_posts;
        if let SuggestedPostPurpose::SendChange { chat_id, .. } = purpose
            && let Some(bar) = posts.bars.get_mut(&chat_id)
            && bar.send().waits_for(request)
        {
            bar.set_send(BarSend::Idle);
        }
        if err.earn_hint == Some(crate::earn::EarnErrorHint::BalanceTooLow) {
            posts.push_failure(SuggestedPostFailure::BalanceLow(purpose));
        } else {
            posts.push_failure(SuggestedPostFailure::Note(error_note(
                purpose,
                &error_reason(err),
            )));
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

    /// Takes the refused requests the UI has not shown, oldest first.
    pub fn take_suggested_post_failures(&mut self) -> Vec<SuggestedPostFailure> {
        std::mem::take(&mut self.payments.suggested_posts.failures)
            .into_iter()
            .collect()
    }
}
