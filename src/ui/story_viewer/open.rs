//! Opening the story viewer: fetching what the clicked story needs, the
//! deferred open while `getStory` is in flight, and the toast when the
//! story turned out to be gone.
#![deny(clippy::indexing_slicing)]

use super::*;
use quill::state::StoryCardState;

impl QuillApp {
    /// Phase 9.1: open the fullscreen story viewer on `(chat_id, story_id)`.
    /// Missing story details for the chat's active stories are fetched with
    /// `getStory` first; the clicked story opens once its `story` response
    /// lands in the cache (`pending_story_open`, resolved on the next
    /// render). `openStory` marks the current story as viewed.
    pub(in crate::ui) fn open_story_viewer(
        &mut self,
        chat_id: ChatId,
        story_id: i32,
        cx: &mut Context<Self>,
    ) {
        let mut wanted: Vec<i32> = self
            .session()
            .and_then(|session| session.stories.tray.get(&chat_id.0))
            .map(|tray| tray.stories.iter().map(|info| info.story_id).collect())
            .unwrap_or_default();
        // A story opened from a message (forward or mention) may not be in
        // the poster's tray at all; fetch it too when it is not cached.
        if !wanted.contains(&story_id) {
            wanted.push(story_id);
        }
        // A story `getStory` already failed for is known to be gone: asking
        // again would only delay the toast below.
        let fetch: Vec<i32> = self
            .session()
            .map(|s| {
                wanted
                    .into_iter()
                    .filter(|id| s.story_card_state(chat_id.0, *id) == StoryCardState::Loading)
                    .collect()
            })
            .unwrap_or_default();
        if let Some(live) = self.live.as_mut() {
            for id in fetch {
                let _ = live.driver.get_story(chat_id, id);
            }
        } else if !fetch.is_empty() {
            self.connection.set_note(StatusNote::silent(tr(
                keys::Q_MEDIA_DEMO_GETSTORY_RUNS_WITH_LIVE,
            )));
        }
        if !self.rebuild_story_viewer(chat_id, story_id, cx) {
            self.chat_list.pending_story_open = Some((chat_id.0, story_id));
        }
        cx.notify();
    }

    /// Phase 9.1: resolve a tapped story once `getStory` answered. A story
    /// that arrived opens; one the server no longer has (deleted, expired
    /// or not reachable) drops the open and says so, like tdesktop's
    /// `lng_stories_link_invalid` toast after `Stories::resolve`
    /// (`window_session_controller.cpp:700-705`).
    pub(in crate::ui) fn resolve_pending_story_open(&mut self, cx: &mut Context<Self>) {
        let Some((chat_id, story_id)) = self.chat_list.pending_story_open else {
            return;
        };
        match self
            .session()
            .map(|s| s.story_card_state(chat_id, story_id))
        {
            Some(StoryCardState::Ready) => {
                self.chat_list.pending_story_open = None;
                self.rebuild_story_viewer(ChatId(chat_id), story_id, cx);
            }
            Some(StoryCardState::Expired) => {
                self.chat_list.pending_story_open = None;
                self.set_status_note(StatusNote::info(tr(keys::STORIES_LINK_INVALID)), cx);
            }
            Some(StoryCardState::Loading) | None => {}
        }
    }

    /// Build the viewer items for `chat_id`'s cached stories and open on
    /// `story_id`. Returns `false` when the clicked story is not cached yet.
    pub(in crate::ui) fn rebuild_story_viewer(
        &mut self,
        chat_id: ChatId,
        story_id: i32,
        cx: &mut Context<Self>,
    ) -> bool {
        let items: Vec<StoryViewerItem> = self
            .session()
            .and_then(|session| {
                session
                    .stories
                    .tray
                    .get(&chat_id.0)
                    .map(|tray| collect_story_items(chat_id, tray, &session.stories.stories))
            })
            .unwrap_or_default();
        // A story in the poster's tray opens on the tray's list, which then
        // follows the tray; one that is not in it opens alone.
        let viewer = match items.iter().position(|item| item.story_id == story_id) {
            Some(index) => StoryViewer::open_poster(chat_id, items, index),
            None => StoryViewer::open(
                self.session()
                    .map(|s| {
                        quill::story_viewer::collect_found_story_items(
                            &[(chat_id.0, story_id)],
                            &s.stories.stories,
                        )
                    })
                    .unwrap_or_default(),
                0,
            ),
        };
        if !viewer.is_open() {
            return false;
        }
        self.start_story_viewer(viewer, cx);
        true
    }

    /// Open the viewer on `items[index]` and start playback.
    pub(in crate::ui) fn begin_story_viewer(
        &mut self,
        items: Vec<StoryViewerItem>,
        index: usize,
        cx: &mut Context<Self>,
    ) {
        self.start_story_viewer(StoryViewer::open(items, index), cx);
    }

    fn start_story_viewer(&mut self, viewer: StoryViewer, cx: &mut Context<Self>) {
        let current = viewer.current().map(StoryViewerItem::key);
        self.stories.viewer = viewer;
        // Phase 9.5: viewers list and report flow are per-story.
        self.stories.viewers_open = false;
        self.stories.report_open = false;
        self.stories.stats_open = false;
        if let (Some(live), Some((chat_id, story_id))) = (self.live.as_mut(), current) {
            let _ = live.driver.open_story(chat_id, story_id);
        }
        // Phase 9.6: (re)start the playback clock + tick whenever the
        // viewer (re)opens — also covers the deferred `pending_story_open`
        // path, which funnels through here.
        self.stories.playback.start(Instant::now());
        self.stories.pause.reset();
        self.stories.native_failed = None;
        self.stories.video_wait_since = Some(Instant::now());
        self.sync_story_video(cx);
        self.ensure_story_tick(cx);
        self.ensure_story_download(cx);
        self.ensure_story_custom_emoji_downloads();
        self.sync_live_story(cx);
    }
}
