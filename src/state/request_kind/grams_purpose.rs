//! Generated-once classification of every `GramsPurpose` variant.
use super::RequestKind;
use crate::premium_hub::ton::GramsPurpose;

impl GramsPurpose {
    /// What this request is. Exhaustive: a new variant must be classified here.
    pub(crate) fn kind(&self) -> RequestKind {
        match self {
            Self::Probe
            | Self::Page { .. }
            | Self::Revenue
            | Self::PeerUser
            | Self::PeerChat
            | Self::WithdrawalUrl => RequestKind::Lookup,
        }
    }
}
