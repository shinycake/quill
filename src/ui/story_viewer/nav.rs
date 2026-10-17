//! Moving through stories: within the current poster's list and, for a
//! viewer opened from the story tray, across poster boundaries the way
//! tdesktop's `subjumpFor` / `jumpFor` do (`media_stories_controller.cpp:1375-1464`).
//! Past the last story the next poster opens; back from the first, the
//! previous poster opens, or the story restarts when there is none.
#![deny(clippy::indexing_slicing)]

use super::*;
use quill::state::StoryCardState;
use quill::story_tray_walk::{StoryStep, TraySource, story_to_open};

impl QuillApp {
    /// The story the neighbour poster in direction `delta` (-1 or 1) opens
    /// on, whether or not it is cached yet.
    pub(in crate::ui) fn story_sibling_key(&self, delta: i32) -> Option<(ChatId, i32)> {
        let (chat_id, shown) = self.stories.viewer.tray_walk()?.neighbour(delta)?;
        let tray = self.session()?.stories.tray.get(&chat_id)?;
        Some((ChatId(chat_id), story_to_open(tray, shown)?))
    }

    /// The neighbour poster's story when it is cached, so a jump can show
    /// it at once (tdesktop closes the viewer on a jump to a story it does
    /// not have, `OverlayWidget::storiesJumpTo`).
    pub(in crate::ui) fn story_sibling(&self, delta: i32) -> Option<(ChatId, i32)> {
        let key = self.story_sibling_key(delta)?;
        let ready = self.session()?.story_card_state(key.0.0, key.1) == StoryCardState::Ready;
        ready.then_some(key)
    }

    fn plan_story_step(&self, delta: i32) -> StoryStep {
        self.stories.viewer.plan_step(
            delta,
            self.story_sibling(-1).is_some(),
            self.story_sibling(1).is_some(),
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
    /// on a sibling preview).
    pub(in crate::ui) fn jump_story_poster(&mut self, delta: i32, cx: &mut Context<Self>) {
        let Some((chat_id, story_id)) = self.story_sibling(delta) else {
            return;
        };
        let Some(session) = self.session() else {
            return;
        };
        let items = session
            .stories
            .tray
            .get(&chat_id.0)
            .map(|tray| collect_story_items(chat_id, tray, &session.stories.stories))
            .unwrap_or_default();
        let sources: Vec<TraySource> = session
            .ordered_story_tray()
            .into_iter()
            .map(TraySource::of)
            .collect();
        let Some(index) = items.iter().position(|item| item.story_id == story_id) else {
            return;
        };
        let prev = self.stories.viewer.current().map(StoryViewerItem::key);
        self.stories.viewer.switch_poster(items, index);
        if let Some(walk) = self.stories.viewer.tray_walk_mut() {
            walk.show(&sources, chat_id.0, story_id);
        }
        self.story_changed(prev, cx);
    }

    /// A new current story (or the same one restarted): tell TDLib which
    /// story is open, drop the per-story panels and restart playback.
    fn story_changed(&mut self, prev: Option<(ChatId, i32)>, cx: &mut Context<Self>) {
        let next = self.stories.viewer.current().map(StoryViewerItem::key);
        if let (Some(prev), Some(next)) = (prev, next)
            && prev != next
            && let Some(live) = self.live.as_mut()
        {
            let _ = live.driver.close_story(prev.0, prev.1);
            let _ = live.driver.open_story(next.0, next.1);
        }
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
        self.ensure_story_custom_emoji_downloads();
        self.sync_live_story(cx);
        cx.notify();
    }
}
