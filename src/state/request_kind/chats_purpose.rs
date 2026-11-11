//! Generated-once classification of every `ChatsPurpose` variant.
use super::RequestKind;
use crate::state::ChatsPurpose;

impl ChatsPurpose {
    /// What this request is. Exhaustive: a new variant must be classified here.
    pub(crate) fn kind(&self) -> RequestKind {
        match self {
            Self::DeepLinkInfo { .. }
            | Self::DeepLinkInternalType { .. }
            | Self::DeepLinkResolve { .. }
            | Self::DeepLinkCheckInvite { .. }
            | Self::GetInstalledBackgrounds
            | Self::SearchBackground
            | Self::GetBackgroundUrl => RequestKind::Lookup,
            Self::DeepLinkJoin { .. }
            | Self::ReportChat
            | Self::RemoveChatActionBar
            | Self::ToggleBusinessBotPaused
            | Self::RemoveBusinessBotFromChat
            | Self::CreatePrivateChat
            | Self::EnsurePrivateChat
            | Self::SetDefaultBackground { .. }
            | Self::DeleteDefaultBackground { .. }
            | Self::RemoveInstalledBackground { .. }
            | Self::SetDefaultBackgroundLocal { .. }
            | Self::SetChatBackground
            | Self::DeleteChatBackground
            | Self::SetChatTheme
            | Self::SetChatAccentColor
            | Self::SetChatProfileAccentColor
            | Self::SetChatEmojiStatus => RequestKind::UserAction,
        }
    }
}
