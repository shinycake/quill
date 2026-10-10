//! Request purposes for groups and channels: members, admin rights, invite links, join requests, boosts, communities.
use crate::state::request_purpose::flat_purposes;
use crate::state::*;

/// In-flight requests for groups and channels: members, admin rights, invite links, join requests, boosts, communities; wrapped as
/// [`RequestPurpose::Groups`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupsPurpose {
    /// `canTransferOwnership`: the 2-step-verification / session-age gate
    /// before a transfer. Answered into `Session::ownership`.
    CanTransferOwnership,
    /// `transferChatOwnership` to `user_id`. Response is `ok`; the
    /// password never rides the purpose.
    TransferChatOwnership {
        user_id: i64,
    },
    /// `getChatOwnerAfterLeaving`: who inherits the chat when the owner
    /// leaves. Response is a `user`; correlated via the chat id.
    GetChatOwnerAfterLeaving,
    /// `getChatMember` for the current user in a channel. Response is
    /// `chatMember`; drives the composer gate and join/leave affordance.
    GetChatMember,
    /// `joinChat`. Response is `ChatJoinResult`; own status also arrives via
    /// `updateChatMember`.
    JoinChat,
    /// `leaveChat`. Response is `ok`; own status also arrives via
    /// `updateChatMember`.
    LeaveChat,
    /// Phase 5.1: `getSupergroup`. Response is `supergroup`; resolves
    /// `ChatSummary::is_forum`.
    GetSupergroup,
    /// Phase 6: `getSupergroupFullInfo`. Response is `supergroupFullInfo`;
    /// correlated via `PendingRequest::supergroup_id`.
    GetSupergroupFullInfo,
    /// Phase D2: `getChatStatistics`. Response is
    /// `chatStatisticsChannel` / `chatStatisticsSupergroup`; correlated
    /// via `PendingRequest::chat_id` (the response carries no chat id).
    GetChatStatistics,
    /// Phase D3a: `getChatInviteLinks`. Response is `chatInviteLinks`;
    /// correlated via `PendingRequest::chat_id`.
    GetChatInviteLinks,
    /// Phase D3a: `createChatInviteLink`. Response is the created
    /// `chatInviteLink`; correlated via `PendingRequest::chat_id`.
    CreateChatInviteLink,
    /// Phase D3a: `editChatInviteLink`. Response is the updated
    /// `chatInviteLink`; correlated via `PendingRequest::chat_id`.
    EditChatInviteLink,
    /// Phase D3a: `revokeChatInviteLink`. Response is the updated
    /// `chatInviteLinks` list; correlated via `PendingRequest::chat_id`.
    RevokeChatInviteLink,
    /// Phase D3a: `getChatJoinRequests`. Response is `chatJoinRequests`;
    /// correlated via `PendingRequest::chat_id`.
    GetChatJoinRequests,
    /// Phase D3a: `processChatJoinRequest`. Response is `ok`; `user_id`
    /// identifies the join request that was approved/declined.
    ProcessChatJoinRequest {
        user_id: i64,
    },
    /// B8: next page of `getChatJoinRequests`; the answer is appended.
    GetMoreChatJoinRequests,
    /// B8: `processChatJoinRequests` for every pending request.
    ProcessAllChatJoinRequests {
        approve: bool,
    },
    /// B8: `getChatInviteLinks` with `is_revoked = true`.
    GetRevokedChatInviteLinks,
    /// B8: `getChatInviteLinkCounts`. Response is `chatInviteLinkCounts`.
    GetChatInviteLinkCounts,
    /// B8: `getChatInviteLinkMembers`; `append` marks a later page.
    GetChatInviteLinkMembers {
        append: bool,
    },
    /// B8: `deleteRevokedChatInviteLink`; the link is looked up in
    /// `Session::revoked_link_deletions` by request id.
    DeleteRevokedChatInviteLink,
    /// B8: `deleteAllRevokedChatInviteLinks`.
    DeleteAllRevokedChatInviteLinks,
    /// `processChatJoinRequests` for one invite link.
    ProcessLinkJoinRequests {
        approve: bool,
    },
    /// `getChatBoostLink`.
    GetChatBoostLink,
    /// `toggleSupergroupUsernameIsActive`. Response `ok`; the new lists
    /// arrive with `updateSupergroup`.
    ToggleSupergroupUsername,
    /// `reorderSupergroupActiveUsernames`. Same follow-up.
    ReorderSupergroupUsernames,
    /// Phase D3b: `getChatAdministrators`. Response is
    /// `chatAdministrators`; correlated via `PendingRequest::chat_id`.
    GetChatAdministrators,
    /// Phase D3b: `setChatMemberStatus`. Response is `ok`; the member
    /// change itself arrives as `updateChatMember`. `user_id` + `kind`
    /// identify which admin-management operation was confirmed.
    SetChatMemberStatus {
        user_id: i64,
        kind: MemberStatusChange,
    },
    /// Phase D3b: `getChatMember` for one administrator's rights (edit
    /// dialog). Response is `chatMember`; `user_id` identifies the admin,
    /// correlated to the chat via `PendingRequest::chat_id`.
    GetAdminRights {
        user_id: i64,
    },
    /// Phase D3b / slice G1: `getSupergroupMembers` for the promote
    /// member picker and the member-management dialog. Response is
    /// `chatMembers`; correlated via `PendingRequest::chat_id`, cached
    /// per (`chat_id`, `filter`).
    GetSupergroupMembers {
        filter: MemberListFilter,
    },
    /// Slice G1: `getBasicGroupFullInfo` (schema 1.8.67, line 11507).
    /// Response is `basicGroupFullInfo`; correlated via
    /// `PendingRequest::chat_id`.
    GetBasicGroupFullInfo,
    /// Phase D3c: `getChatEventLog`. Response is `chatEvents`; correlated
    /// via `PendingRequest::chat_id`. `from_event_id` is the paging
    /// cursor: 0 replaces the cached page, a nonzero id appends the
    /// older page to it.
    GetChatEventLog {
        from_event_id: i64,
    },
    /// Phase A1: `setChatSlowModeDelay`. Response is `ok`; the new delay
    /// arrives via `updateSupergroupFullInfo`.
    SetChatSlowModeDelay,
    /// Slice G1: `createNewBasicGroupChat` (schema 1.8.67, line 13327).
    /// Response is `createdBasicGroupChat`; the chat itself arrives as
    /// `updateNewChat`.
    CreateBasicGroup,
    /// Slice G1: `createNewSupergroupChat` (schema 1.8.67, line 13337).
    /// Response is the new `chat`; `updateNewChat` follows as well.
    CreateSupergroupChannel {
        is_channel: bool,
    },
    /// Slice G1: `toggleSupergroupIsBroadcastGroup` (schema 1.8.67, line
    /// 15221). Response is `ok`; `updateSupergroup` carries the new
    /// `is_broadcast_group`. One-way: supergroup → broadcast group.
    ToggleBroadcastGroup,
    /// Slice G1: `addChatMembers` (schema 1.8.67, line 13584). Response is
    /// `failedToAddMembers`; added members arrive as `updateChatMember`.
    /// The single bulk response replaces the failure count.
    AddChatMembers,
    /// Slice G1 fix-up: one `addChatMember` per user for basic groups
    /// (schema 1.8.67, line 13578 — also answers `failedToAddMembers`,
    /// 0 or 1 failures each). Per-user responses accumulate into the
    /// failure count instead of replacing it.
    AddChatMember,
    /// Slice G1: `setChatPermissions` (schema 1.8.67, line 13464).
    /// Response is `ok`; `updateChatPermissions` carries the new block.
    /// Applied optimistically by the driver at send time; a TDLib error
    /// restores the previous block via `PendingRequest::rollback`.
    SetChatPermissions,
    /// Slice G1: `replacePrimaryChatInviteLink` (schema 1.8.67, line
    /// 14089). Response is the new `chatInviteLink`; correlated via
    /// `PendingRequest::chat_id`.
    ReplacePrimaryChatInviteLink,
    /// Slice G1: `toggleSupergroupJoinByRequest` (schema 1.8.67, line
    /// 15188). Response is `ok`; `updateSupergroup` carries the new
    /// `join_by_request`. Applied optimistically by the driver at send
    /// time; a TDLib error restores the previous flag via
    /// `PendingRequest::rollback`.
    ToggleSupergroupJoinByRequest,
    /// Slice G1: `setSupergroupUsername` (schema 1.8.67, line 15136).
    /// Response is `ok`; `updateSupergroup` carries the new username.
    /// Applied optimistically by the driver at send time; a TDLib error
    /// restores the previous username via `PendingRequest::rollback`.
    SetSupergroupUsername,
    /// Slice: group/channel title edit — `setChatTitle` (schema 1.8.67,
    /// line 13430). Response is `ok`; `updateChatTitle` carries the new
    /// title. No optimistic state: the update arrives from the server.
    SetChatTitle,
    /// Slice: group/channel description edit — `setChatDescription`
    /// (schema 1.8.67, line 13533). Response is `ok`; TDLib has no
    /// `updateChatDescription` broadcast, so the new description arrives
    /// on the next full-info pull. No optimistic state.
    SetChatDescription,
    /// Slice: group/channel photo edit — `setChatPhoto` (schema 1.8.67,
    /// line 13435). Response is `ok`; `updateChatPhoto` carries the new
    /// photo. No optimistic state: the update arrives from the server.
    SetChatPhoto,
    /// Slice S11: `setSupergroupStickerSet` (schema 1.8.67, line 15154).
    /// Response is `ok`; `updateSupergroupFullInfo` carries the new
    /// `sticker_set_id`. No optimistic state: the update arrives from
    /// the server. Driver validates the id client-side (negative is
    /// refused); 0 removes the group sticker set per the schema.
    SetSupergroupStickerSet,
    /// Slice S11: `setSupergroupCustomEmojiStickerSet` (schema 1.8.67,
    /// line 15159). Response is `ok`; `updateSupergroupFullInfo` carries
    /// the new `custom_emoji_sticker_set_id`. No optimistic state.
    SetSupergroupCustomEmojiStickerSet,
    /// Slice G1: `setChatMemberTag` (schema 1.8.67, line 13598) — the
    /// admin custom-title setter (Telegram X `EditRightsController`
    /// drives the "Custom title" field through it). Response is `ok`;
    /// member caches are invalidated so the new tag is refetched.
    SetChatMemberTag {
        user_id: i64,
    },
    /// Slice G2: `toggleSupergroupSignMessages` (schema 1.8.67, line
    /// 15175). Response is `ok`; `updateSupergroup` carries the new
    /// `sign_messages` / `show_message_sender`. Applied optimistically;
    /// a TDLib error restores the previous flags via
    /// `PendingRequest::rollback`.
    ToggleSupergroupSignMessages,
    /// Slice G2: `toggleSupergroupHasAggressiveAntiSpamEnabled` (schema
    /// 1.8.67, line 15212). Response is `ok`;
    /// `updateSupergroupFullInfo` carries the new
    /// `has_aggressive_anti_spam_enabled`. Applied optimistically; a
    /// TDLib error restores the previous flag via
    /// `PendingRequest::rollback`.
    ToggleSupergroupAggressiveAntiSpam,
    /// Slice G2: `getChatBoostStatus` (schema 1.8.67, line 13917).
    /// Response is `chatBoostStatus`; correlated via
    /// `PendingRequest::chat_id`.
    GetChatBoostStatus,
    /// Slice G2: `getAvailableChatBoostSlots` as the first half of the
    /// boost action (schema 1.8.67, line 13914). Response is
    /// `chatBoostSlots`; the driver chains `boostChat` with the first
    /// slot id. Correlated via `PendingRequest::chat_id`.
    GetBoostSlotsForBoost,
    /// Slice G2: `boostChat` (schema 1.8.67, line 13922). Response is
    /// `chatBoostSlots`; the boost status is refetched afterwards.
    BoostChat,
    /// Slice G2: `loadChatWelcomeMessages` (schema 1.8.67, line 12630).
    /// The pack arrives as `updateChatWelcomeMessages`.
    LoadChatWelcomeMessages,
    /// Slice G2: `addChatWelcomeMessage` / `editChatWelcomeMessage` /
    /// `deleteChatWelcomeMessage` (schema 1.8.67, lines 12639/12646/
    /// 12651). Responses are `ok`; the pack is reloaded on success.
    AddChatWelcomeMessage,
    EditChatWelcomeMessage {
        welcome_message_id: i32,
    },
    DeleteChatWelcomeMessage {
        welcome_message_id: i32,
    },
    /// B7: `toggleSupergroupIsForum` (line 15218). Response `ok`; the new
    /// `is_forum` arrives via `updateSupergroup`.
    ToggleSupergroupIsForum,
    /// B7: `toggleSupergroupIsAllHistoryAvailable` (line 15191). Applied
    /// optimistically; rolled back on error.
    ToggleSupergroupIsAllHistoryAvailable,
    /// B7: `toggleSupergroupJoinToSendMessages` (line 15180). Optimistic.
    ToggleSupergroupJoinToSendMessages,
    /// B7: `toggleSupergroupHasHiddenMembers` (line 15207). Optimistic.
    ToggleSupergroupHasHiddenMembers,
    /// B7: `toggleChatHasProtectedContent` (line 13504). Optimistic.
    ToggleChatHasProtectedContent,
    /// B7: `setChatAvailableReactions` (line 13527). Optimistic.
    SetChatAvailableReactions,
    /// B7: `setChatDiscussionGroup` (line 13539). Response `ok`; the new
    /// link arrives via `updateSupergroupFullInfo`.
    SetChatDiscussionGroup,
    /// B7: `upgradeBasicGroupChatToSupergroupChat` (line 13343). The
    /// answer is the new supergroup `chat`; pending `chat_id` is the old
    /// basic group chat.
    UpgradeBasicGroup,
    /// Phase B4: `setChatMessageAutoDeleteTime`. Response is `ok`; the
    /// new timer arrives as `updateChatMessageAutoDeleteTime` (plus a
    /// `messageChatSetMessageAutoDeleteTime` service message in history).
    SetChatMessageAutoDeleteTime,
    /// Slice (communities backend core): `createCommunity` (schema 1.8.67,
    /// line 11806). Response is `communityId`; the driver chains it into
    /// `getCommunityFullInfo`.
    CreateCommunity,
    /// `getCommunityFullInfo` (TDLib 1.8.68; replaced
    /// `loadCommunityFullInfo`). Response is `communityFullInfo`,
    /// correlated through `PendingRequest::community_id`.
    GetCommunityFullInfo,
    /// Slice (communities backend core): `setCommunityName` (schema 1.8.67,
    /// line 11811). Response is `ok`; the pack is reloaded on success and
    /// the new name arrives via `updateCommunity`.
    SetCommunityName,
    /// `setCommunityPhoto` (TDLib 1.8.68). Response is `ok`; the new
    /// photo arrives via `updateCommunity` / `updateCommunityFullInfo`.
    SetCommunityPhoto,
    /// `setCommunityPermissions` (TDLib 1.8.68). Response is `ok`; the
    /// new permissions arrive via `updateCommunity`.
    SetCommunityPermissions,
    /// `deleteCommunity` (TDLib 1.8.68). Response is `ok`; the state
    /// drops the community and its full-info pack.
    DeleteCommunity,
}

flat_purposes!(Groups(GroupsPurpose) {
    CanTransferOwnership,
    GetChatOwnerAfterLeaving,
    GetChatMember,
    JoinChat,
    LeaveChat,
    GetSupergroup,
    GetSupergroupFullInfo,
    GetChatStatistics,
    GetChatInviteLinks,
    CreateChatInviteLink,
    EditChatInviteLink,
    RevokeChatInviteLink,
    GetChatJoinRequests,
    GetMoreChatJoinRequests,
    GetRevokedChatInviteLinks,
    GetChatInviteLinkCounts,
    DeleteRevokedChatInviteLink,
    DeleteAllRevokedChatInviteLinks,
    GetChatBoostLink,
    ToggleSupergroupUsername,
    ReorderSupergroupUsernames,
    GetChatAdministrators,
    GetBasicGroupFullInfo,
    SetChatSlowModeDelay,
    CreateBasicGroup,
    ToggleBroadcastGroup,
    AddChatMembers,
    AddChatMember,
    SetChatPermissions,
    ReplacePrimaryChatInviteLink,
    ToggleSupergroupJoinByRequest,
    SetSupergroupUsername,
    SetChatTitle,
    SetChatDescription,
    SetChatPhoto,
    SetSupergroupStickerSet,
    SetSupergroupCustomEmojiStickerSet,
    ToggleSupergroupSignMessages,
    ToggleSupergroupAggressiveAntiSpam,
    GetChatBoostStatus,
    GetBoostSlotsForBoost,
    BoostChat,
    LoadChatWelcomeMessages,
    AddChatWelcomeMessage,
    ToggleSupergroupIsForum,
    ToggleSupergroupIsAllHistoryAvailable,
    ToggleSupergroupJoinToSendMessages,
    ToggleSupergroupHasHiddenMembers,
    ToggleChatHasProtectedContent,
    SetChatAvailableReactions,
    SetChatDiscussionGroup,
    UpgradeBasicGroup,
    SetChatMessageAutoDeleteTime,
    CreateCommunity,
    GetCommunityFullInfo,
    SetCommunityName,
    SetCommunityPhoto,
    SetCommunityPermissions,
    DeleteCommunity,
});
