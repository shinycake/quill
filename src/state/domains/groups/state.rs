//! Groups and channels: members, admins, rights, invite links, join requests, boosts, communities, event logs: the `groups` domain's share of `Session`.
//! Added to by features in this domain only; `Session::new` builds it
//! with `new`.
use crate::state::*;

pub struct GroupsState {
    /// Transfer-ownership gate, transfer progress and the "next owner" lookup.
    pub ownership: OwnershipState,
    /// Own status and admin rights in each basic group (`updateBasicGroup`).
    pub basic_group_own: HashMap<i64, BasicGroupOwn>,
    /// Member counts from `updateSupergroup` / `updateBasicGroup` (the
    /// header's fallback before full info loads), keyed by group id.
    pub supergroup_member_counts: HashMap<i64, i32>,
    pub basic_group_member_counts: HashMap<i64, i32>,
    /// Slice G1 fix-up: one-shot; set when an invite-link mutation
    /// (create/edit/revoke/replace-primary) errors. The UI drains it into
    /// the status note — the previously loaded list is kept, not wiped.
    pub invite_link_error: Option<String>,
    /// Phase 6: cached `getSupergroupFullInfo`, keyed by supergroup id.
    /// Presence records "fetched".
    pub supergroup_full_infos: HashMap<i64, SupergroupFullInfoData>,
    /// Slice (communities backend core): communities by id, fed by
    /// `updateCommunity` (schema 1.8.67, line 10726),
    /// create-on-first-sight.
    pub communities: HashMap<i64, ParsedCommunity>,
    /// Slice (communities backend core): `communityFullInfo` cache, keyed
    /// by community id, fed by the `getCommunityFullInfo` answer and
    /// `updateCommunityFullInfo` (TDLib 1.8.68). Presence records
    /// "fetched".
    pub community_full_infos: HashMap<i64, ParsedCommunityFullInfo>,
    /// TDLib 1.8.68 community management: one-shot; set when
    /// `setCommunityName` / `setCommunityPhoto` / `setCommunityPermissions`
    /// / `deleteCommunity` errors. The UI drains it into the status note.
    pub community_error: Option<String>,
    /// Phase D2: `getChatStatistics` fetch state, keyed by chat id.
    pub chat_statistics: HashMap<i64, ChatStatisticsFetch>,
    /// Phase D3a: `getChatInviteLinks` fetch state, keyed by chat id.
    pub invite_links: HashMap<i64, InviteLinkFetch>,
    /// Phase D3a: `getChatJoinRequests` fetch state, keyed by chat id.
    pub join_requests: HashMap<i64, JoinRequestFetch>,
    /// B8: the search query the cached `join_requests` list was fetched
    /// with (absent = no query), keyed by chat id.
    pub join_request_queries: HashMap<i64, String>,
    /// B8: newest join-request page request per chat; replies with any
    /// other id are stale (an older search) and dropped.
    pub join_request_latest: HashMap<i64, RequestId>,
    /// B8: `getChatInviteLinks` with `is_revoked = true`, keyed by chat id.
    pub revoked_invite_links: HashMap<i64, InviteLinkFetch>,
    /// Another admin's invite links (owner only), by chat id.
    pub admin_invite_links: HashMap<i64, AdminLinksState>,
    /// Pending join requests of the invite link whose details are open.
    pub link_join_requests: HashMap<i64, LinkRequestsState>,
    /// `getChatBoosts` list of the open tab, by chat id.
    pub chat_boost_lists: HashMap<i64, BoostsListState>,
    /// `getChatBoostLink` answer `(link, is_public)`, by chat id.
    pub chat_boost_links: HashMap<i64, (String, bool)>,
    /// `supergroup.usernames`, by supergroup id.
    pub supergroup_username_lists: HashMap<i64, crate::telegram::envelope::SupergroupUsernames>,
    /// B8: `getChatInviteLinkCounts` (owner only), keyed by chat id.
    pub invite_link_counts: HashMap<i64, InviteLinkCountsFetch>,
    /// B8: members of the invite link whose details are open, by chat id.
    pub invite_link_members: HashMap<i64, InviteLinkMembersState>,
    /// B8: in-flight `deleteRevokedChatInviteLink` requests, request id to
    /// `(chat id, link)`; a purpose is `Copy` so the link rides here.
    pub revoked_link_deletions: HashMap<RequestId, (i64, String)>,
    /// Phase D3a: latest `updateChatPendingJoinRequests` total per chat
    /// (schema 1.8.67, line 10555). The full request list still needs
    /// `getChatJoinRequests`; this is only the badge count.
    pub pending_join_request_counts: HashMap<i64, i32>,
    /// Batch 8: `chatJoinRequestsInfo.user_ids` (the newest requesters)
    /// from the same update, for the requests bar's avatars.
    pub pending_join_request_users: HashMap<i64, Vec<i64>>,
    /// Phase D3c: `getChatEventLog` fetch state, keyed by chat id.
    pub event_logs: HashMap<i64, ChatEventLogFetch>,
    /// Slice G2: per-chat event-log filters (`chatEventLogFilters`,
    /// schema 1.8.67, line 7956). Absent = all event types (the schema's
    /// `null`).
    pub event_log_filters: HashMap<i64, ChatEventLogFilterSet>,
    /// Slice G2: per-chat event-log text search (the `query` parameter of
    /// `getChatEventLog`, schema 1.8.67, line 15252). Absent = no search.
    pub event_log_queries: HashMap<i64, String>,
    /// Per-chat admin filter: the `user_ids` of `getChatEventLog` (the server
    /// filters, so the admin list stays complete). Empty or absent = everyone.
    pub event_log_users: HashMap<i64, Vec<i64>>,
    /// Slice G2: `supergroup.sign_messages` (schema 1.8.67, line 2746),
    /// keyed by supergroup id. Drives the channel "Sign messages" toggle.
    pub supergroup_sign_messages: HashMap<i64, bool>,
    /// Slice G2: `supergroup.show_message_sender` (schema 1.8.67, line
    /// 2746), keyed by supergroup id. Drives the "Show authors" toggle.
    pub supergroup_show_message_sender: HashMap<i64, bool>,
    /// Slice G2: `supergroupFullInfo.has_aggressive_anti_spam_enabled`
    /// (schema 1.8.67, line 2792), keyed by supergroup id.
    pub supergroup_anti_spam_enabled: HashMap<i64, bool>,
    /// Slice G2: `supergroupFullInfo.can_toggle_aggressive_anti_spam`
    /// (schema 1.8.67, line 2792), keyed by supergroup id. Gates the
    /// anti-spam toggle.
    pub supergroup_can_toggle_anti_spam: HashMap<i64, bool>,
    /// Slice G2: the viewer's `rights.can_manage_topics` per supergroup
    /// (schema 1.8.67, line 1092). Gates forum topic management.
    pub supergroup_manage_topics_right: HashMap<i64, bool>,
    /// Slice G2: the viewer's `rights.can_change_info` per supergroup
    /// (schema 1.8.67, line 1092). `toggleSupergroupSignMessages`
    /// requires this right.
    pub supergroup_change_info_right: HashMap<i64, bool>,
    /// Slice G2: the viewer's `rights.can_send_welcome_messages` per
    /// supergroup (schema 1.8.67, line 1090). Gates welcome-message
    /// management.
    pub supergroup_send_welcome_right: HashMap<i64, bool>,
    /// Slice G2: `chat.has_welcome_messages` (schema 1.8.67, line 3627)
    /// / `updateChatHasWelcomeMessages` (line 10600), keyed by chat id.
    pub chat_has_welcome_messages: HashMap<i64, bool>,
    /// Slice G2: the welcome-message pack per chat
    /// (`updateChatWelcomeMessages`, schema 1.8.67, line 10649).
    pub welcome_messages: HashMap<i64, Vec<ParsedWelcomeMessage>>,
    /// Slice G2: `loadChatWelcomeMessages` fetch state, keyed by chat id.
    pub welcome_message_fetches: HashMap<i64, WelcomeMessagesFetch>,
    /// Slice G2: `(level, boost_count)` from `getChatBoostStatus`
    /// (schema 1.8.67, lines 13917/6943), keyed by chat id.
    pub chat_boost_status: HashMap<i64, (i32, i32)>,
    /// Slice G2: available boost slot ids from `getAvailableChatBoostSlots`
    /// (schema 1.8.67, line 13914), keyed by chat id. The driver consumes
    /// them to chain `boostChat` once per boost intent.
    pub boost_slots_by_chat: HashMap<i64, Vec<i32>>,
    /// Slice G2: the chat id of a pending user boost intent — set by
    /// `request_chat_boost`, consumed by the driver's `boostChat` chain.
    pub boost_intent: Option<i64>,
    /// Parity slice: first active username per supergroup (`supergroup`
    /// object / `updateSupergroup`, schema 1.8.67 line 2746), keyed by
    /// supergroup id. Feeds the channel/supergroup header's @username.
    pub supergroup_usernames: HashMap<i64, String>,
    /// Chat-row title badge: `supergroup.verification_status` per
    /// supergroup (`updateSupergroup`), keyed by supergroup id.
    pub supergroup_verification: HashMap<i64, crate::peer_badge::VerificationStatus>,
    /// Phase A1: the viewer's own `chatMemberStatus*` per supergroup
    /// (`supergroup.status` / `updateSupergroup`, schema 1.8.67 line 2746).
    /// Drives the slow-mode bypass (admins/creators are exempt) and gates
    /// the admin slow-mode control. Absent = unknown (gated, no bypass).
    pub supergroup_member_status: HashMap<i64, ChannelMemberStatus>,
    /// Phase A1: the viewer's `rights.can_restrict_members` per supergroup
    /// from own `chatMemberStatusAdministrator` (schema 1.8.67, lines
    /// 2500/`chatAdministratorRights` 1092). `setChatSlowModeDelay` requires
    /// 13551). Absent = unknown, treated as lacking the right.
    pub supergroup_restrict_right: HashMap<i64, bool>,
    /// Phase D3a: the viewer's `rights.can_invite_users` per supergroup
    /// from own `chatMemberStatusAdministrator` (schema 1.8.67, line
    /// 1092). Invite-link management requires this right (or creator
    /// status). Absent = unknown, treated as lacking the right.
    pub supergroup_invite_right: HashMap<i64, bool>,
    /// Phase D3b: `getChatAdministrators` fetch state, keyed by chat id.
    pub admin_lists: HashMap<i64, AdminListFetch>,
    /// Phase D3b / slice G1: `getSupergroupMembers` fetch state for the
    /// promote member picker and the member-management dialog, keyed by
    /// (chat id, filter). One page per filter.
    pub supergroup_members: HashMap<(i64, MemberListFilter), SupergroupMembersFetch>,
    /// Slice G1: `getBasicGroupFullInfo` fetch state (the member list for
    /// basic groups), keyed by chat id. Reuses `SupergroupMembersFetch`
    /// (Loading / Loaded / Failed).
    pub basic_group_members: HashMap<i64, SupergroupMembersFetch>,
    /// Slice G1: `supergroup.join_by_request` (schema 1.8.67, lines
    /// 2733/2746), keyed by supergroup id. Drives the "Approve new
    /// members" toggle.
    pub supergroup_join_by_request: HashMap<i64, bool>,
    /// B7: `supergroup.join_to_send_messages` (schema 1.8.67, line 2746),
    /// keyed by supergroup id.
    pub supergroup_join_to_send: HashMap<i64, bool>,
    /// B7: the viewer's own `basicGroup.status`, keyed by basic group id.
    pub basic_group_status: HashMap<i64, ChannelMemberStatus>,
    /// B7: `can_change_info` of the viewer's administrator status in a
    /// basic group.
    pub basic_group_change_info_right: HashMap<i64, bool>,
    /// B7: `basicGroup.is_active` (false after the upgrade).
    pub basic_group_active: HashMap<i64, bool>,
    /// B7: steps waiting for a group-admin request to succeed.
    pub admin_followups: Vec<(RequestId, AdminFollowup)>,
    /// Slice G1: `supergroup.is_broadcast_group` (schema 1.8.67, lines
    /// 2736/2746), keyed by supergroup id. Set by
    /// `toggleSupergroupIsBroadcastGroup` (one-way upgrade).
    pub supergroup_is_broadcast: HashMap<i64, bool>,
    /// Slice G1: `addChatMembers` failure count from the last add
    /// attempt — `failedToAddMembers.failed_to_add_members.len()` for the
    /// bulk path (schema 1.8.67, line 3640), or the accumulated per-user
    /// `addChatMember` `failedToAddMembers` counts plus error responses
    /// for basic groups — keyed by chat id.
    /// Reset when a new add starts; cleared when the dialog closes.
    pub add_members_failed: HashMap<i64, i32>,
    /// Slice G1 fix-up: chat ids whose member-list caches were dropped by
    /// an `updateChatMember` while a member dialog may be open. One-shot;
    /// the UI drains it and refetches the open dialog's page.
    pub member_list_stale: Vec<i64>,
    /// Slice G1: last member-action failure for the member-management
    /// dialog (`setChatMemberTag` / `setChatMemberStatus`), keyed by
    /// chat id. The dialog reads the member-list fetch states, not
    /// `admin_lists`, so action failures need their own slot to be
    /// visible where the action was taken. Cleared when the dialog
    /// opens.
    pub member_action_error: HashMap<i64, String>,
    /// Phase D3b: one administrator's parsed `chatAdministratorRights`
    /// fetch state, keyed by (chat_id, user_id). Filled by `getChatMember`
    /// (purpose `GetAdminRights`); drives the edit-rights dialog.
    pub admin_rights: HashMap<(i64, i64), AdminRightsFetch>,
    /// Phase D3b: the viewer's `rights.can_promote_members` per supergroup
    /// from own `chatMemberStatusAdministrator` (schema 1.8.67, line
    /// 1092). Admin management requires this right (or creator status).
    /// Absent = unknown, treated as lacking the right.
    pub supergroup_promote_right: HashMap<i64, bool>,
    /// Slice G1: own `rights.can_manage_tags` per supergroup (schema
    /// 1.8.67, line 1092) — gates custom-title changes for others.
    pub supergroup_manage_tags_right: HashMap<i64, bool>,
}

impl GroupsState {
    pub(crate) fn new() -> Self {
        Self {
            ownership: OwnershipState::default(),
            basic_group_own: HashMap::new(),
            supergroup_member_counts: HashMap::new(),
            basic_group_member_counts: HashMap::new(),
            invite_link_error: None,
            supergroup_full_infos: HashMap::new(),
            communities: HashMap::new(),
            community_full_infos: HashMap::new(),
            community_error: None,
            chat_statistics: HashMap::new(),
            invite_links: HashMap::new(),
            join_requests: HashMap::new(),
            join_request_queries: HashMap::new(),
            join_request_latest: HashMap::new(),
            revoked_invite_links: HashMap::new(),
            admin_invite_links: HashMap::new(),
            link_join_requests: HashMap::new(),
            chat_boost_lists: HashMap::new(),
            chat_boost_links: HashMap::new(),
            supergroup_username_lists: HashMap::new(),
            invite_link_counts: HashMap::new(),
            invite_link_members: HashMap::new(),
            revoked_link_deletions: HashMap::new(),
            pending_join_request_counts: HashMap::new(),
            pending_join_request_users: HashMap::new(),
            event_logs: HashMap::new(),
            event_log_filters: HashMap::new(),
            event_log_queries: HashMap::new(),
            event_log_users: HashMap::new(),
            supergroup_sign_messages: HashMap::new(),
            supergroup_show_message_sender: HashMap::new(),
            supergroup_anti_spam_enabled: HashMap::new(),
            supergroup_can_toggle_anti_spam: HashMap::new(),
            supergroup_manage_topics_right: HashMap::new(),
            supergroup_change_info_right: HashMap::new(),
            supergroup_send_welcome_right: HashMap::new(),
            chat_has_welcome_messages: HashMap::new(),
            welcome_messages: HashMap::new(),
            welcome_message_fetches: HashMap::new(),
            chat_boost_status: HashMap::new(),
            boost_slots_by_chat: HashMap::new(),
            boost_intent: None,
            supergroup_usernames: HashMap::new(),
            supergroup_verification: HashMap::new(),
            supergroup_member_status: HashMap::new(),
            supergroup_restrict_right: HashMap::new(),
            supergroup_invite_right: HashMap::new(),
            admin_lists: HashMap::new(),
            supergroup_members: HashMap::new(),
            basic_group_members: HashMap::new(),
            supergroup_join_by_request: HashMap::new(),
            supergroup_join_to_send: HashMap::new(),
            basic_group_status: HashMap::new(),
            basic_group_change_info_right: HashMap::new(),
            basic_group_active: HashMap::new(),
            admin_followups: Vec::new(),
            supergroup_is_broadcast: HashMap::new(),
            add_members_failed: HashMap::new(),
            member_list_stale: Vec::new(),
            member_action_error: HashMap::new(),
            admin_rights: HashMap::new(),
            supergroup_promote_right: HashMap::new(),
            supergroup_manage_tags_right: HashMap::new(),
        }
    }
}
