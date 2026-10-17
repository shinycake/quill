//! Opening the story viewer: fetching what the clicked story needs, the
//! deferred open while `getStory` is in flight, and the toast when a
//! linked story turned out to be gone. What the viewer shows depends on
//! where it was opened from ([`StoryOpenScope`]).
#![deny(clippy::indexing_slicing)]

use super::*;
use quill::state::StoryCardState;
use quill::story_repost::RepostGone;
use quill::story_tray_walk::TrayWalk;
use quill::story_viewer::{StoryOpenScope, collect_found_story_items, story_open_switch};

/// A story open waiting for its `getStory` answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::ui) struct PendingStoryOpen {
    pub chat_id: i64,
    pub story_id: i32,
    pub scope: StoryOpenScope,
    /// A repost's press: what to show when the original turns out gone
    /// (tdesktop's `lookupHandler` failure branch).
    pub if_gone: Option<RepostGone>,
}

impl QuillApp {
    /// Phase 9.1: open the fullscreen story viewer on `(chat_id, story_id)`
    /// from `scope`. A tray open fetches the poster's other active stories
    /// with `getStory` too; a single open fetches only its story. The story
    /// opens once its `story` response lands in the cache
    /// (`pending_story_open`, resolved on the next render). `openStory`
    /// marks the current story as viewed.
    pub(in crate::ui) fn open_story_viewer(
        &mut self,
        chat_id: ChatId,
        story_id: i32,
        scope: StoryOpenScope,
        cx: &mut Context<Self>,
    ) {
        let open = PendingStoryOpen {
            chat_id: chat_id.0,
            story_id,
            scope,
            if_gone: None,
        };
        self.open_story(open, cx);
    }

    /// [`Self::open_story_viewer`] for a prepared `open`.
    pub(super) fn open_story(&mut self, open: PendingStoryOpen, cx: &mut Context<Self>) {
        let (chat_id, story_id, scope) = (ChatId(open.chat_id), open.story_id, open.scope);
        let mut wanted: Vec<i32> = match scope {
            StoryOpenScope::Tray => self
                .session()
                .and_then(|session| session.stories.tray.get(&chat_id.0))
                .map(|tray| tray.stories.iter().map(|info| info.story_id).collect())
                .unwrap_or_default(),
            StoryOpenScope::Single | StoryOpenScope::Link => Vec::new(),
        };
        if !wanted.contains(&story_id) {
            wanted.push(story_id);
        }
        // A story `getStory` already failed for is known to be gone: asking
        // again would only delay the answer below.
        let fetch: Vec<i32> = self
            .session()
            .map(|s| {
                wanted
                    .into_iter()
                    .filter(|id| s.story_card_state(chat_id.0, *id) == StoryCardState::Loading)
                    .collect()
            })
            .unwrap_or_default();
        // The opened story is asked for at once, even while a failed ask
        // waits out its retry clock; the poster's others are prefetches.
        if let Some(live) = self.live.as_mut() {
            for id in fetch {
                let _ = if id == story_id {
                    live.driver.get_story_now(chat_id, id)
                } else {
                    live.driver.get_story(chat_id, id)
                };
            }
        } else {
            if let Some(session) = self.demo_session.as_mut() {
                session.stories.story_retry.remove(&(chat_id.0, story_id));
            }
            if !fetch.is_empty() {
                self.connection.set_note(StatusNote::silent(tr(
                    keys::Q_MEDIA_DEMO_GETSTORY_RUNS_WITH_LIVE,
                )));
            }
        }
        // A newer open replaces one still waiting.
        let rebuilt = self.rebuild_story_viewer(open, cx);
        self.chat_list.pending_story_open = (!rebuilt).then_some(open);
        cx.notify();
    }

    /// Phase 9.1: resolve a tapped story once `getStory` answered. A story
    /// that arrived opens; one the server no longer has (deleted, expired
    /// or not reachable) drops the open. Only a link says so, like
    /// tdesktop's `lng_stories_link_invalid` toast after `Stories::resolve`
    /// (`window_session_controller.cpp:700-705`); a card or reply header
    /// shows the story as expired already, and tdesktop's `openPeerStory`
    /// stays silent. An ask that failed for a reason that may pass (the
    /// open cleared the story's retry clock, so a clock now means it
    /// failed) drops the open too: a link gets the same toast, as tdesktop
    /// runs the same callback on any failure, and any other open says the
    /// story could not be loaded. A repost's press shows its `if_gone` in
    /// both cases instead.
    pub(in crate::ui) fn resolve_pending_story_open(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(open) = self.chat_list.pending_story_open else {
            return;
        };
        let Some(session) = self.session() else {
            return;
        };
        let failed = session
            .stories
            .story_retry
            .contains_key(&(open.chat_id, open.story_id));
        let state = session.story_card_state(open.chat_id, open.story_id);
        if let Some(gone) = open.if_gone
            && matches!(
                (state, failed),
                (StoryCardState::Expired, _) | (StoryCardState::Loading, true)
            )
        {
            self.chat_list.pending_story_open = None;
            return self.show_story_repost_gone(gone, window, cx);
        }
        match state {
            StoryCardState::Ready => {
                self.chat_list.pending_story_open = None;
                self.rebuild_story_viewer(open, cx);
            }
            StoryCardState::Expired => {
                self.chat_list.pending_story_open = None;
                if open.scope == StoryOpenScope::Link {
                    self.set_status_note(StatusNote::info(tr(keys::STORIES_LINK_INVALID)), cx);
                }
            }
            StoryCardState::Loading if failed => {
                self.chat_list.pending_story_open = None;
                let note = match open.scope {
                    StoryOpenScope::Link => keys::STORIES_LINK_INVALID,
                    StoryOpenScope::Single | StoryOpenScope::Tray => {
                        keys::Q_MEDIA_STORY_LOAD_FAILED
                    }
                };
                self.set_status_note(StatusNote::info(tr(note)), cx);
            }
            StoryCardState::Loading => {}
        }
    }

    /// Build the viewer for `open` from the cached stories. Returns `false`
    /// when the opened story is not cached yet.
    fn rebuild_story_viewer(&mut self, open: PendingStoryOpen, cx: &mut Context<Self>) -> bool {
        let Some(session) = self.session() else {
            return false;
        };
        let chat_id = ChatId(open.chat_id);
        let alone = || {
            StoryViewer::open(
                collect_found_story_items(
                    &[(open.chat_id, open.story_id)],
                    &session.stories.stories,
                ),
                0,
            )
        };
        // A tray open shows the poster's active stories, which then follow
        // the tray, inside the walk across the story list the poster is in
        // (tdesktop's `StorySourcesList` context). A single open, or a
        // story no longer in its poster's tray, shows that story alone
        // (`StoriesContextSingle`).
        let viewer = match (open.scope, session.stories.tray.get(&open.chat_id)) {
            (StoryOpenScope::Tray, Some(tray)) => {
                let items = collect_story_items(chat_id, tray, &session.stories.stories);
                let walk = tray.list.and_then(|list| {
                    let unread = session.story_is_unread(open.chat_id, open.story_id);
                    let sources = session.story_walk_sources(list);
                    TrayWalk::open(list, &sources, open.chat_id, open.story_id, unread)
                });
                match (
                    items.iter().position(|item| item.story_id == open.story_id),
                    walk,
                ) {
                    (Some(index), Some(walk)) => StoryViewer::open_tray(walk, items, index),
                    (Some(index), None) => StoryViewer::open(items, index),
                    (None, _) => alone(),
                }
            }
            _ => alone(),
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

    /// TDLib's `openStory` for the story the viewer now shows, after
    /// `closeStory` for the one it replaces, so the two stay paired.
    pub(super) fn switch_open_story(
        &mut self,
        previous: Option<(ChatId, i32)>,
        current: Option<(ChatId, i32)>,
    ) {
        let (close, open) = story_open_switch(previous, current);
        let Some(live) = self.live.as_mut() else {
            return;
        };
        if let Some((chat_id, story_id)) = close {
            let _ = live.driver.close_story(chat_id, story_id);
        }
        if let Some((chat_id, story_id)) = open {
            let _ = live.driver.open_story(chat_id, story_id);
        }
    }

    fn start_story_viewer(&mut self, viewer: StoryViewer, cx: &mut Context<Self>) {
        let previous = self.stories.viewer.current().map(StoryViewerItem::key);
        let current = viewer.current().map(StoryViewerItem::key);
        self.stories.viewer = viewer;
        // Phase 9.5: viewers list and report flow are per-story.
        self.stories.viewers_open = false;
        self.stories.report_open = false;
        self.stories.stats_open = false;
        self.switch_open_story(previous, current);
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
        self.fetch_story_repost_origin();
        self.ensure_story_custom_emoji_downloads();
        self.sync_live_story(cx);
    }
}
