//! Full-info caches, fetch cursors and contact rows.
use super::*;

/// Phase 6: cached `userFullInfo` subset (schema 1.8.67, line 2468) — the
/// bio and the preferred profile-photo file from `photo:chatPhoto`
/// (parsed in `EnvelopePayload::UserFullInfo`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UserFullInfoData {
    pub bio: String,
    /// File id of the preferred `chatPhoto` size (`None` = no photo).
    /// The `ParsedFile` is cached in `Session::files` by the apply arm.
    pub photo_file_id: Option<i32>,
    /// A5: `chatPhoto.id` (schema 1.8.67, line 1030) — the
    /// `profile_photo_id` for `deleteProfilePhoto`.
    pub photo_id: Option<i64>,
    /// Slice A6: `userFullInfo.block_list` is `blockListMain` — drives
    /// the Block/Unblock label in the user info panel.
    pub blocked: bool,
}

/// Phase 6: cached `supergroupFullInfo` subset (schema 1.8.67, line 2792).
#[derive(Debug, Clone, PartialEq)]
pub struct SupergroupFullInfoData {
    pub description: String,
    pub member_count: i32,
    /// Parity slice: `linked_chat_id` (schema 1.8.67, line 2792) — the
    /// discussion-group chat id for a channel; 0 when none.
    pub linked_chat_id: i64,
    /// Phase A1: `slow_mode_delay` (schema 1.8.67, line 2758) — seconds
    /// between messages for non-administrator members; 0 = disabled.
    pub slow_mode_delay: i32,
    /// Phase A1: `slow_mode_delay_expires_in` (schema 1.8.67, line 2759)
    /// — seconds left at fetch time. Decays against `fetched_at_ms`: the
    /// schema warns no `updateSupergroupFullInfo` fires when only this
    /// changes while both old and new values are non-zero.
    pub slow_mode_delay_expires_in: f64,
    /// Phase A1: `my_boost_count` (schema 1.8.67, line 2779).
    pub my_boost_count: i32,
    /// Phase A1: `unrestrict_boost_count` (schema 1.8.67, line 2780) — the
    /// boosts needed to ignore slow mode; 0 if unspecified.
    pub unrestrict_boost_count: i32,
    /// Phase A1: wall-clock ms when this full info arrived (reducer
    /// stamp). `slow_mode_delay_expires_in` decays against it.
    pub fetched_at_ms: u64,
    /// Phase D2: `supergroupFullInfo.can_get_statistics` (schema 1.8.67,
    /// line 2792). Gates the channel statistics entry point in the info
    /// panel; `getChatStatistics` errors when false.
    pub can_get_statistics: bool,
    /// Slice S11: `supergroupFullInfo.can_set_sticker_set` (schema 1.8.67,
    /// line 2765) — true when the supergroup sticker set can be changed;
    /// gates the group sticker-set affordance.
    pub can_set_sticker_set: bool,
    /// Slice S11: `supergroupFullInfo.sticker_set_id` (schema 1.8.67,
    /// line 2792) — the installed group sticker set; 0 when none.
    pub sticker_set_id: i64,
    /// Slice S11: `supergroupFullInfo.custom_emoji_sticker_set_id`
    /// (schema 1.8.67, line 2792) — the group's custom-emoji set; 0
    /// when none.
    pub custom_emoji_sticker_set_id: i64,
}

impl Default for SupergroupFullInfoData {
    fn default() -> Self {
        Self {
            description: String::new(),
            member_count: 0,
            linked_chat_id: 0,
            slow_mode_delay: 0,
            slow_mode_delay_expires_in: 0.0,
            my_boost_count: 0,
            unrestrict_boost_count: 0,
            fetched_at_ms: 0,
            can_get_statistics: false,
            can_set_sticker_set: false,
            sticker_set_id: 0,
            custom_emoji_sticker_set_id: 0,
        }
    }
}

/// Phase D2: fetch state for one chat's `getChatStatistics` result
/// (schema 1.8.67, line 15760). Keyed by chat id. `Loading` is also the
/// in-flight guard — the driver never sends a second request while one
/// is outstanding.
#[derive(Debug, Clone, PartialEq)]
pub enum ChatStatisticsFetch {
    Loading,
    Loaded(Box<ChatStatistics>),
    Failed(String),
}

/// Phase D3a: fetch state for one chat's `getChatInviteLinks` result
/// (schema 1.8.67, line 14138). Keyed by chat id. `Loading` is the
/// in-flight guard — the driver never sends a second request while one
/// is outstanding.
#[derive(Debug, Clone, PartialEq)]
pub enum InviteLinkFetch {
    Loading,
    Loaded(InviteLinkList),
    Failed(String),
}

/// Phase D3a: one chat's invite-link list (`chatInviteLinks`, schema
/// 1.8.67, line 2630).
#[derive(Debug, Clone, PartialEq)]
pub struct InviteLinkList {
    pub total_count: i32,
    pub links: Vec<ParsedChatInviteLink>,
}

/// Phase D3a: fetch state for one chat's `getChatJoinRequests` result
/// (schema 1.8.67, line 14174). Same Loading-guard convention.
#[derive(Debug, Clone, PartialEq)]
pub enum JoinRequestFetch {
    Loading,
    Loaded(JoinRequestList),
    Failed(String),
}

/// Phase D3a: one chat's join-request list (`chatJoinRequests`, schema
/// 1.8.67, line 2691).
#[derive(Debug, Clone, PartialEq)]
pub struct JoinRequestList {
    pub total_count: i32,
    pub requests: Vec<ParsedChatJoinRequest>,
}

/// Phase D3b: fetch state for one administrator's `getChatMember` rights
/// lookup (schema 1.8.67, line 13622), keyed by (chat_id, user_id).
/// Drives the edit-rights dialog's loading / error states.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdminRightsFetch {
    Loading,
    Loaded(ChatAdminRights),
    Failed(String),
}

/// Phase D3b: fetch state for one chat's `getChatAdministrators` result
/// (schema 1.8.67, line 13632). Keyed by chat id. `Loading` is the
/// in-flight guard — the driver never sends a second request while one
/// is outstanding.
#[derive(Debug, Clone, PartialEq)]
pub enum AdminListFetch {
    Loading,
    Loaded(Vec<ChatAdministratorEntry>),
    Failed(String),
}

/// Phase D3b: fetch state for one chat's `getSupergroupMembers` result
/// (schema 1.8.67, line 15238) backing the promote member picker. Same
/// Loading-guard convention.
#[derive(Debug, Clone, PartialEq)]
pub enum SupergroupMembersFetch {
    Loading,
    Loaded {
        members: Vec<ParsedChatMember>,
        total_count: i32,
    },
    Failed(String),
}

/// B4: one `getPollVoters` page (schema 1.8.67, line 12941) backing the
/// poll-voters dialog. Same Loading-guard convention; a failed first page
/// lands in `Failed`, a failed "load more" keeps the loaded page.
#[derive(Debug, Clone, PartialEq)]
pub enum PollVotersFetch {
    Loading,
    Loaded {
        voters: Vec<MessageSender>,
        total_count: i32,
    },
    Failed(String),
}

/// Bots slice: fetch state for the single active inline query
/// (`getInlineQueryResults`, schema 1.8.67, line 13019). Mirrors
/// `PollVotersFetch`: a failed first page lands in `Failed`, a failed
/// page-next keeps the loaded page.
#[derive(Debug, Clone, PartialEq)]
pub enum InlineQueryFetch {
    Loading,
    Loaded {
        inline_query_id: i64,
        button: Option<InlineQueryResultsButton>,
        results: Vec<InlineQueryResultSummary>,
        next_offset: String,
    },
    Failed(String),
}

/// Bots slice: the single active inline-query slot — which (chat, bot)
/// the results belong to, the query text that produced them, and the
/// current fetch state.
#[derive(Debug, Clone, PartialEq)]
pub struct InlineQuerySlot {
    pub chat_id: ChatId,
    pub bot_user_id: i64,
    pub query: String,
    pub fetch: InlineQueryFetch,
}

/// Bots slice: `@botname` → bot user id resolution for inline mode.
/// `is_inline` is `Some` when the bot was found in the local user cache
/// (`ParsedUser.is_inline`); `None` when it came from `searchPublicChat`
/// (the chat answer carries no user object — the query attempt itself is
/// the capability check, and its error surfaces honestly). `generation`
/// correlates the in-flight request with its answer (see
/// `RequestPurpose::ResolveInlineBot`).
#[derive(Debug, Clone, PartialEq)]
pub enum InlineBotResolve {
    Resolving {
        username: String,
        generation: u64,
    },
    Resolved {
        username: String,
        user_id: i64,
        is_inline: Option<bool>,
    },
    Failed {
        username: String,
        reason: String,
    },
}

/// Phase D3c: `getChatEventLog` page size (schema 1.8.67, line 15252:
/// "up to 100"). Shared by the driver and the `has_more` heuristic in
/// `Session::apply` — a short page means the log is exhausted.
pub const CHAT_EVENT_LOG_PAGE_SIZE: i32 = 100;

/// Phase D3c: fetch state for one chat's `getChatEventLog` result
/// (schema 1.8.67, line 15252). Keyed by chat id. `Loading` is the
/// in-flight guard — the driver never sends a second request while one
/// is outstanding.
#[derive(Debug, Clone, PartialEq)]
pub enum ChatEventLogFetch {
    Loading,
    Loaded(ChatEventLogPage),
    Failed(String),
}

/// Phase D3c: one cached `getChatEventLog` result. Events arrive in
/// reverse chronological order (decreasing event `id`, schema 1.8.67,
/// line 15252); pages append older events, deduped by event id.
/// `has_more` is true when the last page was full — an older page is
/// worth requesting.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ChatEventLogPage {
    pub events: Vec<ParsedChatEvent>,
    pub has_more: bool,
}

impl ChatEventLogPage {
    /// Slice G2: distinct admin user ids (first-seen order) from a loaded
    /// page; drives the per-admin filter chips in the info panel. Chat
    /// senders (`MessageSender::Chat`) are not admins and are skipped.
    pub fn admin_user_ids(&self) -> Vec<i64> {
        let mut admins = Vec::new();
        for event in &self.events {
            if let MessageSender::User { user_id } = event.member_id
                && !admins.contains(&user_id)
            {
                admins.push(user_id);
            }
        }
        admins
    }
}

/// Slice G2: fetch state for one chat's welcome-message pack
/// (`loadChatWelcomeMessages`, schema 1.8.67, line 12630). `Loading` is
/// the in-flight guard — the driver never sends a second request while
/// one is outstanding.
#[derive(Debug, Clone, PartialEq)]
pub enum WelcomeMessagesFetch {
    Loading,
    Loaded,
    Failed(String),
}

/// Slice G2: the channel-comments viewer result — the latest
/// `getMessageThreadHistory` answer for one channel post.
#[derive(Debug, Clone, PartialEq)]
pub struct CommentThreadFetch {
    pub chat_id: ChatId,
    pub message_id: MessageId,
    pub messages: Vec<ParsedMessage>,
    pub failed: Option<String>,
}

/// Slice CL: the chat-list peek preview result — the latest
/// `getChatHistory` answer for one unopened chat.
#[derive(Debug, Clone, PartialEq)]
pub struct PreviewHistoryFetch {
    pub chat_id: ChatId,
    pub messages: Vec<ParsedMessage>,
    pub failed: Option<String>,
}

/// Phase D3c: relative timestamp for admin-log rows. The log only
/// covers the last 48 hours (schema 1.8.67, line 15252), so relative
/// forms are always meaningful; no date crate is pulled in for this.
/// Pure in `now_unix` for tests. `pub` (not `pub(crate)`) because the
/// `ui` module is built against the lib as an external crate.
pub fn event_log_relative_time_for(date_unix: i32, now_unix: i64) -> String {
    let age = now_unix.saturating_sub(i64::from(date_unix));
    if age < 60 {
        "just now".to_owned()
    } else if age < 3600 {
        format!("{}m ago", age / 60)
    } else if age < 86_400 {
        format!("{}h ago", age / 3600)
    } else {
        format!("{}d ago", age / 86_400)
    }
}

/// Phase D3c: relative timestamp for admin-log rows, against the
/// current wall clock. `pub` (not `pub(crate)`) because the `ui` module
/// is built against the lib as an external crate.
pub fn event_log_relative_time(date_unix: i32) -> String {
    let now_unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    event_log_relative_time_for(date_unix, now_unix)
}

/// Phase A1: wall-clock milliseconds. Used to timestamp
/// `supergroupFullInfo` arrivals so the slow-mode expiry decays locally.
pub fn unix_ms_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Phase 6: one rendered contacts-list row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContactRow {
    pub user_id: i64,
    pub name: String,
    pub status_text: String,
    pub is_online: bool,
    pub is_contact: bool,
}

/// `parity:platform-deep-links`: the actionable destination parsed out of
/// a `deepLinkInfo` answer. TDLib marks the action with
/// `textEntityTypeTextUrl` entities whose `url` is a `tg://` URL (see
/// `parse_deep_link_action` in `connect::deep_links`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeepLinkAction {
    /// `tg://resolve?domain=<username>` with optional `start=`,
    /// `startgroup=`, `post=`, `story=` query params.
    OpenUsername {
        domain: String,
        start_param: Option<String>,
        post: Option<i64>,
        story_id: Option<i32>,
    },
    /// `tg://join?invite=<hash>`.
    JoinInvite { hash: String },
    /// `tg://openmessage?user_id=<id>&message_id=<id>`.
    OpenMessage { user_id: i64, message_id: i64 },
}

/// `parity:platform-deep-links`: the single active deep-link flow. One
/// slot, not a map — the app processes at most one launch link.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeepLinkState {
    /// `getDeepLinkInfo` in flight; `generation` drops stale answers.
    ResolvingInfo { generation: u64 },
    /// `deepLinkInfo` answer parsed. The UI consumes this once: with an
    /// action it fires the follow-up request, otherwise (or when
    /// `need_update`) it shows `text` in a dialog.
    Info {
        text: String,
        need_update: bool,
        action: Option<DeepLinkAction>,
        generation: u64,
    },
    /// Follow-up request (`searchPublicChat` / `createPrivateChat` /
    /// `joinChatByInviteLink`) in flight.
    ResolvingChat {
        action: DeepLinkAction,
        generation: u64,
    },
    /// Follow-up resolved to a chat; the UI consumes this once to open
    /// the chat (and jump / prefill / open the story per `action`).
    ChatReady {
        chat_id: ChatId,
        action: DeepLinkAction,
    },
    /// Info or error text for the UI to show in a dialog, consumed once.
    ShowText(String),
}
