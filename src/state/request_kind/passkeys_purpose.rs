//! Generated-once classification of every `PasskeysPurpose` variant.
use super::RequestKind;
use crate::passkeys::PasskeysPurpose;

impl PasskeysPurpose {
    /// What this request is. Exhaustive: a new variant must be classified here.
    pub(crate) fn kind(&self) -> RequestKind {
        match self {
            Self::List(..) | Self::Parameters | Self::LoginConfig | Self::LoginParameters => {
                RequestKind::Lookup
            }
            Self::Remove | Self::Add | Self::CheckLogin => RequestKind::UserAction,
        }
    }
}
