//! Generated-once classification of every `GroupsPurpose` variant.
use super::RequestKind;
use crate::state::GroupsPurpose;

impl GroupsPurpose {
    /// What this request is. Exhaustive: a new variant must be classified here.
    pub(crate) fn kind(&self) -> RequestKind {
        match self {
            Self::GroupType(inner) => inner.kind(),
            Self::CanTransferOwnership
            | Self::GetChatOwnerAfterLeaving
            | Self::GetChatMember
            | Self::GetSupergroup
            | Self::GetSupergroupFullInfo
            | Self::GetChatStatistics
            | Self::GetStatisticalGraph
            | Self::GetMessageStatistics { .. }
            | Self::GetMessagePublicForwards { .. }
            | Self::GetStatsPost { .. }
            | Self::GetChatInviteLinks
            | Self::GetChatJoinRequests
            | Self::GetMoreChatJoinRequests
            | Self::GetRevokedChatInviteLinks
            | Self::GetChatInviteLinkCounts
            | Self::GetChatInviteLinkMembers { .. }
            | Self::GetChatBoostLink
            | Self::GetChatAdministrators
            | Self::GetAdminRights { .. }
            | Self::GetDirectMessagesMember { .. }
            | Self::GetSupergroupMembers { .. }
            | Self::GetSupergroupMembersMore { .. }
            | Self::GetBasicGroupFullInfo
            | Self::GetChatEventLog { .. }
            | Self::PollChatEventLog
            | Self::GetChatBoostStatus
            | Self::GetChatBoostFeatures
            | Self::GetBoostsPerGift
            | Self::GetBoostSlotsForBoost
            | Self::LoadChatWelcomeMessages
            | Self::GetCommunityFullInfo => RequestKind::Lookup,
            Self::TransferChatOwnership { .. }
            | Self::JoinChat
            | Self::LeaveChat
            | Self::CreateChatInviteLink
            | Self::EditChatInviteLink
            | Self::RevokeChatInviteLink
            | Self::ProcessChatJoinRequest { .. }
            | Self::ProcessAllChatJoinRequests { .. }
            | Self::DeleteRevokedChatInviteLink
            | Self::DeleteAllRevokedChatInviteLinks
            | Self::ProcessLinkJoinRequests { .. }
            | Self::ToggleSupergroupUsername
            | Self::ReorderSupergroupUsernames
            | Self::SetChatMemberStatus { .. }
            | Self::SetChatSlowModeDelay
            | Self::CreateBasicGroup
            | Self::CreateSupergroupChannel { .. }
            | Self::ToggleBroadcastGroup
            | Self::AddChatMembers
            | Self::AddChatMember
            | Self::SetChatPermissions
            | Self::ReplacePrimaryChatInviteLink
            | Self::ToggleSupergroupJoinByRequest
            | Self::SetSupergroupUsername
            | Self::SetChatTitle
            | Self::SetChatDescription
            | Self::SetChatPhoto
            | Self::SetSupergroupStickerSet
            | Self::SetSupergroupCustomEmojiStickerSet
            | Self::SetChatMemberTag { .. }
            | Self::ToggleSupergroupSignMessages
            | Self::ToggleSupergroupAggressiveAntiSpam
            | Self::SetSupergroupUnrestrictBoostCount
            | Self::BoostChat
            | Self::AddChatWelcomeMessage
            | Self::EditChatWelcomeMessage { .. }
            | Self::DeleteChatWelcomeMessage { .. }
            | Self::ToggleSupergroupIsForum
            | Self::ToggleSupergroupIsAllHistoryAvailable
            | Self::ToggleSupergroupJoinToSendMessages
            | Self::ToggleSupergroupHasHiddenMembers
            | Self::ToggleSupergroupCanHaveSponsoredMessages
            | Self::ToggleChatHasProtectedContent
            | Self::SetChatAvailableReactions
            | Self::SetChatDiscussionGroup
            | Self::UpgradeBasicGroup
            | Self::SetChatMessageAutoDeleteTime
            | Self::CreateCommunity
            | Self::SetCommunityName
            | Self::SetCommunityPhoto
            | Self::SetCommunityPermissions
            | Self::DeleteCommunity
            | Self::AnswerSharingRequest { .. } => RequestKind::UserAction,
        }
    }
}
