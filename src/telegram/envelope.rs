use crate::ids::{ChatId, FileId, MessageId, RequestId, UserId};
use crate::text::{TextEntity, TextEntityKind, utf16_to_utf8_offset};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde::Deserialize;
use serde_json::Value;
use std::str::FromStr;

#[derive(Debug, Clone, PartialEq)]
pub struct Envelope {
    pub type_name: String,
    pub extra: Option<RequestId>,
    pub client_id: Option<i32>,
    pub payload: EnvelopePayload,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InviteGroupCallParticipantResult {
    /// `inviteGroupCallParticipantResultSuccess` — carries the
    /// invitation service message's chat/message ids (usable with
    /// `declineGroupCallInvitation` to cancel).
    Success {
        chat_id: i64,
        message_id: i64,
    },
    UserPrivacyRestricted,
    UserAlreadyParticipant,
    UserWasBanned,
}

#[derive(Debug, Clone, PartialEq)]
pub enum EnvelopePayload {
    UpdateAuthorizationState(AuthorizationState),
    UpdateNewMessage(ParsedMessage),
    UpdateMessageSendSucceeded {
        message: ParsedMessage,
        old_message_id: MessageId,
    },
    UpdateMessageSendFailed {
        message: ParsedMessage,
        old_message_id: MessageId,
        error: TdError,
    },
    UpdateMessageSendAcknowledged {
        chat_id: ChatId,
        message_id: MessageId,
    },
    UpdateDeleteMessages {
        chat_id: ChatId,
        message_ids: Vec<MessageId>,
        is_permanent: bool,
        from_cache: bool,
    },
    UpdateMessageContent {
        chat_id: ChatId,
        message_id: MessageId,
        content: MessageContent,
        files: Vec<ParsedFile>,
    },
    /// `updateMessageContentOpened` — voice note listened (`is_listened`) or
    /// video note viewed (`is_viewed`).
    UpdateMessageContentOpened {
        chat_id: ChatId,
        message_id: MessageId,
    },
    /// `updateMessageEdited` (TDLib 1.8.67, `schema/td_api.tl:10431`): bots
    /// edit inline keyboards this way — the new `reply_markup` replaces the
    /// message's keyboard (Phase 3.2).
    UpdateMessageEdited {
        chat_id: ChatId,
        message_id: MessageId,
        edit_date: i32,
        reply_markup: Option<InlineKeyboard>,
    },
    /// `updatePoll` (TDLib 1.8.67, `schema/td_api.tl:11179`): vote counts /
    /// chosen marks changed. Carries only the new `poll` — no chat or
    /// message id — so the reducer matches it by `poll.id` (Phase 4.2).
    UpdatePoll {
        poll: Poll,
    },
    UpdateChatPosition(ChatPositionUpdate),
    UpdateChatTitle {
        chat_id: ChatId,
        title: String,
    },
    UpdateChatLastMessage {
        chat_id: ChatId,
        last_message: Option<ParsedMessage>,
        positions: Vec<ChatPositionUpdate>,
    },
    UpdateChatAddedToList {
        chat_id: ChatId,
        list: ChatList,
    },
    UpdateChatRemovedFromList {
        chat_id: ChatId,
        list: ChatList,
    },
    UpdateChatReadInbox {
        chat_id: ChatId,
        last_read_inbox_message_id: MessageId,
        unread_count: i32,
    },
    UpdateChatReadOutbox {
        chat_id: ChatId,
        last_read_outbox_message_id: MessageId,
    },
    UpdateConnectionState(ConnectionState),
    UpdateNewChat {
        chat_id: ChatId,
        title: String,
        kind: ChatKind,
        unread_count: i32,
        last_read_inbox_message_id: MessageId,
        last_read_outbox_message_id: MessageId,
        notification_settings: ChatNotificationSettings,
        /// `chat.draft_message`. Null when the chat has no draft.
        draft: Option<ChatDraft>,
        /// Parity slice: `chat.photo.small` (`chatPhotoInfo`, schema 1.8.67,
        /// line 762). `None` when the chat has no photo.
        photo: Option<ParsedFile>,
        /// Parity slice 4: `chat.permissions.can_send_basic_messages`
        /// (`chatPermissions`, schema 1.8.67, line 1070). Defaults to true
        /// when the block is absent (the real `chat` object always carries
        /// it); refreshed by `updateChatPermissions`.
        can_send_basic_messages: bool,
        /// Phase B4: `chat.message_auto_delete_time` (schema 1.8.67,
        /// lines 3616 / 3627) — the chat-level auto-delete or
        /// self-destruct (secret chats) timer, in seconds; 0 when
        /// disabled. Refreshed by `updateChatMessageAutoDeleteTime`.
        message_auto_delete_time: i32,
        /// Phase C3a: `chat.video_chat` (`videoChat`, schema 1.8.67,
        /// lines 3576 / 3579 / 3627). `None` when `group_call_id` is 0
        /// (no active video chat).
        video_chat: Option<ParsedVideoChat>,
    },
    /// `updateChatDraftMessage`. Positions are the new chat-list orders.
    UpdateChatDraftMessage {
        chat_id: ChatId,
        draft: Option<ChatDraft>,
        positions: Vec<ChatPositionUpdate>,
    },
    /// Parity slice 4: `updateChatPermissions` (schema 1.8.67, line 10500).
    /// Only `can_send_basic_messages` is kept — the topic-composer gate.
    UpdateChatPermissions {
        chat_id: ChatId,
        can_send_basic_messages: bool,
    },
    /// Phase B4: `updateChatMessageAutoDeleteTime` (schema 1.8.67,
    /// line 10549) — the chat-level auto-delete or self-destruct
    /// (secret chats) timer changed.
    UpdateChatMessageAutoDeleteTime {
        chat_id: ChatId,
        message_auto_delete_time: i32,
    },
    /// `updateUser` — Phase 6 keeps the full parsed user (contacts list,
    /// user info panel) in `Session::users`; the bot bit still drives the
    /// private-chat draft gate.
    UpdateUser {
        user_id: UserId,
        user: ParsedUser,
    },
    /// `updateUserStatus` (schema 1.8.67, line 10729) — online / last-seen
    /// for a known user; refreshes the contacts list row.
    UpdateUserStatus {
        user_id: UserId,
        status: UserStatusKind,
    },
    /// `users` — `getContacts` response (schema 1.8.67, line 2471). Only
    /// the ids are authoritative here; the user objects themselves arrive
    /// via `updateUser`. `total_count` is dropped.
    Users {
        user_ids: Vec<i64>,
    },
    /// `updateChatNotificationSettings` — chat mute / sound exception changed.
    UpdateChatNotificationSettings {
        chat_id: ChatId,
        notification_settings: ChatNotificationSettings,
    },
    /// Parity slice: `updateChatPhoto` (schema 1.8.67, line 10488) — the
    /// chat's photo changed. `photo` is the new `chatPhotoInfo.small`
    /// file (`None` when the photo was removed).
    UpdateChatPhoto {
        chat_id: ChatId,
        photo: Option<ParsedFile>,
    },
    /// `updateChatAction` — peer activity (`chatActionTyping` / `chatActionCancel`).
    UpdateChatAction {
        chat_id: ChatId,
        sender: MessageSender,
        action: ChatAction,
    },
    /// Phase B1: `updateSecretChat` (schema 1.8.67, line 10741) — the
    /// secret chat's state changed. Guaranteed to arrive before the
    /// chat identifier is returned (i.e. before `updateNewChat`).
    UpdateSecretChat {
        secret_chat: ParsedSecretChat,
    },
    /// Phase B1: `secretChat` (schema 1.8.67, line 2816) — the
    /// `getSecretChat` answer.
    SecretChat {
        secret_chat: ParsedSecretChat,
    },
    /// Phase C1: `updateCall` (schema 1.8.67, line 10816) — a new call
    /// was created or information about a call was updated.
    UpdateCall {
        call: ParsedCall,
    },
    /// Phase C1: `updateNewCallSignalingData` (schema 1.8.67, line
    /// 10862) — new call signaling data arrived. Quill has no media
    /// transport yet (C2), so the session queues it honestly; nothing
    /// consumes it.
    UpdateNewCallSignalingData {
        call_id: i32,
        data: Vec<u8>,
    },
    /// Phase C1: `callId` (schema 1.8.67, line 7034) — the `createCall`
    /// answer. Correlated to the outgoing request via `@extra` /
    /// `RequestPurpose::CreateCall`.
    CallId {
        id: i32,
    },
    /// Phase C3a: `groupCallId` (schema 1.8.67, line 7037) — the
    /// `createVideoChat` answer. Correlated via `@extra` /
    /// `RequestPurpose::CreateVideoChat`.
    GroupCallId {
        id: i32,
    },
    /// Phase C2f: `groupCallInfo` (schema 1.8.67, line 7190) — the
    /// `joinGroupCall` answer (invitation acceptance). Correlated via
    /// `@extra` / `RequestPurpose::JoinGroupCallInvitation`. The
    /// `join_payload` is the tgcalls payload (stored, never consumed —
    /// no media transport until Phase C2); `updateGroupCall` remains
    /// the source of truth for join state.
    GroupCallInfo {
        group_call_id: i32,
        join_payload: String,
    },
    /// Phase C3a: `updateGroupCall` (schema 1.8.67, line 10819) — a
    /// group call was created or its information was updated.
    UpdateGroupCall {
        group_call: ParsedGroupCall,
    },
    /// Phase C3a: `updateGroupCallParticipant` (schema 1.8.67, line
    /// 10824) — information about a group call participant changed.
    UpdateGroupCallParticipant {
        group_call_id: i32,
        participant: ParsedGroupCallParticipant,
    },
    /// Phase C3a: `updateGroupCallParticipants` (schema 1.8.67, line
    /// 10830) — the participant list changed; carries only user ids.
    UpdateGroupCallParticipants {
        group_call_id: i32,
        participant_user_ids: Vec<i64>,
    },
    /// Phase C3a: `updateGroupCallVerificationState` (schema 1.8.67,
    /// line 10836) — E2E verification emojis for the group call.
    UpdateGroupCallVerificationState {
        group_call_id: i32,
        generation: i32,
        emojis: Vec<String>,
    },
    /// Phase C3a: `updateChatVideoChat` (schema 1.8.67, line 10576) —
    /// a chat's video chat changed.
    UpdateChatVideoChat {
        chat_id: i64,
        video_chat: ParsedVideoChat,
    },
    /// Phase C2h: `updateNewGroupCallMessage` (schema 1.8.67, line
    /// 10839) — a message was sent in a group call (including by the
    /// current user; the echo is the confirmation).
    UpdateNewGroupCallMessage {
        group_call_id: i32,
        message: ParsedGroupCallMessage,
    },
    /// Phase C2h: `updateGroupCallMessageSendFailed` (schema 1.8.67,
    /// line 10851) — a sent group-call message failed.
    UpdateGroupCallMessageSendFailed {
        group_call_id: i32,
        message_id: i32,
        error: TdError,
    },
    /// Phase C2h: `updateGroupCallMessagesDeleted` (schema 1.8.67,
    /// line 10856) — group-call messages were deleted.
    UpdateGroupCallMessagesDeleted {
        group_call_id: i32,
        message_ids: Vec<i32>,
    },
    Ok,
    /// Phase C3a: `text` (schema 1.8.67, line 10071) — the
    /// `joinVideoChat` answer (join payload for tgcalls).
    Text {
        text: String,
    },
    /// Phase C3a: `httpUrl` (schema 1.8.67, line 7458) — the
    /// `getVideoChatInviteLink` answer.
    HttpUrl {
        url: String,
    },
    /// Phase C2h: `rtmpUrl` (schema 1.8.67, line 7113) — the
    /// `getVideoChatRtmpUrl` / `replaceVideoChatRtmpUrl` answer.
    RtmpUrl {
        url: String,
        stream_key: String,
    },
    /// Phase C2f: `inviteGroupCallParticipantResult*` (schema 1.8.67,
    /// lines 7216-7227) — the `inviteGroupCallParticipant` answer.
    InviteGroupCallParticipantResult(InviteGroupCallParticipantResult),
    Error(TdError),
    Messages(Vec<ParsedMessage>),
    Message(ParsedMessage),
    /// `chats` — `searchChats` / `searchRecentlyFoundChats` / similar.
    Chats {
        total_count: i32,
        chat_ids: Vec<ChatId>,
    },
    /// `foundMessages` — `searchMessages` (and secret-chat search).
    FoundMessages {
        total_count: i32,
        messages: Vec<ParsedMessage>,
        next_offset: String,
    },
    /// Phase C2i: `userPrivacySettingRules` — `getUserPrivacySettingRules`.
    /// Rule constructor names (`userPrivacySettingRuleAllowAll`, …).
    UserPrivacySettingRules {
        rules: Vec<String>,
    },
    /// `foundChatMessages` — `searchChatMessages`.
    FoundChatMessages {
        total_count: i32,
        messages: Vec<ParsedMessage>,
        next_from_message_id: MessageId,
    },
    /// `updateSupergroup` — `supergroup.is_forum` is how Quill learns a
    /// supergroup is a forum (`chatTypeSupergroup` has no forum flag).
    /// Parity slice: the first active username (`supergroup.usernames`,
    /// schema 1.8.67 lines 2746/2372) feeds the channel/supergroup header.
    UpdateSupergroup {
        supergroup_id: i64,
        is_forum: bool,
        username: String,
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
    },
    /// `supergroup` — `getSupergroup` response. Phase A1: also keeps own
    /// `status` (`supergroup.status`, schema 1.8.67 line 2746) for the
    /// slow-mode bypass check.
    Supergroup {
        supergroup_id: i64,
        is_forum: bool,
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
    },
    /// `forumTopics` — `getForumTopics` response. Only the first page is
    /// fetched; `next_offset_*` are dropped (see Phase 5.1 DECISIONS).
    ForumTopics {
        total_count: i32,
        topics: Vec<ForumTopic>,
    },
    UpdateFile(ParsedFile),
    File(ParsedFile),
    /// `stickerSets` — `getInstalledStickerSets`.
    StickerSets {
        total_count: i32,
        sets: Vec<StickerSetInfo>,
    },
    /// `stickerSet` — `getStickerSet`. Files on each sticker are in `files`.
    StickerSet {
        id: i64,
        title: String,
        name: String,
        stickers: Vec<StickerItem>,
        files: Vec<ParsedFile>,
    },
    /// `animations` — `getSavedAnimations`.
    Animations {
        animations: Vec<AnimationItem>,
        files: Vec<ParsedFile>,
    },
    /// `sponsoredMessages` — `getChatSponsoredMessages`. `messages_between` is
    /// the minimum number of ordinary messages between shown sponsored rows
    /// (0 = show after all ordinary messages).
    SponsoredMessages {
        messages: Vec<SponsoredMessage>,
        files: Vec<ParsedFile>,
        messages_between: i32,
    },
    /// `ReportSponsoredResult` — `reportChatSponsoredMessage` /
    /// `reportSponsoredChat` response.
    ReportSponsoredResult(ReportSponsoredResult),
    /// `updateSavedAnimations` — file ids of saved GIFs, newest first.
    UpdateSavedAnimations {
        animation_ids: Vec<i32>,
    },
    /// `notificationSounds` — `getSavedNotificationSounds` response.
    NotificationSounds {
        sounds: Vec<NotificationSound>,
    },
    /// `updateSavedNotificationSounds` — the saved-sound list changed;
    /// the reducer marks the cached list stale (schema line 10947).
    UpdateSavedNotificationSounds {
        sound_ids: Vec<i64>,
    },
    /// `scopeNotificationSettings` — `getScopeNotificationSettings` response.
    ScopeNotificationSettings {
        scope: NotificationSettingsScope,
        settings: ScopeNotificationSettings,
    },
    /// `updateScopeNotificationSettings` — a scope's defaults changed
    /// (schema line 10668).
    UpdateScopeNotificationSettings {
        scope: NotificationSettingsScope,
        settings: ScopeNotificationSettings,
    },
    /// `updateMessageInteractionInfo` — views / forwards / `messageReactions`.
    UpdateMessageInteractionInfo {
        chat_id: ChatId,
        message_id: MessageId,
        interaction_info: Option<MessageInteractionInfo>,
    },
    /// `updateMessageIsPinned` — message pin state changed (TDLib 1.8.67).
    UpdateMessageIsPinned {
        chat_id: ChatId,
        message_id: MessageId,
        is_pinned: bool,
    },
    /// `chatMember` — `getChatMember` response for a channel.
    ChatMember {
        member: ParsedChatMember,
    },
    /// `updateChatMember` — own or peer membership changed; only the
    /// `new_chat_member` is kept.
    UpdateChatMember {
        chat_id: ChatId,
        member: ParsedChatMember,
    },
    /// `user` — `getMe` response; only the id is kept.
    Me {
        user_id: i64,
    },
    /// `userFullInfo` — `getUserFullInfo` response (schema 1.8.67,
    /// `getUserFullInfo user_id:int53 = UserFullInfo`, line 11501). The
    /// response carries no user id; it is resolved from the pending
    /// request in `Session::apply`, so only `bot_info` (from
    /// `userFullInfo.bot_info:botInfo`, line 2468), `bio` (from
    /// `userFullInfo.bio:formattedText`) and the preferred profile-photo
    /// file (from `userFullInfo.photo:chatPhoto` sizes) are kept.
    UserFullInfo {
        bot_info: Option<BotInfo>,
        bio: String,
        /// Preferred size's `photo:file` from `chatPhoto.sizes`
        /// (schema 1.8.67, line 1030); `None` when the user has no photo.
        photo: Option<ParsedFile>,
    },
    /// `updateUserFullInfo` — full info changed (schema 1.8.67, line 10744);
    /// the user id is explicit here.
    UpdateUserFullInfo {
        user_id: UserId,
        bot_info: Option<BotInfo>,
        bio: String,
        photo: Option<ParsedFile>,
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
    /// entry point. Dropped: admin/restricted/banned counts, invite link,
    /// sticker sets, gift fields, paid-message and other statistics flags,
    /// location.
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
    },
    /// Phase D2: `chatStatisticsChannel` / `chatStatisticsSupergroup` —
    /// `getChatStatistics` response (schema 1.8.67, line 15760). The
    /// response carries no chat id; it is resolved from the pending
    /// request in `Session::apply`.
    ChatStatistics {
        statistics: ChatStatistics,
    },
    /// Phase D3a: `chatInviteLink` response (TDLib 1.8.67, line 2627) —
    /// the created/edited link from `createChatInviteLink` /
    /// `editChatInviteLink`. Correlated to the chat by the request's
    /// `PendingRequest::chat_id`.
    ChatInviteLink {
        link: ParsedChatInviteLink,
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
    /// Phase D3b: `chatAdministrators` (TDLib 1.8.67, line 2485) — the
    /// response of `getChatAdministrators` (line 13632). Carries no chat
    /// id; correlated to the chat by the request's
    /// `PendingRequest::chat_id`.
    ChatAdministrators {
        administrators: Vec<ChatAdministratorEntry>,
    },
    /// Phase D3c: `chatEvents` (TDLib 1.8.67, line 7938) — the response
    /// of `getChatEventLog` (line 15252). Carries no chat id; correlated
    /// to the chat by the request's `PendingRequest::chat_id`. Events
    /// arrive in reverse chronological order (decreasing event `id`).
    ChatEvents {
        events: Vec<ParsedChatEvent>,
    },
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
    },
    /// `botCommands` — `getCommands` response (TDLib 1.8.67,
    /// `schema/td_api.tl:829`): the bot's commands for the requested scope
    /// as a bare `vector<botCommand>`. The schema annotates `getCommands`
    /// "for bots only" (line 14953); on a user session the response is an
    /// `error` instead, which `Session::apply` absorbs silently. The
    /// response's `bot_user_id` is cached as the global-scope command set
    /// shown below the bot's `botInfo` commands in the 3.3 `/` menu.
    BotCommands {
        bot_user_id: UserId,
        commands: Vec<BotCommand>,
    },
    /// `ChatJoinResult` — `joinChat` response.
    JoinChatResult(ChatJoinResult),
    /// `callbackQueryAnswer` — response to `getCallbackQueryAnswer` after an
    /// inline keyboard callback-button press (Phase 3.2).
    CallbackQueryAnswer(CallbackQueryAnswer),
    /// `updateChatFolders` (TDLib 1.8.67, `schema/td_api.tl:10606`) — the
    /// full ordered folder list. There is no `getChatFolders` function in
    /// 1.8.67; TDLib pushes this update after authorization and whenever
    /// folders change. `main_chat_list_position` is dropped (folder reorder
    /// always sends position 0); `are_tags_enabled` is kept (parity slice:
    /// folder tags UI).
    UpdateChatFolders {
        folders: Vec<ChatFolderInfo>,
        are_tags_enabled: bool,
    },
    /// Parity slice: `chatFolderInfo` as the response of `createChatFolder`
    /// / `editChatFolder` (TDLib 1.8.67, `schema/td_api.tl:13358` /
    /// `:13361`). The reducer upserts it into `Session::chat_folders`;
    /// `updateChatFolders` stays the source of truth.
    ChatFolderInfo(ChatFolderInfo),
    /// Parity slice: `chatFolder` as the response of `getChatFolder`
    /// (TDLib 1.8.67, `schema/td_api.tl:13355`) — the full editable spec
    /// for the edit dialog prefill / remove-from-folder chain.
    ChatFolder {
        spec: ChatFolderSpec,
    },
    /// Parity slice: `chatLists` as the response of `getChatListsToAddChat`
    /// (TDLib 1.8.67, `schema/td_api.tl:13347`) — the chat lists a chat may
    /// be added to via `addChatToList`. Correlated to the chat by the
    /// request's `PendingRequest::chat_id`.
    ChatLists {
        lists: Vec<ChatList>,
    },
    /// Phase 9.1: `updateChatActiveStories` (TDLib 1.8.67,
    /// `schema/td_api.tl:10911`) — the active stories of a chat changed.
    /// The reducer keeps it in `Session::story_tray`; only entries with
    /// `list == Main` are shown in the story tray above the chat list.
    UpdateChatActiveStories {
        active_stories: ChatActiveStoriesView,
    },
    /// Phase 9.1: `chatActiveStories` — the `getChatActiveStories` response
    /// (TDLib 1.8.67, `schema/td_api.tl:13768`); handled like the update.
    ChatActiveStories {
        active_stories: ChatActiveStoriesView,
    },
    /// Phase 9.1: `story` — the `getStory` response (TDLib 1.8.67,
    /// `schema/td_api.tl:13695`); also the `updateStory` update (line
    /// 10895). The reducer keeps it in `Session::stories` keyed by
    /// `(poster_chat_id, id)` for the story viewer.
    Story {
        story: ParsedStory,
        files: Vec<ParsedFile>,
    },
    /// Phase 9.2: `updateStoryDeleted` (TDLib 1.8.67, `schema/td_api.tl:10898`)
    /// — a story was deleted. The reducer drops it from `Session::stories`
    /// and from the poster's tray entry.
    UpdateStoryDeleted {
        poster_chat_id: i64,
        story_id: i32,
    },
    /// Phase 9.2: `updateStoryPostSucceeded` (TDLib 1.8.67,
    /// `schema/td_api.tl:10901`) — a story posted from another client is
    /// live. The reducer upserts it into `Session::stories` and asks the
    /// driver to refresh the poster's tray (`getChatActiveStories`), so an
    /// own story appears in the tray.
    UpdateStoryPostSucceeded {
        story: ParsedStory,
        files: Vec<ParsedFile>,
        old_story_id: i32,
    },
    /// Phase 9.2: `updateStoryPostFailed` (TDLib 1.8.67,
    /// `schema/td_api.tl:10907`) — a story failed to post. The reducer drops
    /// the failed story from `Session::stories` and the poster's tray entry
    /// (it never went live). Unreachable without `sendStory` (absent from
    /// 1.8.67), parsed for schema completeness.
    UpdateStoryPostFailed {
        story: ParsedStory,
        error: TdError,
    },
    /// Phase 9.2: `availableReactions` — the `getStoryAvailableReactions`
    /// response (TDLib 1.8.67, `schema/td_api.tl:13802`). The reducer keeps
    /// it in `Session::story_available_reactions` for the viewer picker.
    StoryAvailableReactions {
        reactions: Vec<StoryAvailableReactionView>,
    },
    Unknown(UnknownKind),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownKind {
    pub type_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TdError {
    pub code: i32,
    /// Error *class* only. The TDLib `message` field is not stored; it can
    /// contain phone numbers or other secrets.
    pub class: ErrorClass,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorClass {
    NotFound,
    Unauthorized,
    Flood,
    Invalid,
    Other,
}

impl TdError {
    pub fn from_code(code: i32) -> Self {
        let class = match code {
            404 => ErrorClass::NotFound,
            401 => ErrorClass::Unauthorized,
            429 => ErrorClass::Flood,
            400 => ErrorClass::Invalid,
            _ => ErrorClass::Other,
        };
        Self { code, class }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthorizationState {
    WaitTdlibParameters,
    WaitPhoneNumber,
    WaitPremiumPurchase,
    WaitEmailAddress,
    WaitEmailCode,
    WaitCode { code_length: Option<i32> },
    WaitOtherDeviceConfirmation,
    WaitRegistration,
    WaitPassword { has_recovery_email: bool },
    Ready,
    LoggingOut,
    Closing,
    Closed,
    Unknown(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    WaitingForNetwork,
    ConnectingToProxy,
    Connecting,
    Updating,
    Ready,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatKind {
    Private {
        user_id: UserId,
    },
    BasicGroup {
        basic_group_id: i64,
    },
    Supergroup {
        supergroup_id: i64,
        is_channel: bool,
    },
    Secret {
        secret_chat_id: i32,
        user_id: UserId,
    },
    Unknown,
}

impl ChatKind {
    /// Phase B1: secret chats are now supported — they render, open,
    /// and send through the same chat pipeline as cloud chats (TDLib
    /// handles the E2E crypto internally). The old name said "cloud";
    /// kept short since it gates general chat support now.
    pub fn is_supported_chat(&self) -> bool {
        match self {
            ChatKind::Private { .. } | ChatKind::BasicGroup { .. } => true,
            // Phase 2.2: broadcast channels are ungated (sponsored-content
            // handling landed in 2.1).
            ChatKind::Supergroup { .. } => true,
            // Phase B1: secret chats ungated.
            ChatKind::Secret { .. } => true,
            ChatKind::Unknown => false,
        }
    }

    pub fn gate_reason(&self) -> Option<&'static str> {
        match self {
            ChatKind::Unknown => Some("This conversation type is not supported yet."),
            _ => None,
        }
    }

    /// `chatTypeSupergroup` with `is_channel: true` (TDLib 1.8.67).
    pub fn is_channel(&self) -> bool {
        matches!(
            self,
            ChatKind::Supergroup {
                is_channel: true,
                ..
            }
        )
    }
}

/// Phase B1: `SecretChatState` (TDLib 1.8.67, `schema/td_api.tl:2795`):
/// `secretChatStatePending` (:2798, "waiting for the other user to get
/// online"), `secretChatStateReady` (:2801), `secretChatStateClosed`
/// (:2804).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SecretChatState {
    Pending,
    Ready,
    Closed,
    Unknown(String),
}

impl SecretChatState {
    pub fn from_type_name(type_name: &str) -> Self {
        match type_name {
            "secretChatStatePending" => SecretChatState::Pending,
            "secretChatStateReady" => SecretChatState::Ready,
            "secretChatStateClosed" => SecretChatState::Closed,
            other => SecretChatState::Unknown(other.to_string()),
        }
    }
}

/// Phase B1: `secretChat` subset (TDLib 1.8.67, `schema/td_api.tl:2816`):
/// `secretChat id:int32 user_id:int53 state:SecretChatState
/// is_outbound:Bool key_hash:bytes layer:int32 = SecretChat;`
/// `key_hash` (36 little-endian bytes) is kept raw for the B2 key
/// verification UI; the layer is kept for future capability gating.
///
/// Security: key material stays in memory only — never logged, never
/// written to disk. `Debug` is hand-written and prints only the hash
/// *length*, so formatting an envelope can never leak key bytes.
#[derive(Clone, PartialEq, Eq)]
pub struct ParsedSecretChat {
    pub id: i32,
    pub user_id: i64,
    pub state: SecretChatState,
    pub is_outbound: bool,
    pub key_hash: Vec<u8>,
    pub layer: i32,
}

impl std::fmt::Debug for ParsedSecretChat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ParsedSecretChat")
            .field("id", &self.id)
            .field("user_id", &self.user_id)
            .field("state", &self.state)
            .field("is_outbound", &self.is_outbound)
            .field("key_hash_len", &self.key_hash.len())
            .field("layer", &self.layer)
            .finish()
    }
}

fn parse_secret_chat(value: Option<&Value>) -> Option<ParsedSecretChat> {
    let value = value?;
    Some(ParsedSecretChat {
        id: value.get("id").and_then(Value::as_i64).unwrap_or(0) as i32,
        user_id: int53(value.get("user_id")).ok()?,
        state: SecretChatState::from_type_name(
            value
                .get("state")
                .and_then(|s| s.get("@type"))
                .and_then(Value::as_str)
                .unwrap_or(""),
        ),
        is_outbound: value
            .get("is_outbound")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        key_hash: value
            .get("key_hash")
            .and_then(Value::as_str)
            .and_then(|s| STANDARD.decode(s).ok())
            .unwrap_or_default(),
        layer: value.get("layer").and_then(Value::as_i64).unwrap_or(0) as i32,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatPositionUpdate {
    pub chat_id: ChatId,
    pub list: ChatList,
    pub order: i64,
    pub is_pinned: bool,
}

/// Phase C1: `CallDiscardReason` (TDLib 1.8.67,
/// `schema/td_api.tl:6981`): `callDiscardReasonEmpty` (:6984),
/// `callDiscardReasonMissed` (:6987), `callDiscardReasonDeclined`
/// (:6990), `callDiscardReasonDisconnected` (:6993),
/// `callDiscardReasonHungUp` (:6996),
/// `callDiscardReasonUpgradeToGroupCall` (:6999, carries an invite
/// link — group calls are C3, the link is kept but unused).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallDiscardReason {
    Empty,
    Missed,
    Declined,
    Disconnected,
    HungUp,
    UpgradeToGroupCall { invite_link: String },
    Unknown(String),
}

impl CallDiscardReason {
    pub fn from_value(value: Option<&Value>) -> Self {
        let type_name = value
            .and_then(|v| v.get("@type"))
            .and_then(Value::as_str)
            .unwrap_or("");
        match type_name {
            "callDiscardReasonEmpty" => CallDiscardReason::Empty,
            "callDiscardReasonMissed" => CallDiscardReason::Missed,
            "callDiscardReasonDeclined" => CallDiscardReason::Declined,
            "callDiscardReasonDisconnected" => CallDiscardReason::Disconnected,
            "callDiscardReasonHungUp" => CallDiscardReason::HungUp,
            "callDiscardReasonUpgradeToGroupCall" => CallDiscardReason::UpgradeToGroupCall {
                invite_link: value
                    .and_then(|v| v.get("invite_link"))
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
            },
            other => CallDiscardReason::Unknown(other.to_string()),
        }
    }

    /// Human-readable ended-call line shown on the call-end screen.
    /// `is_outgoing` disambiguates "Declined" (they declined ours vs.
    /// we declined theirs).
    pub fn summary(&self, is_outgoing: bool) -> String {
        match self {
            CallDiscardReason::Empty => "Call ended".to_string(),
            CallDiscardReason::Missed => {
                if is_outgoing {
                    "Call not answered".to_string()
                } else {
                    "Missed call".to_string()
                }
            }
            CallDiscardReason::Declined => {
                if is_outgoing {
                    "Declined".to_string()
                } else {
                    "You declined the call".to_string()
                }
            }
            CallDiscardReason::Disconnected => "Call disconnected".to_string(),
            CallDiscardReason::HungUp => "Call ended".to_string(),
            CallDiscardReason::UpgradeToGroupCall { .. } => "Upgraded to a group call".to_string(),
            CallDiscardReason::Unknown(_) => "Call ended".to_string(),
        }
    }
}

/// Phase C1: `CallState` (TDLib 1.8.67, `schema/td_api.tl:7051`):
/// `callStatePending` (:7058, `is_created` / `is_received`),
/// `callStateExchangingKeys` (:7063), `callStateReady` (:7066),
/// `callStateHangingUp` (:7077), `callStateDiscarded` (:7080 —
/// `reason` / `need_rating` / `need_debug_information` / `need_log`),
/// `callStateError` (:7081 — the `error` wrapper; only its numeric
/// code is kept, never the message text, which can contain
/// secrets).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedRtcServer {
    pub id: u64,
    pub ipv4: String,
    pub ipv6: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub turn: bool,
    pub stun: bool,
    pub tcp: bool,
    pub peer_tag: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadyParams {
    pub encryption_key: Vec<u8>,
    pub servers: Vec<ParsedRtcServer>,
    pub allow_p2p: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallState {
    Pending {
        is_created: bool,
        is_received: bool,
    },
    ExchangingKeys,
    Ready,
    HangingUp,
    Discarded {
        reason: CallDiscardReason,
        need_rating: bool,
        need_debug_information: bool,
        need_log: bool,
    },
    /// TDLib error code only — the `message` text is deliberately not
    /// stored (it can contain phone numbers or other secrets).
    Error {
        code: i32,
    },
    Unknown(String),
}

impl CallState {
    pub fn from_value(value: Option<&Value>) -> Self {
        let value = match value {
            Some(v) => v,
            None => return CallState::Unknown(String::new()),
        };
        let type_name = value.get("@type").and_then(Value::as_str).unwrap_or("");
        match type_name {
            "callStatePending" => CallState::Pending {
                is_created: value
                    .get("is_created")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                is_received: value
                    .get("is_received")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            },
            "callStateExchangingKeys" => CallState::ExchangingKeys,
            "callStateReady" => CallState::Ready,
            "callStateHangingUp" => CallState::HangingUp,
            "callStateDiscarded" => CallState::Discarded {
                reason: CallDiscardReason::from_value(value.get("reason")),
                need_rating: value
                    .get("need_rating")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                need_debug_information: value
                    .get("need_debug_information")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                need_log: value
                    .get("need_log")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            },
            "callStateError" => {
                let error = value.get("error");
                CallState::Error {
                    code: error
                        .and_then(|e| e.get("code"))
                        .and_then(Value::as_i64)
                        .unwrap_or(0) as i32,
                }
            }
            other => CallState::Unknown(other.to_string()),
        }
    }

    /// Terminal states end the tracked call. `Unknown` is deliberately
    /// *not* terminal — a future state the schema doesn't know yet
    /// must not silently drop a live call; the UI labels it honestly.
    pub fn is_terminal(&self) -> bool {
        matches!(self, CallState::Discarded { .. } | CallState::Error { .. })
    }
}

fn parse_rtc_server(value: &Value) -> Option<ParsedRtcServer> {
    let kind = value.get("type")?;
    let type_name = kind.get("@type").and_then(Value::as_str).unwrap_or("");
    let (username, password, turn, stun, tcp, peer_tag) = match type_name {
        "callServerTypeTelegramReflector" => (
            String::new(),
            String::new(),
            true,
            false,
            kind.get("is_tcp").and_then(Value::as_bool).unwrap_or(false),
            kind.get("peer_tag")
                .and_then(Value::as_str)
                .and_then(|tag| STANDARD.decode(tag).ok())
                .unwrap_or_default(),
        ),
        "callServerTypeWebrtc" => (
            kind.get("username")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            kind.get("password")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            kind.get("supports_turn")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            kind.get("supports_stun")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            false,
            Vec::new(),
        ),
        _ => return None,
    };
    Some(ParsedRtcServer {
        id: value
            .get("id")
            .and_then(|id| id.as_u64().or_else(|| id.as_str()?.parse().ok()))?,
        ipv4: value
            .get("ip_address")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        ipv6: value
            .get("ipv6_address")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        port: value.get("port")?.as_u64()?.try_into().ok()?,
        username,
        password,
        turn,
        stun,
        tcp,
        peer_tag,
    })
}

/// Phase C1: `call` subset (TDLib 1.8.67, `schema/td_api.tl:7287`):
/// `call id:int32 unique_id:int64 user_id:int53 is_outgoing:Bool
/// is_video:Bool state:CallState = Call;`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedCall {
    pub id: i32,
    pub unique_id: i64,
    pub user_id: i64,
    pub is_outgoing: bool,
    pub is_video: bool,
    pub state: CallState,
    pub ready: Option<ReadyParams>,
}

fn parse_call(value: Option<&Value>) -> Option<ParsedCall> {
    let value = value?;
    let state_value = value.get("state");
    let ready = state_value
        .filter(|state| state.get("@type").and_then(Value::as_str) == Some("callStateReady"))
        .map(|state| ReadyParams {
            encryption_key: state
                .get("encryption_key")
                .and_then(Value::as_str)
                .and_then(|key| STANDARD.decode(key).ok())
                .unwrap_or_default(),
            servers: state
                .get("servers")
                .and_then(Value::as_array)
                .map(|servers| servers.iter().filter_map(parse_rtc_server).collect())
                .unwrap_or_default(),
            allow_p2p: state
                .get("allow_p2p")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        });
    Some(ParsedCall {
        id: value.get("id").and_then(Value::as_i64).unwrap_or(0) as i32,
        unique_id: int53_or_zero(value.get("unique_id")),
        user_id: int53(value.get("user_id")).ok()?,
        is_outgoing: value
            .get("is_outgoing")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        is_video: value
            .get("is_video")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        state: CallState::from_value(state_value),
        ready,
    })
}

/// Phase C3a: `groupCall` subset (TDLib 1.8.67, `schema/td_api.tl:7154`).
/// Only the fields the signaling surface needs are parsed: identity,
/// join state, admin rights, participant bookkeeping, self video
/// state, and recent speakers. Video/RTMP/record fields are dropped —
/// media transport is Phase C2.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedGroupCall {
    pub id: i32,
    pub title: String,
    pub is_active: bool,
    pub is_video_chat: bool,
    pub is_joined: bool,
    pub need_rejoin: bool,
    pub is_owned: bool,
    pub can_be_managed: bool,
    pub participant_count: i32,
    pub loaded_all_participants: bool,
    /// `(participant_id, is_speaking)` from
    /// `groupCallRecentSpeaker` (schema 1.8.67, line 7118).
    pub recent_speakers: Vec<(MessageSender, bool)>,
    pub is_my_video_enabled: bool,
    pub is_my_video_paused: bool,
    pub can_enable_video: bool,
    pub mute_new_participants: bool,
    pub can_toggle_mute_new_participants: bool,
    pub scheduled_start_date: i32,
    /// Phase C2h: message permissions (schema 1.8.67, lines
    /// 7147-7150) — gate the in-call chat UI.
    pub can_send_messages: bool,
    pub are_messages_allowed: bool,
    pub can_toggle_are_messages_allowed: bool,
    pub can_delete_messages: bool,
    /// Phase C2h: recording state (schema 1.8.67, lines 7151-7152):
    /// ongoing recording duration in seconds (0 = none) and whether a
    /// video file is being recorded.
    pub record_duration: i32,
    pub is_video_recorded: bool,
}

fn parse_group_call(value: Option<&Value>) -> Option<ParsedGroupCall> {
    let value = value?;
    let recent_speakers = value
        .get("recent_speakers")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|item| {
                    let participant_id = parse_message_sender(item.get("participant_id")).ok()?;
                    let is_speaking = item
                        .get("is_speaking")
                        .and_then(Value::as_bool)
                        .unwrap_or(false);
                    Some((participant_id, is_speaking))
                })
                .collect()
        })
        .unwrap_or_default();
    Some(ParsedGroupCall {
        id: value.get("id").and_then(Value::as_i64).unwrap_or(0) as i32,
        title: value
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        is_active: value
            .get("is_active")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        is_video_chat: value
            .get("is_video_chat")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        is_joined: value
            .get("is_joined")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        need_rejoin: value
            .get("need_rejoin")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        is_owned: value
            .get("is_owned")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        can_be_managed: value
            .get("can_be_managed")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        participant_count: value
            .get("participant_count")
            .and_then(Value::as_i64)
            .unwrap_or(0) as i32,
        loaded_all_participants: value
            .get("loaded_all_participants")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        recent_speakers,
        is_my_video_enabled: value
            .get("is_my_video_enabled")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        is_my_video_paused: value
            .get("is_my_video_paused")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        can_enable_video: value
            .get("can_enable_video")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        mute_new_participants: value
            .get("mute_new_participants")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        can_toggle_mute_new_participants: value
            .get("can_toggle_mute_new_participants")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        scheduled_start_date: value
            .get("scheduled_start_date")
            .and_then(Value::as_i64)
            .unwrap_or(0) as i32,
        can_send_messages: value
            .get("can_send_messages")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        are_messages_allowed: value
            .get("are_messages_allowed")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        can_toggle_are_messages_allowed: value
            .get("can_toggle_are_messages_allowed")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        can_delete_messages: value
            .get("can_delete_messages")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        record_duration: value
            .get("record_duration")
            .and_then(Value::as_i64)
            .unwrap_or(0) as i32,
        is_video_recorded: value
            .get("is_video_recorded")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
}

/// Phase C2h: `groupCallMessage` subset (TDLib 1.8.67,
/// `schema/td_api.tl:7200`):
/// `groupCallMessage message_id:int32 sender_id:MessageSender date:int32
/// text:formattedText paid_message_star_count:int53 is_from_owner:Bool
/// can_be_deleted:Bool = GroupCallMessage;`
/// Entities are dropped — plain text only (the in-call chat is a
/// minimal list + composer).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedGroupCallMessage {
    pub message_id: i32,
    pub sender_id: MessageSender,
    pub date: i32,
    pub text: String,
    pub is_from_owner: bool,
    pub can_be_deleted: bool,
}

fn parse_group_call_message(value: Option<&Value>) -> Option<ParsedGroupCallMessage> {
    let value = value?;
    Some(ParsedGroupCallMessage {
        message_id: value.get("message_id").and_then(Value::as_i64)? as i32,
        sender_id: parse_message_sender(value.get("sender_id")).ok()?,
        date: value.get("date").and_then(Value::as_i64).unwrap_or(0) as i32,
        text: parse_formatted_text(value.get("text")),
        is_from_owner: value
            .get("is_from_owner")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        can_be_deleted: value
            .get("can_be_deleted")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
}

/// Phase C2g: `groupCallVideoSourceGroup` (TDLib 1.8.67,
/// `schema/td_api.tl:7157`):
/// `groupCallVideoSourceGroup semantics:string source_ids:vector<int32>
/// = GroupCallVideoSourceGroup;`
/// The `source_ids` are the RTP synchronization sources of one video
/// channel; ntgcalls' `ntg_add_incoming_video` subscribes by endpoint +
/// these groups, and incoming frames are attributed to the participant
/// by matching `ntg_frame.ssrc` against them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupCallVideoSourceGroup {
    pub semantics: String,
    pub source_ids: Vec<u32>,
}

/// Phase C2g: `groupCallParticipantVideoInfo` (TDLib 1.8.67,
/// `schema/td_api.tl:7163`):
/// `groupCallParticipantVideoInfo source_groups:vector<groupCallVideoSourceGroup>
/// endpoint_id:string is_paused:Bool = GroupCallParticipantVideoInfo;`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupCallVideoInfo {
    pub endpoint_id: String,
    pub is_paused: bool,
    pub source_groups: Vec<GroupCallVideoSourceGroup>,
}

fn parse_group_call_video_info(value: Option<&Value>) -> Option<GroupCallVideoInfo> {
    let value = value?;
    if value.is_null() {
        return None;
    }
    let source_groups = value
        .get("source_groups")
        .and_then(Value::as_array)
        .map(|groups| {
            groups
                .iter()
                .map(|group| GroupCallVideoSourceGroup {
                    semantics: group
                        .get("semantics")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string(),
                    source_ids: group
                        .get("source_ids")
                        .and_then(Value::as_array)
                        .map(|ids| {
                            ids.iter()
                                .filter_map(Value::as_i64)
                                .map(|id| id as u32)
                                .collect()
                        })
                        .unwrap_or_default(),
                })
                .collect()
        })
        .unwrap_or_default();
    Some(GroupCallVideoInfo {
        endpoint_id: value
            .get("endpoint_id")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        is_paused: value
            .get("is_paused")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        source_groups,
    })
}

/// Phase C3a: `groupCallParticipant` subset (TDLib 1.8.67,
/// `schema/td_api.tl:7184`). Video info fields
/// (`video_info`/`screen_sharing_video_info`) are parsed in Phase C2g
/// and drive the engine's incoming-video subscriptions. An empty
/// `order` means the participant must be removed from the list
/// (schema note on `order`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedGroupCallParticipant {
    pub participant_id: MessageSender,
    pub audio_source_id: i32,
    pub is_current_user: bool,
    pub is_speaking: bool,
    pub is_hand_raised: bool,
    pub can_be_muted_for_all_users: bool,
    pub can_be_unmuted_for_all_users: bool,
    pub can_be_muted_for_current_user: bool,
    pub can_be_unmuted_for_current_user: bool,
    pub is_muted_for_all_users: bool,
    pub is_muted_for_current_user: bool,
    pub can_unmute_self: bool,
    pub volume_level: i32,
    pub order: String,
    /// Phase C3a: `video_info != null` / `screen_sharing_video_info !=
    /// null` (schema 1.8.67, line 7184).
    pub video_enabled: bool,
    pub screen_sharing_enabled: bool,
    /// Phase C2g: the parsed `video_info` / `screen_sharing_video_info`
    /// channels; `None` matches the `*_enabled` flags above. The engine
    /// subscribes to `video_info` endpoints via
    /// `ntg_add_incoming_video`.
    pub video_info: Option<GroupCallVideoInfo>,
    pub screen_sharing_video_info: Option<GroupCallVideoInfo>,
}

fn parse_group_call_participant(value: Option<&Value>) -> Option<ParsedGroupCallParticipant> {
    let value = value?;
    let flag = |name: &str| value.get(name).and_then(Value::as_bool).unwrap_or(false);
    Some(ParsedGroupCallParticipant {
        participant_id: parse_message_sender(value.get("participant_id")).ok()?,
        audio_source_id: value
            .get("audio_source_id")
            .and_then(Value::as_i64)
            .unwrap_or(0) as i32,
        is_current_user: flag("is_current_user"),
        is_speaking: flag("is_speaking"),
        is_hand_raised: flag("is_hand_raised"),
        can_be_muted_for_all_users: flag("can_be_muted_for_all_users"),
        can_be_unmuted_for_all_users: flag("can_be_unmuted_for_all_users"),
        can_be_muted_for_current_user: flag("can_be_muted_for_current_user"),
        can_be_unmuted_for_current_user: flag("can_be_unmuted_for_current_user"),
        is_muted_for_all_users: flag("is_muted_for_all_users"),
        is_muted_for_current_user: flag("is_muted_for_current_user"),
        can_unmute_self: flag("can_unmute_self"),
        volume_level: value
            .get("volume_level")
            .and_then(Value::as_i64)
            .unwrap_or(0) as i32,
        order: value
            .get("order")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        video_enabled: value.get("video_info").is_some_and(|info| !info.is_null()),
        screen_sharing_enabled: value
            .get("screen_sharing_video_info")
            .is_some_and(|info| !info.is_null()),
        video_info: parse_group_call_video_info(value.get("video_info")),
        screen_sharing_video_info: parse_group_call_video_info(
            value.get("screen_sharing_video_info"),
        ),
    })
}

/// Phase C3a: `videoChat` (TDLib 1.8.67, `schema/td_api.tl:3579`):
/// `videoChat group_call_id:int32 has_participants:Bool
/// default_participant_id:MessageSender = VideoChat;`
/// `group_call_id` is 0 when the chat has no active video chat.
/// `default_participant_id` is dropped (not needed this slice).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedVideoChat {
    pub group_call_id: i32,
    pub has_participants: bool,
}

fn parse_video_chat(value: Option<&Value>) -> Option<ParsedVideoChat> {
    let value = value?;
    Some(ParsedVideoChat {
        group_call_id: value
            .get("group_call_id")
            .and_then(Value::as_i64)
            .unwrap_or(0) as i32,
        has_participants: value
            .get("has_participants")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatList {
    Main,
    Archive,
    Folder(i32),
    Unknown,
}

/// `chatFolderInfo` (TDLib 1.8.67, `schema/td_api.tl:3485`). Only the fields
/// Quill needs for folder tabs: identifier, display name (plain text — the
/// schema allows only CustomEmoji entities in folder names, which are
/// dropped), icon name, and color id. `is_shareable` / `has_my_invite_links`
/// are dropped (folder create/edit/share is out of scope).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatFolderInfo {
    pub id: i32,
    pub name: String,
    pub icon_name: String,
    pub color_id: i32,
}

/// Parity slice: the full editable `chatFolder` spec (TDLib 1.8.67,
/// `schema/td_api.tl:3476`) behind `createChatFolder` / `editChatFolder`
/// (`:13358` / `:13361`) and returned by `getChatFolder` (`:13355`).
/// Quill always sends `icon: null` (default icon) and `color_id: -1`
/// (disabled) — custom-emoji icon rendering is out of scope — and
/// `is_shareable: false` (invite links out of scope). The remaining fields
/// are the create/edit dialog's model.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ChatFolderSpec {
    pub name: String,
    pub pinned_chat_ids: Vec<i64>,
    pub included_chat_ids: Vec<i64>,
    pub excluded_chat_ids: Vec<i64>,
    pub exclude_muted: bool,
    pub exclude_read: bool,
    pub exclude_archived: bool,
    pub include_contacts: bool,
    pub include_non_contacts: bool,
    pub include_bots: bool,
    pub include_groups: bool,
    pub include_channels: bool,
}

/// tdesktop default mute submenu (`SessionSettings::mutePeriods` when unset /
/// `DefaultTimePickerValues`): 1 hour, 8 hours, 2 days. Seconds, matching
/// `chatNotificationSettings.mute_for`.
pub const MUTE_FOR_1_HOUR: i32 = 3600;
pub const MUTE_FOR_8_HOURS: i32 = 8 * 3600;
pub const MUTE_FOR_2_DAYS: i32 = 2 * 86400;
/// tdesktop `MuteMenu::kMuteForeverValue` (`numeric_limits<int>::max()`).
/// TDLib: mute_for longer than 366 days is muted forever.
pub const MUTE_FOREVER: i32 = i32::MAX;
pub const MUTE_FOREVER_AFTER_SECONDS: i32 = 366 * 86400;

/// `messageSenderUser` / `messageSenderChat` (TDLib 1.8.67).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageSender {
    User { user_id: i64 },
    Chat { chat_id: i64 },
}

/// Own `chatMemberStatus*` for a broadcast channel (TDLib 1.8.67).
/// Drives the composer gate (admins post in 2.3) and the join/leave affordance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelMemberStatus {
    Creator,
    Administrator,
    Member,
    Restricted,
    Left,
    Banned,
    Unknown,
}

impl ChannelMemberStatus {
    pub fn is_admin(self) -> bool {
        matches!(
            self,
            ChannelMemberStatus::Creator | ChannelMemberStatus::Administrator
        )
    }

    pub fn is_joined(self) -> bool {
        matches!(
            self,
            ChannelMemberStatus::Creator
                | ChannelMemberStatus::Administrator
                | ChannelMemberStatus::Member
        )
    }
}

/// Typed `chatMember` (TDLib 1.8.67). Only `member_id` and `status` are kept;
/// `tag` / `inviter_user_id` / `joined_chat_date` stay out of this slice.
/// `admin_can_post_messages` carries `rights.can_post_messages` from
/// `chatMemberStatusAdministrator` (schema 1.8.67:
/// `chatAdministratorRights ... can_post_messages:Bool ...`), driving the
/// channel-admin composer gate; `None` for every other status or when the
/// rights block is absent. `admin_can_invite_users` carries
/// `rights.can_invite_users` (schema 1.8.67, line 1092), driving the
/// Phase D3a invite-link / join-request management gate; `None` for every
/// other status or when the rights block is absent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParsedChatMember {
    pub member_id: MessageSender,
    pub status: ChannelMemberStatus,
    pub admin_can_post_messages: Option<bool>,
    pub admin_can_invite_users: Option<bool>,
    /// Phase D3b: the full `rights` block from
    /// `chatMemberStatusAdministrator` (TDLib 1.8.67,
    /// `chatAdministratorRights`, schema line 1092). `Some` only for an
    /// administrator with a parsed rights block; `None` for every other
    /// status or an absent rights block. Drives the promote/edit-rights
    /// flows and the `can_promote_members` gate.
    pub admin_rights: Option<ChatAdminRights>,
}

/// Phase D3b: `chatAdministratorRights` (TDLib 1.8.67,
/// `schema/td_api.tl:1092`):
/// `chatAdministratorRights can_manage_chat:Bool can_change_info:Bool
/// can_post_messages:Bool can_edit_messages:Bool can_delete_messages:Bool
/// can_invite_users:Bool can_restrict_members:Bool can_pin_messages:Bool
/// can_manage_topics:Bool can_promote_members:Bool
/// can_manage_video_chats:Bool can_post_stories:Bool can_edit_stories:Bool
/// can_delete_stories:Bool can_manage_direct_messages:Bool
/// can_manage_tags:Bool can_send_welcome_messages:Bool is_anonymous:Bool =
/// ChatAdministratorRights;`
/// Fields are declared in schema order. Missing JSON fields parse to
/// `false` (deny-by-default); TDLib always sends the full block.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ChatAdminRights {
    pub can_manage_chat: bool,
    pub can_change_info: bool,
    pub can_post_messages: bool,
    pub can_edit_messages: bool,
    pub can_delete_messages: bool,
    pub can_invite_users: bool,
    pub can_restrict_members: bool,
    pub can_pin_messages: bool,
    pub can_manage_topics: bool,
    pub can_promote_members: bool,
    pub can_manage_video_chats: bool,
    pub can_post_stories: bool,
    pub can_edit_stories: bool,
    pub can_delete_stories: bool,
    pub can_manage_direct_messages: bool,
    pub can_manage_tags: bool,
    pub can_send_welcome_messages: bool,
    pub is_anonymous: bool,
}

impl ChatAdminRights {
    /// All rights granted. A UI convenience for the promote dialog's
    /// default checkbox state — not a server fact.
    pub fn all() -> Self {
        Self {
            can_manage_chat: true,
            can_change_info: true,
            can_post_messages: true,
            can_edit_messages: true,
            can_delete_messages: true,
            can_invite_users: true,
            can_restrict_members: true,
            can_pin_messages: true,
            can_manage_topics: true,
            can_promote_members: true,
            can_manage_video_chats: true,
            can_post_stories: true,
            can_edit_stories: true,
            can_delete_stories: true,
            can_manage_direct_messages: true,
            can_manage_tags: true,
            can_send_welcome_messages: true,
            is_anonymous: true,
        }
    }

    /// Serialize as `chatAdministratorRights` JSON for
    /// `setChatMemberStatus` (schema 1.8.67, lines 2500/1092).
    pub fn to_json(&self) -> Value {
        serde_json::json!({
            "@type": "chatAdministratorRights",
            "can_manage_chat": self.can_manage_chat,
            "can_change_info": self.can_change_info,
            "can_post_messages": self.can_post_messages,
            "can_edit_messages": self.can_edit_messages,
            "can_delete_messages": self.can_delete_messages,
            "can_invite_users": self.can_invite_users,
            "can_restrict_members": self.can_restrict_members,
            "can_pin_messages": self.can_pin_messages,
            "can_manage_topics": self.can_manage_topics,
            "can_promote_members": self.can_promote_members,
            "can_manage_video_chats": self.can_manage_video_chats,
            "can_post_stories": self.can_post_stories,
            "can_edit_stories": self.can_edit_stories,
            "can_delete_stories": self.can_delete_stories,
            "can_manage_direct_messages": self.can_manage_direct_messages,
            "can_manage_tags": self.can_manage_tags,
            "can_send_welcome_messages": self.can_send_welcome_messages,
            "is_anonymous": self.is_anonymous,
        })
    }
}

/// Phase D3b: parse a `chatAdministratorRights` block (TDLib 1.8.67,
/// schema line 1092); `None` unless `@type` matches or the value is
/// absent/null.
pub fn parse_chat_admin_rights(value: Option<&Value>) -> Option<ChatAdminRights> {
    let value = value.filter(|v| !v.is_null())?;
    if value.get("@type").and_then(Value::as_str) != Some("chatAdministratorRights") {
        return None;
    }
    let right = |name: &str| value.get(name).and_then(Value::as_bool).unwrap_or(false);
    Some(ChatAdminRights {
        can_manage_chat: right("can_manage_chat"),
        can_change_info: right("can_change_info"),
        can_post_messages: right("can_post_messages"),
        can_edit_messages: right("can_edit_messages"),
        can_delete_messages: right("can_delete_messages"),
        can_invite_users: right("can_invite_users"),
        can_restrict_members: right("can_restrict_members"),
        can_pin_messages: right("can_pin_messages"),
        can_manage_topics: right("can_manage_topics"),
        can_promote_members: right("can_promote_members"),
        can_manage_video_chats: right("can_manage_video_chats"),
        can_post_stories: right("can_post_stories"),
        can_edit_stories: right("can_edit_stories"),
        can_delete_stories: right("can_delete_stories"),
        can_manage_direct_messages: right("can_manage_direct_messages"),
        can_manage_tags: right("can_manage_tags"),
        can_send_welcome_messages: right("can_send_welcome_messages"),
        is_anonymous: right("is_anonymous"),
    })
}

/// Phase D3b: `chatAdministrator` (TDLib 1.8.67, `schema/td_api.tl:2482`):
/// `chatAdministrator user_id:int53 custom_title:string is_owner:Bool
/// can_be_edited:Bool = ChatAdministrator;`
/// One entry of the `chatAdministrators` response. This schema version has
/// no rights block here (and no `setChatAdministratorCustomTitle`), so
/// per-admin rights come from `getChatMember` on demand.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatAdministratorEntry {
    pub user_id: i64,
    pub custom_title: String,
    pub is_owner: bool,
    pub can_be_edited: bool,
}

/// `botCommand` (TDLib 1.8.67, `schema/td_api.tl:826`):
/// `botCommand command:string description:string is_ephemeral:Bool =
/// BotCommand`. `is_ephemeral` is not kept — the panel only lists commands;
/// tapping one inserts plain text into the composer (Phase 3.3 owns the
/// command menu).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BotCommand {
    pub command: String,
    pub description: String,
}

/// `botInfo` subset (TDLib 1.8.67, `schema/td_api.tl:2430`): only what the
/// bot panel renders — `short_description`, `description`, and
/// `commands:vector<botCommand>` (a bare vector of `botCommand`, not the
/// `botCommands` wrapper). Photo, menu button, rights, and links are
/// intentionally not kept.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BotInfo {
    pub short_description: String,
    pub description: String,
    pub commands: Vec<BotCommand>,
}

/// Phase 6: `userStatus*` (TDLib 1.8.67, `schema/td_api.tl:6407`).
/// Unknown constructors fall back to `Empty` — a hostile status can never
/// crash the parse.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum UserStatusKind {
    #[default]
    Empty,
    Online,
    Offline {
        was_online: i32,
    },
    Recently,
    LastWeek,
    LastMonth,
}

impl UserStatusKind {
    /// Cheap status line for the contacts list / user info panel.
    pub fn display(&self) -> String {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        self.display_at(now)
    }

    fn display_at(&self, now_secs: u64) -> String {
        match *self {
            UserStatusKind::Empty => String::new(),
            UserStatusKind::Online => "online".to_string(),
            UserStatusKind::Recently => "last seen recently".to_string(),
            UserStatusKind::LastWeek => "last seen within a week".to_string(),
            UserStatusKind::LastMonth => "last seen within a month".to_string(),
            UserStatusKind::Offline { was_online } => {
                let was = was_online.max(0) as u64;
                if was == 0 || was > now_secs {
                    return "last seen a long time ago".to_string();
                }
                let ago = now_secs - was;
                if ago < 60 {
                    "last seen just now".to_string()
                } else if ago < 3600 {
                    format!("last seen {}m ago", ago / 60)
                } else if ago < 86400 {
                    format!("last seen {}h ago", ago / 3600)
                } else if ago < 7 * 86400 {
                    format!("last seen {}d ago", ago / 86400)
                } else {
                    "last seen a long time ago".to_string()
                }
            }
        }
    }

    pub fn is_online(&self) -> bool {
        matches!(self, UserStatusKind::Online)
    }
}

/// Phase 6: `user` subset (TDLib 1.8.67, `schema/td_api.tl:2403`) kept for
/// the contacts list and the user info panel. Dropped (documented, not
/// forgotten): accent/background color ids, emoji status, verification
/// status, premium/support flags, restriction info, active story state,
/// new-chat restrictions, paid-message star count, access flags, chat
/// language, attachment-menu flag.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ParsedUser {
    pub id: i64,
    pub first_name: String,
    pub last_name: String,
    /// First entry of `usernames.active_usernames` (schema 1.8.67, line
    /// 2372 — this schema version has no singular `username` field).
    pub username: String,
    pub phone_number: String,
    pub is_contact: bool,
    pub is_bot: bool,
    pub status: UserStatusKind,
    /// `profile_photo.small.id` (`profilePhoto`, schema 1.8.67 line 754);
    /// 0 = no photo.
    pub photo_small_file_id: i32,
}

impl ParsedUser {
    pub fn display_name(&self) -> String {
        let name = format!("{} {}", self.first_name, self.last_name);
        let name = name.trim();
        if name.is_empty() {
            format!("User {}", self.id)
        } else {
            name.to_string()
        }
    }

    /// Two-letter avatar fallback ("Ada Lovelace" → "AL").
    pub fn initials(&self) -> String {
        let mut out = String::new();
        for part in [&self.first_name, &self.last_name] {
            if let Some(ch) = part.chars().next() {
                out.push(ch);
                if out.chars().count() == 2 {
                    break;
                }
            }
        }
        if out.is_empty() {
            out.push('?');
        }
        out
    }
}

/// `buttonStyle*` (TDLib 1.8.67, `schema/td_api.tl:3696`). Unknown styles
/// fall back to `Default` — the keyboard must never fail to render.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InlineKeyboardButtonStyle {
    #[default]
    Default,
    Primary,
    Danger,
    Success,
    Link,
}

/// `targetChat*` for `inlineKeyboardButtonTypeSwitchInline`
/// (TDLib 1.8.67, `schema/td_api.tl:7476`). Only `Current` is actionable in
/// this slice; the rest insert into the current chat's composer too (chat
/// picker is out of scope).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InlineKeyboardTargetChat {
    Current,
    Chosen,
    InternalLink,
    Unknown,
}

/// `inlineKeyboardButtonType*` (TDLib 1.8.67, `schema/td_api.tl:3774`).
/// Types needing a TDLib round-trip or platform flow we do not have yet
/// (`LoginUrl` → `getLoginUrlInfo`, `WebApp` → `openWebApp`,
/// `CallbackWithPassword` → password prompt, `CallbackGame`, `Buy`,
/// `User`) are parsed and stored but render as disabled buttons. Anything
/// unrecognized becomes `Unknown` and also renders disabled — never a crash.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InlineKeyboardButtonType {
    Url {
        url: String,
    },
    LoginUrl {
        url: String,
    },
    WebApp {
        url: String,
    },
    Callback {
        data: Vec<u8>,
    },
    CallbackWithPassword {
        data: Vec<u8>,
    },
    CallbackGame,
    SwitchInline {
        query: String,
        target: InlineKeyboardTargetChat,
    },
    Buy,
    User {
        user_id: i64,
    },
    CopyText {
        text: String,
    },
    Disabled,
    Unknown {
        type_name: String,
    },
}

/// `inlineKeyboardButton` (TDLib 1.8.67, `schema/td_api.tl:3828`).
/// `icon_custom_emoji_id` is parsed (schema presence) but not rendered yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InlineKeyboardButton {
    pub text: String,
    pub style: InlineKeyboardButtonStyle,
    pub kind: InlineKeyboardButtonType,
}

/// `replyMarkupInlineKeyboard` (TDLib 1.8.67, `schema/td_api.tl:3855`):
/// `rows` is a vector of rows (`vector<vector<inlineKeyboardButton>>`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InlineKeyboard {
    pub rows: Vec<Vec<InlineKeyboardButton>>,
    pub force_reply: bool,
}

/// `callbackQueryAnswer` (TDLib 1.8.67, `schema/td_api.tl:7747`): the bot's
/// answer to a callback query sent via `getCallbackQueryAnswer`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallbackQueryAnswer {
    pub text: String,
    pub show_alert: bool,
    pub url: String,
}

/// Typed `ChatJoinResult` — `joinChat` response (TDLib 1.8.67: no
/// invite-link variant exists in this schema). The other variants surface as
/// a fixed note (no TDLib text).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatJoinResult {
    Success { chat_id: ChatId },
    RequestSent,
    GuardBotApprovalRequired,
    Declined,
}

/// `draftMessage` text this slice restores. Voice/video/rich drafts are ignored.
/// `reply_to` is same-chat `inputMessageReplyToMessage` only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatDraft {
    pub text: String,
    pub reply_to_message_id: Option<MessageId>,
}

/// `ChatAction` values this slice acts on. Other constructors stay `Other`
/// so a replacement action clears typing without inventing labels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatAction {
    /// `chatActionTyping`
    Typing,
    /// `chatActionCancel`, or a null action (schema: null cancels).
    Cancel,
    Other,
}

/// `chatNotificationSettings` (TDLib 1.8.67). Other fields are copied through
/// so a mute change does not reset sound / preview exceptions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatNotificationSettings {
    pub use_default_mute_for: bool,
    pub mute_for: i32,
    pub use_default_sound: bool,
    pub sound_id: i64,
    pub use_default_show_preview: bool,
    pub show_preview: bool,
    pub use_default_mute_stories: bool,
    pub mute_stories: bool,
    pub use_default_story_sound: bool,
    pub story_sound_id: i64,
    pub use_default_show_story_poster: bool,
    pub show_story_poster: bool,
    pub use_default_disable_pinned_message_notifications: bool,
    pub disable_pinned_message_notifications: bool,
    pub use_default_disable_mention_notifications: bool,
    pub disable_mention_notifications: bool,
}

impl Default for ChatNotificationSettings {
    fn default() -> Self {
        Self {
            use_default_mute_for: true,
            mute_for: 0,
            use_default_sound: true,
            sound_id: 0,
            use_default_show_preview: true,
            show_preview: false,
            use_default_mute_stories: true,
            mute_stories: false,
            use_default_story_sound: true,
            story_sound_id: 0,
            use_default_show_story_poster: true,
            show_story_poster: false,
            use_default_disable_pinned_message_notifications: true,
            disable_pinned_message_notifications: false,
            use_default_disable_mention_notifications: true,
            disable_mention_notifications: false,
        }
    }
}

impl ChatNotificationSettings {
    /// Exception mute. `use_default_mute_for` stays true until the user sets one
    /// (Unigram clones settings and clears the default flag).
    pub fn with_mute_for(mut self, mute_for: i32) -> Self {
        self.use_default_mute_for = false;
        self.mute_for = mute_for;
        self
    }

    /// Effective chat mute. Scope defaults are not applied here.
    pub fn is_muted(&self) -> bool {
        !self.use_default_mute_for && self.mute_for > 0
    }

    pub fn is_muted_forever(&self) -> bool {
        self.is_muted() && self.mute_for > MUTE_FOREVER_AFTER_SECONDS
    }
}

/// `notificationSound` (TDLib 1.8.67, line 8857): "Describes a notification
/// sound in MP3 format".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationSound {
    pub id: i64,
    pub duration: i32,
    pub date: i32,
    pub title: String,
    pub data: String,
    /// `sound:file` — downloaded on demand with `downloadFile` when a
    /// notification needs it.
    pub sound: ParsedFile,
}

/// `NotificationSettingsScope` (TDLib 1.8.67, lines 3337–3343).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NotificationSettingsScope {
    PrivateChats,
    GroupChats,
    ChannelChats,
}

impl NotificationSettingsScope {
    /// The `@type` constructor name for `getScopeNotificationSettings` /
    /// `setScopeNotificationSettings` requests.
    pub fn type_name(&self) -> &'static str {
        match self {
            NotificationSettingsScope::PrivateChats => "notificationSettingsScopePrivateChats",
            NotificationSettingsScope::GroupChats => "notificationSettingsScopeGroupChats",
            NotificationSettingsScope::ChannelChats => "notificationSettingsScopeChannelChats",
        }
    }

    /// Human label for the scope-defaults settings UI.
    pub fn label(&self) -> &'static str {
        match self {
            NotificationSettingsScope::PrivateChats => "Private chats",
            NotificationSettingsScope::GroupChats => "Groups",
            NotificationSettingsScope::ChannelChats => "Channels",
        }
    }

    /// All three scopes, in UI order.
    pub const ALL: [NotificationSettingsScope; 3] = [
        NotificationSettingsScope::PrivateChats,
        NotificationSettingsScope::GroupChats,
        NotificationSettingsScope::ChannelChats,
    ];
}

/// `scopeNotificationSettings` (TDLib 1.8.67, line 3375): defaults applied
/// when a chat's `chatNotificationSettings` keeps a `use_default_*` flag.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopeNotificationSettings {
    pub mute_for: i32,
    /// 0 = disabled; -1 = app-dependent default sound (schema line 3368).
    pub sound_id: i64,
    pub show_preview: bool,
    pub use_default_mute_stories: bool,
    pub mute_stories: bool,
    pub story_sound_id: i64,
    pub show_story_poster: bool,
    pub disable_pinned_message_notifications: bool,
    pub disable_mention_notifications: bool,
}

impl Default for ScopeNotificationSettings {
    fn default() -> Self {
        Self {
            mute_for: 0,
            sound_id: -1,
            show_preview: true,
            use_default_mute_stories: true,
            mute_stories: false,
            story_sound_id: -1,
            show_story_poster: true,
            disable_pinned_message_notifications: false,
            disable_mention_notifications: false,
        }
    }
}

/// `message.forward_info.origin` (TDLib 1.8.67 `MessageOrigin`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MessageOrigin {
    User {
        user_id: UserId,
    },
    HiddenUser {
        sender_name: String,
    },
    Chat {
        chat_id: ChatId,
        author_signature: String,
    },
    Channel {
        chat_id: ChatId,
        message_id: MessageId,
        author_signature: String,
    },
}

/// Typed `messageForwardInfo`. `source` (Saved Messages / Replies) stays out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageForwardInfo {
    pub origin: MessageOrigin,
    pub date: i32,
}

/// Phase D2: `statisticalValue` (TDLib 1.8.67, `schema/td_api.tl:10139`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StatisticalValue {
    pub value: f64,
    pub previous_value: f64,
    pub growth_rate_percentage: f64,
}

impl StatisticalValue {
    fn parse(value: &Value) -> Result<Self, ParseError> {
        // `statisticalValue value:double previous_value:double
        // growth_rate_percentage:double` (schema 1.8.67, line 10139) — all
        // three fields are required; missing or mistyped fields are a
        // parse error rather than fabricated zeros.
        Ok(StatisticalValue {
            value: value
                .get("value")
                .and_then(Value::as_f64)
                .ok_or(ParseError::MissingField)?,
            previous_value: value
                .get("previous_value")
                .and_then(Value::as_f64)
                .ok_or(ParseError::MissingField)?,
            growth_rate_percentage: value
                .get("growth_rate_percentage")
                .and_then(Value::as_f64)
                .ok_or(ParseError::MissingField)?,
        })
    }
}

/// Phase D2: `StatisticalGraph` (TDLib 1.8.67) — `statisticalGraphData`
/// (`schema/td_api.tl:10145`), `statisticalGraphAsync` (`:10148`),
/// `statisticalGraphError` (`:10151`). Unknown variants are a parse error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StatisticalGraph {
    Data {
        json_data: String,
        zoom_token: String,
    },
    Async {
        token: String,
    },
    Error {
        error_message: String,
    },
}

impl StatisticalGraph {
    fn parse(value: &Value) -> Result<Self, ParseError> {
        match value.get("@type").and_then(Value::as_str) {
            Some("statisticalGraphData") => Ok(StatisticalGraph::Data {
                json_data: value
                    .get("json_data")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                zoom_token: value
                    .get("zoom_token")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
            }),
            Some("statisticalGraphAsync") => Ok(StatisticalGraph::Async {
                token: value
                    .get("token")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
            }),
            Some("statisticalGraphError") => Ok(StatisticalGraph::Error {
                error_message: value
                    .get("error_message")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
            }),
            _ => Err(ParseError::MissingField),
        }
    }
}

/// Phase D2: `ChatStatisticsObjectType` (TDLib 1.8.67) —
/// `chatStatisticsObjectTypeMessage` (`schema/td_api.tl:10157`),
/// `chatStatisticsObjectTypeStory` (`schema/td_api.tl:10160`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatStatisticsObject {
    Message { message_id: i64 },
    Story { story_id: i32 },
}

/// Phase D2: `chatStatisticsInteractionInfo` (TDLib 1.8.67,
/// `schema/td_api.tl:10168`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChatStatisticsInteractionInfo {
    pub object: ChatStatisticsObject,
    pub view_count: i32,
    pub forward_count: i32,
    pub reaction_count: i32,
}

/// Phase D2: `chatStatisticsMessageSenderInfo` (TDLib 1.8.67,
/// `schema/td_api.tl:10174`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChatStatisticsMessageSenderInfo {
    pub user_id: i64,
    pub sent_message_count: i32,
    pub average_character_count: i32,
}

/// Phase D2: `chatStatisticsAdministratorActionsInfo` (TDLib 1.8.67,
/// `schema/td_api.tl:10181`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChatStatisticsAdministratorActionsInfo {
    pub user_id: i64,
    pub deleted_message_count: i32,
    pub banned_user_count: i32,
    pub restricted_user_count: i32,
}

/// Phase D2: `chatStatisticsInviterInfo` (TDLib 1.8.67,
/// `schema/td_api.tl:10186`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChatStatisticsInviterInfo {
    pub user_id: i64,
    pub added_member_count: i32,
}

/// Phase D2: `chatStatisticsChannel` (TDLib 1.8.67, `schema/td_api.tl:10233`).
#[derive(Debug, Clone, PartialEq)]
pub struct ChannelStatistics {
    pub period_start: i32,
    pub period_end: i32,
    pub member_count: StatisticalValue,
    pub mean_message_view_count: StatisticalValue,
    pub mean_message_share_count: StatisticalValue,
    pub mean_message_reaction_count: StatisticalValue,
    pub mean_story_view_count: StatisticalValue,
    pub mean_story_share_count: StatisticalValue,
    pub mean_story_reaction_count: StatisticalValue,
    pub enabled_notifications_percentage: f64,
    pub member_count_graph: StatisticalGraph,
    pub join_graph: StatisticalGraph,
    pub mute_graph: StatisticalGraph,
    pub view_count_by_hour_graph: StatisticalGraph,
    pub view_count_by_source_graph: StatisticalGraph,
    pub join_by_source_graph: StatisticalGraph,
    pub language_graph: StatisticalGraph,
    pub message_interaction_graph: StatisticalGraph,
    pub message_reaction_graph: StatisticalGraph,
    pub story_interaction_graph: StatisticalGraph,
    pub story_reaction_graph: StatisticalGraph,
    pub instant_view_interaction_graph: StatisticalGraph,
    pub recent_interactions: Vec<ChatStatisticsInteractionInfo>,
}

/// Phase D2: `chatStatisticsSupergroup` (TDLib 1.8.67,
/// `schema/td_api.tl:10208`).
#[derive(Debug, Clone, PartialEq)]
pub struct SupergroupStatistics {
    pub period_start: i32,
    pub period_end: i32,
    pub member_count: StatisticalValue,
    pub message_count: StatisticalValue,
    pub viewer_count: StatisticalValue,
    pub sender_count: StatisticalValue,
    pub member_count_graph: StatisticalGraph,
    pub join_graph: StatisticalGraph,
    pub join_by_source_graph: StatisticalGraph,
    pub language_graph: StatisticalGraph,
    pub message_content_graph: StatisticalGraph,
    pub action_graph: StatisticalGraph,
    pub day_graph: StatisticalGraph,
    pub week_graph: StatisticalGraph,
    pub top_senders: Vec<ChatStatisticsMessageSenderInfo>,
    pub top_administrators: Vec<ChatStatisticsAdministratorActionsInfo>,
    pub top_inviters: Vec<ChatStatisticsInviterInfo>,
}

/// Phase D2: `ChatStatistics` (TDLib 1.8.67) — the `getChatStatistics`
/// response (`schema/td_api.tl:15760`). Revenue/star variants stay out of
/// this slice.
#[derive(Debug, Clone, PartialEq)]
pub enum ChatStatistics {
    Channel(Box<ChannelStatistics>),
    Supergroup(Box<SupergroupStatistics>),
}

/// Phase D3a: `starSubscriptionPricing` (TDLib 1.8.67,
/// `schema/td_api.tl:1252`): `starSubscriptionPricing period:int32
/// star_count:int53 = StarSubscriptionPricing;`
#[derive(Debug, Clone, PartialEq)]
pub struct StarSubscriptionPricing {
    pub period: i32,
    pub star_count: i64,
}

/// Phase D3a: `chatInviteLink` (TDLib 1.8.67, `schema/td_api.tl:2627`).
/// `subscription_pricing` is `Option` because TDLib only attaches it to
/// subscription-priced links; everything else is required by the schema.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedChatInviteLink {
    pub invite_link: String,
    pub name: String,
    pub creator_user_id: i64,
    pub date: i32,
    pub edit_date: i32,
    pub expiration_date: i32,
    pub subscription_pricing: Option<StarSubscriptionPricing>,
    pub member_limit: i32,
    pub member_count: i32,
    pub expired_member_count: i32,
    pub pending_join_request_count: i32,
    pub creates_join_request: bool,
    pub is_primary: bool,
    pub is_revoked: bool,
}

/// Phase D3a: `chatJoinRequest` (TDLib 1.8.67, `schema/td_api.tl:2688`):
/// `chatJoinRequest user_id:int53 date:int32 bio:string = ChatJoinRequest;`
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedChatJoinRequest {
    pub user_id: i64,
    pub date: i32,
    pub bio: String,
}

/// Phase D3c: `chatEventAction` (TDLib 1.8.67, schema lines 7764–7928).
/// The 16 high-value constructors are typed below; every other
/// constructor maps to `Unsupported` carrying its constructor name, so
/// the UI renders an honest generic row instead of invented details.
/// (The schema has no `chatEventInviteLinkCreated` — link creation has
/// no event constructor in 1.8.67.)
#[derive(Debug, Clone, PartialEq)]
pub enum ChatEventAction {
    /// `chatEventMessageEdited` (line 7764). `text` is a short excerpt of
    /// the new message's text (empty for non-text content).
    MessageEdited { message_id: i64, text: String },
    /// `chatEventMessageDeleted` (line 7767).
    MessageDeleted { message_id: i64, text: String },
    /// `chatEventMessagePinned` (line 7770).
    MessagePinned { message_id: i64, text: String },
    /// `chatEventMessageUnpinned` (line 7773).
    MessageUnpinned { message_id: i64, text: String },
    /// `chatEventMemberJoined` (line 7779).
    MemberJoined,
    /// `chatEventMemberJoinedByInviteLink` (line 7782).
    MemberJoinedByInviteLink {
        invite_link: String,
        invite_link_name: String,
    },
    /// `chatEventMemberJoinedByRequest` (line 7785).
    MemberJoinedByRequest {
        approver_user_id: i64,
        invite_link: String,
    },
    /// `chatEventMemberInvited` (line 7788). The invitee's resulting
    /// status is carried for honest phrasing ("invited"/"added").
    MemberInvited {
        user_id: i64,
        status: ChannelMemberStatus,
    },
    /// `chatEventMemberPromoted` (line 7794).
    MemberPromoted {
        user_id: i64,
        old_status: ChannelMemberStatus,
        new_status: ChannelMemberStatus,
    },
    /// `chatEventMemberRestricted` (line 7797). Covers restrictions,
    /// bans, and their reversals (old/new statuses distinguish them).
    MemberRestricted {
        member_id: MessageSender,
        old_status: ChannelMemberStatus,
        new_status: ChannelMemberStatus,
    },
    /// `chatEventDescriptionChanged` (line 7812).
    DescriptionChanged {
        old_description: String,
        new_description: String,
    },
    /// `chatEventPhotoChanged` (line 7830).
    PhotoChanged,
    /// `chatEventTitleChanged` (line 7842).
    TitleChanged {
        old_title: String,
        new_title: String,
    },
    /// `chatEventInviteLinkEdited` (line 7886).
    InviteLinkEdited {
        old_url: String,
        old_name: String,
        new_url: String,
        new_name: String,
    },
    /// `chatEventInviteLinkRevoked` (line 7889).
    InviteLinkRevoked { url: String, name: String },
    /// `chatEventInviteLinkDeleted` (line 7892).
    InviteLinkDeleted { url: String, name: String },
    /// Any other `chatEvent*` constructor — the schema defines 53 (lines
    /// 7764–7928); the remainder render as generic rows.
    Unsupported { type_name: String },
}

/// Phase D3c: `chatEvent` (TDLib 1.8.67, `schema/td_api.tl:7935`):
/// `chatEvent id:int64 date:int32 member_id:MessageSender action:ChatEventAction = ChatEvent;`
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedChatEvent {
    pub id: i64,
    pub date: i32,
    pub member_id: MessageSender,
    pub action: ChatEventAction,
}

fn parse_star_subscription_pricing(value: Option<&Value>) -> Option<StarSubscriptionPricing> {
    let value = value?;
    if value.get("@type").and_then(Value::as_str) != Some("starSubscriptionPricing") {
        return None;
    }
    Some(StarSubscriptionPricing {
        period: int53(value.get("period")).ok()? as i32,
        star_count: int53(value.get("star_count")).ok()?,
    })
}

fn parse_chat_invite_link(value: Option<&Value>) -> Option<ParsedChatInviteLink> {
    let value = value?;
    if value.get("@type").and_then(Value::as_str) != Some("chatInviteLink") {
        return None;
    }
    Some(ParsedChatInviteLink {
        invite_link: value.get("invite_link").and_then(Value::as_str)?.to_owned(),
        name: value
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        creator_user_id: int53(value.get("creator_user_id")).ok()?,
        date: int53(value.get("date")).ok()? as i32,
        edit_date: int53(value.get("edit_date")).ok().unwrap_or(0) as i32,
        expiration_date: int53(value.get("expiration_date")).ok().unwrap_or(0) as i32,
        subscription_pricing: parse_star_subscription_pricing(value.get("subscription_pricing")),
        member_limit: int53(value.get("member_limit")).ok().unwrap_or(0) as i32,
        member_count: int53(value.get("member_count")).ok().unwrap_or(0) as i32,
        expired_member_count: int53(value.get("expired_member_count")).ok().unwrap_or(0) as i32,
        pending_join_request_count: int53(value.get("pending_join_request_count"))
            .ok()
            .unwrap_or(0) as i32,
        creates_join_request: value
            .get("creates_join_request")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        is_primary: value
            .get("is_primary")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        is_revoked: value
            .get("is_revoked")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
}

fn parse_chat_join_request(value: Option<&Value>) -> Option<ParsedChatJoinRequest> {
    let value = value?;
    if value.get("@type").and_then(Value::as_str) != Some("chatJoinRequest") {
        return None;
    }
    Some(ParsedChatJoinRequest {
        user_id: int53(value.get("user_id")).ok()?,
        date: int53(value.get("date")).ok()? as i32,
        bio: value
            .get("bio")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
    })
}

/// Phase D3c: short text excerpt of a `message` object inside a
/// `chatEvent*` action (edited/deleted/pinned). Only `messageText`
/// content yields text; anything else is an empty string so the UI
/// falls back to an honest media-neutral phrasing.
fn chat_event_message_excerpt(message: Option<&Value>) -> (i64, String) {
    let message = match message {
        Some(message) => message,
        None => return (0, String::new()),
    };
    let id = int53(message.get("id")).unwrap_or(0);
    let raw = message
        .get("content")
        .filter(|content| content.get("@type").and_then(Value::as_str) == Some("messageText"))
        .and_then(|content| content.get("text"))
        .and_then(|text| text.get("text"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let mut excerpt: String = raw.chars().take(80).collect();
    if raw.chars().count() > 80 {
        excerpt.push('…');
    }
    (id, excerpt)
}

/// Phase D3c: `chatEventAction` object → typed action. The 16 handled
/// constructors (schema lines 7764/7767/7770/7773/7779/7782/7785/7788/
/// 7794/7797/7812/7830/7842/7886/7889/7892) parse their fields; every
/// other constructor degrades to `ChatEventAction::Unsupported` with its
/// constructor name (never invented details).
fn parse_chat_event_action(value: Option<&Value>) -> ChatEventAction {
    let value = match value {
        Some(value) => value,
        None => {
            return ChatEventAction::Unsupported {
                type_name: String::new(),
            };
        }
    };
    let type_name = value
        .get("@type")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    let member_status = |value: Option<&Value>| {
        parse_channel_member_status(value)
            .map(|(status, _)| status)
            .unwrap_or(ChannelMemberStatus::Unknown)
    };
    let invite_link_url_name = |value: Option<&Value>| {
        parse_chat_invite_link(value)
            .map(|link| (link.invite_link, link.name))
            .unwrap_or_default()
    };
    let string_field = |value: &Value, field: &str| {
        value
            .get(field)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned()
    };
    match type_name.as_str() {
        "chatEventMessageEdited" => {
            let (message_id, text) = chat_event_message_excerpt(value.get("new_message"));
            ChatEventAction::MessageEdited { message_id, text }
        }
        "chatEventMessageDeleted" => {
            let (message_id, text) = chat_event_message_excerpt(value.get("message"));
            ChatEventAction::MessageDeleted { message_id, text }
        }
        "chatEventMessagePinned" => {
            let (message_id, text) = chat_event_message_excerpt(value.get("message"));
            ChatEventAction::MessagePinned { message_id, text }
        }
        "chatEventMessageUnpinned" => {
            let (message_id, text) = chat_event_message_excerpt(value.get("message"));
            ChatEventAction::MessageUnpinned { message_id, text }
        }
        "chatEventMemberJoined" => ChatEventAction::MemberJoined,
        "chatEventMemberJoinedByInviteLink" => {
            let (invite_link, invite_link_name) = invite_link_url_name(value.get("invite_link"));
            ChatEventAction::MemberJoinedByInviteLink {
                invite_link,
                invite_link_name,
            }
        }
        "chatEventMemberJoinedByRequest" => {
            let (invite_link, _) = invite_link_url_name(value.get("invite_link"));
            ChatEventAction::MemberJoinedByRequest {
                approver_user_id: int53(value.get("approver_user_id")).unwrap_or(0),
                invite_link,
            }
        }
        "chatEventMemberInvited" => ChatEventAction::MemberInvited {
            user_id: int53(value.get("user_id")).unwrap_or(0),
            status: member_status(value.get("status")),
        },
        "chatEventMemberPromoted" => ChatEventAction::MemberPromoted {
            user_id: int53(value.get("user_id")).unwrap_or(0),
            old_status: member_status(value.get("old_status")),
            new_status: member_status(value.get("new_status")),
        },
        "chatEventMemberRestricted" => ChatEventAction::MemberRestricted {
            member_id: parse_message_sender(value.get("member_id"))
                .unwrap_or(MessageSender::User { user_id: 0 }),
            old_status: member_status(value.get("old_status")),
            new_status: member_status(value.get("new_status")),
        },
        "chatEventDescriptionChanged" => ChatEventAction::DescriptionChanged {
            old_description: string_field(value, "old_description"),
            new_description: string_field(value, "new_description"),
        },
        "chatEventPhotoChanged" => ChatEventAction::PhotoChanged,
        "chatEventTitleChanged" => ChatEventAction::TitleChanged {
            old_title: string_field(value, "old_title"),
            new_title: string_field(value, "new_title"),
        },
        "chatEventInviteLinkEdited" => {
            let (old_url, old_name) = invite_link_url_name(value.get("old_invite_link"));
            let (new_url, new_name) = invite_link_url_name(value.get("new_invite_link"));
            ChatEventAction::InviteLinkEdited {
                old_url,
                old_name,
                new_url,
                new_name,
            }
        }
        "chatEventInviteLinkRevoked" => {
            let (url, name) = invite_link_url_name(value.get("invite_link"));
            ChatEventAction::InviteLinkRevoked { url, name }
        }
        "chatEventInviteLinkDeleted" => {
            let (url, name) = invite_link_url_name(value.get("invite_link"));
            ChatEventAction::InviteLinkDeleted { url, name }
        }
        _ => ChatEventAction::Unsupported { type_name },
    }
}

/// Phase D3c: `chatEvent` (schema 1.8.67, line 7935). Events whose
/// `member_id` (the actor) fails to parse are dropped rather than
/// misattributed; `id` and `date` are required.
fn parse_chat_event(value: &Value) -> Option<ParsedChatEvent> {
    Some(ParsedChatEvent {
        id: int53(value.get("id")).ok()?,
        date: int53(value.get("date")).ok()? as i32,
        member_id: parse_message_sender(value.get("member_id")).ok()?,
        action: parse_chat_event_action(value.get("action")),
    })
}

fn parse_statistical_value(value: Option<&Value>) -> Result<StatisticalValue, ParseError> {
    // All `statisticalValue` fields on `chatStatisticsChannel` /
    // `chatStatisticsSupergroup` are required by the schema; a missing,
    // null, or mistyped value is a parse error rather than fabricated
    // zeros.
    let value = value
        .filter(|v| !v.is_null())
        .ok_or(ParseError::MissingField)?;
    StatisticalValue::parse(value)
}

fn parse_statistical_graph(value: Option<&Value>) -> Result<StatisticalGraph, ParseError> {
    let value = value
        .filter(|v| !v.is_null())
        .ok_or(ParseError::MissingField)?;
    StatisticalGraph::parse(value)
}

fn parse_statistics_period(value: Option<&Value>) -> Result<(i32, i32), ParseError> {
    // `dateRange start_date:int32 end_date:int32` (schema 1.8.67, line 10135).
    let value = value
        .filter(|v| !v.is_null())
        .ok_or(ParseError::MissingField)?;
    let start = value
        .get("start_date")
        .and_then(Value::as_i64)
        .ok_or(ParseError::MissingField)? as i32;
    let end = value
        .get("end_date")
        .and_then(Value::as_i64)
        .ok_or(ParseError::MissingField)? as i32;
    Ok((start, end))
}

fn parse_chat_statistics_object(value: Option<&Value>) -> Option<ChatStatisticsObject> {
    let value = value?;
    match value.get("@type").and_then(Value::as_str) {
        Some("chatStatisticsObjectTypeMessage") => Some(ChatStatisticsObject::Message {
            message_id: int53_or_zero(value.get("message_id")),
        }),
        Some("chatStatisticsObjectTypeStory") => Some(ChatStatisticsObject::Story {
            story_id: value.get("story_id").and_then(Value::as_i64).unwrap_or(0) as i32,
        }),
        _ => None,
    }
}

fn parse_chat_statistics_interaction_info(value: &Value) -> Option<ChatStatisticsInteractionInfo> {
    Some(ChatStatisticsInteractionInfo {
        object: parse_chat_statistics_object(value.get("object_type"))?,
        view_count: value.get("view_count").and_then(Value::as_i64).unwrap_or(0) as i32,
        forward_count: value
            .get("forward_count")
            .and_then(Value::as_i64)
            .unwrap_or(0) as i32,
        reaction_count: value
            .get("reaction_count")
            .and_then(Value::as_i64)
            .unwrap_or(0) as i32,
    })
}

/// Phase D2: parses `chatStatisticsChannel` / `chatStatisticsSupergroup`
/// into `ChatStatistics`. Unknown `ChatStatistics` variants are a parse
/// error (the panel renders an honest "unsupported" state).
fn parse_chat_statistics(value: &Value) -> Result<ChatStatistics, ParseError> {
    match value.get("@type").and_then(Value::as_str) {
        Some("chatStatisticsChannel") => {
            let (period_start, period_end) = parse_statistics_period(value.get("period"))?;
            let graph = |name: &str| parse_statistical_graph(value.get(name));
            Ok(ChatStatistics::Channel(Box::new(ChannelStatistics {
                period_start,
                period_end,
                member_count: parse_statistical_value(value.get("member_count"))?,
                mean_message_view_count: parse_statistical_value(
                    value.get("mean_message_view_count"),
                )?,
                mean_message_share_count: parse_statistical_value(
                    value.get("mean_message_share_count"),
                )?,
                mean_message_reaction_count: parse_statistical_value(
                    value.get("mean_message_reaction_count"),
                )?,
                mean_story_view_count: parse_statistical_value(value.get("mean_story_view_count"))?,
                mean_story_share_count: parse_statistical_value(
                    value.get("mean_story_share_count"),
                )?,
                mean_story_reaction_count: parse_statistical_value(
                    value.get("mean_story_reaction_count"),
                )?,
                enabled_notifications_percentage: value
                    .get("enabled_notifications_percentage")
                    .and_then(Value::as_f64)
                    .unwrap_or(0.0),
                member_count_graph: graph("member_count_graph")?,
                join_graph: graph("join_graph")?,
                mute_graph: graph("mute_graph")?,
                view_count_by_hour_graph: graph("view_count_by_hour_graph")?,
                view_count_by_source_graph: graph("view_count_by_source_graph")?,
                join_by_source_graph: graph("join_by_source_graph")?,
                language_graph: graph("language_graph")?,
                message_interaction_graph: graph("message_interaction_graph")?,
                message_reaction_graph: graph("message_reaction_graph")?,
                story_interaction_graph: graph("story_interaction_graph")?,
                story_reaction_graph: graph("story_reaction_graph")?,
                instant_view_interaction_graph: graph("instant_view_interaction_graph")?,
                recent_interactions: value
                    .get("recent_interactions")
                    .and_then(Value::as_array)
                    .map(|items| {
                        items
                            .iter()
                            .filter_map(parse_chat_statistics_interaction_info)
                            .collect()
                    })
                    .unwrap_or_default(),
            })))
        }
        Some("chatStatisticsSupergroup") => {
            let (period_start, period_end) = parse_statistics_period(value.get("period"))?;
            let graph = |name: &str| parse_statistical_graph(value.get(name));
            Ok(ChatStatistics::Supergroup(Box::new(SupergroupStatistics {
                period_start,
                period_end,
                member_count: parse_statistical_value(value.get("member_count"))?,
                message_count: parse_statistical_value(value.get("message_count"))?,
                viewer_count: parse_statistical_value(value.get("viewer_count"))?,
                sender_count: parse_statistical_value(value.get("sender_count"))?,
                member_count_graph: graph("member_count_graph")?,
                join_graph: graph("join_graph")?,
                join_by_source_graph: graph("join_by_source_graph")?,
                language_graph: graph("language_graph")?,
                message_content_graph: graph("message_content_graph")?,
                action_graph: graph("action_graph")?,
                day_graph: graph("day_graph")?,
                week_graph: graph("week_graph")?,
                top_senders: value
                    .get("top_senders")
                    .and_then(Value::as_array)
                    .map(|items| {
                        items
                            .iter()
                            .map(|item| ChatStatisticsMessageSenderInfo {
                                user_id: int53_or_zero(item.get("user_id")),
                                sent_message_count: item
                                    .get("sent_message_count")
                                    .and_then(Value::as_i64)
                                    .unwrap_or(0)
                                    as i32,
                                average_character_count: item
                                    .get("average_character_count")
                                    .and_then(Value::as_i64)
                                    .unwrap_or(0)
                                    as i32,
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
                top_administrators: value
                    .get("top_administrators")
                    .and_then(Value::as_array)
                    .map(|items| {
                        items
                            .iter()
                            .map(|item| ChatStatisticsAdministratorActionsInfo {
                                user_id: int53_or_zero(item.get("user_id")),
                                deleted_message_count: item
                                    .get("deleted_message_count")
                                    .and_then(Value::as_i64)
                                    .unwrap_or(0)
                                    as i32,
                                banned_user_count: item
                                    .get("banned_user_count")
                                    .and_then(Value::as_i64)
                                    .unwrap_or(0)
                                    as i32,
                                restricted_user_count: item
                                    .get("restricted_user_count")
                                    .and_then(Value::as_i64)
                                    .unwrap_or(0)
                                    as i32,
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
                top_inviters: value
                    .get("top_inviters")
                    .and_then(Value::as_array)
                    .map(|items| {
                        items
                            .iter()
                            .map(|item| ChatStatisticsInviterInfo {
                                user_id: int53_or_zero(item.get("user_id")),
                                added_member_count: item
                                    .get("added_member_count")
                                    .and_then(Value::as_i64)
                                    .unwrap_or(0)
                                    as i32,
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
            })))
        }
        _ => Err(ParseError::MissingField),
    }
}

/// Official Telegram default emoji reactions (tdesktop active-emoji first
/// row / Unigram default picker). Custom emoji and paid stay out of this slice.
pub const DEFAULT_EMOJI_REACTIONS: &[&str] = &[
    "👍", "👎", "❤", "🔥", "🥰", "👏", "😁", "🤔", "🤯", "😱", "🤬", "😢", "🎉", "🤩", "🤮", "💩",
];

/// `ReactionType` (TDLib 1.8.67). Phase 1 chips use emoji only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReactionType {
    Emoji { emoji: String },
    CustomEmoji { custom_emoji_id: i64 },
    Paid,
    Unknown,
}

impl ReactionType {
    pub fn emoji(emoji: impl Into<String>) -> Self {
        Self::Emoji {
            emoji: emoji.into(),
        }
    }

    pub fn emoji_text(&self) -> Option<&str> {
        match self {
            Self::Emoji { emoji } => Some(emoji.as_str()),
            _ => None,
        }
    }

    /// JSON object for `addMessageReaction` / `removeMessageReaction`.
    pub fn to_tdlib_json(&self) -> serde_json::Value {
        match self {
            Self::Emoji { emoji } => serde_json::json!({
                "@type": "reactionTypeEmoji",
                "emoji": emoji
            }),
            Self::CustomEmoji { custom_emoji_id } => serde_json::json!({
                "@type": "reactionTypeCustomEmoji",
                "custom_emoji_id": custom_emoji_id.to_string()
            }),
            Self::Paid => serde_json::json!({ "@type": "reactionTypePaid" }),
            Self::Unknown => serde_json::json!({ "@type": "reactionTypeEmoji", "emoji": "" }),
        }
    }
}

/// `messageReaction` (TDLib 1.8.67). `used_sender_id` / recent senders stay out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageReaction {
    pub reaction_type: ReactionType,
    pub total_count: i32,
    pub is_chosen: bool,
}

impl MessageReaction {
    /// Chip label: emoji + count (tdesktop InlineList / Unigram ReactionButton).
    pub fn chip_label(&self) -> Option<String> {
        let emoji = self.reaction_type.emoji_text()?;
        Some(format!("{emoji} {}", self.total_count))
    }
}

/// `messageReactions` (TDLib 1.8.67). Tags / paid reactors stay out of display.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MessageReactions {
    pub reactions: Vec<MessageReaction>,
    pub are_tags: bool,
}

impl MessageReactions {
    pub fn emoji_chips(&self) -> impl Iterator<Item = &MessageReaction> {
        self.reactions
            .iter()
            .filter(|reaction| reaction.reaction_type.emoji_text().is_some())
    }

    pub fn chosen_emoji(&self, emoji: &str) -> bool {
        self.reactions.iter().any(|reaction| {
            reaction.is_chosen && reaction.reaction_type.emoji_text() == Some(emoji)
        })
    }
}

/// `messageInteractionInfo` (TDLib 1.8.67). `reply_info` stays out of this slice.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MessageInteractionInfo {
    pub view_count: i32,
    pub forward_count: i32,
    pub reactions: Option<MessageReactions>,
}

impl MessageInteractionInfo {
    pub fn emoji_chips(&self) -> Vec<&MessageReaction> {
        self.reactions
            .as_ref()
            .map(|reactions| reactions.emoji_chips().collect())
            .unwrap_or_default()
    }

    pub fn chosen_emoji(&self, emoji: &str) -> bool {
        self.reactions
            .as_ref()
            .is_some_and(|reactions| reactions.chosen_emoji(emoji))
    }
}

/// Toggle the current user's chosen emoji (tdesktop chip click / Unigram
/// ReactionButton). Used to build the next `updateMessageInteractionInfo`.
pub fn toggle_chosen_emoji_reaction(
    current: Option<&MessageInteractionInfo>,
    emoji: &str,
) -> MessageInteractionInfo {
    let mut info = current.cloned().unwrap_or_default();
    let mut reactions = info.reactions.take().unwrap_or_default();
    if let Some(existing) = reactions
        .reactions
        .iter_mut()
        .find(|reaction| reaction.reaction_type.emoji_text() == Some(emoji))
    {
        if existing.is_chosen {
            existing.is_chosen = false;
            existing.total_count = (existing.total_count - 1).max(0);
        } else {
            existing.is_chosen = true;
            existing.total_count += 1;
        }
    } else {
        reactions.reactions.push(MessageReaction {
            reaction_type: ReactionType::emoji(emoji),
            total_count: 1,
            is_chosen: true,
        });
    }
    reactions
        .reactions
        .retain(|reaction| reaction.total_count > 0);
    info.reactions = if reactions.reactions.is_empty() {
        None
    } else {
        Some(reactions)
    };
    info
}

/// Same-chat / known-chat reply metadata from `message.reply_to`.
/// Stories and unknown `MessageReplyTo` variants are dropped (out of scope).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageReplyTo {
    pub chat_id: ChatId,
    pub message_id: MessageId,
    pub quote_text: Option<String>,
    pub content_preview: Option<String>,
}

impl MessageReplyTo {
    pub fn is_same_chat(&self, open_chat: ChatId) -> bool {
        self.chat_id.0 == 0 || self.chat_id == open_chat
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedMessage {
    pub id: MessageId,
    pub chat_id: ChatId,
    pub is_outgoing: bool,
    /// Schema `message.is_pinned` (TDLib 1.8.67).
    pub is_pinned: bool,
    /// Schema `message.topic_id` (TDLib 1.8.67, line 3165): the
    /// `forum_topic_id` when the topic is `messageTopicForum`, `None` for
    /// every other `MessageTopic` variant (threads, direct messages, saved
    /// messages) and when the field is absent. Parity slice 4 routes topic
    /// messages into the topic's history.
    pub topic_id: Option<i32>,
    /// Schema `message.media_album_id` (int64). `0` means the message is not in an album.
    pub media_album_id: i64,
    /// Phase D2: schema `message.author_signature` (TDLib 1.8.67, lines
    /// 3155/3165) — "For channel posts and anonymous group messages,
    /// optional author signature". `None` when absent or empty; renders as
    /// the small signature line under the post (suppressed under
    /// forwarded-message headers, which already attribute it).
    pub author_signature: Option<String>,
    pub content: MessageContent,
    pub files: Vec<ParsedFile>,
    pub reply_to: Option<MessageReplyTo>,
    pub forward_info: Option<MessageForwardInfo>,
    pub interaction_info: Option<MessageInteractionInfo>,
    /// Schema `message.reply_markup` (TDLib 1.8.67). Only
    /// `replyMarkupInlineKeyboard` is kept; other markups are `None`.
    pub reply_markup: Option<InlineKeyboard>,
    /// Phase B3: `message.self_destruct_type` / `message.self_destruct_in`
    /// (TDLib 1.8.67, `schema/td_api.tl:3146`–`:3147` / `:3165`).
    /// `messageSelfDestructTypeTimer` (line 5915) /
    /// `messageSelfDestructTypeImmediately` (line 5918); unknown future
    /// variants degrade to `None` (message renders without a timer badge).
    pub self_destruct: Option<MessageSelfDestruct>,
    /// Phase B4: `message.auto_delete_in` (TDLib 1.8.67,
    /// `schema/td_api.tl:3148` / `:3165`) — seconds left before the
    /// chat's `message_auto_delete_time` setting deletes this message;
    /// `None` when never. Renders as a countdown chip on the row; the
    /// row itself leaves via `updateDeleteMessages`.
    pub auto_delete: Option<MessageAutoDelete>,
}

/// Phase B3: the configured self-destruct mode of a message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelfDestructKind {
    /// `messageSelfDestructTypeTimer` — destroyed `secs` seconds after the
    /// content was opened (schema 1.8.67 line 5915).
    Timer { secs: i32 },
    /// `messageSelfDestructTypeImmediately` — destroyed once closed after a
    /// single viewing (schema 1.8.67 line 5918).
    Immediately,
}

/// Phase B3: parsed `message.self_destruct_type` + `message.self_destruct_in`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MessageSelfDestruct {
    pub kind: SelfDestructKind,
    /// Latest `message.self_destruct_in`, converted to whole milliseconds.
    /// `0` means the destruction isn't scheduled yet (the content hasn't
    /// been opened; schema 1.8.67 line 3147). TDLib emits
    /// `updateDeleteMessages` when the timer fires, so the row disappears
    /// through the normal delete path — no special handling needed.
    /// Whole milliseconds (not `f64` seconds) so the struct keeps the
    /// `Eq` derive that `ParsedMessage` / `HistoryMessage` require.
    pub expires_in_ms: i64,
    /// Local clock (ms) when `expires_in_ms` was received, for the local
    /// countdown decay (`slow_mode_delay_expires_in` pattern, Phase A1).
    pub fetched_at_ms: u64,
}

impl MessageSelfDestruct {
    /// Locally decayed whole seconds left, or `None` when destruction isn't
    /// scheduled yet. The value keeps decaying to 0 (the row stays until
    /// TDLib's `updateDeleteMessages` removes it).
    pub fn remaining_secs(&self, now_ms: u64) -> Option<u64> {
        if self.expires_in_ms <= 0 {
            return None;
        }
        let elapsed_ms = now_ms.saturating_sub(self.fetched_at_ms) as i64;
        let remaining_ms = self.expires_in_ms - elapsed_ms;
        Some(if remaining_ms > 0 {
            ((remaining_ms + 999) / 1000) as u64
        } else {
            0
        })
    }

    /// Timer badge for media rows: "⏱ view once" / "⏱ 60s" (not scheduled
    /// yet) / "⏱ 42s left" (scheduled).
    pub fn badge_label(&self, now_ms: u64) -> String {
        match self.kind {
            SelfDestructKind::Immediately => "⏱ view once".to_string(),
            SelfDestructKind::Timer { secs } => match self.remaining_secs(now_ms) {
                Some(left) => format!("⏱ {left}s left"),
                None => format!("⏱ {secs}s"),
            },
        }
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Parse `message.self_destruct_type` + `message.self_destruct_in` (schema
/// 1.8.67 lines 3146–3147 / 3165 / 5915 / 5918). `self_destruct_in` arrives
/// as a double (seconds, possibly fractional); non-finite or negative
/// values are treated as unscheduled.
fn parse_self_destruct(
    type_value: Option<&Value>,
    in_value: Option<&Value>,
) -> Option<MessageSelfDestruct> {
    let type_value = type_value?;
    if type_value.is_null() {
        return None;
    }
    let kind = match type_value.get("@type").and_then(Value::as_str) {
        Some("messageSelfDestructTypeTimer") => SelfDestructKind::Timer {
            secs: type_value
                .get("self_destruct_time")
                .and_then(Value::as_i64)
                .map(|s| s.clamp(0, i32::MAX as i64) as i32)
                .unwrap_or(0),
        },
        Some("messageSelfDestructTypeImmediately") => SelfDestructKind::Immediately,
        _ => return None,
    };
    let expires_in_ms = in_value
        .and_then(Value::as_f64)
        .filter(|v| v.is_finite() && *v >= 0.0)
        .map(|v| (v * 1000.0).round() as i64)
        .unwrap_or(0);
    Some(MessageSelfDestruct {
        kind,
        expires_in_ms,
        fetched_at_ms: now_ms(),
    })
}

/// Phase B4: parsed `message.auto_delete_in` (schema 1.8.67, lines
/// 3148 / 3165) — "Time left before the message will be automatically
/// deleted by message_auto_delete_time setting of the chat, in seconds;
/// 0 if never". Arrives as a double (seconds, possibly fractional);
/// non-finite or negative values degrade to `None` (the row renders
/// without a countdown). TDLib removes the row via `updateDeleteMessages`
/// when it fires — no special deletion code. Whole milliseconds (not
/// `f64`) so the struct keeps the `Eq` derive that `ParsedMessage` /
/// `HistoryMessage` require.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MessageAutoDelete {
    pub expires_in_ms: i64,
    /// Local clock (ms) when `expires_in_ms` was received, for the local
    /// countdown decay (same pattern as `MessageSelfDestruct`, Phase B3).
    pub fetched_at_ms: u64,
}

impl MessageAutoDelete {
    /// Locally decayed whole seconds left. The value keeps decaying to 0
    /// (the row stays until TDLib's `updateDeleteMessages` removes it).
    pub fn remaining_secs(&self, now_ms: u64) -> u64 {
        let elapsed_ms = now_ms.saturating_sub(self.fetched_at_ms) as i64;
        let remaining_ms = self.expires_in_ms - elapsed_ms;
        if remaining_ms > 0 {
            ((remaining_ms + 999) / 1000) as u64
        } else {
            0
        }
    }

    /// Countdown chip label for message rows, e.g. "🗑 59m left".
    pub fn chip_label(&self, now_ms: u64) -> String {
        format!(
            "🗑 {} left",
            format_countdown_secs(self.remaining_secs(now_ms))
        )
    }
}

/// Compact duration for countdown labels: 45 → "45s", 90 → "2m",
/// 3600 → "1h", 90000 → "1d". Rounds up like the self-destruct badge.
pub fn format_countdown_secs(secs: u64) -> String {
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{}m", secs.div_ceil(60))
    } else if secs < 86400 {
        format!("{}h", secs.div_ceil(3600))
    } else {
        format!("{}d", secs.div_ceil(86400))
    }
}

/// Phase B4: compact label for a chat-level TTL setting in seconds —
/// "5s" / "30s" / "1m" / "1h" / "1d" / "7d" for the values the picker
/// offers, exact units for arbitrary values other clients may set
/// (90 → "2m" would be wrong; 90s → "90s"). Picks the largest unit that
/// divides the value evenly.
pub fn format_ttl_setting(secs: i32) -> String {
    if secs <= 0 {
        return "Off".to_string();
    }
    if secs % 86400 == 0 {
        format!("{}d", secs / 86400)
    } else if secs % 3600 == 0 {
        format!("{}h", secs / 3600)
    } else if secs % 60 == 0 {
        format!("{}m", secs / 60)
    } else {
        format!("{secs}s")
    }
}

/// Phase B4: service-row label for `messageChatSetMessageAutoDeleteTime`
/// (schema 1.8.67, line 5387). `secret` selects the official wording:
/// "Self-destruct timer" in secret chats, "Auto-delete timer" elsewhere.
pub fn chat_ttl_service_label(secs: i32, secret: bool) -> String {
    let noun = if secret {
        "Self-destruct timer"
    } else {
        "Auto-delete timer"
    };
    if secs > 0 {
        format!("{noun} set to {}", format_ttl_setting(secs))
    } else {
        format!("{noun} turned off")
    }
}

/// Parse `message.auto_delete_in` (schema 1.8.67, lines 3148 / 3165).
/// `None` when the field is absent, null, or 0 (never auto-deleted);
/// garbage degrades to `None`.
fn parse_auto_delete_in(value: Option<&Value>) -> Option<MessageAutoDelete> {
    let secs = value?.as_f64()?;
    if !secs.is_finite() || secs <= 0.0 {
        return None;
    }
    Some(MessageAutoDelete {
        expires_in_ms: (secs * 1000.0).round() as i64,
        fetched_at_ms: now_ms(),
    })
}

/// One `forumTopic` (TDLib 1.8.67, `schema/td_api.tl:3968` + `forumTopicInfo`
/// at 3953). Only the fields the topic list / topic view need are kept;
/// dropped fields are documented in the Phase 5.1 DECISIONS entry:
/// `icon` (custom-emoji topic icons are not rendered), `creation_date`,
/// `creator_id`, `is_outgoing`, `is_hidden`, `is_name_implicit`,
/// `last_read_inbox/outbox_message_id`, `unread_mention_count`,
/// `unread_reaction_count`, `unread_poll_vote_count`,
/// `notification_settings`, `draft_message`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForumTopic {
    pub forum_topic_id: i32,
    pub name: String,
    pub is_general: bool,
    pub is_closed: bool,
    pub is_pinned: bool,
    pub unread_count: i32,
    /// Schema `forumTopic.order` — topics sort by order descending.
    pub order: i64,
    /// Cheap preview of `forumTopic.last_message` via
    /// `MessageContent::preview`; empty when there is no last message.
    pub last_message_preview: String,
}

/// Parse one `forumTopic` object. Returns `None` when `info` is missing or
/// malformed (the row is skipped, matching the lenient message parsing).
fn parse_forum_topic(value: &Value) -> Option<ForumTopic> {
    let info = value.get("info")?;
    let forum_topic_id = info.get("forum_topic_id")?.as_i64()? as i32;
    let name = json_field_str(info, "name");
    let is_general = info
        .get("is_general")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let is_closed = info
        .get("is_closed")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let is_pinned = value
        .get("is_pinned")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let unread_count = value
        .get("unread_count")
        .and_then(Value::as_i64)
        .unwrap_or(0) as i32;
    let order = int53_or_zero(value.get("order"));
    let last_message_preview = value
        .get("last_message")
        .and_then(|m| parse_message(m).ok())
        .map(|m| m.content.preview())
        .unwrap_or_default();
    Some(ForumTopic {
        forum_topic_id,
        name,
        is_general,
        is_closed,
        is_pinned,
        unread_count,
        order,
        last_message_preview,
    })
}

/// Phase 9.1: a story list identifier — `storyListMain` /
/// `storyListArchive` (TDLib 1.8.67, `schema/td_api.tl:6684-6690`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoryListView {
    Main,
    Archive,
}

/// Phase 9.1: `storyInfo` — basic information about one active story
/// (TDLib 1.8.67, `schema/td_api.tl:6767-6773`). `chatActiveStories.stories`
/// arrive in chronological order (increasing `story_id`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoryInfoView {
    pub story_id: i32,
    pub date: i32,
    pub is_for_close_friends: bool,
    pub is_live: bool,
}

/// Phase 9.1: `chatActiveStories` — active stories posted by a chat (TDLib
/// 1.8.67, `schema/td_api.tl:6776-6783`). Only the tray needs are kept:
/// `list` (null when the stories are not shown in any story list),
/// `order` (tray sort key), `max_read_story_id` (unread rings), and the
/// `storyInfo` list. Dropped: `can_be_archived`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatActiveStoriesView {
    pub chat_id: i64,
    pub list: Option<StoryListView>,
    pub order: i64,
    pub max_read_story_id: i32,
    pub stories: Vec<StoryInfoView>,
}

impl ChatActiveStoriesView {
    /// True when any active story is newer than `max_read_story_id` — the
    /// tray ring renders unread.
    pub fn has_unread(&self) -> bool {
        self.stories
            .iter()
            .any(|story| story.story_id > self.max_read_story_id)
    }
}

/// Phase 9.1: story media Quill renders in the viewer: photo and video only.
/// Live stories and unsupported content keep the story item but render a
/// placeholder (no group-call join, no RTMP — out of scope).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoryContentView {
    Photo {
        sizes: Vec<PhotoSizeView>,
    },
    Video {
        /// `storyVideo.thumbnail.file` (`thumbnail`, TDLib 1.8.67,
        /// `schema/td_api.tl:6633`) — the only display candidate; the full
        /// clip is not renderable by the image element (same call as the
        /// Phase 4.5 media viewer).
        thumb_file_id: Option<FileId>,
        thumb_width: i32,
        thumb_height: i32,
        /// `storyVideo.duration:double` — whole seconds for the label.
        duration_secs: i32,
        /// `storyVideo.video:file` — downloaded when there is no thumbnail
        /// so the file lands local.
        file_id: FileId,
    },
    Live,
    Unsupported,
}

/// Phase 9.2: `storyInteractionInfo` — interaction counters on a story
/// (TDLib 1.8.67, `schema/td_api.tl:6712`). Only populated by TDLib for
/// stories the current user posted (`story.can_get_interactions`); kept so
/// the viewer can render view/reaction counts on own stories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StoryInteractionInfoView {
    pub view_count: i32,
    pub forward_count: i32,
    pub reaction_count: i32,
}

impl StoryInteractionInfoView {
    /// True when at least one counter is nonzero.
    pub fn any_nonzero(&self) -> bool {
        self.view_count > 0 || self.forward_count > 0 || self.reaction_count > 0
    }
}

/// Phase 9.2: one emoji reaction the story picker can offer —
/// `availableReaction` (TDLib 1.8.67, `schema/td_api.tl:7321`). Only
/// `reactionTypeEmoji` entries render; custom-emoji entries are dropped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoryAvailableReactionView {
    pub emoji: String,
    pub needs_premium: bool,
}

/// Phase 9.1: `story` — the full story object (TDLib 1.8.67,
/// `schema/td_api.tl:6742`). Kept: ids, `date`, `content`, `caption`;
/// Phase 9.2 keeps: `chosen_reaction_type` (the user's own reaction),
/// `interaction_info` (view/forward/reaction counts), and the
/// `can_be_deleted` / `can_be_replied` / `can_get_interactions` gates.
/// Dropped (see DECISIONS.md Phase 9.1): repost info, privacy settings,
/// clickable areas, album ids, and the other `is_*` / `can_be_*` flags.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedStory {
    pub id: i32,
    pub poster_chat_id: i64,
    pub date: i32,
    pub content: StoryContentView,
    pub caption: String,
    pub caption_entities: Vec<TextEntity>,
    /// Phase 9.2: the user's own reaction on this story (`emoji`), `None`
    /// when none is chosen or when the chosen reaction is a custom emoji /
    /// paid reaction (same call as message reactions).
    pub chosen_reaction_emoji: Option<String>,
    /// Phase 9.2: `storyInteractionInfo` — counters, meaningful only when
    /// `can_get_interactions`.
    pub interaction_info: Option<StoryInteractionInfoView>,
    /// Phase 9.2: `story.can_be_deleted` — gates the viewer Delete button
    /// (`deleteStory`, `schema/td_api.tl:13754`).
    pub can_be_deleted: bool,
    /// Phase 9.2: `story.can_be_replied` — gates the viewer Reply affordance
    /// (`inputMessageReplyToStory`, `schema/td_api.tl:3099`).
    pub can_be_replied: bool,
    /// Phase 9.2: `story.can_get_interactions` — the interaction counters are
    /// the user's own.
    pub can_get_interactions: bool,
}

fn parse_story_list(value: Option<&Value>) -> Option<StoryListView> {
    match value
        .and_then(|value| value.get("@type"))
        .and_then(Value::as_str)
    {
        Some("storyListMain") => Some(StoryListView::Main),
        Some("storyListArchive") => Some(StoryListView::Archive),
        _ => None,
    }
}

fn parse_story_info(value: &Value) -> Option<StoryInfoView> {
    Some(StoryInfoView {
        story_id: value.get("story_id")?.as_i64()? as i32,
        date: value.get("date").and_then(Value::as_i64).unwrap_or(0) as i32,
        is_for_close_friends: value
            .get("is_for_close_friends")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        is_live: value
            .get("is_live")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
}

fn parse_chat_active_stories(value: &Value) -> Option<ChatActiveStoriesView> {
    Some(ChatActiveStoriesView {
        chat_id: int53(value.get("chat_id")).ok()?,
        list: parse_story_list(value.get("list")),
        order: int53_or_zero(value.get("order")),
        max_read_story_id: value
            .get("max_read_story_id")
            .and_then(Value::as_i64)
            .unwrap_or(0) as i32,
        stories: value
            .get("stories")
            .and_then(Value::as_array)
            .map(|stories| stories.iter().filter_map(parse_story_info).collect())
            .unwrap_or_default(),
    })
}

/// Parse one `story` object. Returns `None` when the required ids are
/// missing; unknown or missing `content` degrades to `Unsupported` rather
/// than failing the row. Collects the content's `file`s like the message
/// content parsers do.
fn parse_story(value: &Value) -> Option<(ParsedStory, Vec<ParsedFile>)> {
    let id = value.get("id")?.as_i64()? as i32;
    let poster_chat_id = int53(value.get("poster_chat_id")).ok()?;
    let (caption, caption_entities) = parse_caption(value.get("caption"));
    let mut files = Vec::new();
    let content = parse_story_content(value.get("content"), &mut files);
    files.retain(|file| file.id.0 != 0);
    Some((
        ParsedStory {
            id,
            poster_chat_id,
            date: value.get("date").and_then(Value::as_i64).unwrap_or(0) as i32,
            content,
            caption,
            caption_entities,
            chosen_reaction_emoji: parse_story_chosen_reaction(value.get("chosen_reaction_type")),
            interaction_info: parse_story_interaction_info(value.get("interaction_info")),
            can_be_deleted: value
                .get("can_be_deleted")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            can_be_replied: value
                .get("can_be_replied")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            can_get_interactions: value
                .get("can_get_interactions")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        },
        files,
    ))
}

/// Phase 9.2: `chosen_reaction_type` on a `story` — returns the emoji when
/// the user's chosen reaction is a `reactionTypeEmoji`, else `None` (no
/// reaction, custom emoji, or paid reaction).
fn parse_story_chosen_reaction(value: Option<&Value>) -> Option<String> {
    let reaction_type = value.filter(|value| !value.is_null())?;
    if reaction_type.get("@type").and_then(Value::as_str) != Some("reactionTypeEmoji") {
        return None;
    }
    reaction_type
        .get("emoji")
        .and_then(Value::as_str)
        .filter(|emoji| !emoji.is_empty())
        .map(str::to_string)
}

/// Phase 9.2: `storyInteractionInfo` counters; `None` when the field is
/// missing or null.
fn parse_story_interaction_info(value: Option<&Value>) -> Option<StoryInteractionInfoView> {
    let info = value.filter(|value| !value.is_null())?;
    Some(StoryInteractionInfoView {
        view_count: info.get("view_count").and_then(Value::as_i64).unwrap_or(0) as i32,
        forward_count: info
            .get("forward_count")
            .and_then(Value::as_i64)
            .unwrap_or(0) as i32,
        reaction_count: info
            .get("reaction_count")
            .and_then(Value::as_i64)
            .unwrap_or(0) as i32,
    })
}

/// Phase 9.2: one `availableReaction` row into the picker shape; drops
/// non-emoji reactions (custom emoji previews stay out of this slice).
fn parse_story_available_reaction(value: &Value) -> Option<StoryAvailableReactionView> {
    let reaction = value.get("type")?;
    if reaction.get("@type").and_then(Value::as_str) != Some("reactionTypeEmoji") {
        return None;
    }
    let emoji = reaction
        .get("emoji")
        .and_then(Value::as_str)
        .filter(|emoji| !emoji.is_empty())?
        .to_string();
    Some(StoryAvailableReactionView {
        emoji,
        needs_premium: value
            .get("needs_premium")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
}

fn parse_story_content(value: Option<&Value>, files: &mut Vec<ParsedFile>) -> StoryContentView {
    let Some(value) = value.filter(|value| !value.is_null()) else {
        return StoryContentView::Unsupported;
    };
    match value.get("@type").and_then(Value::as_str) {
        Some("storyContentPhoto") => match value.get("photo") {
            Some(photo) => {
                let (sizes, mut photo_files) = parse_photo_sizes(photo);
                files.append(&mut photo_files);
                StoryContentView::Photo { sizes }
            }
            None => StoryContentView::Unsupported,
        },
        Some("storyContentVideo") => {
            let video = value.get("video");
            let duration_secs = video
                .and_then(|video| video.get("duration"))
                .and_then(|duration| {
                    duration
                        .as_f64()
                        .or_else(|| duration.as_i64().map(|d| d as f64))
                })
                .unwrap_or(0.0) as i32;
            let file_id = video
                .and_then(|video| parse_file(video.get("video")).ok())
                .map(|file| {
                    let id = file.id;
                    files.push(file);
                    id
                })
                .unwrap_or(FileId(0));
            let thumb = video
                .and_then(|video| video.get("thumbnail"))
                .filter(|thumb| !thumb.is_null());
            let (thumb_file_id, thumb_width, thumb_height) = match thumb {
                Some(thumb) => (
                    parse_file(thumb.get("file"))
                        .ok()
                        .map(|file| {
                            let id = file.id;
                            files.push(file);
                            id
                        })
                        .filter(|id| id.0 != 0),
                    int53_or_zero(thumb.get("width")) as i32,
                    int53_or_zero(thumb.get("height")) as i32,
                ),
                None => (None, 0, 0),
            };
            StoryContentView::Video {
                thumb_file_id,
                thumb_width,
                thumb_height,
                duration_secs,
                file_id,
            }
        }
        Some("storyContentLive") => StoryContentView::Live,
        _ => StoryContentView::Unsupported,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MessageContent {
    Text(TextContent),
    Photo(PhotoContent),
    Document(DocumentContent),
    Sticker(StickerContent),
    Animation(AnimationContent),
    Video(VideoContent),
    VideoNote(VideoNoteContent),
    VoiceNote(VoiceNoteContent),
    Audio(AudioContent),
    /// Phase 4.2: `messagePoll` (TDLib 1.8.67, `schema/td_api.tl:5241`).
    Poll(PollContent),
    /// Phase 4.3: `messageLocation` (TDLib 1.8.67, `schema/td_api.tl:5214`)
    /// and `messageLiveLocation` (`schema/td_api.tl:5211`). The latter
    /// carries `LiveLocation` state; the former sets `live: None`.
    Location(LocationContent),
    /// Phase 4.3: `messageVenue` (TDLib 1.8.67, `schema/td_api.tl:5217`).
    Venue(VenueContent),
    /// Phase 4.3: `messageContact` (TDLib 1.8.67, `schema/td_api.tl:5220`).
    Contact(ContactContent),
    /// Phase 4.4: `messageDice` (TDLib 1.8.67, `schema/td_api.tl:5231`).
    Dice(DiceContent),
    /// Phase B4: `messageChatSetMessageAutoDeleteTime` (TDLib 1.8.67,
    /// `schema/td_api.tl:5387`) — the chat's auto-delete / self-destruct
    /// (secret chats) timer was changed; new value in seconds, 0 when
    /// disabled. Rendered as a centered service row (Quill has no generic
    /// service-message pipeline; this is the first one).
    ChatTtlChanged {
        secs: i32,
    },
    /// Phase C2f: `messageGroupCall` (TDLib 1.8.67,
    /// `schema/td_api.tl:5288`) — a group call not bound to a chat.
    /// Incoming, not active, not missed: an invitation the user can
    /// accept (`joinGroupCall`) or decline
    /// (`declineGroupCallInvitation`). `other_participant_ids` is not
    /// kept (the row only needs the invitation state).
    GroupCallInvitation {
        unique_id: i64,
        is_active: bool,
        was_missed: bool,
        is_video: bool,
    },
    /// Phase C2i: `messageCall` (TDLib 1.8.67,
    /// `schema/td_api.tl:5277`) — a 1:1 call entry in chat history.
    /// `messageCall unique_id:int64 is_video:Bool
    /// discard_reason:CallDiscardReason duration:int32 = MessageContent;`
    /// `unique_id` is not kept (the row needs only kind/reason/duration).
    Call {
        is_video: bool,
        discard_reason: CallDiscardReason,
        duration: i32,
    },
    /// Phase S1: `messageScreenshotTaken` (TDLib 1.8.67,
    /// `schema/td_api.tl:5375`) — a screenshot of a message in the chat
    /// has been taken. No fields; attribution comes from
    /// `message.is_outgoing` at render time.
    ScreenshotTaken,
    Unsupported {
        type_name: String,
    },
}
