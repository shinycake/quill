//! Request purposes for the TDLib 1.8.70 community methods.

/// A member of a community, as `toggleCommunityMemberIsBanned` and
/// `getCommunityMemberChats` take it (`MessageSender`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CommunityMember {
    User(i64),
    Chat(i64),
}

/// In-flight requests for communities; wrapped as
/// [`RequestPurpose::Communities`]. The community rides on
/// `PendingRequest::community_id`; the fields say what was asked, so the
/// answer lands on that ask.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommunitiesPurpose {
    /// `getJoinedCommunities`; answers `communities`.
    GetJoined,
    /// `getOwnedBots`; answers `users`.
    GetOwnedBots,
    /// `addCommunityChat`. `ok` added the chat; the error
    /// `COMMUNITY_REQUEST_CREATED` means a request went to the admins.
    AddChat { chat_id: i64, is_hidden: bool },
    /// `removeCommunityChat`; answers `ok`.
    RemoveChat { chat_id: i64 },
    /// `getCommunityAddChatRequests`; `first_page` replaces the list,
    /// a later page extends it.
    GetAddChatRequests { first_page: bool },
    /// `processCommunityAddChatRequest`; answers `ok`.
    ProcessAddChatRequest { chat_id: i64, approve: bool },
    /// `processCommunityAddChatRequests`; `count` is the number of
    /// requests the confirmation named.
    ProcessAddChatRequests { approve: bool, count: i32 },
    /// `toggleCommunityAreChatsCollapsed`; answers `ok`, the new flag
    /// arrives with `updateCommunity`.
    ToggleCollapsed { collapsed: bool },
    /// `setCommunityNotificationSettings`; the sent settings wait in
    /// `CommunitiesState::notification_sends` under the request id.
    SetNotificationSettings,
    /// `getCommunityMemberChats`; answers `communityMemberChats`.
    GetMemberChats { member: CommunityMember },
    /// `toggleCommunityMemberIsBanned`; answers `ok`.
    ToggleMemberBanned {
        member: CommunityMember,
        is_banned: bool,
    },
}
