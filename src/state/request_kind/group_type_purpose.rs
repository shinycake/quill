//! Generated-once classification of every `GroupTypePurpose` variant.
use super::RequestKind;
use crate::state::GroupTypePurpose;

impl GroupTypePurpose {
    /// What this request is. Exhaustive: a new variant must be classified here.
    pub(crate) fn kind(&self) -> RequestKind {
        match self {
            Self::GetCreatedPublicChats => RequestKind::Lookup,
            Self::CheckChatUsername { .. }
            | Self::CheckPublicChatsLimit
            | Self::DisableAllUsernames => RequestKind::UserAction,
        }
    }
}
