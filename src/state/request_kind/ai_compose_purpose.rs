//! Generated-once classification of every `AiComposePurpose` variant.
use super::RequestKind;
use crate::state::AiComposePurpose;

impl crate::state::RequestPurpose {
    /// The style preview waits on these two: a lost one settles as an error
    /// the preview draws (`is_sweepable`).
    pub(crate) fn is_style_preview_lookup(self) -> bool {
        matches!(
            self,
            Self::Messages(crate::state::MessagesPurpose::AiCompose(
                AiComposePurpose::SearchStyle | AiComposePurpose::GetExample
            ))
        )
    }
}

impl AiComposePurpose {
    /// What this request is. Exhaustive: a new variant must be classified here.
    pub(crate) fn kind(&self) -> RequestKind {
        match self {
            Self::SearchStyle | Self::GetExample => RequestKind::Lookup,
            Self::BoxCompose
            | Self::BoxFix
            | Self::ApplyInPlace
            | Self::CreateStyle
            | Self::EditStyle
            | Self::DeleteStyle
            | Self::RemoveStyle
            | Self::AddStyle => RequestKind::UserAction,
        }
    }
}
