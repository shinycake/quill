//! What a press on a repost's origin does, after tdesktop's
//! `RepostView::lookupHandler` (`media/stories/media_stories_repost_view.cpp`).

use crate::state::StoryCardState;
use crate::telegram::envelope::StoryOriginView;

/// The result of pressing "Reposted from ...".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepostPress {
    /// The original story is loaded: open it.
    OpenStory { chat_id: i64, story_id: i32 },
    /// The story is not available (deleted, or not loaded): show its
    /// poster's profile.
    OpenPeer { chat_id: i64 },
    /// The original poster is hidden, so there is nothing to open: say the
    /// story has expired.
    Expired,
}

/// `state` is the cache state of the origin story, if it has a chat.
pub fn press(origin: &StoryOriginView, state: impl Fn(i64, i32) -> StoryCardState) -> RepostPress {
    match origin {
        StoryOriginView::PublicStory { chat_id, story_id } => {
            if state(*chat_id, *story_id) == StoryCardState::Ready {
                RepostPress::OpenStory {
                    chat_id: *chat_id,
                    story_id: *story_id,
                }
            } else {
                RepostPress::OpenPeer { chat_id: *chat_id }
            }
        }
        StoryOriginView::HiddenUser { .. } => RepostPress::Expired,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn public() -> StoryOriginView {
        StoryOriginView::PublicStory {
            chat_id: 5,
            story_id: 9,
        }
    }

    #[test]
    fn a_loaded_origin_opens_the_story() {
        assert_eq!(
            press(&public(), |_, _| StoryCardState::Ready),
            RepostPress::OpenStory {
                chat_id: 5,
                story_id: 9
            }
        );
    }

    #[test]
    fn a_missing_origin_opens_the_poster() {
        for state in [StoryCardState::Expired, StoryCardState::Loading] {
            assert_eq!(
                press(&public(), |_, _| state),
                RepostPress::OpenPeer { chat_id: 5 }
            );
        }
    }

    #[test]
    fn a_hidden_poster_says_the_story_expired() {
        let origin = StoryOriginView::HiddenUser {
            poster_name: "Someone".into(),
        };
        assert_eq!(
            press(&origin, |_, _| StoryCardState::Ready),
            RepostPress::Expired
        );
    }
}
