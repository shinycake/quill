//! The "Reposted from ..." label under a repost's caption and its press,
//! after tdesktop's `RepostView` (`media_stories_repost_view.cpp`): the
//! origin is fetched while the repost is shown, and a press opens it alone
//! (`StoriesContextSingle`), else its poster's profile, else a note that
//! the story expired.
#![deny(clippy::indexing_slicing)]

use super::*;
use quill::lang::{keys, tr, tr_with};
use quill::story_repost::{RepostGone, RepostPress};
use quill::story_viewer::StoryOpenScope;

impl QuillApp {
    /// Phase 9.5: the label (`story.repost_info`, `td_api.tl:6742`). A
    /// chat that is not loaded gets no name: tdesktop's `RepostView`
    /// always has the peer, so it has no placeholder to copy.
    pub(in crate::ui) fn story_repost_line(&self) -> Option<String> {
        let story = self.current_story()?;
        let repost = story.repost_info.as_ref()?;
        let origin = match &repost.origin {
            StoryOriginView::PublicStory { chat_id, .. } => self
                .session()
                .and_then(|s| s.chats.get(chat_id))
                .map(|chat| chat.title.clone()),
            StoryOriginView::HiddenUser { poster_name } => Some(poster_name.clone()),
        };
        Some(match origin {
            Some(origin) => tr_with(keys::Q_MEDIA_REPOSTED_FROM, &[("origin", &origin)]),
            None => tr(keys::Q_MEDIA_REPOSTED_STORY).to_string(),
        })
    }

    /// What a press on the label does now; `None` when it would do
    /// nothing, so the label is plain text.
    pub(in crate::ui) fn story_repost_press(&self) -> Option<RepostPress> {
        let origin = self.current_story()?.repost_info?.origin;
        let session = self.session()?;
        quill::story_repost::press(
            &origin,
            |chat, story| session.story_card_state(chat, story),
            |chat| {
                session
                    .avatar_profile_target(MessageSender::Chat { chat_id: chat })
                    .is_some()
            },
        )
    }

    /// The press. An origin not cached yet opens once its `getStory`
    /// answer lands, or shows `if_gone` when the answer says it is gone.
    pub(in crate::ui) fn press_story_repost(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.story_repost_press() {
            Some(RepostPress::OpenStory {
                chat_id,
                story_id,
                if_gone,
            }) => {
                let open = PendingStoryOpen {
                    chat_id,
                    story_id,
                    scope: StoryOpenScope::Single,
                    if_gone: Some(if_gone),
                };
                self.open_story(open, cx);
            }
            Some(RepostPress::Gone(gone)) => self.show_story_repost_gone(gone, window, cx),
            None => {}
        }
    }

    /// The press's answer when the original story is gone.
    pub(super) fn show_story_repost_gone(
        &mut self,
        gone: RepostGone,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match gone {
            RepostGone::OpenPeer { chat_id } => {
                self.open_story_poster_profile(MessageSender::Chat { chat_id }, window, cx);
            }
            RepostGone::Expired => {
                self.stories.notice = Some(tr(keys::FORWARDED_STORY_EXPIRED).into());
                cx.notify();
            }
        }
    }

    /// Ask for the current repost's origin, as tdesktop resolves it while
    /// drawing the repost, so a press already knows whether it is there.
    pub(super) fn fetch_story_repost_origin(&mut self) {
        let Some(origin) = self
            .current_story()
            .and_then(|story| story.repost_info)
            .map(|repost| repost.origin)
        else {
            return;
        };
        let Some((chat_id, story_id)) = self.session().and_then(|session| {
            quill::story_repost::origin_to_fetch(&origin, |chat, story| {
                session.story_card_state(chat, story)
            })
        }) else {
            return;
        };
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.get_story(ChatId(chat_id), story_id);
        }
    }
}
