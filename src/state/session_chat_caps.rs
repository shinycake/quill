//! Chat capability queries: what the user may do in a chat.
use super::*;
use crate::telegram::envelope::MessageSchedulingState;

impl Session {
    /// Private chats only, and not a known bot. Channels, groups, secret chats skip drafts.
    pub fn accepts_composer_draft(&self, chat_id: ChatId) -> bool {
        let Some(chat) = self.chats.get(&chat_id.0) else {
            return false;
        };
        match chat.kind {
            ChatKind::Private { user_id } => !self.bot_user_ids.contains(&user_id.0),
            _ => false,
        }
    }

    /// Phase 3.1: bot chats ride the ordinary private-chat path
    /// (`is_supported_chat` / `can_post`) — no special gate. This
    /// resolves the peer bot user id for a private chat whose user is a
    /// known `userTypeBot`, feeding the lazy `getUserFullInfo` fetch.
    /// `None` for every other chat kind.
    pub fn bot_user_id_for_chat(&self, chat_id: ChatId) -> Option<i64> {
        let chat = self.chats.get(&chat_id.0)?;
        match chat.kind {
            ChatKind::Private { user_id } if self.bot_user_ids.contains(&user_id.0) => {
                Some(user_id.0)
            }
            _ => None,
        }
    }

    /// Cached `botInfo` for the open chat's bot, if the lazy fetch (or an
    /// `updateUserFullInfo`) already populated it.
    pub fn bot_info_for_chat(&self, chat_id: ChatId) -> Option<&BotInfo> {
        self.bot_user_id_for_chat(chat_id)
            .and_then(|user_id| self.bot_info.get(&user_id))
            .and_then(|info| info.as_ref())
    }

    /// Phase 6: the user id behind any private chat (bot or not).
    pub fn private_chat_user_id(&self, chat_id: ChatId) -> Option<i64> {
        let chat = self.chats.get(&chat_id.0)?;
        match chat.kind {
            ChatKind::Private { user_id } => Some(user_id.0),
            _ => None,
        }
    }

    /// Telegram Desktop's `Element::fromLink` (history_view_element.cpp):
    /// clicking a sender's userpic shows that peer's profile. A user
    /// (bot or not, yourself included) opens the user profile; a
    /// `messageSenderChat` sender (an anonymous admin posting as the
    /// group itself, a channel posting into its discussion group) opens
    /// that chat's profile. `None` when the chat is unknown or has no
    /// profile panel (the click then does nothing).
    pub fn avatar_profile_target(&self, sender: MessageSender) -> Option<InfoPanelTarget> {
        match sender {
            MessageSender::User { user_id } => Some(InfoPanelTarget::User(user_id)),
            MessageSender::Chat { chat_id } => self.info_panel_target_for_chat(ChatId(chat_id)),
        }
    }

    /// Phase 6: info-panel target for a chat header — the peer user for a
    /// private chat, the supergroup for a group/channel chat, the chat
    /// partner for a secret chat (Phase B2: their panel hosts the
    /// encryption-key section), `None` for basic groups and unknown
    /// kinds.
    pub fn info_panel_target_for_chat(&self, chat_id: ChatId) -> Option<InfoPanelTarget> {
        let chat = self.chats.get(&chat_id.0)?;
        match chat.kind {
            ChatKind::Private { user_id } => Some(InfoPanelTarget::User(user_id.0)),
            ChatKind::Secret { user_id, .. } => Some(InfoPanelTarget::User(user_id.0)),
            ChatKind::Supergroup { supergroup_id, .. } => {
                Some(InfoPanelTarget::Supergroup(supergroup_id))
            }
            ChatKind::BasicGroup { basic_group_id } => {
                Some(InfoPanelTarget::BasicGroup(basic_group_id))
            }
            _ => None,
        }
    }

    /// Phase B2: the full `ParsedSecretChat` record behind the *open*
    /// chat, when the open chat is a **Ready** secret chat whose partner is
    /// `user_id`. Drives the encryption-key section of the partner's info
    /// panel. `None` for non-Ready chats (the key is only meaningful once
    /// the session is established) and while the record hasn't arrived.
    ///
    /// Security: the returned record borrows session memory; callers pass
    /// the bytes through `key_fingerprint::key_hash_pixels` and render
    /// pixel colors only — raw bytes never leave `Session`.
    pub fn open_ready_secret_chat_for_user(&self, user_id: i64) -> Option<&ParsedSecretChat> {
        let open = self.open_chat?;
        let chat = self.chats.get(&open.0)?;
        let ChatKind::Secret {
            secret_chat_id,
            user_id: partner,
        } = &chat.kind
        else {
            return None;
        };
        if partner.0 != user_id {
            return None;
        }
        if chat.secret_state != Some(SecretChatState::Ready) {
            return None;
        }
        self.secret_chat_states.get(secret_chat_id)
    }

    /// Phase 6: cached user object, if an `updateUser` has been seen.
    /// The group/channel header's counts: members (full info when loaded,
    /// else the base group object) and members online (opened groups
    /// only). `None` for private chats.
    pub fn group_member_counts(&self, chat: &ChatSummary) -> Option<(i32, i32)> {
        use crate::telegram::envelope::ChatKind;
        let members = match chat.kind {
            ChatKind::Supergroup { supergroup_id, .. } => self
                .groups
                .supergroup_full_infos
                .get(&supergroup_id)
                .map(|info| info.member_count)
                .filter(|count| *count > 0)
                .or_else(|| {
                    self.groups
                        .supergroup_member_counts
                        .get(&supergroup_id)
                        .copied()
                })
                .unwrap_or(0),
            ChatKind::BasicGroup { basic_group_id } => self
                .groups
                .basic_group_member_counts
                .get(&basic_group_id)
                .copied()
                .unwrap_or(0),
            _ => return None,
        };
        let online = self
            .chats_state
            .chat_online_counts
            .get(&chat.id.0)
            .copied()
            .unwrap_or(0);
        Some((members, online))
    }

    /// A private chat's user is online right now (bots and the account's
    /// own chat excluded): the chat list draws a dot on its avatar.
    pub fn chat_peer_online(&self, chat: &ChatSummary) -> bool {
        let crate::telegram::envelope::ChatKind::Private { user_id } = chat.kind else {
            return false;
        };
        Some(user_id.0) != self.my_user_id
            && self
                .user(user_id.0)
                .is_some_and(|user| !user.is_bot && user.status.is_online())
    }

    /// The badge after a chat row's title (`peer_badge::title_badge`):
    /// users carry verification, Premium and an emoji status; supergroups
    /// and channels carry verification only; basic groups have none.
    pub fn chat_title_badge(&self, chat: &ChatSummary) -> Option<crate::peer_badge::TitleBadge> {
        let (verification, premium, status) = self.chat_badge_inputs(chat)?;
        crate::peer_badge::title_badge(verification, premium, status)
    }

    /// The badges after the open chat's title in the header
    /// (`peer_badge::header_badges`): the status and the check together.
    pub fn chat_header_badges(&self, chat: &ChatSummary) -> Vec<crate::peer_badge::TitleBadge> {
        self.chat_badge_inputs(chat)
            .map(|(verification, premium, status)| {
                crate::peer_badge::header_badges(verification, premium, status)
            })
            .unwrap_or_default()
    }

    fn chat_badge_inputs(
        &self,
        chat: &ChatSummary,
    ) -> Option<(crate::peer_badge::VerificationStatus, bool, i64)> {
        use crate::telegram::envelope::ChatKind;
        match chat.kind {
            ChatKind::Private { user_id } | ChatKind::Secret { user_id, .. } => {
                let user = self.user(user_id.0)?;
                Some((user.verification, user.is_premium, user.emoji_status_id))
            }
            ChatKind::Supergroup { supergroup_id, .. } => Some((
                self.groups
                    .supergroup_verification
                    .get(&supergroup_id)
                    .copied()
                    .unwrap_or_default(),
                false,
                0,
            )),
            ChatKind::BasicGroup { .. } | ChatKind::Unknown => None,
        }
    }

    pub fn user(&self, user_id: i64) -> Option<&ParsedUser> {
        self.users.get(&user_id)
    }

    /// Phase 6: cached `userFullInfo` bio, if fetched.
    pub fn user_full_info(&self, user_id: i64) -> Option<&UserFullInfoData> {
        self.user_full_infos.get(&user_id)
    }

    /// Phase 6: cached `supergroupFullInfo`, if fetched.
    pub fn supergroup_full_info(&self, supergroup_id: i64) -> Option<&SupergroupFullInfoData> {
        self.groups.supergroup_full_infos.get(&supergroup_id)
    }

    /// Phase A1: the viewer's own `chatMemberStatus*` in a supergroup
    /// (`supergroup.status`, schema 1.8.67 line 2746), if seen yet.
    pub fn supergroup_own_status(&self, supergroup_id: i64) -> Option<ChannelMemberStatus> {
        self.groups
            .supergroup_member_status
            .get(&supergroup_id)
            .copied()
    }

    /// Phase A1: whether the viewer's own administrator rights in a
    /// supergroup include `can_restrict_members` (schema 1.8.67, lines
    /// 2500/1092), which `setChatSlowModeDelay` requires (line 13551).
    /// Creators hold all rights implicitly — check
    /// `supergroup_own_status` for that. Absent = unknown, treated as
    /// lacking the right (the admin control stays hidden).
    pub fn supergroup_can_restrict_members(&self, supergroup_id: i64) -> bool {
        self.groups
            .supergroup_restrict_right
            .get(&supergroup_id)
            .copied()
            .unwrap_or(false)
    }

    /// Phase D3a: whether the viewer's own administrator rights in a
    /// supergroup include `can_invite_users` (schema 1.8.67, line 1092),
    /// which invite-link management requires. Creators hold all rights
    /// implicitly — check `supergroup_own_status` for that. Absent =
    /// unknown, treated as lacking the right.
    pub fn supergroup_can_invite_users(&self, supergroup_id: i64) -> bool {
        self.groups
            .supergroup_invite_right
            .get(&supergroup_id)
            .copied()
            .unwrap_or(false)
    }

    /// Phase D3b: whether the viewer's own administrator rights in a
    /// supergroup include `can_promote_members` (schema 1.8.67, line
    /// 1092), which admin management requires. Creators hold all rights
    /// implicitly — check `supergroup_own_status` for that. Absent =
    /// unknown, treated as lacking the right.
    pub fn supergroup_can_promote_members(&self, supergroup_id: i64) -> bool {
        self.groups
            .supergroup_promote_right
            .get(&supergroup_id)
            .copied()
            .unwrap_or(false)
    }

    /// Slice G1: whether the viewer's own administrator rights in a
    /// supergroup include `can_manage_tags` (schema 1.8.67, line 1092),
    /// which changing another member's custom title requires. Creators
    /// hold all rights implicitly — check `supergroup_own_status` for
    /// that. Absent = unknown, treated as lacking the right.
    pub fn supergroup_can_manage_tags(&self, supergroup_id: i64) -> bool {
        self.groups
            .supergroup_manage_tags_right
            .get(&supergroup_id)
            .copied()
            .unwrap_or(false)
    }

    /// Slice G1: whether the viewer may set a member's custom title in
    /// this chat — creator, or an admin with `can_manage_tags`.
    /// Basic groups: creators only (basic groups expose no per-admin
    /// rights; the server rejects anything else).
    pub fn chat_can_manage_tags(&self, chat_id: ChatId) -> bool {
        let Some(chat) = self.chats.get(&chat_id.0) else {
            return false;
        };
        match chat.kind {
            ChatKind::Supergroup { supergroup_id, .. } => {
                self.supergroup_own_status(supergroup_id) == Some(ChannelMemberStatus::Creator)
                    || self.supergroup_can_manage_tags(supergroup_id)
            }
            ChatKind::BasicGroup { .. } => self.chat_is_owner(chat_id),
            _ => false,
        }
    }

    /// Slice G2: whether the viewer holds `can_manage_topics` in a
    /// supergroup — creator, or an admin with the right (schema 1.8.67,
    /// line 1092). Gates forum topic management.
    pub fn chat_can_manage_topics(&self, chat_id: ChatId) -> bool {
        let Some(chat) = self.chats.get(&chat_id.0) else {
            return false;
        };
        match chat.kind {
            ChatKind::Supergroup {
                supergroup_id,
                is_channel: false,
            } => {
                self.supergroup_own_status(supergroup_id) == Some(ChannelMemberStatus::Creator)
                    || self
                        .groups
                        .supergroup_manage_topics_right
                        .get(&supergroup_id)
                        .copied()
                        .unwrap_or(false)
            }
            _ => false,
        }
    }

    /// Slice G2: whether the viewer may change a channel's info —
    /// creator, or an admin with `can_change_info` (schema 1.8.67, line
    /// 15175: `toggleSupergroupSignMessages` requires it). The
    /// `ChatSummary` path covers channels (own membership probed via
    /// `getChatMember`); supergroups carry the right on the
    /// `updateSupergroup` / `getSupergroup` status block.
    pub fn chat_can_change_info(&self, chat_id: ChatId) -> bool {
        let Some(chat) = self.chats.get(&chat_id.0) else {
            return false;
        };
        if chat.can_change_info() {
            return true;
        }
        match chat.kind {
            ChatKind::Supergroup { supergroup_id, .. } => {
                self.supergroup_own_status(supergroup_id) == Some(ChannelMemberStatus::Creator)
                    || self
                        .groups
                        .supergroup_change_info_right
                        .get(&supergroup_id)
                        .copied()
                        .unwrap_or(false)
            }
            // B7: basic group creator or administrator with the right.
            ChatKind::BasicGroup { basic_group_id } => {
                self.groups.basic_group_status.get(&basic_group_id)
                    == Some(&ChannelMemberStatus::Creator)
                    || self
                        .groups
                        .basic_group_change_info_right
                        .get(&basic_group_id)
                        .copied()
                        .unwrap_or(false)
            }
            _ => false,
        }
    }

    /// Slice G2: whether the viewer may manage welcome messages —
    /// creator, or an admin with `can_send_welcome_messages` (schema
    /// 1.8.67, line 1090). Same two paths as `chat_can_change_info`.
    pub fn chat_can_send_welcome_messages(&self, chat_id: ChatId) -> bool {
        let Some(chat) = self.chats.get(&chat_id.0) else {
            return false;
        };
        if chat.can_send_welcome_messages() {
            return true;
        }
        match chat.kind {
            ChatKind::Supergroup { supergroup_id, .. } => {
                self.supergroup_own_status(supergroup_id) == Some(ChannelMemberStatus::Creator)
                    || self
                        .groups
                        .supergroup_send_welcome_right
                        .get(&supergroup_id)
                        .copied()
                        .unwrap_or(false)
            }
            _ => false,
        }
    }

    /// Slice G2: cached `supergroup.sign_messages` (schema 1.8.67, line
    /// 2746). Absent = unknown → shown off.
    pub fn chat_sign_messages(&self, chat_id: ChatId) -> bool {
        self.chat_supergroup(chat_id).is_some_and(|id| {
            self.groups
                .supergroup_sign_messages
                .get(&id)
                .copied()
                .unwrap_or(false)
        })
    }

    /// Slice G2: cached `supergroup.show_message_sender` (schema 1.8.67,
    /// line 2746).
    pub fn chat_show_message_sender(&self, chat_id: ChatId) -> bool {
        self.chat_supergroup(chat_id).is_some_and(|id| {
            self.groups
                .supergroup_show_message_sender
                .get(&id)
                .copied()
                .unwrap_or(false)
        })
    }

    /// Slice G2: cached
    /// `supergroupFullInfo.has_aggressive_anti_spam_enabled` (schema
    /// 1.8.67, line 2792).
    pub fn chat_anti_spam_enabled(&self, chat_id: ChatId) -> bool {
        self.chat_supergroup(chat_id).is_some_and(|id| {
            self.groups
                .supergroup_anti_spam_enabled
                .get(&id)
                .copied()
                .unwrap_or(false)
        })
    }

    /// Slice G2: cached
    /// `supergroupFullInfo.can_toggle_aggressive_anti_spam` (schema
    /// 1.8.67, line 2792) — the only gate for the anti-spam toggle.
    pub fn chat_can_toggle_anti_spam(&self, chat_id: ChatId) -> bool {
        self.chat_supergroup(chat_id).is_some_and(|id| {
            self.groups
                .supergroup_can_toggle_anti_spam
                .get(&id)
                .copied()
                .unwrap_or(false)
        })
    }

    /// Slice S11: group sticker-set gate for a chat —
    /// `supergroupFullInfo.can_set_sticker_set` (schema 1.8.67, line 2765).
    /// False while the full info hasn't been fetched (fail closed).
    pub fn chat_can_set_sticker_set(&self, chat_id: ChatId) -> bool {
        self.chat_supergroup(chat_id).is_some_and(|id| {
            self.groups
                .supergroup_full_infos
                .get(&id)
                .is_some_and(|info| info.can_set_sticker_set)
        })
    }

    /// The chat's pinned messages, newest first: the fetched list, or
    /// until it arrives, the pinned rows of loaded history. Rows in loaded
    /// history win, as they carry edits.
    pub fn pinned_list(&self, chat_id: ChatId) -> Vec<&HistoryMessage> {
        let history = self.histories.get(&chat_id.0);
        match self.pinned_messages.get(&chat_id.0) {
            Some(list) => list
                .iter()
                .map(|message| {
                    history
                        .and_then(|history| history.messages.get(&message.id.0))
                        .unwrap_or(message)
                })
                .collect(),
            None => history
                .map(|history| {
                    history
                        .messages
                        .values()
                        .rev()
                        .filter(|message| message.is_pinned)
                        .collect()
                })
                .unwrap_or_default(),
        }
    }

    /// Whether the chat's content is protected from saving, forwarding
    /// and copying (`chat.has_protected_content`).
    pub fn chat_has_protected_content(&self, chat_id: ChatId) -> bool {
        self.chats_state.protected_chats.contains(&chat_id.0)
    }

    /// `editMessageSchedulingState` succeeded: "Send now" drops the entry
    /// from the scheduled list (the sent copy arrives as a new message);
    /// a reschedule rewrites its planned send time in place.
    pub(crate) fn finish_scheduling_edit(
        &mut self,
        message_id: MessageId,
        scheduling: ComposerScheduling,
    ) {
        match scheduling {
            ComposerScheduling::None => {
                self.scheduled_messages.retain(|m| m.id != message_id);
                self.message_action_note = Some("message sent".into());
            }
            ComposerScheduling::SendAtDate(send_date) => {
                if let Some(message) = self
                    .scheduled_messages
                    .iter_mut()
                    .find(|m| m.id == message_id)
                {
                    message.scheduling_state = Some(MessageSchedulingState::SendAtDate {
                        send_date: send_date as i32,
                    });
                }
                self.message_action_note = Some("message rescheduled".into());
            }
            ComposerScheduling::SendWhenOnline => {
                if let Some(message) = self
                    .scheduled_messages
                    .iter_mut()
                    .find(|m| m.id == message_id)
                {
                    message.scheduling_state = Some(MessageSchedulingState::SendWhenOnline);
                }
                self.message_action_note = Some("message rescheduled".into());
            }
        }
    }

    /// Whether the user is a known bot.
    pub fn is_bot_user(&self, user_id: i64) -> bool {
        self.bot_user_ids.contains(&user_id)
    }

    /// Whether the chat has scheduled messages
    /// (`chat.has_scheduled_messages`, `updateChatHasScheduledMessages`).
    pub fn chat_has_scheduled_messages(&self, chat_id: ChatId) -> bool {
        self.chats_state.scheduled_chats.contains(&chat_id.0)
    }

    pub(crate) fn set_chat_has_scheduled(&mut self, chat_id: i64, has: bool) {
        if has {
            self.chats_state.scheduled_chats.insert(chat_id);
        } else {
            self.chats_state.scheduled_chats.remove(&chat_id);
        }
    }

    pub(crate) fn set_chat_protected(&mut self, chat_id: i64, protected: bool) {
        if protected {
            self.chats_state.protected_chats.insert(chat_id);
        } else {
            self.chats_state.protected_chats.remove(&chat_id);
        }
    }

    /// Slice G2: cached `chat.has_welcome_messages` (schema 1.8.67, line
    /// 3627).
    pub fn chat_has_welcome_messages_flag(&self, chat_id: ChatId) -> bool {
        self.groups
            .chat_has_welcome_messages
            .get(&chat_id.0)
            .copied()
            .unwrap_or(false)
    }

    /// Slice G2: supergroup id for any supergroup-kind chat (channels
    /// included); `None` for basic groups and other kinds.
    pub fn chat_supergroup(&self, chat_id: ChatId) -> Option<i64> {
        self.chats.get(&chat_id.0).and_then(|chat| match chat.kind {
            ChatKind::Supergroup { supergroup_id, .. } => Some(supergroup_id),
            _ => None,
        })
    }

    /// Phase D3a: invite-link / join-request gate for a chat. The
    /// `ChatSummary` path covers channels (own membership probed via
    /// `getChatMember`); non-channel supergroups carry own admin rights
    /// on the `updateSupergroup` / `getSupergroup` status block instead.
    pub fn chat_can_invite_users(&self, chat_id: ChatId) -> bool {
        let Some(chat) = self.chats.get(&chat_id.0) else {
            return false;
        };
        if chat.can_invite_users() {
            return true;
        }
        match chat.kind {
            ChatKind::Supergroup {
                supergroup_id,
                is_channel: false,
            } => {
                self.supergroup_own_status(supergroup_id) == Some(ChannelMemberStatus::Creator)
                    || self.supergroup_can_invite_users(supergroup_id)
            }
            _ => false,
        }
    }

    /// Slice G1: whether the viewer may add members to a chat.
    /// `addChatMember` / `addChatMembers` require the `can_invite_users`
    /// *member* right (schema 1.8.67, lines 13574/13580) — a plain member
    /// with the default permission qualifies, so the default
    /// `chat.permissions` block governs; the admin invite right (or
    /// creator status) is a blanket override. Unknown permissions keep
    /// the gate closed.
    pub fn chat_can_add_members(&self, chat_id: ChatId) -> bool {
        let Some(chat) = self.chats.get(&chat_id.0) else {
            return false;
        };
        if self.chat_can_invite_users(chat_id) {
            return true;
        }
        chat.permissions
            .as_ref()
            .is_some_and(|p| p.can_invite_users)
    }
    /// Phase D3b: admin-management gate for a chat. The `ChatSummary`
    /// path covers channels (own membership probed via `getChatMember`);
    /// non-channel supergroups carry own admin rights on the
    /// `updateSupergroup` / `getSupergroup` status block instead.
    pub fn chat_can_manage_admins(&self, chat_id: ChatId) -> bool {
        let Some(chat) = self.chats.get(&chat_id.0) else {
            return false;
        };
        if chat.can_manage_admins() {
            return true;
        }
        match chat.kind {
            ChatKind::Supergroup { supergroup_id, .. } => {
                self.supergroup_own_status(supergroup_id) == Some(ChannelMemberStatus::Creator)
                    || self.supergroup_can_promote_members(supergroup_id)
            }
            ChatKind::BasicGroup { basic_group_id } => self
                .groups
                .basic_group_own
                .get(&basic_group_id)
                .is_some_and(|own| {
                    own.status == ChannelMemberStatus::Creator || own.can_promote_members
                }),
            _ => false,
        }
    }

    /// Slice G1: restrict/ban gate for a chat. `setChatMemberStatus`
    /// requires the `can_restrict_members` administrator right "to change
    /// restrictions of a user" (schema 1.8.67, lines 13586-13587).
    /// Deny-by-default, following the D3b `chat_can_manage_admins`
    /// pattern: the `ChatSummary` path covers channels (own membership
    /// probed via `getChatMember`); non-channel supergroups carry own
    /// admin status on the `updateSupergroup` / `getSupergroup` status
    /// block instead. Note `chatMemberStatusRestricted` is "not supported
    /// in basic groups and channels" (schema line 2510) — restrict applies
    /// to non-channel supergroups only; ban works in supergroups and
    /// channels.
    pub fn chat_can_restrict_members(&self, chat_id: ChatId) -> bool {
        let Some(chat) = self.chats.get(&chat_id.0) else {
            return false;
        };
        if chat.can_restrict_members() {
            return true;
        }
        match chat.kind {
            ChatKind::Supergroup { supergroup_id, .. } => {
                self.supergroup_own_status(supergroup_id) == Some(ChannelMemberStatus::Creator)
                    || self.supergroup_can_restrict_members(supergroup_id)
            }
            ChatKind::BasicGroup { basic_group_id } => self
                .groups
                .basic_group_own
                .get(&basic_group_id)
                .is_some_and(|own| {
                    own.status == ChannelMemberStatus::Creator || own.can_restrict_members
                }),
            _ => false,
        }
    }

    /// Slice G1: whether the viewer owns the chat (creator status).
    /// `toggleSupergroupIsBroadcastGroup` and `setSupergroupUsername`
    /// require owner privileges (schema 1.8.67, lines 15220/15133).
    pub fn chat_is_owner(&self, chat_id: ChatId) -> bool {
        let Some(chat) = self.chats.get(&chat_id.0) else {
            return false;
        };
        if chat.my_member_status == Some(ChannelMemberStatus::Creator) {
            return true;
        }
        match chat.kind {
            ChatKind::Supergroup { supergroup_id, .. } => {
                self.supergroup_own_status(supergroup_id) == Some(ChannelMemberStatus::Creator)
            }
            ChatKind::BasicGroup { basic_group_id } => self
                .groups
                .basic_group_own
                .get(&basic_group_id)
                .is_some_and(|own| own.status == ChannelMemberStatus::Creator),
            _ => false,
        }
    }

    /// Phase D3c: admin-log gate for a chat. `getChatEventLog` "requires
    /// administrator rights" and is "available only in supergroups and
    /// channels" (schema 1.8.67, line 15252). Deny-by-default, following
    /// the D3b `chat_can_manage_admins` pattern: the `ChatSummary` path
    /// covers channels (own membership probed via `getChatMember`);
    /// supergroups carry own admin status on the `updateSupergroup` /
    /// `getSupergroup` status block instead. Unlike admin management
    /// (which needs the explicit `can_promote_members` right), any
    /// administrator or the creator may view the log. Unknown/absent
    /// status keeps the section hidden and the request unsent rather
    /// than fabricating a right.
    pub fn chat_can_view_event_log(&self, chat_id: ChatId) -> bool {
        let Some(chat) = self.chats.get(&chat_id.0) else {
            return false;
        };
        if chat.is_admin_or_creator() {
            return true;
        }
        match chat.kind {
            ChatKind::Supergroup { supergroup_id, .. } => matches!(
                self.supergroup_own_status(supergroup_id),
                Some(ChannelMemberStatus::Creator | ChannelMemberStatus::Administrator)
            ),
            _ => false,
        }
    }

    /// Phase A1: slow-mode gate for a chat. Returns the remaining wait in
    /// whole seconds when all of these hold:
    /// - `chat_id` is a non-channel supergroup with cached full info whose
    ///   `slow_mode_delay` (schema 1.8.67, line 2758) is positive;
    /// - the viewer lacks bypass rights: not an administrator/creator
    ///   (`supergroup.status`, schema line 2746) and not boost-exempt
    ///   (`my_boost_count >= unrestrict_boost_count > 0`, schema lines
    ///   2779–2780);
    /// - the server-reported `slow_mode_delay_expires_in` (schema line
    ///   2759), decayed against `fetched_at_ms`, is still positive. The
    ///   schema warns no `updateSupergroupFullInfo` fires when only the
    ///   expiry changes (both old and new non-zero), so local decay is the
    ///   countdown; callers re-fetch on blocked sends for a fresh value.
    ///
    /// `None` = no gate (send freely). Pure in `now_ms` for replay tests.
    pub fn slow_mode_wait_secs(&self, chat_id: ChatId, now_ms: u64) -> Option<u64> {
        let chat = self.chats.get(&chat_id.0)?;
        let supergroup_id = match chat.kind {
            ChatKind::Supergroup {
                supergroup_id,
                is_channel: false,
            } => supergroup_id,
            _ => return None,
        };
        let info = self.groups.supergroup_full_infos.get(&supergroup_id)?;
        if info.slow_mode_delay <= 0 {
            return None;
        }
        // Bypass: administrators and the creator are exempt (tdesktop
        // slow-mode applies to non-administrator members, schema line
        // 2758 comment). Unknown status = no bypass.
        if self
            .supergroup_own_status(supergroup_id)
            .is_some_and(ChannelMemberStatus::is_admin)
        {
            return None;
        }
        // Bypass: enough boosts ignore slow mode (schema 1.8.67, line 2780
        // comment); `unrestrict_boost_count` 0 = unspecified.
        if info.unrestrict_boost_count > 0 && info.my_boost_count >= info.unrestrict_boost_count {
            return None;
        }
        let elapsed_s = now_ms.saturating_sub(info.fetched_at_ms) as f64 / 1000.0;
        let remaining = info.slow_mode_delay_expires_in - elapsed_s;
        if remaining > 0.0 {
            Some(remaining.ceil() as u64)
        } else {
            None
        }
    }

    /// Phase B3: whether the currently open chat's loaded history contains
    /// any message with a still-live `self_destruct_in` countdown — drives
    /// the 1-second render tick that keeps the timer badges fresh.
    pub fn open_chat_has_live_self_destruct(&self, now_ms: u64) -> bool {
        let Some(open) = self.open_chat else {
            return false;
        };
        self.histories.get(&open.0).is_some_and(|history| {
            history.messages.values().any(|message| {
                // Phase B4: auto-delete countdowns share the 1-second
                // render tick with self-destruct badges.
                message.has_live_self_destruct(now_ms) || message.has_live_auto_delete(now_ms)
            })
        })
    }

    /// Seconds until the soonest status line of a running live location in
    /// the open chat's loaded history reads differently, or `None` when
    /// the chat shows none. Drives the redraw that keeps the countdown
    /// current (`ensure_live_location_tick`), at the pace the labels
    /// change rather than every frame.
    pub fn open_chat_live_location_refresh(&self, now: i64) -> Option<u32> {
        let open = self.open_chat?;
        self.histories
            .get(&open.0)?
            .messages
            .values()
            .filter_map(|message| match &message.content {
                crate::telegram::envelope::MessageContent::Location(location) => {
                    location.live.as_ref()?.refresh_in_at(now)
                }
                _ => None,
            })
            .min()
    }

    /// Phase 6: contacts-list rows in server order with a name/status view
    /// model, sorted case-insensitively by display name. Users not yet
    /// seen via `updateUser` are skipped (their rows fill in when the
    /// updates arrive).
    pub fn contact_rows(&self) -> Vec<ContactRow> {
        let mut rows: Vec<ContactRow> = self
            .contacts
            .as_deref()
            .unwrap_or(&[])
            .iter()
            .filter_map(|id| self.users.get(id))
            .map(|user| ContactRow {
                user_id: user.id,
                name: user.display_name(),
                status_text: user.status.display(),
                is_online: user.status.is_online(),
                is_contact: user.is_contact,
                last_seen: crate::contacts_index::last_seen_rank(&user.status),
            })
            .collect();
        rows.sort_by(|a, b| {
            a.name
                .to_lowercase()
                .cmp(&b.name.to_lowercase())
                .then_with(|| a.user_id.cmp(&b.user_id))
        });
        rows
    }

    /// Phase 6: `true` once a `users` answer (or a failed attempt) settled —
    /// the contacts tab shows rows, an error, or a loading state.
    pub fn contacts_settled(&self) -> bool {
        self.contacts.is_some() || self.contacts_error
    }

    /// Phase 3.3: merged `/`-menu rows for the open chat's bot — the
    /// bot's `botInfo` commands first, then cached `getCommands`
    /// (global scope) results below, deduped by command name. Empty for
    /// non-bot chats, unknown chats, or when no commands are known yet.
    pub fn command_menu_items(&self, chat_id: ChatId) -> Vec<CommandMenuItem> {
        let Some(user_id) = self.bot_user_id_for_chat(chat_id) else {
            return Vec::new();
        };
        let specific: &[BotCommand] = self
            .bot_info_for_chat(chat_id)
            .map(|info| info.commands.as_slice())
            .unwrap_or(&[]);
        let global: &[BotCommand] = self
            .bot_commands
            .get(&user_id)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        merge_command_menu_items(specific, global)
    }

    pub fn mark_draft_dirty(&mut self, chat_id: ChatId) {
        self.draft_dirty.insert(chat_id.0);
    }

    pub fn draft_is_dirty(&self, chat_id: ChatId) -> bool {
        self.draft_dirty.contains(&chat_id.0)
    }

    /// Local persist (after `setChatDraftMessage`, or the demo path). Clears the dirty bit.
    pub fn store_composer_draft(&mut self, chat_id: ChatId, draft: Option<ChatDraft>) {
        if let Some(chat) = self.chats.get_mut(&chat_id.0) {
            chat.draft = draft;
        }
        self.draft_dirty.remove(&chat_id.0);
    }
}

/// The message whose inline buttons the number keys press in fast buttons
/// mode (tdesktop: the history's last message, and only when it carries an
/// inline keyboard).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FastButtonTarget {
    pub bot_id: i64,
    pub message_id: MessageId,
    /// Button count of each keyboard row.
    pub row_lens: Vec<usize>,
}

impl Session {
    /// Fast-button target of a bot chat: the newest loaded message when the
    /// loaded window reaches the end of the chat and the message shows an
    /// inline keyboard. `None` for other chats.
    pub fn fast_button_target(&self, chat_id: ChatId) -> Option<FastButtonTarget> {
        let bot_id = self.bot_user_id_for_chat(chat_id)?;
        let history = self.histories.get(&chat_id.0)?;
        if history.has_newer {
            return None;
        }
        let message = history.messages.values().next_back()?;
        let markup = message
            .ephemeral
            .as_ref()
            .and_then(|ephemeral| ephemeral.reply_markup.as_ref())
            .or(message.reply_markup.as_ref())?;
        let crate::telegram::envelope::ReplyMarkup::InlineKeyboard(keyboard) = markup else {
            return None;
        };
        let row_lens: Vec<usize> = keyboard.rows.iter().map(Vec::len).collect();
        row_lens
            .iter()
            .any(|len| *len > 0)
            .then_some(FastButtonTarget {
                bot_id,
                message_id: message.id,
                row_lens,
            })
    }
}
