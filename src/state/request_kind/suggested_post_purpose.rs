//! Generated-once classification of every `SuggestedPostPurpose` variant.
use super::RequestKind;
use crate::suggested_posts::SuggestedPostPurpose;

impl SuggestedPostPurpose {
    /// What this request is. Exhaustive: a new variant must be classified here.
    pub(crate) fn kind(&self) -> RequestKind {
        match self {
            Self::Approve { .. }
            | Self::Decline { .. }
            | Self::AddOffer { .. }
            | Self::SendChange { .. } => RequestKind::UserAction,
        }
    }
}
