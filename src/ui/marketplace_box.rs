//! The "Buy a collectible by name" box's own state: which chat it buys
//! for, the privacy choice and the last error. The quote and the purchase
//! live in the session (`quill::marketplace::GiftPurchase`).
#![deny(clippy::indexing_slicing)]

use quill::ids::ChatId;

/// What the box holds for one chat. The chat is fixed when the box opens:
/// the Send Gift box's recipient, or the open chat from the menu. Never the
/// chat open behind it later.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct MarketplaceDraft {
    pub(super) chat: ChatId,
    /// "Comment and sender visible to the receiver only".
    pub(super) private: bool,
    pub(super) error: Option<String>,
}

impl MarketplaceDraft {
    pub(super) fn new(chat: ChatId) -> Self {
        Self {
            chat,
            private: true,
            error: None,
        }
    }
}

/// Closed, or open with its draft. A closed box keeps its draft so the box
/// of a purchase still on its way comes back as it was; a log-out, an
/// account change (the whole `PaymentUi`) or a fresh purchase drops it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) enum MarketplaceBox {
    #[default]
    Closed,
    Kept(MarketplaceDraft),
    Open(MarketplaceDraft),
}

impl MarketplaceBox {
    pub(super) fn is_open(&self) -> bool {
        matches!(self, Self::Open(_))
    }

    /// The chat of the draft, open or kept.
    pub(super) fn chat(&self) -> Option<ChatId> {
        self.draft().map(|draft| draft.chat)
    }

    pub(super) fn draft(&self) -> Option<&MarketplaceDraft> {
        match self {
            Self::Open(draft) | Self::Kept(draft) => Some(draft),
            Self::Closed => None,
        }
    }

    /// The draft of the open box.
    pub(super) fn open_draft_mut(&mut self) -> Option<&mut MarketplaceDraft> {
        match self {
            Self::Open(draft) => Some(draft),
            Self::Kept(_) | Self::Closed => None,
        }
    }

    /// Open for `chat`: with the kept draft when it is for that chat and
    /// `resume`, otherwise a fresh one.
    pub(super) fn open_for(&mut self, chat: ChatId, resume: bool) {
        let kept = match std::mem::take(self) {
            Self::Kept(draft) | Self::Open(draft) if resume && draft.chat == chat => Some(draft),
            _ => None,
        };
        *self = Self::Open(kept.unwrap_or_else(|| MarketplaceDraft::new(chat)));
    }

    /// Close the box, keeping its draft.
    pub(super) fn close(&mut self) {
        *self = match std::mem::take(self) {
            Self::Open(draft) | Self::Kept(draft) => Self::Kept(draft),
            Self::Closed => Self::Closed,
        };
    }

    /// The box and its draft go (a log-out).
    pub(super) fn forget(&mut self) {
        *self = Self::Closed;
    }

    pub(super) fn set_error(&mut self, error: Option<String>) {
        if let Some(draft) = self.open_draft_mut() {
            draft.error = error;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closing_keeps_the_draft_for_a_purchase_on_its_way() {
        let mut state = MarketplaceBox::default();
        state.open_for(ChatId(5), false);
        if let Some(draft) = state.open_draft_mut() {
            draft.private = false;
        }
        state.close();
        assert!(!state.is_open());
        assert_eq!(state.chat(), Some(ChatId(5)));
        state.open_for(ChatId(5), true);
        assert_eq!(state.draft().map(|d| d.private), Some(false));
        state.close();
        state.open_for(ChatId(5), false);
        assert_eq!(state.draft().map(|d| d.private), Some(true), "fresh");
    }

    #[test]
    fn another_chat_never_inherits_a_draft() {
        let mut state = MarketplaceBox::default();
        state.open_for(ChatId(5), false);
        state.set_error(Some("x".into()));
        state.close();
        state.open_for(ChatId(6), true);
        assert_eq!(state.draft(), Some(&MarketplaceDraft::new(ChatId(6))));
    }

    #[test]
    fn a_logout_forgets_the_box() {
        let mut state = MarketplaceBox::default();
        state.open_for(ChatId(5), false);
        state.forget();
        assert_eq!(state, MarketplaceBox::Closed);
    }
}
