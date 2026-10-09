//! B10: reducer pieces behind the profile and contact panels — the
//! chat-id lists (groups in common, similar channels, personal channel
//! candidates) and the profile photo gallery.
use super::*;
use crate::telegram::envelope::ParsedProfilePhoto;

impl Session {
    /// The `profile_chat_lists` key a pending `GetProfileChats` request
    /// answers: the user id for groups in common, the channel chat id for
    /// similar channels, 0 for the personal channel candidates.
    pub(crate) fn profile_chats_key(pending: &PendingRequest) -> Option<(ProfileChatsKind, i64)> {
        let RequestPurpose::GetProfileChats(kind) = pending.purpose else {
            return None;
        };
        let id = match kind {
            ProfileChatsKind::GroupsInCommon => pending.user_id?,
            ProfileChatsKind::SimilarChats => pending.chat_id?.0,
            ProfileChatsKind::SuitablePersonalChats => 0,
        };
        Some((kind, id))
    }

    /// A `chats` answer to one of our profile list fetches.
    pub(crate) fn apply_profile_chats(&mut self, chat_ids: &[ChatId], pending: &PendingRequest) {
        if let Some(key) = Self::profile_chats_key(pending) {
            self.profile_chat_lists.insert(
                key,
                ProfileChatsFetch::Loaded(chat_ids.iter().map(|id| id.0).collect()),
            );
        }
    }

    /// A refused list or gallery fetch: keep the reason so the panel can
    /// show a Retry row instead of spinning forever.
    pub(crate) fn fail_profile_fetch(&mut self, pending: &PendingRequest, reason: String) {
        if let Some(key) = Self::profile_chats_key(pending) {
            self.profile_chat_lists
                .insert(key, ProfileChatsFetch::Failed(reason));
        } else if pending.purpose == RequestPurpose::GetUserProfilePhotos
            && let Some(user_id) = pending.user_id
        {
            self.user_profile_photos
                .insert(user_id, ProfilePhotosFetch::Failed(reason));
        }
    }

    /// A `chatPhotos` answer: cache every size file and the gallery.
    pub(crate) fn apply_profile_photos(
        &mut self,
        total_count: i32,
        photos: Vec<ParsedProfilePhoto>,
        pending: &PendingRequest,
    ) {
        let Some(user_id) = pending.user_id else {
            return;
        };
        let mut gallery = Vec::with_capacity(photos.len());
        for photo in photos {
            gallery.push(ProfilePhoto {
                id: photo.id,
                added_date: photo.added_date,
                thumb_file_id: photo.thumb_file_id.0,
                full_file_id: photo.full_file_id.0,
                width: photo.width,
                height: photo.height,
            });
            for file in photo.files {
                self.upsert_file(file, false);
            }
        }
        self.user_profile_photos.insert(
            user_id,
            ProfilePhotosFetch::Loaded {
                total_count,
                photos: gallery,
            },
        );
    }

    /// The chat ids of a loaded profile list; empty while loading.
    pub fn profile_chat_list(&self, kind: ProfileChatsKind, id: i64) -> &[i64] {
        match self.profile_chat_lists.get(&(kind, id)) {
            Some(ProfileChatsFetch::Loaded(ids)) => ids,
            _ => &[],
        }
    }
}
