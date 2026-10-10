//! TDLib updates and answers for groups and channels: members, admin rights, invite links, join requests, boosts, communities.
mod parse;

use crate::ids::ChatId;
use crate::telegram::envelope::*;
pub(crate) use parse::parse_groups_payload;

/// Payloads for groups and channels: members, admin rights, invite links, join requests, boosts, communities; wrapped as
/// [`EnvelopePayload::Groups`].
#[derive(Debug, Clone, PartialEq)]
pub enum GroupsPayload {
    /// `updateSupergroup` — `supergroup.is_forum` is how Quill learns a
    /// supergroup is a forum (`chatTypeSupergroup` has no forum flag).
    /// Parity slice: the first active username (`supergroup.usernames`,
    /// schema 1.8.67 lines 2746/2372) feeds the channel/supergroup header.
    /// `updateBasicGroup` — only `basicGroup.member_count` is kept (the
    /// header's "N members").
    UpdateBasicGroup {
        basic_group_id: i64,
        member_count: i32,
        /// Own `basicGroup.status` (schema 1.8.67, line 2714); `Unknown`
        /// when missing. Gates member moderation and ownership transfer.
        status: ChannelMemberStatus,
        can_restrict_members: bool,
        can_promote_members: bool,
        can_manage_tags: bool,
        /// B7: `rights.can_change_info` of an administrator status.
        can_change_info: Option<bool>,
        /// B7: `basicGroup.is_active` - false once upgraded to a
        /// supergroup.
        is_active: bool,
    },
    UpdateSupergroup {
        supergroup_id: i64,
        /// `supergroup.verification_status` (schema line 2746): the chat
        /// row's verified check / SCAM / FAKE label.
        verification: crate::peer_badge::VerificationStatus,
        is_forum: bool,
        /// Subsection tabs: `supergroup.has_forum_tabs` (schema 1.8.67,
        /// line 2746) — a forum whose topics show as tabs, the way
        /// Telegram Desktop shows them (`ChannelData::useSubsectionTabs`).
        has_forum_tabs: bool,
        /// `supergroup.has_automatic_translation` (schema 1.8.67, line
        /// 2746): the channel shows its messages translated for everyone.
        has_automatic_translation: bool,
        username: String,
        /// `supergroup.usernames`: active/disabled/collectible lists.
        usernames: SupergroupUsernames,
        /// `supergroup.member_count` — may be 0 until full info is known.
        member_count: i32,
        /// Phase A1: own `chatMemberStatus*` (`supergroup.status`, schema
        /// 1.8.67 line 2746 — "Current user status in the supergroup or
        /// channel"). Drives slow-mode bypass (admins/creators are exempt).
        status: ChannelMemberStatus,
        /// Phase A1: `rights.can_restrict_members` from own
        /// `chatMemberStatusAdministrator` (schema 1.8.67, lines 2500/1092);
        /// `None` for any other status or a missing rights block.
        /// `setChatSlowModeDelay` requires this right (line 13551).
        can_restrict_members: Option<bool>,
        /// Phase D3a: `rights.can_invite_users` from own
        /// `chatMemberStatusAdministrator` (schema 1.8.67, line 1092);
        /// `None` for any other status or a missing rights block.
        /// Invite-link management requires this right (or creator status).
        can_invite_users: Option<bool>,
        /// Phase D3b: `rights.can_promote_members` from own
        /// `chatMemberStatusAdministrator` (schema 1.8.67, line 1092);
        /// `None` for any other status or a missing rights block.
        /// Admin management requires this right (or creator status).
        can_promote_members: Option<bool>,
        /// Slice G1: `rights.can_manage_tags` from own
        /// `chatMemberStatusAdministrator` (schema 1.8.67, line 1092);
        /// `None` for any other status or a missing rights block.
        /// Changing another member's custom title requires this right
        /// (or creator status); changing your own tag is always allowed.
        can_manage_tags: Option<bool>,
        /// Slice G2: `rights.can_manage_topics` from own
        /// `chatMemberStatusAdministrator` (schema 1.8.67, line 1092);
        /// `None` for any other status or a missing rights block.
        /// Forum topic management requires this right (or creator
        /// status).
        can_manage_topics: Option<bool>,
        /// Slice G2: `rights.can_change_info` from own
        /// `chatMemberStatusAdministrator` (schema 1.8.67, line 1092);
        /// `None` for any other status or a missing rights block.
        /// `toggleSupergroupSignMessages` requires this right.
        can_change_info: Option<bool>,
        /// Slice G2: `rights.can_send_welcome_messages` from own
        /// `chatMemberStatusAdministrator` (schema 1.8.67, line 1090);
        /// `None` for any other status or a missing rights block.
        /// Welcome-message management requires this right (or creator
        /// status).
        can_send_welcome_messages: Option<bool>,
        /// Slice G1: `supergroup.join_by_request` (schema 1.8.67, lines
        /// 2733/2746) — drives the "Approve new members" toggle.
        join_by_request: bool,
        /// Slice G1: `supergroup.is_broadcast_group` (schema 1.8.67,
        /// lines 2736/2746) — drives the broadcast-group toggle.
        is_broadcast_group: bool,
        /// Slice G2: `supergroup.sign_messages` (schema 1.8.67, line 2746)
        /// — channel author signatures.
        sign_messages: bool,
        /// Slice G2: `supergroup.show_message_sender` (schema 1.8.67, line
        /// 2746) — sender shown alongside the signature; only meaningful
        /// when `sign_messages` is true.
        show_message_sender: bool,
        /// B7: `supergroup.join_to_send_messages` (schema 1.8.67, line
        /// 2746) — discussion group members must join to write.
        join_to_send_messages: bool,
    },
    /// `supergroup` — `getSupergroup` response. Phase A1: also keeps own
    /// `status` (`supergroup.status`, schema 1.8.67 line 2746) for the
    /// slow-mode bypass check.
    Supergroup {
        supergroup_id: i64,
        is_forum: bool,
        /// Subsection tabs: `supergroup.has_forum_tabs` (schema 1.8.67,
        /// line 2746).
        has_forum_tabs: bool,
        /// `supergroup.has_automatic_translation` (schema 1.8.67, line
        /// 2746): the channel shows its messages translated for everyone.
        has_automatic_translation: bool,
        username: String,
        status: ChannelMemberStatus,
        /// Phase A1: `rights.can_restrict_members` from own
        /// `chatMemberStatusAdministrator` (schema 1.8.67, lines 2500/1092);
        /// `None` for any other status or a missing rights block.
        can_restrict_members: Option<bool>,
        /// Phase D3a: `rights.can_invite_users` from own
        /// `chatMemberStatusAdministrator` (schema 1.8.67, line 1092);
        /// `None` for any other status or a missing rights block.
        /// Invite-link management requires this right (or creator status).
        can_invite_users: Option<bool>,
        /// Phase D3b: `rights.can_promote_members` from own
        /// `chatMemberStatusAdministrator` (schema 1.8.67, line 1092);
        /// `None` for any other status or a missing rights block.
        /// Admin management requires this right (or creator status).
        can_promote_members: Option<bool>,
        /// Slice G1: `rights.can_manage_tags` from own
        /// `chatMemberStatusAdministrator` (schema 1.8.67, line 1092);
        /// `None` for any other status or a missing rights block.
        /// Changing another member's custom title requires this right
        /// (or creator status); changing your own tag is always allowed.
        can_manage_tags: Option<bool>,
        /// Slice G2: `rights.can_manage_topics` from own
        /// `chatMemberStatusAdministrator` (schema 1.8.67, line 1092);
        /// `None` for any other status or a missing rights block.
        can_manage_topics: Option<bool>,
        /// Slice G2: `rights.can_change_info` from own
        /// `chatMemberStatusAdministrator` (schema 1.8.67, line 1092);
        /// `None` for any other status or a missing rights block.
        can_change_info: Option<bool>,
        /// Slice G2: `rights.can_send_welcome_messages` from own
        /// `chatMemberStatusAdministrator` (schema 1.8.67, line 1090);
        /// `None` for any other status or a missing rights block.
        can_send_welcome_messages: Option<bool>,
        /// Slice G1: `supergroup.join_by_request` (schema 1.8.67, lines
        /// 2733/2746) — drives the "Approve new members" toggle.
        join_by_request: bool,
        /// Slice G1: `supergroup.is_broadcast_group` (schema 1.8.67,
        /// lines 2736/2746) — drives the broadcast-group toggle.
        is_broadcast_group: bool,
        /// Slice G2: `supergroup.sign_messages` (schema 1.8.67, line 2746).
        sign_messages: bool,
        /// Slice G2: `supergroup.show_message_sender` (schema 1.8.67, line
        /// 2746).
        show_message_sender: bool,
        /// B7: `supergroup.join_to_send_messages` (schema 1.8.67, line
        /// 2746).
        join_to_send_messages: bool,
    },
    /// `chatMember` — `getChatMember` response for a channel.
    ChatMember { member: ParsedChatMember },
    /// `updateChatMember` — own or peer membership changed; only the
    /// `new_chat_member` is kept.
    UpdateChatMember {
        chat_id: ChatId,
        member: ParsedChatMember,
    },
    /// `supergroupFullInfo` — `getSupergroupFullInfo` response (schema
    /// 1.8.67, line 11513). The response carries no supergroup id; it is
    /// resolved from the pending request in `Session::apply`. Kept:
    /// `description`, `member_count`, `linked_chat_id` (schema 1.8.67,
    /// line 2792; the discussion-group chat id for the channel header's
    /// "Discuss" affordance), plus the slow-mode fields and boost counts
    /// (Phase A1: `slow_mode_delay` / `slow_mode_delay_expires_in`, schema
    /// 1.8.67 lines 2758–2759; `my_boost_count` / `unrestrict_boost_count`,
    /// lines 2779–2780) that drive composer slow-mode enforcement, plus
    /// Phase D2's `can_get_statistics` (line 2792) gating the statistics
    /// entry point. Slice S11: `can_set_sticker_set` (line 2765),
    /// `sticker_set_id` / `custom_emoji_sticker_set_id` (line 2792) for
    /// group sticker-set management. Dropped: admin/restricted/banned
    /// counts, invite link, gift fields, paid-message and other statistics
    /// flags, location.
    SupergroupFullInfo {
        description: String,
        member_count: i32,
        linked_chat_id: i64,
        slow_mode_delay: i32,
        slow_mode_delay_expires_in: f64,
        my_boost_count: i32,
        unrestrict_boost_count: i32,
        /// Phase D2: `supergroupFullInfo.can_get_statistics` (schema 1.8.67,
        /// line 2792) — true when chat statistics are available via
        /// `getChatStatistics`. Gates the statistics entry point.
        can_get_statistics: bool,
        /// Slice G2: `supergroupFullInfo.has_aggressive_anti_spam_enabled`
        /// (schema 1.8.67, line 2792).
        has_aggressive_anti_spam_enabled: bool,
        /// Slice G2: `supergroupFullInfo.can_toggle_aggressive_anti_spam`
        /// (schema 1.8.67, line 2792) — gates the anti-spam toggle.
        can_toggle_aggressive_anti_spam: bool,
        /// Slice S11: `supergroupFullInfo.can_set_sticker_set` (schema
        /// 1.8.67, line 2765) — true when the supergroup sticker set can
        /// be changed; gates the group sticker-set affordance.
        can_set_sticker_set: bool,
        /// Slice S11: `supergroupFullInfo.sticker_set_id` (schema 1.8.67,
        /// line 2792) — the installed group sticker set; 0 when none.
        sticker_set_id: i64,
        /// Slice S11: `supergroupFullInfo.custom_emoji_sticker_set_id`
        /// (schema 1.8.67, line 2792) — the group's custom-emoji set; 0
        /// when none.
        custom_emoji_sticker_set_id: i64,
        /// B7: admin-toggle flags of `supergroupFullInfo`.
        admin: SupergroupFullAdmin,
    },
    /// Slice (communities backend core): `updateCommunity` (schema 1.8.67,
    /// line 10726) — the update carries the full `community` object and is
    /// guaranteed to arrive before the `communityId` of a just-created
    /// community, so the reducer inserts it create-on-first-sight.
    UpdateCommunity { community: ParsedCommunity },
    /// Slice (communities backend core): `updateCommunityFullInfo`
    /// (schema 1.8.67, line 10753) — carries its own `community_id`, so
    /// it applies whenever it arrives (no pending-request correlation).
    /// TDLib sends it whenever the pack changes; the first fetch is the
    /// direct `getCommunityFullInfo` answer below.
    UpdateCommunityFullInfo {
        community_id: i64,
        full_info: ParsedCommunityFullInfo,
    },
    /// TDLib 1.8.68: `communityFullInfo` — the direct answer of
    /// `getCommunityFullInfo` (replaced 1.8.67's `loadCommunityFullInfo`,
    /// which answered `ok` and delivered the pack through
    /// `updateCommunityFullInfo`). Carries no community id; the reducer
    /// correlates it through `PendingRequest::community_id`.
    CommunityFullInfo { full_info: ParsedCommunityFullInfo },
    /// Slice (communities backend core): `communityId` (schema 1.8.67,
    /// line 2264) — the response of `createCommunity` (line 11806). The
    /// driver chains it into `getCommunityFullInfo`.
    CommunityId { id: i64 },
    /// Slice G2: `updateChatWelcomeMessages` (schema 1.8.67, line 10649)
    /// — the chat's welcome-message pack, sent after
    /// `loadChatWelcomeMessages` and whenever the pack changes.
    UpdateChatWelcomeMessages {
        chat_id: i64,
        messages: Vec<ParsedWelcomeMessage>,
    },
    /// Slice G2: `updateChatHasWelcomeMessages` (schema 1.8.67, line
    /// 10600) — the chat's `has_welcome_messages` field changed.
    UpdateChatHasWelcomeMessages {
        chat_id: i64,
        has_welcome_messages: bool,
    },
    /// Slice G2: `chatBoostStatus` (schema 1.8.67, line 6943) — the
    /// `getChatBoostStatus` response. Only `level` and `boost_count`
    /// drive the channel profile row.
    ChatBoostStatus { level: i32, boost_count: i32 },
    /// Slice G2: `chatBoostSlots` (schema 1.8.67, line 6968) — the
    /// `getAvailableChatBoostSlots` / `boostChat` response; only the slot
    /// ids are kept.
    ChatBoostSlots { slots: Vec<i32> },
    /// Phase D2: `chatStatisticsChannel` / `chatStatisticsSupergroup` —
    /// `getChatStatistics` response (schema 1.8.67, line 15760). The
    /// response carries no chat id; it is resolved from the pending
    /// request in `Session::apply`.
    ChatStatistics { statistics: ChatStatistics },
    /// Phase D3a: `chatInviteLink` response (TDLib 1.8.67, line 2627) —
    /// the created/edited link from `createChatInviteLink` /
    /// `editChatInviteLink`. Correlated to the chat by the request's
    /// `PendingRequest::chat_id`.
    ChatInviteLink { link: ParsedChatInviteLink },
    /// `checkChatInviteLink` answer (schema 1.8.67, line 2684).
    ChatInviteLinkInfo {
        title: String,
        member_count: i32,
        creates_join_request: bool,
        is_channel: bool,
    },
    /// Phase D3a: `chatInviteLinks` (TDLib 1.8.67, line 2630) — the
    /// response of `getChatInviteLinks` / `revokeChatInviteLink`.
    /// Correlated to the chat by the request's `PendingRequest::chat_id`.
    ChatInviteLinks {
        total_count: i32,
        links: Vec<ParsedChatInviteLink>,
    },
    /// Phase D3a: `chatJoinRequests` (TDLib 1.8.67, line 2691) — the
    /// response of `getChatJoinRequests`. Correlated to the chat by the
    /// request's `PendingRequest::chat_id`.
    ChatJoinRequests {
        total_count: i32,
        requests: Vec<ParsedChatJoinRequest>,
    },
    /// `foundChatBoosts` — the `getChatBoosts` answer.
    FoundChatBoosts {
        total_count: i32,
        boosts: Vec<ParsedChatBoost>,
        next_offset: String,
    },
    /// `chatBoostLink` — the `getChatBoostLink` answer.
    ChatBoostLink { link: String, is_public: bool },
    /// B8: `chatInviteLinkCounts` — the `getChatInviteLinkCounts` answer.
    ChatInviteLinkCounts {
        counts: Vec<ParsedChatInviteLinkCount>,
    },
    /// B8: `chatInviteLinkMembers` — the `getChatInviteLinkMembers` answer.
    ChatInviteLinkMembers {
        total_count: i32,
        members: Vec<ParsedChatInviteLinkMember>,
    },
    /// Phase D3b: `chatAdministrators` (TDLib 1.8.67, line 2485) — the
    /// response of `getChatAdministrators` (line 13632). Carries no chat
    /// id; correlated to the chat by the request's
    /// `PendingRequest::chat_id`.
    ChatAdministrators {
        administrators: Vec<ChatAdministratorEntry>,
    },
    /// Slice G1: `createdBasicGroupChat` (TDLib 1.8.67, line 3644) — the
    /// response of `createNewBasicGroupChat` (line 13327):
    /// `createdBasicGroupChat chat_id:int53
    /// failed_to_add_members:failedToAddMembers = CreatedBasicGroupChat;`
    /// The new chat itself arrives as `updateNewChat`.
    CreatedBasicGroupChat { chat_id: i64 },
    /// Slice G1: `failedToAddMembers` (TDLib 1.8.67, line 3640) — the
    /// response of `addChatMembers` (line 13584):
    /// `failedToAddMembers
    /// failed_to_add_members:vector<failedToAddMember> =
    /// FailedToAddMembers;`
    /// Only the failure count is kept; per-user errors are dropped.
    FailedToAddMembers { failed_count: i32 },
    /// Slice G1: `basicGroupFullInfo` (TDLib 1.8.67, line 2714) — the
    /// response of `getBasicGroupFullInfo` (line 11507):
    /// `basicGroupFullInfo photo:chatPhoto description:string
    /// creator_user_id:int53 members:vector<chatMember> ... =
    /// BasicGroupFullInfo;`
    /// Only the member list is kept (the basic-group member dialog);
    /// correlated to the chat by the request's `PendingRequest::chat_id`.
    BasicGroupFullInfo { members: Vec<ParsedChatMember> },
    /// Phase D3c: `chatEvents` (TDLib 1.8.67, line 7938) — the response
    /// of `getChatEventLog` (line 15252). Carries no chat id; correlated
    /// to the chat by the request's `PendingRequest::chat_id`. Events
    /// arrive in reverse chronological order (decreasing event `id`).
    ChatEvents { events: Vec<ParsedChatEvent> },
    /// Phase D3b: `chatMembers` (TDLib 1.8.67, line 2529) — the response
    /// of `getSupergroupMembers` (line 15238). Drives the promote flow's
    /// member picker. Carries no supergroup id; correlated by the
    /// request's `PendingRequest::chat_id`.
    SupergroupMembers {
        members: Vec<ParsedChatMember>,
        total_count: i32,
    },
    /// Phase D3a: `updateNewChatJoinRequest` (TDLib 1.8.67, line 11210) —
    /// a user requested to join the chat. Carries its own `chat_id`.
    UpdateNewChatJoinRequest {
        chat_id: i64,
        request: ParsedChatJoinRequest,
        user_chat_id: i64,
        invite_link: ParsedChatInviteLink,
        query_id: i64,
    },
    /// Phase D3a: `updateChatPendingJoinRequests` (TDLib 1.8.67, line
    /// 10555) — the pending-join-request summary changed. Carries its own
    /// `chat_id`; the full request list still needs `getChatJoinRequests`.
    UpdateChatPendingJoinRequests {
        chat_id: i64,
        total_count: i32,
        user_ids: Vec<i64>,
    },
    /// Parity slice: `updateSupergroupFullInfo` (schema 1.8.67, line 10750)
    /// — the update carries its own `supergroup_id`, so it applies
    /// whenever it arrives (no pending-request correlation). Phase A1:
    /// also carries the slow-mode fields (schema lines 2758–2759) and
    /// boost counts (lines 2779–2780); note the schema warns no update
    /// fires when only `slow_mode_delay_expires_in` changes while both
    /// old and new values are non-zero, so the reducer timestamps every
    /// arrival and the gate decays locally.
    UpdateSupergroupFullInfo {
        supergroup_id: i64,
        description: String,
        member_count: i32,
        linked_chat_id: i64,
        slow_mode_delay: i32,
        slow_mode_delay_expires_in: f64,
        my_boost_count: i32,
        unrestrict_boost_count: i32,
        /// Phase D2: `supergroupFullInfo.can_get_statistics` (schema 1.8.67,
        /// line 2792).
        can_get_statistics: bool,
        /// Slice G2: `supergroupFullInfo.has_aggressive_anti_spam_enabled`
        /// (schema 1.8.67, line 2792).
        has_aggressive_anti_spam_enabled: bool,
        /// Slice G2: `supergroupFullInfo.can_toggle_aggressive_anti_spam`
        /// (schema 1.8.67, line 2792) — gates the anti-spam toggle.
        can_toggle_aggressive_anti_spam: bool,
        /// Slice S11: sticker-set fields (schema 1.8.67, lines 2765 and
        /// 2792), nested like the other fields.
        can_set_sticker_set: bool,
        sticker_set_id: i64,
        custom_emoji_sticker_set_id: i64,
        /// B7: admin-toggle flags of `supergroupFullInfo`.
        admin: SupergroupFullAdmin,
    },
    /// `ChatJoinResult` — `joinChat` response.
    JoinChatResult(ChatJoinResult),
    /// `canTransferOwnership` answer (TDLib 1.8.67, `schema/td_api.tl:8568`);
    /// honored only for the pending `CanTransferOwnership` purpose.
    CanTransferOwnershipResult { result: CanTransferOwnershipResult },
}
