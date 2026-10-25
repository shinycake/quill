//! Video userpics beside messages and in the chat list: tdesktop's
//! `validateVideoUserpic` (history_inner_widget.cpp,
//! history_view_list_widget.cpp, dialogs_inner_widget.cpp) plays a
//! Premium user's video photo wherever their small userpic is drawn.
#![deny(clippy::indexing_slicing)]

use crate::ids::FileId;
use crate::state::Session;
use crate::telegram::envelope::ParsedProfilePhoto;

/// What a small userpic of a user shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UserpicVideo {
    /// The still photo: no video photo, not Premium, or clips are off.
    Still,
    /// A video photo whose clips are not known yet: they come with the
    /// user's full info (tdesktop `updateFullForced`).
    Unknown,
    /// The clip to play, `small_animation` first.
    Clip(FileId),
}

impl Session {
    /// What `user_id`'s small userpic shows. tdesktop plays it when the
    /// user is Premium and the current photo is a video
    /// (`isPremium() && userpicHasVideo()`). Like the profile cover, the
    /// still stays while "Autoplay GIFs" is off or data saver is on.
    pub fn userpic_video(&self, user_id: i64) -> UserpicVideo {
        let prefs = &self.settings.media_prefs;
        if !prefs.autoplay_gifs || prefs.data_saver {
            return UserpicVideo::Still;
        }
        let Some(user) = self.users.get(&user_id) else {
            return UserpicVideo::Still;
        };
        if !user.is_premium || !user.photo_has_animation || user.photo_id == 0 {
            return UserpicVideo::Still;
        }
        match self.current_full_photo(user_id, user.photo_id) {
            Some(photo) => photo.animation.map_or(UserpicVideo::Still, |clip| {
                UserpicVideo::Clip(clip.userpic_file_id())
            }),
            None => UserpicVideo::Unknown,
        }
    }

    /// The full `chatPhoto` of the user's current photo, from their full
    /// info: the personal, main or public photo with that id.
    fn current_full_photo(&self, user_id: i64, photo_id: i64) -> Option<&ParsedProfilePhoto> {
        let extras = &self.user_full_info(user_id)?.extras;
        [&extras.personal_photo, &extras.photo, &extras.public_photo]
            .into_iter()
            .flatten()
            .find(|photo| photo.id == photo_id)
    }

    /// Whether the user's full info should be asked for now to learn the
    /// clips of their current photo: once per photo, so an answer that
    /// still lacks it never loops.
    pub fn take_userpic_full_info_ask(&mut self, user_id: i64) -> bool {
        if self.userpic_video(user_id) != UserpicVideo::Unknown {
            return false;
        }
        let photo_id = self.users.get(&user_id).map_or(0, |user| user.photo_id);
        self.users_state
            .userpic_full_info_asked
            .insert((user_id, photo_id))
    }
}
