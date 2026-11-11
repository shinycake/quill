//! Generated-once classification of every `CommunitiesPurpose` variant.
use super::RequestKind;
use crate::state::CommunitiesPurpose;

impl CommunitiesPurpose {
    /// What this request is. Exhaustive: a new variant must be classified here.
    pub(crate) fn kind(&self) -> RequestKind {
        match self {
            Self::GetJoined
            | Self::GetOwnedBots
            | Self::GetAddChatRequests { .. }
            | Self::GetMemberChats { .. } => RequestKind::Lookup,
            Self::AddChat { .. }
            | Self::RemoveChat { .. }
            | Self::ProcessAddChatRequest { .. }
            | Self::ProcessAddChatRequests { .. }
            | Self::ToggleCollapsed { .. }
            | Self::SetNotificationSettings
            | Self::ToggleMemberBanned { .. } => RequestKind::UserAction,
        }
    }
}
