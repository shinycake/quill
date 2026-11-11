//! Generated-once classification of every `AiComposePurpose` variant.
use super::RequestKind;
use crate::state::AiComposePurpose;

impl AiComposePurpose {
    /// What this request is. Exhaustive: a new variant must be classified here.
    pub(crate) fn kind(&self) -> RequestKind {
        match self {
            Self::SearchStyle { .. } | Self::GetExample { .. } => RequestKind::Lookup,
            Self::BoxCompose { .. }
            | Self::BoxFix { .. }
            | Self::ApplyInPlace { .. }
            | Self::CreateStyle
            | Self::EditStyle
            | Self::DeleteStyle
            | Self::RemoveStyle
            | Self::AddStyle => RequestKind::UserAction,
        }
    }
}
