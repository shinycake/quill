//! The viewer's story list as a function of session state: the followed
//! poster's active stories, rebuilt every frame from the tray and the story
//! cache, so stories still loading when the viewer opened join as their
//! `getStory` answers land (tdesktop rebuilds on `Stories::sourceChanged`,
//! `media_stories_controller.cpp:868-874`).
#![deny(clippy::indexing_slicing)]

use super::*;
use quill::state::StoryCardState;

impl QuillApp {
    pub(in crate::ui) fn follow_story_tray(&mut self) {
        let Some(poster) = self.stories.viewer.followed_poster() else {
            return;
        };
        let Some(session) = self.session() else {
            return;
        };
        let Some(tray) = session.stories.tray.get(&poster.0) else {
            return;
        };
        let items = collect_story_items(poster, tray, &session.stories.stories);
        let fetch: Vec<i32> = tray
            .stories
            .iter()
            .map(|info| info.story_id)
            .filter(|id| session.story_card_state(poster.0, *id) == StoryCardState::Loading)
            .collect();
        self.stories.viewer.refresh(items);
        if let Some(live) = self.live.as_mut() {
            for id in fetch {
                let _ = live.driver.get_story(poster, id);
            }
        }
    }
}
