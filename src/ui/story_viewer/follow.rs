//! The viewer's story list as a function of session state: the followed
//! poster's active stories, rebuilt every frame from the tray and the story
//! cache, so stories still loading when the viewer opened join as their
//! `getStory` answers land (tdesktop rebuilds on `Stories::sourceChanged`,
//! `media_stories_controller.cpp:868-874`). The walk across the tray's
//! posters merges the tray's order the same way, the neighbour posters'
//! stories are fetched ahead of a jump, and more of the tray loads when
//! fewer than three posters remain (`kPreloadPeersCount`, :773).
#![deny(clippy::indexing_slicing)]

use super::*;
use quill::state::StoryCardState;
use quill::story_tray_walk::wants_more_tray;
use quill::telegram::envelope::StoryListView;

impl QuillApp {
    pub(in crate::ui) fn follow_story_tray(&mut self, cx: &mut Context<Self>) {
        let Some((poster, story_id)) = self.stories.viewer.current().map(StoryViewerItem::key)
        else {
            return;
        };
        if self.stories.viewer.followed_poster() != Some(poster) {
            return;
        }
        let Some(list) = self.stories.viewer.tray_walk().map(|walk| walk.list()) else {
            return;
        };
        let Some(session) = self.session() else {
            return;
        };
        let Some(tray) = session.stories.tray.get(&poster.0) else {
            return;
        };
        let items = collect_story_items(poster, tray, &session.stories.stories);
        let mut fetch: Vec<(ChatId, i32)> = tray
            .stories
            .iter()
            .map(|info| (poster, info.story_id))
            .collect();
        let sources = session.story_walk_sources(list);
        self.stories.viewer.refresh(items);
        if let Some(walk) = self.stories.viewer.tray_walk_mut() {
            walk.show(&sources, poster.0, story_id);
        }
        fetch.extend(
            [-1, 1]
                .into_iter()
                .filter_map(|d| self.story_sibling_key(d)),
        );
        let Some(session) = self.session() else {
            return;
        };
        fetch.retain(|(chat, id)| session.story_card_state(chat.0, *id) == StoryCardState::Loading);
        let pictures: Vec<FileId> = [-1, 1]
            .into_iter()
            .filter_map(|d| self.story_sibling_picture_to_download(d))
            .collect();
        for file_id in pictures {
            self.request_media_download(file_id, None, cx);
        }
        if let Some(live) = self.live.as_mut() {
            for (chat, id) in fetch {
                let _ = live.driver.get_story(chat, id);
            }
            // Only the main list pages (`loadActiveStories(storyListMain)`).
            if list == StoryListView::Main
                && wants_more_tray(&sources, poster.0)
                && live
                    .driver
                    .session
                    .stories
                    .rearm_active_stories_load(Instant::now())
            {
                let _ = live.driver.maybe_load_active_stories();
            }
        }
    }
}
