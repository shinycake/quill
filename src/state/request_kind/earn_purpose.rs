//! Generated-once classification of every `EarnPurpose` variant.
use super::RequestKind;
use crate::earn::EarnPurpose;

impl EarnPurpose {
    /// What this request is. Exhaustive: a new variant must be classified here.
    pub(crate) fn kind(&self) -> RequestKind {
        match self {
            Self::TonStats { .. }
            | Self::TonBalance { .. }
            | Self::TonTransactions { .. }
            | Self::StarStats { .. }
            | Self::StarTransactions { .. }
            | Self::StarBalance { .. }
            | Self::AdAccountUrl { .. }
            | Self::WithdrawalUrl { .. }
            | Self::AppConfig
            | Self::SearchPrograms { .. }
            | Self::ConnectedPrograms { .. }
            | Self::ConnectedProgram { .. }
            | Self::RecipientChats
            | Self::RecipientBots => RequestKind::Lookup,
            Self::SetProgram { .. } | Self::Connect { .. } | Self::Disconnect => {
                RequestKind::UserAction
            }
        }
    }
}
