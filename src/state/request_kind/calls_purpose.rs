//! Generated-once classification of every `CallsPurpose` variant.
use super::RequestKind;
use crate::state::CallsPurpose;

impl CallsPurpose {
    /// What this request is. Exhaustive: a new variant must be classified here.
    pub(crate) fn kind(&self) -> RequestKind {
        match self {
            Self::SearchCallMessages
            | Self::GetCallPrivacyRules { .. }
            | Self::GetGroupCall { .. }
            | Self::LoadGroupCallParticipants { .. }
            | Self::GetVideoChatAvailableParticipants { .. }
            | Self::GetVideoChatInviteLink { .. }
            | Self::GetVideoChatRtmpUrl { .. }
            | Self::SearchAddPeopleOnServer
            | Self::SearchAddPeoplePublic => RequestKind::Lookup,
            Self::CreateCall { .. }
            | Self::AcceptCall { .. }
            | Self::SendCallSignalingData
            | Self::DiscardCall
            | Self::SendCallRating
            | Self::SendCallDebugInformation
            | Self::DeleteAllCallMessages
            | Self::SetCallPrivacyRules { .. }
            | Self::SendCallLog
            | Self::CreateVideoChat { .. }
            | Self::JoinVideoChat { .. }
            | Self::LeaveGroupCall { .. }
            | Self::EndGroupCall { .. }
            | Self::StartGroupCallScreenSharing { .. }
            | Self::EndGroupCallScreenSharing { .. }
            | Self::CreateVideoChatJoinAs { .. }
            | Self::SetVideoChatDefaultParticipant { .. }
            | Self::SetVideoChatTitle { .. }
            | Self::RevokeVideoChatInviteLink { .. }
            | Self::StartGroupCallRecording { .. }
            | Self::EndGroupCallRecording { .. }
            | Self::StartScheduledVideoChat { .. }
            | Self::ToggleVideoChatEnabledStartNotification { .. }
            | Self::ReplaceVideoChatRtmpUrl { .. }
            | Self::SendGroupCallMessage { .. }
            | Self::ToggleGroupCallAreMessagesAllowed { .. }
            | Self::ToggleGroupCallVideo { .. }
            | Self::ToggleGroupCallScreenPaused { .. }
            | Self::ToggleGroupCallParticipantMute { .. }
            | Self::ToggleGroupCallParticipantHand { .. }
            | Self::ToggleVideoChatMuteNew { .. }
            | Self::InviteGroupCallParticipant { .. }
            | Self::InviteVideoChatParticipants { .. }
            | Self::BanGroupCallParticipants { .. }
            | Self::SetGroupCallParticipantVolumeLevel { .. }
            | Self::SetGroupCallParticipantIsSpeaking { .. }
            | Self::JoinConference
            | Self::DeclineGroupCallInvitation { .. }
            | Self::GroupCallStream { .. } => RequestKind::UserAction,
        }
    }
}
