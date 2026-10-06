//! Chat-member and secret-chat acceptors.
use super::*;

impl Session {
    /// Whether the current user has Telegram Premium (`user.is_premium`,
    /// schema 1.8.67 line 2403) — gates `premiumFeatureRichMessages`
    /// ("The ability to send rich messages"). False until our own user
    /// object arrives.
    /// The chat with yourself (TDLib: a private chat whose id is your user
    /// id), shown as "Saved Messages" with a bookmark.
    pub fn is_saved_messages(&self, chat_id: ChatId) -> bool {
        self.my_user_id == Some(chat_id.0)
    }

    /// Who wrote a chat's last message, for group previews ("Dad: hi",
    /// "You: hi"), as Telegram Desktop shows them. `None` for private
    /// chats and channels, which show the text alone.
    pub fn chat_preview_sender(&self, chat: &ChatSummary) -> Option<String> {
        let group = matches!(
            chat.kind,
            ChatKind::BasicGroup { .. }
                | ChatKind::Supergroup {
                    is_channel: false,
                    ..
                }
        );
        let last = chat.last_message.filter(|_| group)?;
        if last.is_outgoing {
            return Some("You".to_string());
        }
        match last.sender? {
            MessageSender::User { user_id } => self.users.get(&user_id).map(|user| {
                if user.first_name.trim().is_empty() {
                    user.display_name()
                } else {
                    user.first_name.clone()
                }
            }),
            MessageSender::Chat { chat_id } => self.chats.get(&chat_id).map(|c| c.title.clone()),
        }
    }

    /// TDLib's `is_premium` option when known, else the own user record.
    pub fn my_is_premium(&self) -> bool {
        self.premium_option.unwrap_or_else(|| {
            self.my_user_id
                .and_then(|me| self.users.get(&me))
                .is_some_and(|user| user.is_premium)
        })
    }

    /// Record own channel membership from `getChatMember` / `updateChatMember`.
    /// The member is only trusted when `member_id` is the current user.
    pub fn accept_own_chat_member(&mut self, chat_id: ChatId, member: ParsedChatMember) {
        let own = self
            .my_user_id
            .is_some_and(|me| member.member_id == MessageSender::User { user_id: me });
        if !own {
            self.diagnostics.record(Diagnostic {
                category: "reducer",
                type_name: Some("chatMember".into()),
                extra: Some(chat_id.0 as u64),
                seq: None,
                note: "foreign-member-ignored",
            });
            return;
        }
        if let Some(chat) = self.chats.get_mut(&chat_id.0) {
            chat.set_member_status(member.status, member.admin_can_post_messages);
            chat.set_admin_can_invite_users(member.admin_can_invite_users);
            chat.set_admin_can_promote_members(member.admin_rights.map(|r| r.can_promote_members));
            chat.set_admin_can_restrict_members(
                member.admin_rights.map(|r| r.can_restrict_members),
            );
            chat.set_admin_can_pin_messages(member.admin_rights.map(|r| r.can_pin_messages));
            // Slice G2: sign-messages + welcome-message rights for the
            // channel path.
            chat.set_admin_can_change_info(member.admin_rights.map(|r| r.can_change_info));
            chat.set_admin_can_send_welcome_messages(
                member.admin_rights.map(|r| r.can_send_welcome_messages),
            );
        }
    }

    /// Phase B1: record a secret chat (`updateSecretChat` or a
    /// `getSecretChat` answer), fanning the state out to the chat
    /// summary when the chat is already known. `updateSecretChat` is
    /// guaranteed to arrive *before* the chat identifier is returned
    /// (schema 1.8.67, line 10740), hence the session-level map that
    /// `updateNewChat` hydrates from; a satisfied fetch leaves
    /// `secret_chat_fetch_queue`.
    pub(crate) fn accept_secret_chat(&mut self, secret_chat: &ParsedSecretChat) {
        let state = secret_chat.state.clone();
        self.secret_chat_states
            .insert(secret_chat.id, secret_chat.clone());
        self.secret_chat_fetch_queue
            .retain(|id| *id != secret_chat.id);
        for chat in self.chats.values_mut() {
            if let ChatKind::Secret { secret_chat_id, .. } = &chat.kind
                && *secret_chat_id == secret_chat.id
            {
                chat.secret_state = Some(state.clone());
            }
        }
    }
}
