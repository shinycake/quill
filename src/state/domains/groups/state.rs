//! Groups and channels: members, admins, rights, invite links, join requests, boosts, communities, event logs: the `groups` domain's share of `Session`.
//! Added to by features in this domain only; `Session::new` builds it
//! with `new`.
use crate::state::*;
use crate::telegram::envelope::{ParsedBoostFeatures, ParsedBoostStatus};

impl GroupsState {
    /// Where `chat`'s event log fetch stands; `None` until it was asked.
    pub fn event_log_fetch(&self, chat: i64) -> Option<&ChatEventLogFetch> {
        self.event_logs
            .get(&chat)
            .map(|log| &log.fetch)
            .filter(|fetch| !matches!(fetch, ChatEventLogFetch::Idle))
    }

    /// The text search `chat`'s event log is asked with.
    pub fn event_log_query(&self, chat: i64) -> &str {
        self.event_logs.get(&chat).map_or("", |log| &log.query)
    }

    /// `chat`'s event-type filter; `None` = every type.
    pub fn event_log_filter(&self, chat: i64) -> Option<ChatEventLogFilterSet> {
        self.event_logs.get(&chat).and_then(|log| log.filter)
    }

    /// The admins `chat`'s event log is filtered to; empty = everyone.
    pub fn event_log_users(&self, chat: i64) -> &[i64] {
        self.event_logs.get(&chat).map_or(&[], |log| &log.users)
    }

    /// `chat`'s event log has a filter or an admin filter set.
    pub fn event_log_filtered(&self, chat: i64) -> bool {
        self.event_log_filter(chat).is_some() || !self.event_log_users(chat).is_empty()
    }

    /// `chat`'s log forgets its fetch, and with it every request it waits
    /// for (a refresh): what it is asked with stays.
    pub(crate) fn reset_event_log_fetch(&mut self, chat: i64) {
        if let Some(log) = self.event_logs.get_mut(&chat) {
            log.fetch = ChatEventLogFetch::Idle;
        }
    }

    /// The `getChatEventLog` request `request` was sent for `chat`: the stage
    /// that asked now holds it (the first page: `Loading`; an older page and
    /// the poll: inside the loaded page).
    pub fn event_log_sent(
        &mut self,
        chat: i64,
        request: RequestId,
        polling: bool,
        first_page: bool,
    ) {
        if first_page {
            self.set_event_log_fetch(chat, ChatEventLogFetch::Loading { request });
        } else if let Some(page) = self.loaded_event_log_mut(chat) {
            if polling {
                page.poll = LogPoll::Polling { request };
            } else {
                page.older = OlderEvents::Loading { request };
            }
        }
    }

    /// `chat`'s loaded page, when there is one.
    pub(crate) fn loaded_event_log_mut(&mut self, chat: i64) -> Option<&mut ChatEventLogPage> {
        match &mut self.event_logs.get_mut(&chat)?.fetch {
            ChatEventLogFetch::Loaded(page) => Some(page),
            _ => None,
        }
    }

    /// `chat`'s event log is asked for, settled by `fetch`.
    pub(crate) fn set_event_log_fetch(&mut self, chat: i64, fetch: ChatEventLogFetch) {
        self.event_logs.entry(chat).or_default().fetch = fetch;
    }

    /// The answer to the revoked-links fetch `request` fills the list only
    /// while the list still waits for it (a list closed meanwhile stays
    /// closed).
    pub(crate) fn land_revoked(
        &mut self,
        chat: i64,
        pending: &PendingRequest,
        total_count: i32,
        links: Vec<ParsedChatInviteLink>,
    ) {
        if let Some(slot) = self.revoked_invite_links.get_mut(&chat)
            && *slot == InviteLinkFetch::Loading(pending.id)
        {
            *slot = InviteLinkFetch::Loaded(InviteLinkList { total_count, links });
        }
    }
}

/// A "boosts needed to bypass restrictions" value on its way.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnrestrictSave {
    pub request: RequestId,
    pub value: i32,
}

pub struct GroupsState {
    /// Transfer-ownership gate, transfer progress and the "next owner" lookup.
    pub ownership: OwnershipState,
    /// Own status and admin rights in each basic group (`updateBasicGroup`).
    pub basic_group_own: HashMap<i64, BasicGroupOwn>,
    /// Member counts from `updateSupergroup` / `updateBasicGroup` (the
    /// header's fallback before full info loads), keyed by group id.
    pub supergroup_member_counts: HashMap<i64, i32>,
    pub basic_group_member_counts: HashMap<i64, i32>,
    /// Slice G1 fix-up: set when an invite-link mutation
    /// (create/edit/revoke/replace-primary) errors. A queue: two failures
    /// between two polls are both shown. The UI drains it into the status
    /// note — the previously loaded list is kept, not wiped.
    pub invite_link_error: crate::state::Notes,
    /// Phase 6: cached `getSupergroupFullInfo`, keyed by supergroup id.
    /// Presence records "fetched".
    pub supergroup_full_infos: HashMap<i64, SupergroupFullInfoData>,
    /// Slice (communities backend core): communities by id, fed by
    /// `updateCommunity` (schema 1.8.67, line 10726),
    /// create-on-first-sight.
    pub communities: HashMap<i64, ParsedCommunity>,
    /// Slice (communities backend core): `communityFullInfo` cache, keyed
    /// by community id, fed by the `getCommunityFullInfo` answer and
    /// `updateCommunityFullInfo` (TDLib 1.8.68); one fetch per community
    /// holds the stage, its request and the pack.
    pub community_full_infos: CommunityFullInfos,
    /// TDLib 1.8.68 community management: one-shot; set when
    /// `setCommunityName` / `setCommunityPhoto` / `setCommunityPermissions`
    /// / `deleteCommunity` errors. A queue like `invite_link_error`; the UI
    /// drains it into the status note.
    pub community_error: crate::state::Notes,
    /// Phase D2: `getChatStatistics` fetch state, keyed by chat id.
    pub chat_statistics: HashMap<i64, ChatStatisticsFetch>,
    /// Async and zoomed graphs, message statistics and the panel's page.
    pub stats: StatsState,
    /// Phase D3a: `getChatInviteLinks` fetch state, keyed by chat id.
    pub invite_links: HashMap<i64, InviteLinkFetch>,
    /// Phase D3a: `getChatJoinRequests` fetch state, keyed by chat id.
    pub join_requests: HashMap<i64, JoinRequestFetch>,
    /// B8: `getChatInviteLinks` with `is_revoked = true`, keyed by chat id.
    pub revoked_invite_links: HashMap<i64, InviteLinkFetch>,
    /// The Invite Links section's pages for one chat: another admin's
    /// links (owner only), and the pending join requests and the members
    /// of the link whose details are open. One value, replaced whole.
    pub invite_pages: InvitePages,
    /// `getChatBoosts` list of the open tab, by chat id.
    pub chat_boost_lists: HashMap<i64, BoostsListState>,
    /// `getChatBoostLink` answer `(link, is_public)`, by chat id.
    pub chat_boost_links: HashMap<i64, (String, bool)>,
    /// Channels joined in this session, by chat id: the "You joined this
    /// channel" row with the similar-channels strip under it (tdesktop sets
    /// `SimilarExpanded` on `joinChannel`).
    pub similar_strips: HashMap<i64, SimilarStrip>,
    /// `supergroup.usernames`, by supergroup id.
    pub supergroup_username_lists: HashMap<i64, crate::telegram::envelope::SupergroupUsernames>,
    /// B8: `getChatInviteLinkCounts` (owner only), keyed by chat id.
    pub invite_link_counts: HashMap<i64, InviteLinkCountsFetch>,
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
    /// Phase D3c: each chat's `getChatEventLog` state, keyed by chat id: the
    /// search, the filters (schema 1.8.67, lines 7956 and 15252), the fetch
    /// stage and the page and poll requests it waits for.
    pub event_logs: HashMap<i64, EventLog>,
    /// The viewer posts as the group in this supergroup (own status,
    /// `rights.is_anonymous` or the owner's `is_anonymous`), kept current by
    /// `updateSupergroup`.
    pub supergroup_anonymous: HashMap<i64, bool>,
    /// `can_manage_video_chats` of the viewer's administrator status in a
    /// supergroup or channel (`updateSupergroup` / `getSupergroup`).
    pub supergroup_manage_video_chats_right: HashMap<i64, bool>,
    /// Slice G2: `chat.has_welcome_messages` (schema 1.8.67, line 3627)
    /// / `updateChatHasWelcomeMessages` (line 10600), keyed by chat id.
    pub chat_has_welcome_messages: HashMap<i64, bool>,
    /// Slice G2: the welcome-message pack per chat
    /// (`loadChatWelcomeMessages`, `updateChatWelcomeMessages`, schema
    /// 1.8.67, line 10649): asked, answered or failed, with the pack kept
    /// beside a refetch. `Loading` is the in-flight guard.
    pub welcome_messages: HashMap<i64, crate::state::fetch::Fetch<Vec<ParsedWelcomeMessage>>>,
    /// `getChatBoostStatus` (schema 1.8.67, line 13917), keyed by chat id:
    /// level, boost count, progress to the next level, Premium audience.
    /// A level that moved (`updateSupergroup`) or a boost made marks the
    /// answer `Outdated`: it stays on screen until the refetch replaces it.
    pub chat_boost_status: HashMap<i64, crate::state::fetch::Fetch<ParsedBoostStatus>>,
    /// `getChatBoostFeatures` level tables, keyed by chat id (the table
    /// differs for channels and groups).
    pub chat_boost_features: HashMap<i64, ParsedBoostFeatures>,
    /// "Boosts needed to bypass restrictions" saves in flight, keyed by chat
    /// id: the request and the value it sent. Its answer settles it.
    pub unrestrict_boost_saves: HashMap<i64, UnrestrictSave>,
    /// TDLib option `supergroup_size_max` (0 until it arrives): the
    /// permissions box offers a broadcast group near it.
    pub supergroup_size_max: i32,
    /// TDLib option `subscription_star_count_max`: the dearest monthly
    /// fee of a channel invite link.
    pub subscription_star_max: Option<i64>,
    /// App config `stars_usd_withdraw_rate_x1000` / 1000: US cents per
    /// Star, for the dollar hint next to a link's monthly fee.
    pub stars_usd_rate: Option<f64>,
    /// The open boost box (`crate::boost_box`). Its phase holds the Boost
    /// press waiting for `getAvailableChatBoostSlots` (schema 1.8.67, line
    /// 13914) and the slots that answer; the driver chains `boostChat` from
    /// them.
    pub boost_box: Option<crate::boost_box::BoostBoxState>,
    /// `boosts_per_sent_gift` of the app config: boosts a gifted Premium
    /// subscription gives. Asked once; the answer of that request alone
    /// fills it, and a refused or unsent ask is `Failed` (asked again).
    pub boosts_per_gift: crate::state::fetch::Fetch<i32>,
    /// Parity slice: first active username per supergroup (`supergroup`
    /// object / `updateSupergroup`, schema 1.8.67 line 2746), keyed by
    /// supergroup id. Feeds the channel/supergroup header's @username.
    pub supergroup_usernames: HashMap<i64, String>,
    /// Chat-row title badge: `supergroup.verification_status` per
    /// supergroup (`updateSupergroup`), keyed by supergroup id.
    pub supergroup_verification: HashMap<i64, crate::peer_badge::VerificationStatus>,
    /// `supergroup.boost_level` per supergroup (`updateSupergroup`), keyed by
    /// supergroup id: the last level seen, to tell when it moved.
    pub supergroup_boost_levels: HashMap<i64, i32>,
    /// The chat of each supergroup `updateNewChat` named, so a supergroup
    /// update reaches its chat without a walk over every chat.
    pub supergroup_chat_ids: HashMap<i64, i64>,
    /// Phase A1: the viewer's own `chatMemberStatus*` per supergroup
    /// (`supergroup.status` / `updateSupergroup`, schema 1.8.67 line 2746).
    /// Drives the slow-mode bypass (admins/creators are exempt) and gates
    /// the admin slow-mode control. Absent = unknown (gated, no bypass).
    pub supergroup_member_status: HashMap<i64, ChannelMemberStatus>,
    /// The viewer's rights and the group's flags per supergroup, replaced
    /// whole by each `updateSupergroup` / `getSupergroup` (see
    /// [`SupergroupFacts`]).
    pub supergroups: HashMap<i64, SupergroupFacts>,
    /// Anti-spam state per supergroup, replaced whole by each
    /// `supergroupFullInfo` (see [`SupergroupAntiSpam`]).
    pub anti_spam: HashMap<i64, SupergroupAntiSpam>,
    /// Phase D3b: `getChatAdministrators` fetch state, keyed by chat id.
    pub admin_lists: HashMap<i64, AdminListFetch>,
    /// Phase D3b / slice G1: `getSupergroupMembers` fetch state for the
    /// promote member picker and the member-management dialog, keyed by
    /// (chat id, filter). One page per filter.
    /// Each list holds the query it was asked with, its first-page stage
    /// (and the request it waits for) and its next page; a search for
    /// another query replaces the value whole.
    pub supergroup_members: HashMap<(i64, MemberListFilter), MemberList>,
    /// Slice G1: `getBasicGroupFullInfo` fetch state (the member list for
    /// basic groups), keyed by chat id. Reuses `SupergroupMembersFetch`
    /// (Loading / Loaded / Failed).
    pub basic_group_members: HashMap<i64, SupergroupMembersFetch>,
    /// `basicGroupFullInfo.description`, keyed by basic group id.
    pub basic_group_descriptions: HashMap<i64, String>,
    /// The last refused `setChatTitle` / `setChatDescription` /
    /// `setChatPhoto` of the Edit box, keyed by chat id; cleared when the
    /// box saves again.
    pub info_edit_errors: HashMap<i64, Vec<GroupInfoEditError>>,
    /// The Type box: link checks, the public links limit and its list.
    pub group_type: GroupTypeState,
    /// New Group / New Channel creations, by their create request.
    pub new_chats: NewChatState,
    /// B7: `supergroup.join_to_send_messages` (schema 1.8.67, line 2746),
    /// keyed by supergroup id.
    pub supergroup_join_to_send: HashMap<i64, bool>,
    /// `supergroup.restriction_info.restriction_reason` is set: the
    /// content must be restricted (tdesktop `computeUnavailableReason`).
    pub supergroup_restricted: HashSet<i64>,
    /// The viewer's side in each channel direct messages group
    /// (`supergroup.is_direct_messages_group`), keyed by supergroup id.
    pub supergroup_direct_messages: HashMap<i64, crate::suggested_posts::DirectMessagesRole>,
    /// B7: the viewer's own `basicGroup.status`, keyed by basic group id.
    pub basic_group_status: HashMap<i64, ChannelMemberStatus>,
    /// B7: `can_change_info` of the viewer's administrator status in a
    /// basic group.
    pub basic_group_change_info_right: HashMap<i64, bool>,
    /// `can_manage_video_chats` of the viewer's administrator status in a
    /// basic group.
    pub basic_group_manage_video_chats_right: HashMap<i64, bool>,
    /// B7: `basicGroup.is_active` (false after the upgrade).
    pub basic_group_active: HashMap<i64, bool>,
    /// B7: steps waiting for a group-admin request to succeed.
    pub admin_followups: Vec<(RequestId, AdminFollowup)>,
    /// Slice G1: `addChatMembers` failure count from the last add
    /// attempt — `failedToAddMembers.failed_to_add_members.len()` for the
    /// bulk path (schema 1.8.67, line 3640), or the accumulated per-user
    /// `addChatMember` `failedToAddMembers` counts plus error responses
    /// for basic groups — keyed by chat id.
    /// Reset when a new add starts; cleared when the dialog closes.
    pub add_members_failed: HashMap<i64, i32>,
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
    pub private_sharing: PrivateSharingState,
}

impl GroupsState {
    /// `updateChatWelcomeMessages` carries the whole pack and asks nothing:
    /// it is the newest truth, so it replaces the pack whichever fetch was
    /// in flight (a late answer of that fetch finds `Ready` and is dropped).
    pub(crate) fn welcome_pack_pushed(
        &mut self,
        chat_id: i64,
        messages: Vec<ParsedWelcomeMessage>,
    ) {
        self.welcome_messages
            .entry(chat_id)
            .or_default()
            .succeed(messages);
    }

    pub(crate) fn new() -> Self {
        Self {
            ownership: OwnershipState::default(),
            basic_group_own: HashMap::new(),
            supergroup_member_counts: HashMap::new(),
            basic_group_member_counts: HashMap::new(),
            invite_link_error: Default::default(),
            supergroup_full_infos: HashMap::new(),
            communities: HashMap::new(),
            community_full_infos: CommunityFullInfos::default(),
            community_error: Default::default(),
            chat_statistics: HashMap::new(),
            stats: StatsState::default(),
            invite_links: HashMap::new(),
            join_requests: HashMap::new(),
            revoked_invite_links: HashMap::new(),
            invite_pages: InvitePages::default(),
            chat_boost_lists: HashMap::new(),
            chat_boost_links: HashMap::new(),
            similar_strips: HashMap::new(),
            supergroup_username_lists: HashMap::new(),
            invite_link_counts: HashMap::new(),
            revoked_link_deletions: HashMap::new(),
            pending_join_request_counts: HashMap::new(),
            pending_join_request_users: HashMap::new(),
            event_logs: HashMap::new(),
            supergroup_anonymous: HashMap::new(),
            supergroup_manage_video_chats_right: HashMap::new(),
            chat_has_welcome_messages: HashMap::new(),
            welcome_messages: HashMap::new(),
            chat_boost_status: HashMap::new(),
            chat_boost_features: HashMap::new(),
            unrestrict_boost_saves: HashMap::new(),
            supergroup_size_max: 0,
            subscription_star_max: None,
            stars_usd_rate: None,
            boost_box: None,
            boosts_per_gift: crate::state::fetch::Fetch::Idle,
            supergroup_usernames: HashMap::new(),
            supergroup_verification: HashMap::new(),
            supergroup_boost_levels: HashMap::new(),
            supergroup_chat_ids: HashMap::new(),
            supergroup_member_status: HashMap::new(),
            supergroups: HashMap::new(),
            anti_spam: HashMap::new(),
            admin_lists: HashMap::new(),
            supergroup_members: HashMap::new(),
            basic_group_members: HashMap::new(),
            basic_group_descriptions: HashMap::new(),
            info_edit_errors: HashMap::new(),
            group_type: GroupTypeState::default(),
            new_chats: NewChatState::default(),
            supergroup_join_to_send: HashMap::new(),
            supergroup_restricted: HashSet::new(),
            supergroup_direct_messages: HashMap::new(),
            basic_group_status: HashMap::new(),
            basic_group_change_info_right: HashMap::new(),
            basic_group_manage_video_chats_right: HashMap::new(),
            basic_group_active: HashMap::new(),
            admin_followups: Vec::new(),
            add_members_failed: HashMap::new(),
            member_action_error: HashMap::new(),
            admin_rights: HashMap::new(),
            private_sharing: PrivateSharingState::default(),
        }
    }
}

/// Which Edit box request TDLib refused, and why.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GroupInfoEditError {
    /// The refused request: a box reads only the errors of the requests its
    /// own Save sent.
    pub request: crate::ids::RequestId,
    pub field: GroupInfoField,
    /// TDLib's error class, readable (`error_reason`).
    pub reason: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GroupInfoField {
    Title,
    Description,
    Photo,
    /// The Type box's link (`setSupergroupUsername`).
    Username,
    /// A Save step that is no field: the basic group's upgrade (which the
    /// title and description wait on), or the history, auto-translate and
    /// signature switches. The box stays open with the typed edits.
    Settings,
}
