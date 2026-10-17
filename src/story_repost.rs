//! What a press on a repost's origin does, after tdesktop's
//! `RepostView::lookupHandler` (`media/stories/media_stories_repost_view.cpp`).

use crate::state::StoryCardState;
use crate::telegram::envelope::StoryOriginView;

/// The result of pressing "Reposted from ...".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepostPress {
    /// Open the original story. Quill fetches it first when it is not
    /// cached yet (tdesktop resolves it while showing the repost); if the
    /// answer says it is gone, `if_gone` runs instead.
    OpenStory {
        chat_id: i64,
        story_id: i32,
        if_gone: RepostGone,
    },
    /// The original story is known to be gone.
    Gone(RepostGone),
}

/// What a press shows when the original story is gone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepostGone {
    /// Its poster's profile (tdesktop's short info box).
    OpenPeer { chat_id: i64 },
    /// A note that the story expired: tdesktop's toast when there is no
    /// poster to show.
    Expired,
}

/// The press for `origin`. `state` is the cache state of the origin story
/// and `has_profile` whether its poster's profile can open. `None` means a
/// press would do nothing, so the label is not a button.
pub fn press(
    origin: &StoryOriginView,
    state: impl Fn(i64, i32) -> StoryCardState,
    has_profile: impl Fn(i64) -> bool,
) -> Option<RepostPress> {
    match *origin {
        StoryOriginView::PublicStory { chat_id, story_id } if story_id != 0 => {
            let profile = has_profile(chat_id).then_some(RepostGone::OpenPeer { chat_id });
            match state(chat_id, story_id) {
                StoryCardState::Ready | StoryCardState::Loading => Some(RepostPress::OpenStory {
                    chat_id,
                    story_id,
                    if_gone: profile.unwrap_or(RepostGone::Expired),
                }),
                StoryCardState::Expired => profile.map(RepostPress::Gone),
            }
        }
        StoryOriginView::PublicStory { .. } | StoryOriginView::HiddenUser { .. } => {
            Some(RepostPress::Gone(RepostGone::Expired))
        }
    }
}

/// The origin story to fetch while its repost is shown, as tdesktop's
/// `RepostView::recountDimensions` resolves it, so a press already knows
/// whether it is still there.
pub fn origin_to_fetch(
    origin: &StoryOriginView,
    state: impl Fn(i64, i32) -> StoryCardState,
) -> Option<(i64, i32)> {
    match *origin {
        StoryOriginView::PublicStory { chat_id, story_id }
            if story_id != 0 && state(chat_id, story_id) == StoryCardState::Loading =>
        {
            Some((chat_id, story_id))
        }
        StoryOriginView::PublicStory { .. } | StoryOriginView::HiddenUser { .. } => None,
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
    fn a_loaded_or_loading_origin_opens_the_story_then_the_poster_if_gone() {
        for state in [StoryCardState::Ready, StoryCardState::Loading] {
            assert_eq!(
                press(&public(9), |_, _| state, |_| true),
                Some(RepostPress::OpenStory {
                    chat_id: 5,
                    story_id: 9,
                    if_gone: RepostGone::OpenPeer { chat_id: 5 },
                })
            );
            assert_eq!(
                press(&public(9), |_, _| state, |_| false),
                Some(RepostPress::OpenStory {
                    chat_id: 5,
                    story_id: 9,
                    if_gone: RepostGone::Expired,
                }),
                "no profile to show: the story expired"
            );
        }
    }

    #[test]
    fn a_deleted_origin_opens_the_poster_or_does_nothing() {
        assert_eq!(
            press(&public(9), |_, _| StoryCardState::Expired, |_| true),
            Some(RepostPress::Gone(RepostGone::OpenPeer { chat_id: 5 }))
        );
        assert_eq!(
            press(&public(9), |_, _| StoryCardState::Expired, |_| false),
            None
        );
    }

    #[test]
    fn no_poster_or_no_id_says_the_story_expired() {
        let hidden = StoryOriginView::HiddenUser {
            poster_name: "Someone".into(),
        };
        let gone = Some(RepostPress::Gone(RepostGone::Expired));
        assert_eq!(press(&hidden, |_, _| StoryCardState::Ready, |_| true), gone);
        assert_eq!(
            press(&public(0), |_, _| StoryCardState::Ready, |_| true),
            gone
        );
    }

    #[test]
    fn only_an_origin_not_cached_yet_is_fetched() {
        assert_eq!(
            origin_to_fetch(&public(9), |_, _| StoryCardState::Loading),
            Some((5, 9))
        );
        for state in [StoryCardState::Ready, StoryCardState::Expired] {
            assert_eq!(origin_to_fetch(&public(9), |_, _| state), None);
        }
        assert_eq!(
            origin_to_fetch(&public(0), |_, _| StoryCardState::Loading),
            None
        );
    }
}
