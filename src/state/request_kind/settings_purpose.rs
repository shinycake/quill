//! Generated-once classification of every `SettingsPurpose` variant.
use super::RequestKind;
use crate::state::SettingsPurpose;

impl SettingsPurpose {
    /// What this request is. Exhaustive: a new variant must be classified here.
    pub(crate) fn kind(&self) -> RequestKind {
        match self {
            Self::Language(inner) => inner.kind(),
            Self::Passkeys(inner) => inner.kind(),
            Self::GetSavedNotificationSounds
            | Self::GetScopeNotificationSettings
            | Self::GetChatNotificationSettingsExceptions
            | Self::GetActiveSessions
            | Self::GetConnectedWebsites
            | Self::GetProxies
            | Self::GetSupportUser
            | Self::GetPrivacyRules { .. }
            | Self::GetNewChatPrivacy
            | Self::GetNetworkStatistics
            | Self::GetReadDatePrivacy
            | Self::GetBlockedSenders { .. }
            | Self::GetDefaultAutoDelete
            | Self::GetStorageStatistics
            | Self::SetOnline
            | Self::GetAutoDownloadSettingsPresets
            | Self::GetAccountTtl
            | Self::GetLanguagePackInfo
            | Self::LoadQuickReplyShortcuts
            | Self::LoadQuickReplyMessages { .. }
            | Self::GetTimeZones
            | Self::GetChatLinks
            | Self::GetConnectedBot
            | Self::LookupBusinessBot { .. }
            | Self::CanSendMessageToUser
            | Self::GetPaidMessageRevenue
            | Self::LoadDirectMessagesChat => RequestKind::Lookup,
            Self::ExportAccount
            | Self::ExportChat
            | Self::ExportRichHtml
            | Self::ExportCleanup
            | Self::SetChatNotificationSettings
            | Self::SetScopeNotificationSettings
            | Self::SetReactionNotificationSettings
            | Self::ResetAllNotificationSettings
            | Self::ConfirmDeviceLogin
            | Self::TerminateSession { .. }
            | Self::TerminateAllOtherSessions
            | Self::ToggleSessionSecretChats { .. }
            | Self::ToggleSessionCalls { .. }
            | Self::MutateProxy
            | Self::ReportWebProxyCarrier
            | Self::PingProxy { .. }
            | Self::SetPreferIpv6 { .. }
            | Self::DisconnectWebsite { .. }
            | Self::DisconnectAllWebsites
            | Self::AddSavedNotificationSound
            | Self::UploadSavedNotificationSound
            | Self::RemoveSavedNotificationSound
            | Self::SetPrivacyRules { .. }
            | Self::SetNewChatPrivacy { .. }
            | Self::SetGiftSettings
            | Self::SetInactiveSessionTtl { .. }
            | Self::SetSensitiveContent
            | Self::SetContactJoinedNotifications
            | Self::ResetNetworkStatistics
            | Self::CheckRememberedPassword
            | Self::HideCheckPasswordSuggestion
            | Self::HideSuggestedAction { .. }
            | Self::HideContactCloseBirthdays
            | Self::SetReadDatePrivacy
            | Self::SetSenderBlockList { .. }
            | Self::SetDefaultAutoDelete { .. }
            | Self::SetAutoDownloadSettings { .. }
            | Self::OptimizeStorage
            | Self::SetStorageOption
            | Self::ReviewUnconfirmedSession { .. }
            | Self::AcceptTermsOfService
            | Self::SetAccountTtl { .. }
            | Self::DeleteAccount
            | Self::MutateQuickReplies
            | Self::SaveBusinessSetting { .. }
            | Self::MutateChatLink { .. }
            | Self::SaveConnectedBot
            | Self::SetSponsoredEnabled
            | Self::AllowUnpaidMessages
            | Self::SetDirectMessagesGroup => RequestKind::UserAction,
        }
    }
}
