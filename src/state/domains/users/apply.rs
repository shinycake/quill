//! Applies TDLib updates and answers for users, contacts, profiles and secret chats.
use crate::state::*;
use crate::telegram::envelope::UsersPayload;

impl Session {
    /// Applies one users payload; called by
    /// [`Session::apply_payload`].
    pub(crate) fn apply_users_payload(
        &mut self,
        payload: UsersPayload,
        pending: Option<&PendingRequest>,
        extra: Option<RequestId>,
        seq: u64,
    ) {
        match payload {
            UsersPayload::UpdateUser { user_id, user } => {
                // Phase 6: keep the full user object for the contacts list
                // and info panels.
                let old_photo_file_id = self
                    .users
                    .insert(user_id.0, user.clone())
                    .map(|old| old.photo_small_file_id);
                self.replace_avatar(old_photo_file_id, Some(user.photo_small_file_id));
                if user.is_bot {
                    self.bot_user_ids.insert(user_id.0);
                } else {
                    // No longer a bot: drop any cached bot info so the panel
                    // cannot show stale description/commands (Phase 3.1).
                    self.bot_user_ids.remove(&user_id.0);
                    self.bot_info.remove(&user_id.0);
                }
            }
            UsersPayload::UpdateUserStatus { user_id, status } => {
                // Phase 6: live online / last-seen for the contacts list.
                if let Some(user) = self.users.get_mut(&user_id.0) {
                    user.status = status;
                }
            }
            UsersPayload::Users { user_ids } => {
                // Phase 6: `getContacts` answer — only answers to our own
                // fetch are accepted (matched by `@extra`); the user
                // objects themselves arrive via `updateUser`.
                if self.apply_web_app_users(&user_ids, pending) {
                    // Apps tab: `getGrossingWebAppBots`.
                } else if pending.map(|p| p.purpose) == Some(RequestPurpose::GetContacts) {
                    self.contacts = Some(user_ids);
                    self.contacts_error = false;
                } else if pending.map(|p| p.purpose) == Some(RequestPurpose::GetCloseFriends) {
                    // B14: `getCloseFriends` answer.
                    self.stories.close_friends = Some(user_ids);
                    self.clear_story_page_op(RequestPurpose::GetCloseFriends);
                } else if pending.map(|p| p.purpose) == Some(RequestPurpose::GetRecentInlineBots) {
                    self.reply_keyboards.recent_inline_bots = Some(user_ids);
                } else if pending.map(|p| p.purpose) == Some(RequestPurpose::GetBotSimilarBots)
                    && let Some(pending) = pending
                    && let Some(bot_user_id) = pending.user_id
                {
                    // Slice B2: `getBotSimilarBots` answer — the user
                    // objects arrive via `updateUser`; ids alone drive the
                    // list (Telegram X `SharedChatsController` similarly
                    // resolves users from its cache).
                    self.similar_bots
                        .insert(bot_user_id, SimilarBotsFetch::Loaded(user_ids));
                }
            }
            // Phase B1: secret chat lifecycle (schema 1.8.67, lines
            // 10741 / 2816). `updateSecretChat` may arrive before any
            // `updateNewChat`; `secretChat` is the `getSecretChat` answer.
            // Both funnel into `accept_secret_chat`.
            UsersPayload::UpdateSecretChat { secret_chat } => {
                self.accept_secret_chat(&secret_chat);
            }
            UsersPayload::SecretChat { secret_chat } => {
                self.accept_secret_chat(&secret_chat);
            }
            UsersPayload::Me { user_id } => {
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetMe) {
                    self.my_user_id = Some(user_id);
                } else if pending.map(|p| p.purpose) == Some(RequestPurpose::GetSupportUser) {
                    self.support_user_ready = Some(user_id);
                } else if pending.map(|p| p.purpose)
                    == Some(RequestPurpose::GetChatOwnerAfterLeaving)
                    && let Some(chat_id) = pending.and_then(|p| p.chat_id)
                {
                    self.accept_owner_after_leaving(chat_id.0, user_id);
                } else if let Some(RequestPurpose::Chats(ChatsPurpose::DeepLinkResolve {
                    generation,
                })) = pending.map(|p| p.purpose)
                    && let Some(DeepLinkState::ResolvingChat {
                        action: DeepLinkAction::UserPhone { draft, .. },
                        generation: slot,
                    }) = self.deep_link.clone()
                    && slot == generation
                {
                    // `searchUserByPhoneNumber` answer: open the private chat.
                    self.deep_link = Some(DeepLinkState::Info {
                        text: String::new(),
                        need_update: false,
                        action: Some(DeepLinkAction::OpenUserDraft { user_id, draft }),
                        generation,
                    });
                }
            }
            UsersPayload::UserFullInfo {
                extras,
                bot_info,
                bio,
                photo,
                photo_id,
                blocked,
            } => self.apply_user_full_info(
                bot_info, bio, photo, photo_id, blocked, extras, pending, extra, seq,
            ),
            UsersPayload::UpdateUserFullInfo {
                user_id,
                extras,
                bot_info,
                bio,
                photo,
                photo_id,
                blocked,
            } => {
                self.bot_info.insert(user_id.0, bot_info);
                let photo_file_id = photo.map(|file| {
                    let id = file.id.0;
                    self.upsert_file(file, false);
                    id
                });
                self.user_full_infos.insert(
                    user_id.0,
                    UserFullInfoData {
                        bio,
                        photo_file_id,
                        photo_id,
                        blocked,
                        extras,
                    },
                );
            }
            // A5: `checkChatUsername` answer — the driver stashes the
            // verdict in `Session::username_check` before `apply` takes
            // the pending request; nothing to reduce here.
            UsersPayload::CheckChatUsernameResult(_) => {}
            // Slice A6: `importedContacts` (schema 1.8.67, line 14517) —
            // the `importContacts` answer. Same invalidate + notice as
            // the `ok` of the other contact mutations; the user ids are
            // not merged into the cache (the tab refetches the
            // authoritative list).
            UsersPayload::ImportedContacts { .. } => {
                if pending.map(|p| p.purpose) == Some(RequestPurpose::ImportContacts) {
                    self.contacts = None;
                    self.contacts_error = false;
                    self.contacts_notice = Some("Contacts imported.".to_string());
                }
            }
        }
    }
}
