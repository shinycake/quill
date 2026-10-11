//! Payload handlers: user full info.
use super::*;

impl Session {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn apply_user_full_info(
        &mut self,
        bot_info: Option<BotInfo>,
        bio: String,
        photo: Option<ParsedFile>,
        photo_id: Option<i64>,
        blocked: bool,
        extras: crate::telegram::envelope::UserProfileExtras,
        pending: Option<&PendingRequest>,
        _extra: Option<RequestId>,
        _seq: u64,
    ) {
        // `getUserFullInfo` response: resolve the user id from the
        // pending request's explicit `user_id` (contacts-panel
        // fetch) or its private chat (chat-header fetch). Responses
        // for chats that stopped being private chats are dropped.
        if pending.map(|p| p.purpose) == Some(RequestPurpose::GetUserFullInfo)
            && let Some(pending) = pending
        {
            let user_id = pending.user_id.or_else(|| {
                pending
                    .chat_id
                    .and_then(|chat_id| self.private_chat_user_id(chat_id))
            });
            if let Some(user_id) = user_id {
                if let Some(personal) = &extras.personal_photo {
                    for file in &personal.files {
                        self.upsert_file(file.clone(), false);
                    }
                }
                let photo_file_id = photo.map(|file| {
                    let id = file.id.0;
                    self.upsert_file(file, false);
                    id
                });
                self.users_state.user_full_infos.insert(
                    user_id,
                    UserFullInfoData {
                        bio,
                        photo_file_id,
                        photo_id,
                        blocked,
                        extras,
                    },
                );
                if let Some(bot_id) = pending
                    .chat_id
                    .and_then(|chat_id| self.bot_user_id_for_chat(chat_id))
                {
                    self.bots.bot_info.insert(bot_id, bot_info);
                } else if pending.user_id.is_some() && self.bots.bot_user_ids.contains(&user_id) {
                    self.bots.bot_info.insert(user_id, bot_info);
                }
            }
        }
    }
}
