//! Star reactions: what a chat allows.
use crate::ids::ChatId;
use crate::state::{PaymentsPurpose, PendingRequest, RequestPurpose, Session};
use crate::telegram::envelope::{ChatAvailableReactions, ReactionType};

impl Session {
    /// `ok` for a commit or an Undo: those Stars are settled with TDLib.
    pub(crate) fn apply_paid_reaction_ok(&mut self, pending: Option<&PendingRequest>) {
        if let Some(p) = pending
            && matches!(
                p.purpose,
                RequestPurpose::Payments(
                    PaymentsPurpose::CommitPaidReaction { .. }
                        | PaymentsPurpose::RemovePendingPaidReaction { .. }
                )
            )
        {
            self.payments.paid.settled(p.id);
        }
    }

    /// Whether the chat lists the Star reaction among its allowed ones
    /// (tdesktop `allowedReactions().paidEnabled`). `chatAvailableReactionsAll`
    /// never includes it.
    pub fn paid_reactions_enabled(&self, chat_id: ChatId) -> bool {
        matches!(
            self.chat_available_reactions(chat_id),
            Some(ChatAvailableReactions::Some { reactions, .. })
                if reactions.contains(&ReactionType::Paid)
        )
    }
}
