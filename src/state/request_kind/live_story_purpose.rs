//! Generated-once classification of every `LiveStoryPurpose` variant.
use super::RequestKind;
use crate::state::LiveStoryPurpose;

impl LiveStoryPurpose {
    /// What this request is. Exhaustive: a new variant must be classified here.
    pub(crate) fn kind(&self) -> RequestKind {
        match self {
            Self::GetStreamer { .. }
            | Self::GetTopDonors { .. }
            | Self::GetMessageSenders { .. }
            | Self::GetBursts => RequestKind::Lookup,
            Self::SetMessageSender { .. }
            | Self::SendComment { .. }
            | Self::AddPendingReaction { .. }
            | Self::CommitPendingReactions { .. }
            | Self::RemovePendingReactions { .. }
            | Self::DeleteMessages { .. } => RequestKind::UserAction,
        }
    }
}
