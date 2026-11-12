//! Video userpics beside messages and in the chat list: tdesktop's
//! `validateVideoUserpic` (history_inner_widget.cpp,
//! history_view_list_widget.cpp, dialogs_inner_widget.cpp) plays a
//! Premium user's video photo wherever their small userpic is drawn.
#![deny(clippy::indexing_slicing)]

use crate::ids::{FileId, RequestId};
use crate::state::Session;
use crate::telegram::envelope::ParsedProfilePhoto;
use std::time::{Duration, Instant};

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
    /// clips of their current photo: once per photo while the ask is out
    /// or answered (an answer that still lacks the clip never loops), and
    /// again once a failed ask has waited out [`USERPIC_INFO_RETRY`] or an
    /// ask's request is gone with no answer.
    pub fn userpic_full_info_due(&self, user_id: i64, now: Instant) -> bool {
        if self.userpic_video(user_id) != UserpicVideo::Unknown {
            return false;
        }
        let photo_id = self.users.get(&user_id).map_or(0, |user| user.photo_id);
        match self.users_state.userpic_full_info.get(&user_id) {
            Some(ask) if ask.photo_id == photo_id => match ask.stage {
                UserpicInfoStage::Asked { request } => self.requests.get(request).is_none(),
                UserpicInfoStage::Answered => false,
                UserpicInfoStage::Failed { until } => now >= until,
            },
            _ => true,
        }
    }

    /// The ask for the user's current photo went out under `request`.
    pub fn userpic_full_info_sent(&mut self, user_id: i64, request: RequestId) {
        let photo_id = self.users.get(&user_id).map_or(0, |user| user.photo_id);
        self.users_state.userpic_full_info.insert(
            user_id,
            UserpicInfoAsk {
                photo_id,
                stage: UserpicInfoStage::Asked { request },
            },
        );
    }

    /// `request` came back with the full info (whether or not it names the
    /// clip): its ask is over, and it is not repeated.
    pub fn userpic_full_info_answered(&mut self, request: RequestId) {
        if let Some(ask) = self.userpic_ask_of(request) {
            ask.stage = UserpicInfoStage::Answered;
        }
    }

    /// `request` came back with an error: the ask is repeated once
    /// [`USERPIC_INFO_RETRY`] has passed, not on every frame.
    pub fn userpic_full_info_failed(&mut self, request: RequestId, now: Instant) {
        if let Some(ask) = self.userpic_ask_of(request) {
            ask.stage = UserpicInfoStage::Failed {
                until: now.checked_add(USERPIC_INFO_RETRY).unwrap_or(now),
            };
        }
    }

    fn userpic_ask_of(&mut self, request: RequestId) -> Option<&mut UserpicInfoAsk> {
        self.users_state.userpic_full_info.values_mut().find(
            |ask| matches!(ask.stage, UserpicInfoStage::Asked { request: asked } if asked == request),
        )
    }
}

/// How long a refused ask for a video userpic's clips waits before the
/// next draw asks again.
pub const USERPIC_INFO_RETRY: Duration = Duration::from_secs(60);

/// A user's ask for the clips of their video userpic, for the photo it
/// was made for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UserpicInfoAsk {
    pub photo_id: i64,
    pub stage: UserpicInfoStage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UserpicInfoStage {
    /// `getUserFullInfo` is out under this request.
    Asked { request: RequestId },
    /// It answered; the photo's clips are known or the server has none.
    Answered,
    /// It failed: asked again once `until` has passed.
    Failed { until: Instant },
}
