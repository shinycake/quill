//! Generated-once classification of every `StickerCreatorPurpose` variant.
use super::RequestKind;
use crate::state::StickerCreatorPurpose;

impl StickerCreatorPurpose {
    /// What this request is. Exhaustive: a new variant must be classified here.
    pub(crate) fn kind(&self) -> RequestKind {
        match self {
            Self::AddNewSticker { .. }
            | Self::AddExistingSticker { .. }
            | Self::RemoveSticker { .. }
            | Self::MoveSticker { .. }
            | Self::RenameSet { .. }
            | Self::DeleteSet { .. } => RequestKind::UserAction,
        }
    }
}
