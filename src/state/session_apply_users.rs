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
                self.cache_profile_photo_files(&extras);
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
                        ratings: Default::default(),
                        voice_notes_restricted: false,
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

    /// Cache the files of the photos a `userFullInfo` names (every size
    /// and a video photo's clip).
    pub(crate) fn cache_profile_photo_files(
        &mut self,
        extras: &crate::telegram::envelope::UserProfileExtras,
    ) {
        for photo in [&extras.photo, &extras.personal_photo, &extras.public_photo]
            .into_iter()
            .flatten()
        {
            for file in &photo.files {
                self.upsert_file(file.clone(), false);
            }
        }
    }
}
