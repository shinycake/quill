//! Chat state types: summaries, video-chat info, message helpers.
use super::*;

/// Outgoing read-receipt state from `last_read_outbox_message_id`.
/// Schema supports this; `0` means nothing outgoing has been read yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutboxReceipt {
    /// Not an outgoing server message (incoming or still pending).
    None,
    /// Reached the server; peer has not read past this id.
    Sent,
    /// `last_read_outbox_message_id` is >= this outgoing id.
    Read,
}

/// Slice B2: parse a bot deep link of the form
/// `t.me/<bot_username>?start=<start_parameter>` (with or without the
/// `https://` scheme) into `(bot_username, start_parameter)` — the two
/// pieces of `internalLinkTypeBotStart` (schema 1.8.67, line 9399) the
/// START button needs. Returns `None` for anything that isn't that shape
/// (different host, multi-segment path, missing/empty `start`).
/// URL-decoding of the parameter is deliberately skipped: the parameter
/// goes to `sendBotStartMessage` verbatim (schema line 12216), and
/// inventing a decode here would corrupt parameters the bot generated.
pub fn parse_bot_start_link(link: &str) -> Option<(String, String)> {
    let rest = link
        .strip_prefix("https://t.me/")
        .or_else(|| link.strip_prefix("http://t.me/"))
        .or_else(|| link.strip_prefix("https://telegram.me/"))
        .or_else(|| link.strip_prefix("t.me/"))?;
    let (path, query) = rest.split_once('?')?;
    if path.is_empty() || path.contains('/') {
        return None;
    }
    let parameter = query
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .find(|(key, _)| *key == "start")
        .map(|(_, value)| value.to_string())?;
    if parameter.is_empty() {
        return None;
    }
    Some((path.to_string(), parameter))
}

pub fn outgoing_status_label(pending: bool, receipt: OutboxReceipt) -> &'static str {
    if pending {
        "You (sending)"
    } else {
        match receipt {
            OutboxReceipt::Read => "You · read",
            OutboxReceipt::Sent | OutboxReceipt::None => "You · sent",
        }
    }
}

/// Compact local `HH:MM` timestamp for a TDLib `date` (unix seconds).
/// Returns `None` for `0`/negative (date absent).
pub fn message_time_hhmm(unix: i32) -> Option<String> {
    (unix > 0).then(|| crate::local_time::hhmm(&crate::local_time::civil_local(i64::from(unix))))
}

/// The chat-list row's view of `chat.last_message`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChatLastMessage {
    pub id: MessageId,
    /// Unix seconds.
    pub date: i32,
    pub is_outgoing: bool,
    /// Who sent it: group previews name the sender ("Dad: …").
    pub sender: Option<MessageSender>,
}

#[derive(Debug, Clone)]
pub struct ChatSummary {
    pub id: ChatId,
    pub title: String,
    pub kind: ChatKind,
    pub unread_count: i32,
    pub last_read_inbox_message_id: MessageId,
    pub last_read_outbox_message_id: MessageId,
    pub order: i64,
    pub is_pinned: bool,
    pub in_main_list: bool,
    /// `chatListArchive` placement (a non-zero `chatPosition`; the
    /// add/remove-from-list updates track `chat.chat_lists`, not rows).
    pub in_archive: bool,
    pub archive_order: i64,
    pub archive_is_pinned: bool,
    /// `chatListFolder` membership: folder id → TDLib order
    /// (`updateNewChat` / `updateChatPosition` / `updateChatLastMessage` /
    /// `updateChatDraftMessage` positions; order 0 removes the entry).
    /// `is_pinned` is not tracked — pinned folder chats already sort first
    /// by order.
    pub folder_positions: BTreeMap<i32, i64>,
    /// `chat.notification_settings` / `updateChatNotificationSettings`.
    pub notification_settings: ChatNotificationSettings,
    /// Sidebar preview from `updateChatLastMessage`. Not logged.
    pub last_preview: String,
    /// Slice chatlist-list-style: media icon + formatted-text entities
    /// for the last message, captured wherever `last_preview` is set
    /// (pure functions of the same content).
    pub last_preview_style: ChatPreviewStyle,
    /// Slice chatlist-list-style: sender name for the 3-line row ("You"
    /// for own messages, the author signature for signed channel posts,
    /// else the chat title — the list doesn't parse `sender_id`).
    pub last_preview_sender: String,
    /// The last message's photo minithumbnail: the chat list shows it
    /// small before the preview text, as Telegram Desktop does. Never set
    /// for secret or spoiler photos.
    pub last_preview_thumb: Option<std::sync::Arc<crate::telegram::envelope::MiniThumbnail>>,
    /// Identity, date and direction of `chat.last_message` for the row's
    /// timestamp and outgoing receipt; `None` for an empty chat.
    pub last_message: Option<ChatLastMessage>,
    /// Senders with an active `chatActionTyping` (`updateChatAction`).
    pub typing_senders: Vec<MessageSender>,
    /// Senders with an active `chatActionChoosingSticker` (`updateChatAction`).
    pub choosing_sticker_senders: Vec<MessageSender>,
    /// `chat.draft_message` text draft. Voice/rich drafts are not stored.
    pub draft: Option<ChatDraft>,
    /// Own `chatMemberStatus*` in a broadcast channel (`getChatMember` /
    /// `updateChatMember`). `None` until the first fetch completes; drives the
    /// composer gate and the join/leave affordance.
    pub my_member_status: Option<ChannelMemberStatus>,
    /// `rights.can_post_messages` from `chatMemberStatusAdministrator`
    /// (TDLib 1.8.67). `Some` only when the status is Administrator and the
    /// rights block parsed; `None` means "no explicit restriction" — a bare
    /// admin still posts.
    pub my_admin_can_post_messages: Option<bool>,
    /// Phase D3a: `rights.can_invite_users` from
    /// `chatMemberStatusAdministrator` (TDLib 1.8.67,
    /// `chatAdministratorRights`, schema line 1092). `Some` only when the
    /// status is Administrator and the rights block parsed; `None` for
    /// every other status or an absent rights block. Gates the invite-link
    /// / join-request management UI.
    pub my_admin_can_invite_users: Option<bool>,
    /// Slice G2: `rights.can_change_info` from
    /// `chatMemberStatusAdministrator` (TDLib 1.8.67,
    /// `chatAdministratorRights`, schema line 1092). `Some` only when the
    /// status is Administrator and the rights block parsed; `None` for
    /// every other status or an absent rights block. Gates
    /// `toggleSupergroupSignMessages` in channels (schema line 15175).
    pub my_admin_can_change_info: Option<bool>,
    /// Slice G2: `rights.can_send_welcome_messages` from
    /// `chatMemberStatusAdministrator` (TDLib 1.8.67,
    /// `chatAdministratorRights`, schema line 1090). `Some` only when the
    /// status is Administrator and the rights block parsed. Gates
    /// welcome-message management in channels.
    pub my_admin_can_send_welcome_messages: Option<bool>,
    /// Phase D3b: `rights.can_promote_members` from
    /// `chatMemberStatusAdministrator` (TDLib 1.8.67,
    /// `chatAdministratorRights`, schema line 1092). `Some` only when the
    /// status is Administrator and the rights block parsed; `None` for
    /// every other status or an absent rights block. Gates admin
    /// management (promote / demote / edit rights).
    pub my_admin_can_promote_members: Option<bool>,
    /// Slice G1: `rights.can_restrict_members` from
    /// `chatMemberStatusAdministrator` (TDLib 1.8.67,
    /// `chatAdministratorRights`, schema line 1092). `Some` only when the
    /// status is Administrator and the rights block parsed; `None` for
    /// every other status or an absent rights block. Gates member
    /// restriction / banning (`setChatMemberStatus` with a restricted or
    /// banned status requires this right, schema lines 13586-13587).
    pub my_admin_can_restrict_members: Option<bool>,
    /// MED1: `rights.can_pin_messages` from
    /// `chatMemberStatusAdministrator` (TDLib 1.8.67,
    /// `chatAdministratorRights`, schema line 1092). `Some` only when the
    /// status is Administrator and the rights block parsed; `None` for
    /// every other status or an absent rights block. Gates album pin /
    /// unpin (`pinChatMessage`, schema line 13559).
    pub my_admin_can_pin_messages: Option<bool>,
    /// Phase 5.1: `supergroup.is_forum` (TDLib 1.8.67). `None` until
    /// `updateSupergroup` / the `getSupergroup` response resolves it; only
    /// meaningful for non-channel supergroups.
    pub is_forum: Option<bool>,
    /// Parity slice: `chat.photo.small` file id (`chatPhotoInfo`, schema
    /// 1.8.67 line 762). `None` when the chat has no photo. Updated by
    /// `updateChatPhoto`; the file itself lives in `Session::files`.
    pub photo_file_id: Option<i32>,
    /// Parity slice 4: `chat.permissions.can_send_basic_messages`
    /// (`chatPermissions`, schema 1.8.67 line 1070), refreshed by
    /// `updateChatPermissions` (line 10500). Gates the topic composer
    /// alongside `ForumTopic.is_closed`.
    pub can_send_basic_messages: bool,
    /// Slice G1: the full default `chat.permissions` block
    /// (`chatPermissions`, schema 1.8.67 line 1070), refreshed by
    /// `updateChatPermissions` (line 10500). Drives the default chat
    /// permissions editor; `None` until a full block parses.
    pub permissions: Option<ChatPermissions>,
    /// Slice G1: `chat.can_be_deleted_for_all_users` (schema 1.8.67, line
    /// 3616), refreshed by `updateNewChat`. Gates `deleteChat` (schema
    /// line 11850).
    pub can_be_deleted_for_all_users: bool,
    /// Slice CL1: `chat.can_be_deleted_only_for_self` (schema 1.8.67,
    /// line 3604), refreshed by `updateNewChat`. Together with
    /// `can_be_deleted_for_all_users` gates `deleteChatHistory` (schema
    /// line 11845).
    pub can_be_deleted_only_for_self: bool,
    /// Slice CL1: `chat.is_marked_as_unread` (schema 1.8.67, lines
    /// 3600 / 3627), refreshed by `updateNewChat` and
    /// `updateChatIsMarkedAsUnread` (schema line 10588).
    pub is_marked_as_unread: bool,
    /// Slice CL3: `chat.unread_mention_count` (schema 1.8.67, lines
    /// 3611 / 3627), refreshed by `updateChatUnreadMentionCount`
    /// (schema line 10567). Drives the @ mention badge on the row.
    pub unread_mention_count: i32,
    /// Slice CL3: `chat.unread_reaction_count` (schema 1.8.67, lines
    /// 3612 / 3627), refreshed by `updateChatUnreadReactionCount`
    /// (schema line 10570). Drives the ♥ reaction badge on the row.
    pub unread_reaction_count: i32,
    /// Slice CL3: `chat.can_be_reported` (schema 1.8.67, lines 3606 /
    /// 3627). Gates the row-menu Report item (`reportChat`, schema
    /// line 15693).
    pub can_be_reported: bool,
    /// Slice CL3: the peer is on `blockListMain` (`chat.block_list`,
    /// schema 1.8.67 lines 3627 / 9692), refreshed by
    /// `updateChatBlockList` (schema line 10594). Drives the
    /// row-menu Block/Unblock label.
    pub blocked: bool,
    /// Phase B1: latest known `SecretChatState` for `ChatKind::Secret`
    /// chats (from `updateSecretChat` / `getSecretChat`, schema 1.8.67
    /// lines 10741 / 2816). `None` for other chat kinds and until the
    /// state resolves. Only `Ready` chats can send.
    pub secret_state: Option<SecretChatState>,
    /// Phase B4: `chat.message_auto_delete_time` (schema 1.8.67, lines
    /// 3616 / 3627) — the chat-level auto-delete or self-destruct
    /// (secret chats) timer, in seconds; 0 when disabled. Set by
    /// `updateNewChat`, refreshed by `updateChatMessageAutoDeleteTime`.
    pub message_auto_delete_time: i32,
    /// Phase C3a: `chat.video_chat` (`videoChat`, schema 1.8.67, lines
    /// 3576 / 3579 / 3627). `None` when the chat has no active video
    /// chat. Set by `updateNewChat`, refreshed by `updateChatVideoChat`.
    pub video_chat: Option<VideoChatInfo>,
}

/// Phase C3a: a chat's active video chat (`videoChat`, schema 1.8.67,
/// line 3579).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoChatInfo {
    pub group_call_id: i32,
    pub has_participants: bool,
}

impl ChatSummary {
    /// Delivery state of the last message when it is outgoing: read once
    /// the peer's read cursor reaches it.
    pub fn last_message_receipt(&self) -> OutboxReceipt {
        match self.last_message {
            Some(last) if last.is_outgoing => {
                if last.id.0 <= self.last_read_outbox_message_id.0 {
                    OutboxReceipt::Read
                } else {
                    OutboxReceipt::Sent
                }
            }
            _ => OutboxReceipt::None,
        }
    }

    pub fn supported(&self) -> bool {
        self.kind.is_supported_chat()
    }

    /// `chatTypeSupergroup` with `is_channel: true`.
    pub fn is_channel(&self) -> bool {
        self.kind.is_channel()
    }

    /// Whether the composer is shown for this chat. In 2.3 admins get the
    /// composer in broadcast channels (derived from own membership, see
    /// `channel_admin_can_post`); everyone else in a channel keeps it hidden.
    /// Phase B1: secret chats keep the composer only while their state is
    /// `Ready` — a Pending chat is still handshaking and a Closed chat can
    /// never send again. All other supported chats keep the composer.
    pub fn can_post(&self) -> bool {
        if self.is_channel() {
            return self.channel_admin_can_post();
        }
        if matches!(self.kind, ChatKind::Secret { .. }) {
            return self.secret_state == Some(SecretChatState::Ready);
        }
        self.supported()
    }

    /// Phase B1: the secret-chat id behind a secret chat, if any.
    pub fn secret_chat_id(&self) -> Option<i32> {
        match self.kind {
            ChatKind::Secret { secret_chat_id, .. } => Some(secret_chat_id),
            _ => None,
        }
    }

    /// 2.3: channel posting rights derive from own membership. The creator
    /// always posts; an administrator posts unless their
    /// `rights.can_post_messages` is explicitly false. Unknown/absent
    /// membership (or any other status) keeps the composer hidden.
    pub fn channel_admin_can_post(&self) -> bool {
        match self.my_member_status {
            Some(ChannelMemberStatus::Creator) => true,
            Some(ChannelMemberStatus::Administrator) => {
                self.my_admin_can_post_messages.unwrap_or(true)
            }
            _ => false,
        }
    }

    /// Record own channel membership (`getChatMember` / `updateChatMember` /
    /// join/leave responses). `admin_can_post_messages` is the parsed
    /// `rights.can_post_messages` for an administrator, `None` otherwise.
    /// Returns true when the status changed.
    pub fn set_member_status(
        &mut self,
        status: ChannelMemberStatus,
        admin_can_post_messages: Option<bool>,
    ) -> bool {
        let changed = self.my_member_status != Some(status);
        self.my_member_status = Some(status);
        self.my_admin_can_post_messages = admin_can_post_messages;
        changed
    }

    /// Phase D3a: whether the current user may manage this chat's invite
    /// links and join requests. The creator always can; an administrator
    /// needs the explicit `can_invite_users` right. An absent rights block
    /// keeps the gate closed rather than fabricating a right.
    pub fn can_invite_users(&self) -> bool {
        match self.my_member_status {
            Some(ChannelMemberStatus::Creator) => true,
            Some(ChannelMemberStatus::Administrator) => {
                self.my_admin_can_invite_users.unwrap_or(false)
            }
            _ => false,
        }
    }

    /// Phase D3a: record `rights.can_invite_users` (`None` for non-admin
    /// statuses or an absent rights block).
    pub fn set_admin_can_invite_users(&mut self, can_invite_users: Option<bool>) {
        self.my_admin_can_invite_users = can_invite_users;
    }

    /// Slice G2: record `rights.can_change_info` (`None` for non-admin
    /// statuses or an absent rights block).
    pub fn set_admin_can_change_info(&mut self, can_change_info: Option<bool>) {
        self.my_admin_can_change_info = can_change_info;
    }

    /// Slice G2: whether the current user may change this chat's info.
    /// The creator always can; an administrator needs the explicit
    /// `can_change_info` right.
    pub fn can_change_info(&self) -> bool {
        match self.my_member_status {
            Some(ChannelMemberStatus::Creator) => true,
            Some(ChannelMemberStatus::Administrator) => {
                self.my_admin_can_change_info.unwrap_or(false)
            }
            _ => false,
        }
    }

    /// Slice G2: record `rights.can_send_welcome_messages` (`None` for
    /// non-admin statuses or an absent rights block).
    pub fn set_admin_can_send_welcome_messages(&mut self, can_send: Option<bool>) {
        self.my_admin_can_send_welcome_messages = can_send;
    }

    /// Slice G2: whether the current user may manage this chat's welcome
    /// messages. The creator always can; an administrator needs the
    /// explicit `can_send_welcome_messages` right.
    pub fn can_send_welcome_messages(&self) -> bool {
        match self.my_member_status {
            Some(ChannelMemberStatus::Creator) => true,
            Some(ChannelMemberStatus::Administrator) => {
                self.my_admin_can_send_welcome_messages.unwrap_or(false)
            }
            _ => false,
        }
    }

    /// Phase D3b: whether the current user may manage this chat's
    /// administrators (view the admin list, promote/demote members, edit
    /// admin rights). The creator always can; an administrator needs the
    /// explicit `can_promote_members` right. An absent rights block keeps
    /// the gate closed rather than fabricating a right.
    pub fn can_manage_admins(&self) -> bool {
        match self.my_member_status {
            Some(ChannelMemberStatus::Creator) => true,
            Some(ChannelMemberStatus::Administrator) => {
                self.my_admin_can_promote_members.unwrap_or(false)
            }
            _ => false,
        }
    }

    /// Phase D3c: whether the current user is the creator or an
    /// administrator of this chat (own membership, probed via
    /// `getChatMember`). Unlike `can_manage_admins` (D3b, which needs the
    /// explicit `can_promote_members` right) and `can_invite_users`
    /// (D3a, `can_invite_users`), the event log requires only
    /// administrator rights (schema 1.8.67, line 15252), so any
    /// administrator qualifies. Unknown status keeps the gate closed.
    pub fn is_admin_or_creator(&self) -> bool {
        matches!(
            self.my_member_status,
            Some(ChannelMemberStatus::Creator | ChannelMemberStatus::Administrator)
        )
    }

    /// Phase D3b: record `rights.can_promote_members` (`None` for
    /// non-admin statuses or an absent rights block).
    pub fn set_admin_can_promote_members(&mut self, can_promote_members: Option<bool>) {
        self.my_admin_can_promote_members = can_promote_members;
    }

    /// Slice G1: whether the current user may restrict or ban this chat's
    /// members (`setChatMemberStatus` with a restricted or banned status
    /// requires the `can_restrict_members` administrator right, schema
    /// 1.8.67 lines 13586-13587). The creator always can; an administrator
    /// needs the explicit right. Unknown status or an absent rights block
    /// keeps the gate closed.
    pub fn can_restrict_members(&self) -> bool {
        match self.my_member_status {
            Some(ChannelMemberStatus::Creator) => true,
            Some(ChannelMemberStatus::Administrator) => {
                self.my_admin_can_restrict_members.unwrap_or(false)
            }
            _ => false,
        }
    }

    /// Slice G1: record `rights.can_restrict_members` (`None` for
    /// non-admin statuses or an absent rights block).
    pub fn set_admin_can_restrict_members(&mut self, can_restrict_members: Option<bool>) {
        self.my_admin_can_restrict_members = can_restrict_members;
    }

    /// MED1: whether the current user may pin messages in this chat
    /// (`pinChatMessage`, schema 1.8.67 line 13559). Private and secret
    /// chats allow own-side pins; the creator always can; an
    /// administrator needs the explicit `can_pin_messages` right
    /// (`chatAdministratorRights`, schema line 1092); basic-group and
    /// supergroup members need the `can_pin_messages` member right
    /// (`chatPermissions`, schema line 1070). Unknown status keeps the
    /// gate closed rather than fabricating a right.
    pub fn can_pin_messages(&self) -> bool {
        use crate::telegram::envelope::ChatKind;
        match &self.kind {
            ChatKind::Private { .. } | ChatKind::Secret { .. } => true,
            _ => match self.my_member_status {
                Some(ChannelMemberStatus::Creator) => true,
                Some(ChannelMemberStatus::Administrator) => {
                    self.my_admin_can_pin_messages.unwrap_or(false)
                }
                _ => self
                    .permissions
                    .as_ref()
                    .is_some_and(|p| p.can_pin_messages),
            },
        }
    }

    /// MED1: record `rights.can_pin_messages` (`None` for non-admin
    /// statuses or an absent rights block).
    pub fn set_admin_can_pin_messages(&mut self, can_pin_messages: Option<bool>) {
        self.my_admin_can_pin_messages = can_pin_messages;
    }

    pub fn is_forum_chat(&self) -> bool {
        self.is_forum == Some(true)
    }

    pub fn is_muted(&self) -> bool {
        self.notification_settings.is_muted()
    }

    /// Slice CL2: the TGX unread-filter predicate
    /// (`ChatFilter.unreadFilter.accept`: `unreadCount > 0 ||
    /// isMarkedAsUnread`) — drives the Unread category chip.
    pub fn is_unread(&self) -> bool {
        self.unread_count > 0 || self.is_marked_as_unread
    }

    /// Phase B4: the chat-level timer status line — "Self-destruct: 1h"
    /// for secret chats, "Auto-delete: 7d" for other chats
    /// (`chat.message_auto_delete_time`, schema 1.8.67 lines 3616 /
    /// 3627). `None` when the timer is disabled.
    pub fn ttl_status_line(&self) -> Option<String> {
        if self.message_auto_delete_time <= 0 {
            return None;
        }
        let label = crate::telegram::envelope::format_ttl_setting(self.message_auto_delete_time);
        let noun = if matches!(self.kind, ChatKind::Secret { .. }) {
            "Self-destruct"
        } else {
            "Auto-delete"
        };
        Some(format!("{noun}: {label}"))
    }

    pub fn is_peer_typing(&self) -> bool {
        !self.typing_senders.is_empty()
    }

    pub fn set_sender_action(&mut self, sender: MessageSender, action: ChatAction) {
        self.typing_senders.retain(|existing| *existing != sender);
        self.choosing_sticker_senders
            .retain(|existing| *existing != sender);
        match action {
            ChatAction::Typing => self.typing_senders.push(sender),
            ChatAction::ChoosingSticker => self.choosing_sticker_senders.push(sender),
            ChatAction::Cancel | ChatAction::Other => {}
        }
    }

    /// Slice S17: the peer-activity label for the header and sidebar —
    /// "choosing a sticker…" wins over "typing…" while a peer is picking a
    /// sticker (`chatActionChoosingSticker`, schema 1.8.67 line 6380).
    pub fn peer_activity_label(&self) -> Option<&'static str> {
        if !self.choosing_sticker_senders.is_empty() {
            Some("choosing a sticker…")
        } else if self.is_peer_typing() {
            Some("typing…")
        } else {
            None
        }
    }

    pub fn sidebar_preview(&self) -> String {
        if let Some(reason) = self.kind.gate_reason() {
            return reason.to_string();
        }
        if let Some(label) = self.peer_activity_label() {
            return label.into();
        }
        if let Some(draft) = &self.draft {
            let text = draft.text.replace('\n', " ");
            let text = text.trim();
            if !text.is_empty() {
                return format!("Draft: {text}");
            }
            if draft.reply_to_message_id.is_some() {
                return "Draft:".into();
            }
        }
        if !self.last_preview.is_empty() {
            return self.last_preview.clone();
        }
        if self.unread_count > 0 {
            return format!("{} unread", self.unread_count);
        }
        if matches!(self.kind, ChatKind::Secret { .. }) {
            return "Secret chat".into();
        }
        "No messages yet".into()
    }

    pub fn outbox_receipt(&self, message: &HistoryMessage) -> OutboxReceipt {
        if !message.is_outgoing || message.pending {
            return OutboxReceipt::None;
        }
        if self.last_read_outbox_message_id.0 > 0
            && message.id.0 <= self.last_read_outbox_message_id.0
        {
            OutboxReceipt::Read
        } else {
            OutboxReceipt::Sent
        }
    }
}
