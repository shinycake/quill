//! Message-history state types.
use super::*;

/// Parity slice: map a chat to its `NotificationSettingsScope` for
/// `use_default_*` fallback (`notificationSettingsScope*`, td_api.tl lines
/// 3337–3343). Secret chats share the private-chat scope; unknown kinds
/// fall back to groups.
pub fn scope_for_chat_kind(kind: &ChatKind) -> NotificationSettingsScope {
    match kind {
        ChatKind::Private { .. } | ChatKind::Secret { .. } => {
            NotificationSettingsScope::PrivateChats
        }
        ChatKind::Supergroup {
            is_channel: true, ..
        } => NotificationSettingsScope::ChannelChats,
        _ => NotificationSettingsScope::GroupChats,
    }
}

impl Session {
    /// Parity slice: chats with any non-default notification setting for
    /// the scope — the `getChatNotificationSettingsExceptions`
    /// `compare_sound=false` semantics, computed locally. The screenshot
    /// demo (no driver) answers the request from this; live sessions get
    /// the authoritative server list.
    pub fn local_notification_exceptions(&self, scope: NotificationSettingsScope) -> Vec<i64> {
        let mut ids: Vec<i64> = self
            .chats
            .values()
            .filter(|c| {
                scope_for_chat_kind(&c.kind) == scope
                    && c.notification_settings != ChatNotificationSettings::default()
            })
            .map(|c| c.id.0)
            .collect();
        ids.sort_unstable();
        ids
    }
}

pub(crate) fn placeholder_chat(chat_id: ChatId) -> ChatSummary {
    ChatSummary {
        id: chat_id,
        title: format!("chat {}", chat_id.0),
        kind: ChatKind::Unknown,
        unread_count: 0,
        last_read_inbox_message_id: MessageId(0),
        last_read_outbox_message_id: MessageId(0),
        order: 0,
        is_pinned: false,
        in_main_list: false,
        in_archive: false,
        archive_order: 0,
        archive_is_pinned: false,
        folder_positions: BTreeMap::new(),
        notification_settings: ChatNotificationSettings::default(),
        last_preview: String::new(),
        last_preview_style: ChatPreviewStyle::default(),
        last_preview_sender: String::new(),
        typing_senders: Vec::new(),
        choosing_sticker_senders: Vec::new(),
        draft: None,
        my_member_status: None,
        my_admin_can_post_messages: None,
        my_admin_can_invite_users: None,
        my_admin_can_change_info: None,
        my_admin_can_send_welcome_messages: None,
        my_admin_can_promote_members: None,
        my_admin_can_restrict_members: None,
        my_admin_can_pin_messages: None,
        is_forum: None,
        photo_file_id: None,
        // Parity slice 4: lenient default true — the real `chat` object
        // always carries `permissions`; only `updateNewChat` /
        // `updateChatPermissions` ever set it to false.
        can_send_basic_messages: true,
        permissions: None,
        can_be_deleted_for_all_users: false,
        can_be_deleted_only_for_self: false,
        is_marked_as_unread: false,
        unread_mention_count: 0,
        unread_reaction_count: 0,
        can_be_reported: false,
        blocked: false,
        // Phase B1: unknown until `updateSecretChat` / `getSecretChat`
        // resolves it.
        secret_state: None,
        // Phase B4: 0 = disabled (`chat.message_auto_delete_time`,
        // schema 1.8.67, lines 3616 / 3627).
        message_auto_delete_time: 0,
        // Phase C3a: unknown until `updateNewChat` / `updateChatVideoChat`
        // resolves it.
        video_chat: None,
    }
}

/// M2 fix-up: the preview text of a history row — the content it actually
/// shows, so `ephemeral_content` wins over the regular content (schema
/// 1.8.67, line 3161: "must be shown instead of the regular content").
pub fn effective_preview(message: &HistoryMessage) -> String {
    effective_content(&message.content, message.ephemeral.as_ref()).preview()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryMessage {
    pub id: MessageId,
    pub chat_id: ChatId,
    pub is_outgoing: bool,
    /// Schema `message.date` (unix seconds, server time) — feeds the
    /// in-bubble timestamp. `0` when the source didn't carry one.
    pub date: i32,
    pub content: MessageContent,
    pub pending: bool,
    pub reply_to: Option<MessageReplyTo>,
    pub forward_info: Option<MessageForwardInfo>,
    pub interaction_info: Option<MessageInteractionInfo>,
    /// Schema `message.is_pinned` / `updateMessageIsPinned`.
    pub is_pinned: bool,
    /// Schema `message.media_album_id`. `0` is not an album.
    pub media_album_id: i64,
    /// Schema `message.reply_markup` — all `replyMarkup*` constructors
    /// (B1). Rendered as the button grid under the message (inline) or the
    /// custom keyboard above the composer (showKeyboard).
    pub reply_markup: Option<ReplyMarkup>,
    /// Phase B3: schema `message.self_destruct_type` /
    /// Phase B3: schema `message.self_destruct_type` /
    /// `message.self_destruct_in` (TDLib 1.8.67 lines 3146–3147 / 3165).
    /// `None` for ordinary messages. Self-destructed rows leave via
    /// `updateDeleteMessages` (the normal delete path).
    pub self_destruct: Option<MessageSelfDestruct>,
    /// Phase B4: schema `message.auto_delete_in` (TDLib 1.8.67 lines
    /// 3148 / 3165) — locally decayed countdown until the chat's
    /// `message_auto_delete_time` setting deletes this message; `None`
    /// when never. Renders as a countdown chip on the row; the row
    /// itself leaves via `updateDeleteMessages`.
    pub auto_delete: Option<MessageAutoDelete>,
    /// Phase D2: schema `message.author_signature` (TDLib 1.8.67, lines
    /// 3155/3165) — author signature on channel posts and anonymous group
    /// messages. Rendered as a small signature line under the post, except
    /// under forwarded-message headers (which already attribute it).
    pub author_signature: Option<String>,
    /// M1: the send failed (`updateMessageSendFailed`). `true` means the
    /// row renders a failed state. A "Retry send" affordance is offered
    /// only when `can_retry` is also true — TDLib does not allow every
    /// failed send to be retried.
    pub failed: bool,
    /// M1 fix-up: `message.sending_state.can_retry` (TDLib 1.8.67 line
    /// 3038) — whether the failed send may be retried via
    /// `resendMessages`. Gates the retry affordance and the driver's
    /// `resend_failed_message`.
    pub can_retry: bool,
    /// M2: parsed `message.ephemeral_content` (TDLib 1.8.67 lines
    /// 3161/3165). When present the row renders it instead of `content`
    /// (use `effective_content`); it carries its own `reply_markup`.
    pub ephemeral: Option<EphemeralMessageContent>,
}

impl HistoryMessage {
    pub fn can_react(&self) -> bool {
        !self.pending && self.id.0 > 0
    }

    /// Already-sent messages can be pinned/unpinned (live `messageProperties.can_be_pinned`
    /// stays out — same default as edit/forward/react).
    pub fn can_pin(&self) -> bool {
        !self.pending && self.id.0 > 0
    }

    pub fn emoji_reaction_chips(&self) -> Vec<&MessageReaction> {
        self.interaction_info
            .as_ref()
            .map(MessageInteractionInfo::emoji_chips)
            .unwrap_or_default()
    }

    pub fn chosen_emoji(&self, emoji: &str) -> bool {
        self.interaction_info
            .as_ref()
            .is_some_and(|info| info.chosen_emoji(emoji))
    }

    /// Phase B3: timer badge for self-destructing media rows (`None` for
    /// ordinary messages). The `self_destruct_in` countdown decays locally
    /// against `now_ms` (`slow_mode_delay_expires_in` pattern, Phase A1);
    /// TDLib removes the row via `updateDeleteMessages` when it fires.
    pub fn self_destruct_badge(&self, now_ms: u64) -> Option<String> {
        self.self_destruct.as_ref().map(|sd| sd.badge_label(now_ms))
    }

    /// Phase B3: whether the row still has a live (not yet expired)
    /// `self_destruct_in` countdown — drives the 1-second render tick.
    pub fn has_live_self_destruct(&self, now_ms: u64) -> bool {
        self.self_destruct
            .as_ref()
            .and_then(|sd| sd.remaining_secs(now_ms))
            .is_some_and(|left| left > 0)
    }

    /// Phase B4: countdown chip label for `message.auto_delete_in`
    /// (schema 1.8.67 line 3148) — "🗑 59m left".
    pub fn auto_delete_chip(&self, now_ms: u64) -> Option<String> {
        self.auto_delete.as_ref().map(|ad| ad.chip_label(now_ms))
    }

    /// Phase B4: whether the row has a live auto-delete countdown —
    /// joins the same 1-second render tick as the self-destruct badge.
    pub fn has_live_auto_delete(&self, now_ms: u64) -> bool {
        self.auto_delete
            .as_ref()
            .is_some_and(|ad| ad.remaining_secs(now_ms) > 0)
    }
}

#[derive(Debug, Default)]
pub struct HistoryState {
    pub messages: BTreeMap<i64, HistoryMessage>,
    pub tombstones: HashSet<i64>,
    pub loaded_complete: bool,
    pub view_generation: ViewGeneration,
    /// Message ids TDLib has accepted for `viewMessages` this open generation.
    pub viewed: HashSet<i64>,
    /// In-flight `viewMessages` ids. Cleared on send failure or TDLib error so we can retry.
    pub viewing: HashSet<i64>,
}

impl HistoryState {
    pub fn ordered(&self) -> Vec<&HistoryMessage> {
        self.messages.values().collect()
    }

    pub fn oldest_id(&self) -> Option<MessageId> {
        self.messages.keys().next().copied().map(MessageId)
    }

    pub(crate) fn upsert(&mut self, message: HistoryMessage) {
        if self.tombstones.contains(&message.id.0) {
            return;
        }
        self.messages.insert(message.id.0, message);
    }

    pub(crate) fn remove(&mut self, id: MessageId, permanent: bool) {
        self.messages.remove(&id.0);
        if permanent {
            self.tombstones.insert(id.0);
        }
    }

    pub(crate) fn replace_id(&mut self, old: MessageId, new_message: HistoryMessage) {
        self.messages.remove(&old.0);
        self.upsert(new_message);
    }

    pub fn contains(&self, id: MessageId) -> bool {
        self.messages.contains_key(&id.0)
    }

    pub fn is_tombstone(&self, id: MessageId) -> bool {
        self.tombstones.contains(&id.0)
    }

    pub(crate) fn update_content(&mut self, id: MessageId, content: MessageContent) -> bool {
        if let Some(message) = self.messages.get_mut(&id.0) {
            message.content = content;
            true
        } else {
            false
        }
    }

    /// `parity:msg-ephemeral-updates`: `updateMessageEphemeralContent`
    /// refreshes the ephemeral content in place (schema 1.8.67 line 10424,
    /// secret-chat lane). `None` clears the stored ephemeral content
    /// (schema-legal explicit null).
    pub(crate) fn update_ephemeral(
        &mut self,
        id: MessageId,
        ephemeral: Option<EphemeralMessageContent>,
    ) -> bool {
        if let Some(message) = self.messages.get_mut(&id.0) {
            message.ephemeral = ephemeral;
            true
        } else {
            false
        }
    }

    /// Phase 3.2: `updateMessageEdited` replaces the message's inline
    /// keyboard (or removes it when `None`).
    pub(crate) fn update_reply_markup(
        &mut self,
        id: MessageId,
        reply_markup: Option<ReplyMarkup>,
    ) -> bool {
        if let Some(message) = self.messages.get_mut(&id.0) {
            message.reply_markup = reply_markup;
            true
        } else {
            false
        }
    }

    pub(crate) fn update_interaction_info(
        &mut self,
        id: MessageId,
        interaction_info: Option<MessageInteractionInfo>,
    ) -> bool {
        if let Some(message) = self.messages.get_mut(&id.0) {
            message.interaction_info = interaction_info;
            true
        } else {
            false
        }
    }

    pub(crate) fn update_is_pinned(&mut self, id: MessageId, is_pinned: bool) -> bool {
        if let Some(message) = self.messages.get_mut(&id.0) {
            message.is_pinned = is_pinned;
            true
        } else {
            false
        }
    }

    pub(crate) fn mark_content_opened(&mut self, id: MessageId) {
        if let Some(message) = self.messages.get_mut(&id.0) {
            message.content.mark_content_opened();
        }
    }

    /// Newest pinned message in loaded history (`getChatPinnedMessage` is newest).
    pub fn newest_pinned(&self) -> Option<&HistoryMessage> {
        self.messages
            .values()
            .rev()
            .find(|message| message.is_pinned)
    }
}
