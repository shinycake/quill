//! Generated-once classification of every `UsersPurpose` variant.
use super::RequestKind;
use crate::state::UsersPurpose;

impl UsersPurpose {
    /// What this request is. Exhaustive: a new variant must be classified here.
    pub(crate) fn kind(&self) -> RequestKind {
        match self {
            Self::GetChatMessageCount { .. }
            | Self::OpenChatSimilarChat
            | Self::GetMe
            | Self::GetUserFullInfo
            | Self::GetContacts
            | Self::GetBackgroundEmojiIcons
            | Self::GetProfilePhotoEmojiIcons
            | Self::GetChatPhotoEmojiIcons
            | Self::GetUserProfilePhotos
            | Self::GetChatPhotoHistory
            | Self::GetSecretChat
            | Self::GetProfileShareThemeBackdrops
            | Self::GetUserLink { .. }
            | Self::GetShareThemeGifts { .. } => RequestKind::Lookup,
            Self::AddContact
            | Self::SetName
            | Self::SetBio
            | Self::SetUsername
            | Self::CheckUsername
            | Self::ReorderActiveUsernames
            | Self::ToggleUsernameIsActive
            | Self::SetProfilePhoto
            | Self::DeleteProfilePhoto
            | Self::SetProfileAccentColor
            | Self::SetAccentColor
            | Self::SetUpgradedGiftColors
            | Self::RemoveContact
            | Self::ImportContacts
            | Self::ClearImportedContacts
            | Self::AddContactByPhone
            | Self::SharePhoneNumber
            | Self::SetMessageSenderBlockList { .. }
            | Self::BlockSenderFromReplies
            | Self::SetBirthdate
            | Self::SetMainProfileTab
            | Self::SetPersonalChat
            | Self::SetUserNote
            | Self::SetUserPersonalPhoto
            | Self::ReportChatPhoto
            | Self::CreateNewSecretChat
            | Self::CloseSecretChat
            | Self::SetProfileShareTheme { .. } => RequestKind::UserAction,
        }
    }
}
