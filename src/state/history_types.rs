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
    /// `compare_sound=true` semantics (whole-struct compare, sound-only
    /// chats included), computed locally. The screenshot
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
        last_preview_thumb: None,
        last_message: None,
        sender_actions: Vec::new(),
        draft: None,
        my_member_status: None,
        my_admin_can_post_messages: None,
        my_admin_can_invite_users: None,
        my_admin_can_change_info: None,
        my_admin_can_send_welcome_messages: None,
        my_admin_can_promote_members: None,
        my_admin_can_restrict_members: None,
        my_admin_can_pin_messages: None,
        my_restriction: None,
        my_rights_fetched: false,
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
        unread_poll_vote_count: 0,
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
    pub sender: Option<MessageSender>,
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
    /// `edit_date`, `via_bot_user_id` and `import_info`.
    pub extras: MessageExtras,
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

    /// Emoji and custom-emoji reaction chips (Telegram Desktop order).
    pub fn reaction_chips(&self) -> Vec<&MessageReaction> {
        self.interaction_info
            .as_ref()
            .map(MessageInteractionInfo::display_chips)
            .unwrap_or_default()
    }

    /// The Star reaction (`reactionTypePaid`) with its count, if the
    /// message has Stars.
    pub fn paid_reaction(&self) -> Option<&MessageReaction> {
        self.interaction_info
            .as_ref()?
            .reactions
            .as_ref()?
            .reactions
            .iter()
            .find(|reaction| {
                reaction.reaction_type == crate::telegram::envelope::ReactionType::Paid
            })
    }

    /// Top senders of Stars on this message, most Stars first.
    pub fn paid_reactors(&self) -> &[crate::paid_reaction::PaidReactor] {
        self.interaction_info
            .as_ref()
            .and_then(|info| info.reactions.as_ref())
            .map_or(&[], |reactions| reactions.paid_reactors.as_slice())
    }

    /// Whether the current user chose `choice` on this message.
    pub fn chosen_reaction(&self, choice: &crate::state::ReactionChoice) -> bool {
        let Some(reactions) = self
            .interaction_info
            .as_ref()
            .and_then(|info| info.reactions.as_ref())
        else {
            return false;
        };
        match choice {
            crate::state::ReactionChoice::Emoji(emoji) => reactions.chosen_emoji(emoji),
            crate::state::ReactionChoice::CustomEmoji(id) => reactions.chosen_custom_emoji(*id),
        }
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

/// The next value for a revision counter. Values are unique across the
/// process, so a history that replaced another never repeats
/// a revision a view cached for the old one. The counter is a `u64`: at a
/// billion changes a second it would take centuries to wrap, so wraparound
/// is not handled.
pub fn next_revision() -> u64 {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

/// A chat's loaded messages keyed by id. Reads go through `Deref`; every
/// mutation goes through these methods, so views can cache what they
/// derived from the messages and know exactly what to derive again:
/// `revision` moves on any change, `ids_revision` when a message comes or
/// goes, and `changed_since` names the messages changed in place.
#[derive(Debug)]
pub struct HistoryMessages {
    /// Unique per instance (a `next_revision` value). Revisions and the
    /// removal log mean something only within one instance, so a view that
    /// cached them for another one (the history was replaced wholesale)
    /// must build again.
    instance: u64,
    map: BTreeMap<i64, HistoryMessage>,
    /// The `revision` at which each message was last stored or changed.
    changed_at: HashMap<i64, u64>,
    /// The latest removals, oldest first, with the `revision` each went at
    /// (at most `REMOVED_LOG`).
    removed: std::collections::VecDeque<(i64, u64)>,
    /// `removed` holds every removal after this revision.
    removed_floor: u64,
    revision: u64,
    ids_revision: u64,
}

impl Default for HistoryMessages {
    fn default() -> Self {
        Self {
            instance: next_revision(),
            map: BTreeMap::new(),
            changed_at: HashMap::new(),
            removed: std::collections::VecDeque::new(),
            removed_floor: 0,
            revision: 0,
            ids_revision: 0,
        }
    }
}

/// Removals `HistoryMessages::changes_since` can still name; a view asking
/// from before the oldest of them builds its rows again.
const REMOVED_LOG: usize = 64;

/// What changed in a [`HistoryMessages`] after a revision.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct HistoryChanges {
    /// Messages stored or changed, oldest id first.
    pub changed: Vec<i64>,
    /// Messages removed, oldest id first. One removed and stored again is
    /// in both lists.
    pub removed: Vec<i64>,
}

impl HistoryMessages {
    /// Which instance the revisions belong to.
    pub fn instance(&self) -> u64 {
        self.instance
    }

    /// Moves on every change (a `next_revision` value).
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Moves when a message is added or removed, not when one changes in
    /// place.
    pub fn ids_revision(&self) -> u64 {
        self.ids_revision
    }

    /// Messages stored or changed after `revision` (an earlier value of
    /// [`Self::revision`]), oldest id first.
    pub fn changed_since(&self, revision: u64) -> Vec<i64> {
        let mut ids: Vec<i64> = self
            .changed_at
            .iter()
            .filter(|(_, at)| **at > revision)
            .map(|(id, _)| *id)
            .collect();
        ids.sort_unstable();
        ids
    }

    /// Messages stored, changed or removed after `revision`, a value of
    /// [`Self::revision`] read from the instance `instance`; `None` when
    /// that was another instance, or removals that far back are no longer
    /// listed.
    pub fn changes_since(&self, instance: u64, revision: u64) -> Option<HistoryChanges> {
        if instance != self.instance || revision < self.removed_floor {
            return None;
        }
        let mut removed: Vec<i64> = self
            .removed
            .iter()
            .filter(|(_, at)| *at > revision)
            .map(|(id, _)| *id)
            .collect();
        removed.sort_unstable();
        removed.dedup();
        Some(HistoryChanges {
            changed: self.changed_since(revision),
            removed,
        })
    }

    fn touch(&mut self, id: i64) {
        self.revision = next_revision();
        self.changed_at.insert(id, self.revision);
    }

    fn ids_changed(&mut self) {
        self.revision = next_revision();
        self.ids_revision = self.revision;
    }

    pub fn insert(&mut self, id: i64, message: HistoryMessage) -> Option<HistoryMessage> {
        let before = self.map.insert(id, message);
        if before.is_none() {
            self.ids_changed();
        }
        self.touch(id);
        before
    }

    pub fn remove(&mut self, id: &i64) -> Option<HistoryMessage> {
        let removed = self.map.remove(id)?;
        self.changed_at.remove(id);
        self.ids_changed();
        self.removed.push_back((*id, self.revision));
        while self.removed.len() > REMOVED_LOG {
            if let Some((_, at)) = self.removed.pop_front() {
                self.removed_floor = at;
            }
        }
        Some(removed)
    }

    /// The message to change in place; marks it changed when it exists.
    pub fn get_mut(&mut self, id: &i64) -> Option<&mut HistoryMessage> {
        if self.map.contains_key(id) {
            self.touch(*id);
        }
        self.map.get_mut(id)
    }

    /// Every message, each marked changed.
    pub fn values_mut(
        &mut self,
    ) -> std::collections::btree_map::ValuesMut<'_, i64, HistoryMessage> {
        self.touch_all();
        self.map.values_mut()
    }

    /// Every message with its id, each marked changed.
    pub fn iter_mut(&mut self) -> std::collections::btree_map::IterMut<'_, i64, HistoryMessage> {
        self.touch_all();
        self.map.iter_mut()
    }

    fn touch_all(&mut self) {
        self.revision = next_revision();
        let revision = self.revision;
        for id in self.map.keys() {
            self.changed_at.insert(*id, revision);
        }
    }

    /// Keep the messages `keep` accepts.
    pub fn retain(&mut self, mut keep: impl FnMut(&i64, &HistoryMessage) -> bool) {
        let doomed: Vec<i64> = self
            .map
            .iter()
            .filter(|(id, message)| !keep(id, message))
            .map(|(id, _)| *id)
            .collect();
        for id in &doomed {
            self.remove(id);
        }
    }

    pub fn clear(&mut self) {
        if !self.map.is_empty() {
            self.map.clear();
            self.changed_at.clear();
            self.ids_changed();
            self.removed.clear();
            self.removed_floor = self.revision;
        }
    }
}

impl std::ops::Deref for HistoryMessages {
    type Target = BTreeMap<i64, HistoryMessage>;

    fn deref(&self) -> &Self::Target {
        &self.map
    }
}

#[derive(Debug, Default)]
pub struct HistoryState {
    pub messages: HistoryMessages,
    pub tombstones: HashSet<i64>,
    pub loaded_complete: bool,
    pub view_generation: ViewGeneration,
    /// Message ids TDLib has accepted for `viewMessages` this open generation.
    pub viewed: HashSet<i64>,
    /// In-flight `viewMessages` ids. Cleared on send failure or TDLib error so we can retry.
    pub viewing: HashSet<i64>,
    /// Ids the UI reported as shown (`ConnectDriver::view_messages`) that
    /// have not been sent to `viewMessages` yet.
    pub visible: BTreeSet<i64>,
    /// The UI has reported visible rows this open generation; from then on
    /// only reported rows are viewed (before that, only the newest row).
    pub visible_reported: bool,
    /// TDLib answered a newer page from the window's newest message with
    /// nothing newer while the chat's latest known message was newer (it
    /// is not visible to us): `(latest, newest)`. While no newer message is
    /// known and the window still reaches `newest`, it counts as reaching
    /// the latest, so the pane does not ask for the same page again.
    pub newer_exhausted: Option<(i64, i64)>,
    /// A newer-page request failed; stop auto-loading until the window is
    /// reset, so a persistent error can't re-send on every render.
    pub newer_failed: bool,
    /// The window's page load failed or timed out (error answer or the
    /// pending-request sweep) while the window is empty: the UI shows
    /// "Couldn't load messages · Retry" instead of the skeleton. Cleared by
    /// `ConnectDriver::retry_history`, a window reset, or any message
    /// landing in the window.
    pub load_failed: bool,
    /// The chat's last read incoming message when it was opened with unread
    /// messages: the window loads around it and the UI draws the "Unread
    /// messages" divider after it. Cleared by jumping to the latest.
    pub unread_anchor: Option<MessageId>,
    /// The chat was opened with unread messages and the UI has not shown
    /// anything yet. Separate from `unread_anchor` (the bar), like
    /// tdesktop's `_firstUnreadView` versus `_unreadBarView`: deleting the
    /// bar's message drops the bar but the chat is still not read unseen.
    /// Cleared with the anchor by jumping to the latest or sending.
    pub opened_with_unread: bool,
    /// Bumped whenever the window is replaced, so the UI re-anchors its
    /// scroll position (unread divider, jump target, or bottom).
    pub window_epoch: u64,
    /// The chat was opened at its saved scroll position and the window
    /// around that message is still loading: the UI holds the first
    /// anchoring until it lands, so the position is restored with the
    /// first rows. Cleared by the answer, its failure, or a window reset.
    pub saved_open: Option<MessageId>,
    /// The saved message was not in the window loaded around it (deleted)
    /// or that load failed: the window was emptied and the driver loads
    /// the default one (`Session::take_saved_open_fallback`).
    pub saved_open_missed: bool,
    /// Newest server message id ever seen for this chat (survives window
    /// resets): a lower bound for the chat's latest message when the chat
    /// summary doesn't carry one.
    pub latest_seen: i64,
    /// The message at the top of the screen while the reader is away from
    /// the tail; `None` at the tail. Reported by the view
    /// (`Session::note_history_reader`) and kept while its frames stop, so
    /// a hidden window stays where it was left.
    pub reader_top: Option<i64>,
}

impl HistoryState {
    pub fn ordered(&self) -> Vec<&HistoryMessage> {
        self.messages.values().collect()
    }

    /// The loaded window is one contiguous run of the chat's history; it
    /// stops short of the chat's latest message (`chat_latest`, the chat
    /// summary's last message id, or 0) when it was opened at the first
    /// unread message, jumped back, trimmed, or the chat moved on without
    /// an `updateNewMessage` for every message (TDLib after a long gap).
    /// Newer pages then load on demand and live messages stay out of the
    /// window: inserting them would show a gapless timeline that is
    /// missing messages. Derived from what the window holds and what the
    /// chat is known to have, never stored, so no update can leave it stale.
    pub fn stops_short_of(&self, chat_latest: i64) -> bool {
        let latest = chat_latest.max(self.latest_seen);
        let Some(newest) = self.newest_id().map(|id| id.0) else {
            return false;
        };
        let exhausted = self
            .newer_exhausted
            .is_some_and(|(at_latest, at_newest)| latest <= at_latest && newest >= at_newest);
        !exhausted && newest < latest && !self.contains(MessageId(latest))
    }

    /// Messages were deleted from the chat: when the newest incoming one
    /// ever seen is among them, `latest_seen` falls back to the newest
    /// incoming message still loaded, so a deleted message is not taken
    /// for one the window still has to reach (the chat summary carries the
    /// real latest).
    pub(crate) fn forget_deleted_latest(&mut self, deleted: &[MessageId]) {
        if !deleted.iter().any(|id| id.0 == self.latest_seen) {
            return;
        }
        self.latest_seen = self
            .messages
            .values()
            .rev()
            .find(|message| {
                !message.is_outgoing
                    && !message.pending
                    && !message.failed
                    && !deleted.contains(&message.id)
            })
            .map_or(0, |message| message.id.0);
    }

    pub fn oldest_id(&self) -> Option<MessageId> {
        self.messages.keys().next().copied().map(MessageId)
    }

    /// Newest server message in the window (pending sends excluded: their
    /// temporary ids are not positions in the server history).
    pub fn newest_id(&self) -> Option<MessageId> {
        self.messages
            .values()
            .rev()
            .find(|message| !message.pending && !message.failed)
            .map(|message| message.id)
    }

    /// Drop the loaded window so a new one can load somewhere else in the
    /// chat. Tombstones and `viewMessages` bookkeeping stay.
    pub(crate) fn reset_window(&mut self) {
        self.messages.clear();
        self.loaded_complete = false;
        self.newer_failed = false;
        self.load_failed = false;
        self.saved_open = None;
        self.reader_top = None;
        self.window_epoch = self.window_epoch.wrapping_add(1);
    }

    pub(crate) fn upsert(&mut self, message: HistoryMessage) {
        if self.tombstones.contains(&message.id.0) {
            return;
        }
        // Incoming messages always carry server ids; an outgoing row may
        // still hold a temporary id that never becomes a server position
        // (failed sends), and the chat summary covers outgoing tails.
        if !message.is_outgoing && !message.pending && !message.failed {
            self.latest_seen = self.latest_seen.max(message.id.0);
        }
        self.messages.insert(message.id.0, message);
        self.load_failed = false;
    }

    /// The message the "Unread messages" bar sits above: the first
    /// incoming one after `unread_anchor` (the rule the rows are built by).
    pub fn unread_bar_message(&self) -> Option<MessageId> {
        let anchor = self.unread_anchor?;
        self.messages
            .values()
            .find(|message| !message.is_outgoing && message.id.0 > anchor.0)
            .map(|message| message.id)
    }

    /// Drop the bar when it sits above one of `doomed` (about to be removed
    /// by delete, window trim or prune). Call before removing them.
    pub(crate) fn drop_unread_bar_for(&mut self, doomed: &[i64]) {
        if self.unread_anchor.is_none() || doomed.is_empty() {
            return;
        }
        if self
            .unread_bar_message()
            .is_some_and(|bar| doomed.contains(&bar.0))
        {
            self.unread_anchor = None;
        }
    }

    /// End the opened-with-unread state: the bar goes and reads are no
    /// longer held back.
    pub fn end_unread_open(&mut self) {
        self.unread_anchor = None;
        self.opened_with_unread = false;
    }

    /// Remove a message. The bar belongs to its message (tdesktop
    /// `History::mainViewRemoved` drops `_unreadBarView`), so deleting the
    /// message it sits above removes the bar instead of sliding it down.
    pub(crate) fn remove(&mut self, id: MessageId, permanent: bool) {
        self.drop_unread_bar_for(&[id.0]);
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

    /// Newest pinned message in loaded history (`getChatPinnedMessage` is newest).
    pub fn newest_pinned(&self) -> Option<&HistoryMessage> {
        self.messages
            .values()
            .rev()
            .find(|message| message.is_pinned)
    }
}
