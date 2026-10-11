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
            ProfileChatsKind::SuitablePersonalChats | ProfileChatsKind::SuitableDiscussionChats => {
                0
            }
        };
        Some((kind, id))
    }

    /// A `chats` answer to one of our profile list fetches.
    pub(crate) fn apply_profile_chats(&mut self, chat_ids: &[ChatId], pending: &PendingRequest) {
        if let Some(key) = Self::profile_chats_key(pending) {
            self.users_state.profile_chat_lists.insert(
                key,
                ProfileChatsFetch::Loaded(chat_ids.iter().map(|id| id.0).collect()),
            );
        }
    }

    /// A refused list or gallery fetch: keep the reason so the panel can
    /// show a Retry row instead of spinning forever.
    pub(crate) fn fail_profile_fetch(&mut self, pending: &PendingRequest, reason: String) {
        if let Some(key) = Self::profile_chats_key(pending) {
            self.users_state
                .profile_chat_lists
                .insert(key, ProfileChatsFetch::Failed(reason));
        } else if pending.purpose == RequestPurpose::GetUserProfilePhotos
            && let Some(user_id) = pending.user_id
        {
            self.users_state
                .user_profile_photos
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
        self.users_state.user_profile_photos.insert(
            user_id,
            ProfilePhotosFetch::Loaded {
                total_count,
                photos: gallery,
            },
        );
    }

    /// The photos the profile gallery shows for `user_id`, plus the id of
    /// the one the current user set for this contact. tdesktop shows that
    /// personal photo first ("Photo set by you"); a photo that is already
    /// in the loaded list moves to the front instead of repeating.
    pub fn profile_gallery(&self, user_id: i64) -> Option<(Vec<ProfilePhoto>, Option<i64>)> {
        let Some(ProfilePhotosFetch::Loaded { photos, .. }) =
            self.users_state.user_profile_photos.get(&user_id)
        else {
            return None;
        };
        let personal = self
            .user_full_info(user_id)
            .and_then(|info| info.extras.personal_photo.as_ref());
        Some(merge_personal_photo(photos, personal))
    }

    /// The chat ids of a loaded profile list; empty while loading.
    pub fn profile_chat_list(&self, kind: ProfileChatsKind, id: i64) -> &[i64] {
        match self.users_state.profile_chat_lists.get(&(kind, id)) {
            Some(ProfileChatsFetch::Loaded(ids)) => ids,
            _ => &[],
        }
    }
}

/// Put the personal photo first in a gallery list (see
/// [`Session::profile_gallery`]). Returns the list and the personal
/// photo's id.
pub(crate) fn merge_personal_photo(
    photos: &[ProfilePhoto],
    personal: Option<&ParsedProfilePhoto>,
) -> (Vec<ProfilePhoto>, Option<i64>) {
    let Some(personal) = personal else {
        return (photos.to_vec(), None);
    };
    let first = ProfilePhoto {
        id: personal.id,
        added_date: personal.added_date,
        thumb_file_id: personal.thumb_file_id.0,
        full_file_id: personal.full_file_id.0,
        width: personal.width,
        height: personal.height,
    };
    let mut merged = Vec::with_capacity(photos.len() + 1);
    merged.push(first);
    merged.extend(
        photos
            .iter()
            .filter(|photo| photo.id != personal.id)
            .cloned(),
    );
    (merged, Some(personal.id))
}

#[cfg(test)]
mod gallery_tests {
    use crate::state::ProfilePhoto;
    use crate::telegram::envelope::ParsedProfilePhoto;

    fn photo(id: i64) -> ProfilePhoto {
        ProfilePhoto {
            id,
            added_date: 0,
            thumb_file_id: id as i32,
            full_file_id: id as i32 + 1,
            width: 10,
            height: 10,
        }
    }

    fn parsed(id: i64) -> ParsedProfilePhoto {
        ParsedProfilePhoto {
            id,
            added_date: 5,
            files: Vec::new(),
            thumb_file_id: crate::ids::FileId(900),
            full_file_id: crate::ids::FileId(901),
            width: 20,
            height: 20,
        }
    }

    #[test]
    fn personal_photo_goes_first() {
        let (list, id) = super::merge_personal_photo(&[photo(1), photo(2)], Some(&parsed(9)));
        assert_eq!(id, Some(9));
        assert_eq!(list.iter().map(|p| p.id).collect::<Vec<_>>(), vec![9, 1, 2]);
        assert_eq!(list[0].full_file_id, 901);
    }

    #[test]
    fn personal_photo_is_not_repeated() {
        let (list, id) = super::merge_personal_photo(&[photo(1), photo(9)], Some(&parsed(9)));
        assert_eq!(id, Some(9));
        assert_eq!(list.iter().map(|p| p.id).collect::<Vec<_>>(), vec![9, 1]);
    }

    #[test]
    fn no_personal_photo_keeps_the_list() {
        let (list, id) = super::merge_personal_photo(&[photo(1)], None);
        assert_eq!(id, None);
        assert_eq!(list.len(), 1);
    }
}
