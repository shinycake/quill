//! Star reactions: what a chat allows.
use crate::ids::ChatId;
use crate::state::Session;
use crate::telegram::envelope::{ChatAvailableReactions, ReactionType};

impl Session {
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
