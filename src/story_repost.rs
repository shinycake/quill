//! What a press on a repost's origin does, after tdesktop's
//! `RepostView::lookupHandler` (`media/stories/media_stories_repost_view.cpp`).

use crate::state::StoryCardState;
use crate::telegram::envelope::StoryOriginView;

/// The result of pressing "Reposted from ...".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepostPress {
    /// Open the original story. Quill fetches it first when it is not
    /// cached yet (tdesktop resolves it while showing the repost).
    OpenStory { chat_id: i64, story_id: i32 },
    /// The story is gone: show its poster's profile (tdesktop's short info
    /// box).
    OpenPeer { chat_id: i64 },
    /// No poster or no story id (a hidden poster): say the story expired.
    Expired,
}

/// `state` is the cache state of the origin story, if it has a chat.
pub fn press(origin: &StoryOriginView, state: impl Fn(i64, i32) -> StoryCardState) -> RepostPress {
    match *origin {
        StoryOriginView::PublicStory { chat_id, story_id } if story_id != 0 => {
            match state(chat_id, story_id) {
                StoryCardState::Ready | StoryCardState::Loading => {
                    RepostPress::OpenStory { chat_id, story_id }
                }
                StoryCardState::Expired => RepostPress::OpenPeer { chat_id },
            }
        }
        StoryOriginView::PublicStory { .. } | StoryOriginView::HiddenUser { .. } => {
            RepostPress::Expired
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn public(story_id: i32) -> StoryOriginView {
        StoryOriginView::PublicStory {
            chat_id: 5,
            story_id,
        }
    }

    #[test]
    fn a_loaded_or_loading_origin_opens_the_story() {
        for state in [StoryCardState::Ready, StoryCardState::Loading] {
            assert_eq!(
                press(&public(9), |_, _| state),
                RepostPress::OpenStory {
                    chat_id: 5,
                    story_id: 9
                }
            );
        }
    }

    #[test]
    fn a_deleted_origin_opens_the_poster() {
        assert_eq!(
            press(&public(9), |_, _| StoryCardState::Expired),
            RepostPress::OpenPeer { chat_id: 5 }
        );
    }

    #[test]
    fn no_poster_or_no_id_says_the_story_expired() {
        let hidden = StoryOriginView::HiddenUser {
            poster_name: "Someone".into(),
        };
        assert_eq!(
            press(&hidden, |_, _| StoryCardState::Ready),
            RepostPress::Expired
        );
        assert_eq!(
            press(&public(0), |_, _| StoryCardState::Ready),
            RepostPress::Expired
        );
    }
}
