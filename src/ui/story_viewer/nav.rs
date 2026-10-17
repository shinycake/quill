//! Moving through stories: within the current poster's list and, for a
//! viewer opened from the story tray, across poster boundaries the way
//! tdesktop's `subjumpFor` / `jumpFor` do (`media_stories_controller.cpp:1375-1464`).
//! Past the last story the next poster opens; back from the first, the
//! previous poster opens, or the story restarts when there is none.
#![deny(clippy::indexing_slicing)]

use super::*;
use quill::state::StoryCardState;
use quill::story_tray_walk::{StoryStep, shown_story};
use quill::story_viewer::collect_found_story_items;

impl QuillApp {
    /// The story the neighbour poster in direction `delta` (-1 or 1) opens
    /// on, whether or not it is cached yet. A poster whose story is known
    /// to be gone, waits out a failed fetch, or cannot be shown, is passed
    /// over for the next one in the walk.
    pub(in crate::ui) fn story_sibling_key(&self, delta: i32) -> Option<(ChatId, i32)> {
        let walk = self.stories.viewer.tray_walk()?;
        let session = self.session()?;
        let now = Instant::now();
        walk.neighbours(delta)
            .into_iter()
            .filter_map(|(chat_id, shown)| {
                let tray = session.stories.tray.get(&chat_id)?;
                Some((ChatId(chat_id), shown_story(tray, shown)?))
            })
            .find(|(chat, id)| match session.story_card_state(chat.0, *id) {
                StoryCardState::Loading => !session.stories.story_fetch_waiting((chat.0, *id), now),
                StoryCardState::Ready => {
                    !collect_found_story_items(&[(chat.0, *id)], &session.stories.stories)
                        .is_empty()
                }
                StoryCardState::Expired => false,
            })
    }

    /// The neighbour poster's story when it can be shown now, so a jump
    /// shows it at once (tdesktop closes the viewer on a jump to a story it
    /// does not have, `OverlayWidget::storiesJumpTo`). The one test of
    /// whether Prev / Next can cross to that poster.
    pub(in crate::ui) fn story_sibling_item(&self, delta: i32) -> Option<StoryViewerItem> {
        let (chat_id, story_id) = self.story_sibling_key(delta)?;
        let stories = &self.session()?.stories.stories;
        collect_found_story_items(&[(chat_id.0, story_id)], stories)
            .into_iter()
            .next()
    }

    fn plan_story_step(&self, delta: i32) -> StoryStep {
        self.stories.viewer.plan_step(
            delta,
            self.story_sibling_item(-1).is_some(),
            self.story_sibling_item(1).is_some(),
        )
    }

    /// Whether Prev / Next lead anywhere (tdesktop `subjumpAvailable`).
    pub(in crate::ui) fn story_step_available(&self, delta: i32) -> bool {
        matches!(
            self.plan_story_step(delta),
            StoryStep::Within(_) | StoryStep::Poster(_)
        )
    }

    /// Prev / Next, the arrow keys and auto-advance. A step past the last
    /// story with no poster after it closes the viewer, like tdesktop's
    /// Right key and `updatePlayback`.
    pub(in crate::ui) fn step_story_viewer(&mut self, delta: i32, cx: &mut Context<Self>) {
        let prev = self.stories.viewer.current().map(StoryViewerItem::key);
        match self.plan_story_step(delta) {
            StoryStep::Within(index) => self.stories.viewer.step_to(index),
            StoryStep::Restart => {}
            StoryStep::Poster(delta) => return self.jump_story_poster(delta, cx),
            StoryStep::End => return self.close_story_viewer(cx),
        }
        self.story_changed(prev, cx);
    }

    /// Open the neighbour poster's story (tdesktop `jumpFor`; also a click
    /// on a sibling preview). The list and the walk switch together, or
    /// not at all.
    pub(in crate::ui) fn jump_story_poster(&mut self, delta: i32, cx: &mut Context<Self>) {
        let Some(target) = self.story_sibling_item(delta) else {
            return;
        };
        let (chat_id, story_id) = target.key();
        let Some(session) = self.session() else {
            return;
        };
        let Some(mut walk) = self.stories.viewer.tray_walk().cloned() else {
            return;
        };
        let sources = session.story_walk_sources(walk.list());
        if !walk.show(&sources, chat_id.0, story_id) {
            return;
        }
        let items = session
            .stories
            .tray
            .get(&chat_id.0)
            .map(|tray| collect_story_items(chat_id, tray, &session.stories.stories))
            .unwrap_or_default();
        let Some(index) = items.iter().position(|item| item.key() == target.key()) else {
            return;
        };
        let prev = self.stories.viewer.current().map(StoryViewerItem::key);
        self.stories.viewer.switch_poster(walk, items, index);
        self.story_changed(prev, cx);
    }

    /// A new current story (or the same one restarted): tell TDLib which
    /// story is open, drop the per-story panels and restart playback.
    fn story_changed(&mut self, prev: Option<(ChatId, i32)>, cx: &mut Context<Self>) {
        let next = self.stories.viewer.current().map(StoryViewerItem::key);
        if prev.is_some() && next.is_some() {
            self.switch_open_story(prev, next);
        }
        // The user moved on: a press still waiting for its story is dropped.
        self.chat_list.pending_story_open = None;
        self.stories.reaction_picker_open = false;
        self.end_story_reply(cx);
        self.stories.stats_open = false;
        // B14: a new story starts playing (a Space pause is per story).
        self.stories.pause.reset();
        self.stories.share_open = false;
        self.stories.notice = None;
        self.stories.native_failed = None;
        self.stories.playback.start(Instant::now());
        self.sync_story_video(cx);
        self.ensure_story_tick(cx);
        self.ensure_story_download(cx);
        self.fetch_story_repost_origin();
        self.ensure_story_custom_emoji_downloads();
        self.sync_live_story(cx);
        cx.notify();
    }
}
