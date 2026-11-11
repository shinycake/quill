//! Generated-once classification of every `LanguagePurpose` variant.
use super::RequestKind;
use crate::state::LanguagePurpose;

impl LanguagePurpose {
    /// What this request is. Exhaustive: a new variant must be classified here.
    pub(crate) fn kind(&self) -> RequestKind {
        match self {
            Self::TargetInfo | Self::Strings { .. } | Self::PsaLabels { .. } => RequestKind::Lookup,
            Self::SetOption | Self::Synchronize => RequestKind::UserAction,
        }
    }
}
