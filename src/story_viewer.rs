//! Phase 9.1: fullscreen story viewer.
//!
//! Pure state machine (no GPUI), mirroring the Phase 4.5 media viewer: the
//! open chat's active-story list plus the current position. The UI layer
//! builds items from the session's story cache (`Session::stories` /
//! `story_tray`), resolves display paths through the existing file/download
//! machinery (`usable_path` + `sandboxed_display_path`), and triggers
//! `downloadFile` when nothing viewable is local yet.
//!
//! Scope: photo and video story content. Live and unsupported stories keep
//! their item in the list but render a placeholder. Reactions and replies
//! are Phase 9.2 (viewer overlay actions); posting is out of scope here
//! (it has its own composer module and overlay — see
//! `src/story_composer.rs` and DECISIONS.md Phase 9.3).

use crate::ids::{ChatId, FileId};
use crate::telegram::envelope::{
    ChatActiveStoriesView, ParsedStory, StoryAreaView, StoryContentView,
};
use crate::text::TextEntity;
use crate::voice::format_voice_duration;
use std::time::{Duration, Instant};

/// Phase 9.6: how long a photo (or live/unsupported placeholder) story stays
/// on screen before auto-advance. A client-side constant — the TDLib schema
/// carries no photo duration (`storyContentPhoto photo:photo = StoryContent`,
/// `schema/td_api.tl:6654`); video stories use their own
/// `storyVideo duration:double` (`schema/td_api.tl:6633`). Matches Telegram
/// Desktop: `constexpr auto kPhotoDuration = 5 * crl::time(1000);`
/// (`media_stories_controller.cpp:72`) and Unigram's `StoryContentPhotoTimer`
/// (`TimeSpan.FromSeconds(5)`, `StoryContent.xaml.cs:2082`). Telegram Android
/// uses 10s (`IMAGE_LIVE_TIME = 10_000`, `PeerStoriesView.java:226`) —
/// Desktop wins as the designated secondary reference for this client.
pub const STORY_PHOTO_DURATION: Duration = Duration::from_secs(5);

/// Phase 9.6: deterministic playback clock for the viewer's current story.
/// The wall clock enters only as the `now` argument, so tests drive it
/// without sleeping. Pausing shifts the start anchor (like Telegram
/// Desktop's `PhotoPlayback::togglePaused`), freezing elapsed time.
#[derive(Debug, Clone, Default)]
pub struct StoryPlayback {
    started: Option<Instant>,
    paused_since: Option<Instant>,
}

impl StoryPlayback {
    /// Full display duration of one item: the video's own duration, or the
    /// photo constant. A non-positive video duration falls back to the photo
    /// constant (corrupt fixture data must not freeze the viewer).
    pub fn duration(item: &StoryViewerItem) -> Duration {
        match item.kind {
            StoryViewerKind::Video => item
                .duration_secs
                .filter(|secs| *secs > 0)
                .map(|secs| Duration::from_secs(secs as u64))
                .unwrap_or(STORY_PHOTO_DURATION),
            _ => STORY_PHOTO_DURATION,
        }
    }

    /// (Re)start the clock for the current story.
    pub fn start(&mut self, now: Instant) {
        self.started = Some(now);
        self.paused_since = None;
    }

    pub fn stop(&mut self) {
        self.started = None;
        self.paused_since = None;
    }

    pub fn is_running(&self) -> bool {
        self.started.is_some()
    }

    /// Pause or resume. Resuming shifts `started` forward by the paused
    /// span, so `elapsed`/`finished` stay correct without extra bookkeeping.
    pub fn set_paused(&mut self, paused: bool, now: Instant) {
        if paused {
            if self.paused_since.is_none() && self.started.is_some() {
                self.paused_since = Some(now);
            }
        } else if let Some(paused_since) = self.paused_since.take()
            && let Some(started) = self.started.as_mut()
        {
            *started += now.saturating_duration_since(paused_since);
        }
    }

    /// Elapsed display time for `item` at `now`, frozen while paused and
    /// clamped to the item's duration.
    pub fn elapsed(&self, item: &StoryViewerItem, now: Instant) -> Duration {
        let Some(started) = self.started else {
            return Duration::ZERO;
        };
        let end = self.paused_since.unwrap_or(now);
        end.saturating_duration_since(started)
            .min(Self::duration(item))
    }

    /// 0.0–1.0 fill fraction for the current segment.
    pub fn progress(&self, item: &StoryViewerItem, now: Instant) -> f32 {
        let duration = Self::duration(item);
        if duration.is_zero() {
            return 1.0;
        }
        (self.elapsed(item, now).as_secs_f32() / duration.as_secs_f32()).clamp(0.0, 1.0)
    }

    /// Elapsed long enough to auto-advance (Telegram Desktop's
    /// `IsStoppedAtEnd`; Telegram Android's `timeProgress == 1f` switch).
    pub fn finished(&self, item: &StoryViewerItem, now: Instant) -> bool {
        self.is_running() && self.elapsed(item, now) >= Self::duration(item)
    }
}

/// Story media kinds the viewer renders this slice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoryViewerKind {
    Photo,
    Video,
    Live,
    Unsupported,
}

impl StoryViewerKind {
    pub fn label(&self) -> &'static str {
        match self {
            StoryViewerKind::Photo => "Story",
            StoryViewerKind::Video => "Story",
            StoryViewerKind::Live => "Live story",
            StoryViewerKind::Unsupported => "Story",
        }
    }
}

/// stories-live-play: what the viewer needs to join a live story — the
/// group call behind `storyContentLive` (`schema/td_api.tl:6662`).
/// RTMP playback remains outside this slice and is unverified.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LiveStoryCall {
    pub group_call_id: i32,
    pub is_rtmp_stream: bool,
}

/// One openable story.
#[derive(Debug, Clone, PartialEq)]
pub struct StoryViewerItem {
    pub chat_id: ChatId,
    pub story_id: i32,
    pub kind: StoryViewerKind,
    /// Display candidates, most preferred first. Photo: largest size, thumb,
    /// then any other size. Video: the story video's thumbnail file only —
    /// the full clip is not renderable by the image element (same call as
    /// the Phase 4.5 media viewer).
    pub display_file_ids: Vec<FileId>,
    /// File to `downloadFile` when no display candidate is local yet. Photo:
    /// the largest size. Video: the thumbnail when the story attached one,
    /// else the video file itself (so it lands local).
    pub download_file_id: FileId,
    pub caption: String,
    pub caption_entities: Vec<TextEntity>,
    /// Video duration (`0:12`), shown when no visual is local.
    pub duration_label: Option<String>,
    /// Whole-second `storyVideo.duration` (`schema/td_api.tl:6633`), driving
    /// playback progress / auto-advance for video stories (Phase 9.6).
    /// `None` for photos, live, and unsupported stories.
    pub duration_secs: Option<i32>,
    /// B14: the playable clip (`storyVideo.video`) of a video story, so the
    /// viewer can play it with the native player once it is local.
    pub video_file_id: Option<FileId>,
    /// `storyInfo.is_live` — a live story shows the live placeholder
    /// (or the Join button) even if a `storyVideo` thumbnail were present.
    pub is_live: bool,
    /// stories-live-play: `Some` for live stories — the group call the
    /// viewer's Join button joins. `None` for photo/video/unsupported.
    pub live_call: Option<LiveStoryCall>,
    /// Phase 9.8: clickable areas (`storyArea`, `schema/td_api.tl:6566`),
    /// positioned as media-size fractions; the UI hit-tests them.
    pub areas: Vec<StoryAreaView>,
}

/// Viewer state: the open chat's story list plus the current position.
/// Empty items = closed.
#[derive(Debug, Clone, Default)]
pub struct StoryViewer {
    items: Vec<StoryViewerItem>,
    index: usize,
}

impl StoryViewer {
    pub fn closed() -> Self {
        Self::default()
    }

    pub fn is_open(&self) -> bool {
        !self.items.is_empty()
    }

    /// Open on `index`, clamped into range. An empty list stays closed.
    pub fn open(items: Vec<StoryViewerItem>, index: usize) -> Self {
        let index = index.min(items.len().saturating_sub(1));
        Self { items, index }
    }

    pub fn close(&mut self) {
        self.items.clear();
        self.index = 0;
    }

    pub fn prev(&mut self) {
        if self.is_open() && self.index > 0 {
            self.index -= 1;
        }
    }

    pub fn next(&mut self) {
        if self.is_open() && self.index + 1 < self.items.len() {
            self.index += 1;
        }
    }

    pub fn current(&self) -> Option<&StoryViewerItem> {
        self.items.get(self.index)
    }

    /// 1-based `(position, total)` for the "Story 2 of 5" header.
    pub fn position(&self) -> Option<(usize, usize)> {
        self.is_open().then(|| (self.index + 1, self.items.len()))
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

/// Build viewer items for one chat from the tray's `storyInfo` list and the
/// fetched-story cache. `storyInfo`s arrive in chronological order (oldest
/// first), which is also the tap-to-advance order. Stories whose full
/// `story` hasn't been fetched yet are skipped — the UI prefetches them
/// before opening.
pub fn collect_story_items(
    chat_id: ChatId,
    tray: &ChatActiveStoriesView,
    stories: &std::collections::HashMap<(i64, i32), ParsedStory>,
) -> Vec<StoryViewerItem> {
    tray.stories
        .iter()
        .filter_map(|info| stories.get(&(chat_id.0, info.story_id)))
        .filter_map(|story| story_viewer_item(chat_id, story))
        .collect()
}

fn story_viewer_item(chat_id: ChatId, story: &ParsedStory) -> Option<StoryViewerItem> {
    let item = match &story.content {
        StoryContentView::Photo { sizes } => {
            let largest = sizes
                .iter()
                .max_by_key(|size| i64::from(size.width) * i64::from(size.height))?;
            let mut display = vec![largest.file_id];
            for size in sizes {
                if !display.contains(&size.file_id) {
                    display.push(size.file_id);
                }
            }
            StoryViewerItem {
                chat_id,
                story_id: story.id,
                kind: StoryViewerKind::Photo,
                display_file_ids: display,
                download_file_id: largest.file_id,
                caption: story.caption.clone(),
                caption_entities: story.caption_entities.clone(),
                duration_label: None,
                duration_secs: None,
                video_file_id: None,
                is_live: false,
                live_call: None,
                areas: story.areas.clone(),
            }
        }
        StoryContentView::Video {
            thumb_file_id,
            duration_secs,
            file_id,
            ..
        } => {
            let download = (*thumb_file_id).unwrap_or(*file_id);
            if download.0 == 0 {
                return None;
            }
            StoryViewerItem {
                chat_id,
                story_id: story.id,
                kind: StoryViewerKind::Video,
                display_file_ids: (*thumb_file_id).into_iter().collect(),
                download_file_id: download,
                caption: story.caption.clone(),
                caption_entities: story.caption_entities.clone(),
                duration_label: Some(format_voice_duration(*duration_secs)),
                duration_secs: Some(*duration_secs),
                video_file_id: (file_id.0 != 0).then_some(*file_id),
                is_live: false,
                live_call: None,
                areas: story.areas.clone(),
            }
        }
        StoryContentView::Live {
            group_call_id,
            is_rtmp_stream,
        } => StoryViewerItem {
            chat_id,
            story_id: story.id,
            kind: StoryViewerKind::Live,
            display_file_ids: Vec::new(),
            download_file_id: FileId(0),
            caption: story.caption.clone(),
            caption_entities: story.caption_entities.clone(),
            duration_label: None,
            duration_secs: None,
            video_file_id: None,
            is_live: true,
            live_call: Some(LiveStoryCall {
                group_call_id: *group_call_id,
                is_rtmp_stream: *is_rtmp_stream,
            }),
            areas: story.areas.clone(),
        },
        StoryContentView::Unsupported => StoryViewerItem {
            chat_id,
            story_id: story.id,
            kind: StoryViewerKind::Unsupported,
            display_file_ids: Vec::new(),
            download_file_id: FileId(0),
            caption: story.caption.clone(),
            caption_entities: story.caption_entities.clone(),
            duration_label: None,
            duration_secs: None,
            video_file_id: None,
            is_live: false,
            live_call: None,
            areas: story.areas.clone(),
        },
    };
    Some(item)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::telegram::envelope::{PhotoSizeView, StoryInfoView};

    fn size(file_id: i32, width: i32, height: i32) -> PhotoSizeView {
        PhotoSizeView {
            type_name: "x".to_string(),
            width,
            height,
            file_id: FileId(file_id),
        }
    }

    fn photo_story(chat: i64, id: i32, sizes: Vec<PhotoSizeView>, caption: &str) -> ParsedStory {
        ParsedStory {
            id,
            poster_chat_id: chat,
            date: 1,
            content: StoryContentView::Photo { sizes },
            caption: caption.to_string(),
            caption_entities: Vec::new(),
            chosen_reaction_emoji: None,
            chosen_reaction_extra: None,
            interaction_info: None,
            can_be_deleted: false,
            can_be_replied: false,
            can_get_interactions: false,
            can_be_edited: false,
            can_set_privacy_settings: false,
            can_be_forwarded: false,
            is_edited: false,
            repost_info: None,
            privacy_settings: None,
            area_link_url: None,
            area_reaction_emojis: Vec::new(),
            can_be_added_to_album: false,
            is_posted_to_chat_page: false,
            can_toggle_is_posted_to_chat_page: false,
            can_get_statistics: false,
            areas: Vec::new(),
        }
    }

    fn video_story(
        chat: i64,
        id: i32,
        thumb: Option<i32>,
        file: i32,
        duration: i32,
    ) -> ParsedStory {
        ParsedStory {
            id,
            poster_chat_id: chat,
            date: 1,
            content: StoryContentView::Video {
                thumb_file_id: thumb.map(FileId),
                thumb_width: 320,
                thumb_height: 240,
                duration_secs: duration,
                file_id: FileId(file),
            },
            caption: String::new(),
            caption_entities: Vec::new(),
            chosen_reaction_emoji: None,
            chosen_reaction_extra: None,
            interaction_info: None,
            can_be_deleted: false,
            can_be_replied: false,
            can_get_interactions: false,
            can_be_edited: false,
            can_set_privacy_settings: false,
            can_be_forwarded: false,
            is_edited: false,
            repost_info: None,
            privacy_settings: None,
            area_link_url: None,
            area_reaction_emojis: Vec::new(),
            can_be_added_to_album: false,
            is_posted_to_chat_page: false,
            can_toggle_is_posted_to_chat_page: false,
            can_get_statistics: false,
            areas: Vec::new(),
        }
    }

    fn tray(chat: i64, story_ids: &[i32]) -> ChatActiveStoriesView {
        ChatActiveStoriesView {
            chat_id: chat,
            list: Some(crate::telegram::envelope::StoryListView::Main),
            order: 1,
            max_read_story_id: 0,
            stories: story_ids
                .iter()
                .map(|&story_id| StoryInfoView {
                    story_id,
                    date: story_id,
                    is_for_close_friends: false,
                    is_live: false,
                })
                .collect(),
        }
    }

    fn cache(stories: Vec<ParsedStory>) -> std::collections::HashMap<(i64, i32), ParsedStory> {
        stories
            .into_iter()
            .map(|story| ((story.poster_chat_id, story.id), story))
            .collect()
    }

    fn item(kind: StoryViewerKind, story_id: i32) -> StoryViewerItem {
        playback_item(kind, story_id, None)
    }

    fn playback_item(
        kind: StoryViewerKind,
        story_id: i32,
        duration_secs: Option<i32>,
    ) -> StoryViewerItem {
        StoryViewerItem {
            chat_id: ChatId(7),
            story_id,
            kind,
            display_file_ids: vec![FileId(1)],
            download_file_id: FileId(1),
            caption: String::new(),
            caption_entities: Vec::new(),
            duration_label: None,
            duration_secs,
            video_file_id: None,
            is_live: false,
            live_call: None,
            areas: Vec::new(),
        }
    }

    #[test]
    fn closed_has_no_current_or_position() {
        let viewer = StoryViewer::closed();
        assert!(!viewer.is_open());
        assert_eq!(viewer.current(), None);
        assert_eq!(viewer.position(), None);
        assert_eq!(viewer.len(), 0);
    }

    #[test]
    fn open_clamps_index_and_reports_position() {
        let items = vec![
            item(StoryViewerKind::Photo, 5),
            item(StoryViewerKind::Video, 6),
        ];
        let viewer = StoryViewer::open(items, 9);
        assert!(viewer.is_open());
        assert_eq!(viewer.current().unwrap().story_id, 6);
        assert_eq!(viewer.position(), Some((2, 2)));
    }

    #[test]
    fn open_empty_stays_closed() {
        assert!(!StoryViewer::open(Vec::new(), 0).is_open());
    }

    #[test]
    fn prev_next_walk_and_clamp_at_ends() {
        let items = vec![
            item(StoryViewerKind::Photo, 5),
            item(StoryViewerKind::Photo, 6),
            item(StoryViewerKind::Photo, 7),
        ];
        let mut viewer = StoryViewer::open(items, 1);
        viewer.prev();
        assert_eq!(viewer.current().unwrap().story_id, 5);
        assert_eq!(viewer.position(), Some((1, 3)));
        viewer.prev();
        assert_eq!(viewer.current().unwrap().story_id, 5);
        viewer.next();
        viewer.next();
        assert_eq!(viewer.current().unwrap().story_id, 7);
        viewer.next();
        assert_eq!(viewer.current().unwrap().story_id, 7);
    }

    #[test]
    fn close_resets_state() {
        let mut viewer = StoryViewer::open(vec![item(StoryViewerKind::Photo, 5)], 0);
        viewer.close();
        assert!(!viewer.is_open());
        assert_eq!(viewer.current(), None);
        viewer.prev();
        viewer.next();
        assert!(!viewer.is_open());
    }

    #[test]
    fn collect_skips_unfetched_and_keeps_tray_order() {
        let stories = cache(vec![
            photo_story(7, 10, vec![size(1, 100, 100)], "ten"),
            photo_story(7, 11, vec![size(2, 100, 100)], "eleven"),
        ]);
        let items = collect_story_items(ChatId(7), &tray(7, &[10, 99, 11]), &stories);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].story_id, 10);
        assert_eq!(items[1].story_id, 11);
        assert_eq!(items[1].caption, "eleven");
    }

    #[test]
    fn collect_photo_prefers_largest_size() {
        let stories = cache(vec![photo_story(
            7,
            10,
            vec![size(2, 320, 240), size(1, 800, 600)],
            "",
        )]);
        let items = collect_story_items(ChatId(7), &tray(7, &[10]), &stories);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].kind, StoryViewerKind::Photo);
        assert_eq!(items[0].display_file_ids[0], FileId(1));
        assert!(items[0].display_file_ids.contains(&FileId(2)));
        assert_eq!(items[0].download_file_id, FileId(1));
        assert_eq!(items[0].duration_label, None);
    }

    #[test]
    fn collect_video_uses_thumb_and_downloads_it() {
        let stories = cache(vec![video_story(7, 10, Some(50), 51, 12)]);
        let items = collect_story_items(ChatId(7), &tray(7, &[10]), &stories);
        assert_eq!(items.len(), 1);
        let item = &items[0];
        assert_eq!(item.kind, StoryViewerKind::Video);
        assert_eq!(item.display_file_ids, vec![FileId(50)]);
        assert_eq!(item.download_file_id, FileId(50));
        assert_eq!(item.duration_label.as_deref(), Some("0:12"));
    }

    #[test]
    fn collect_video_without_thumb_downloads_video_file() {
        let stories = cache(vec![video_story(7, 10, None, 51, 12)]);
        let items = collect_story_items(ChatId(7), &tray(7, &[10]), &stories);
        assert_eq!(items.len(), 1);
        assert!(items[0].display_file_ids.is_empty());
        assert_eq!(items[0].download_file_id, FileId(51));
    }

    #[test]
    fn collect_live_and_unsupported_keep_placeholder_items() {
        let mut live = photo_story(7, 10, vec![size(1, 100, 100)], "");
        live.content = StoryContentView::Live {
            group_call_id: 99,
            is_rtmp_stream: false,
        };
        let mut unsupported = photo_story(7, 11, vec![size(2, 100, 100)], "");
        unsupported.content = StoryContentView::Unsupported;
        let stories = cache(vec![live, unsupported]);
        let items = collect_story_items(ChatId(7), &tray(7, &[10, 11]), &stories);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].kind, StoryViewerKind::Live);
        assert!(items[0].is_live);
        assert_eq!(
            items[0].live_call,
            Some(LiveStoryCall {
                group_call_id: 99,
                is_rtmp_stream: false,
            })
        );
        assert_eq!(items[1].kind, StoryViewerKind::Unsupported);
        assert!(items[0].display_file_ids.is_empty());
    }

    // Phase 9.6: the pure playback clock — what these validate is the
    // auto-advance trigger math (durations, progress fraction, pause freeze)
    // driven by an injected `now`, with no wall-clock sleep.

    #[test]
    fn photo_duration_is_five_seconds() {
        let item = playback_item(StoryViewerKind::Photo, 5, None);
        assert_eq!(StoryPlayback::duration(&item), Duration::from_secs(5));
    }

    #[test]
    fn video_uses_its_own_duration() {
        let item = playback_item(StoryViewerKind::Video, 6, Some(9));
        assert_eq!(StoryPlayback::duration(&item), Duration::from_secs(9));
    }

    #[test]
    fn broken_video_duration_falls_back_to_photo() {
        for secs in [0, -3] {
            let item = playback_item(StoryViewerKind::Video, 6, Some(secs));
            assert_eq!(StoryPlayback::duration(&item), STORY_PHOTO_DURATION);
        }
    }

    #[test]
    fn collect_video_carries_duration_secs() {
        let stories = cache(vec![video_story(7, 10, Some(50), 51, 12)]);
        let items = collect_story_items(ChatId(7), &tray(7, &[10]), &stories);
        assert_eq!(items[0].duration_secs, Some(12));
        assert_eq!(StoryPlayback::duration(&items[0]), Duration::from_secs(12));
        let stories = cache(vec![photo_story(7, 11, vec![size(1, 100, 100)], "")]);
        let items = collect_story_items(ChatId(7), &tray(7, &[11]), &stories);
        assert_eq!(items[0].duration_secs, None);
    }

    #[test]
    fn progress_rises_then_clamps_and_finishes() {
        let item = playback_item(StoryViewerKind::Photo, 5, None);
        let now = Instant::now();
        let mut playback = StoryPlayback::default();
        playback.start(now);
        assert_eq!(playback.progress(&item, now), 0.0);
        let fifth = playback.progress(&item, now + Duration::from_secs(1));
        assert!((fifth - 0.2).abs() < 0.001, "got {fifth}");
        assert!(!playback.finished(&item, now + Duration::from_secs(4)));
        assert!(playback.finished(&item, now + Duration::from_secs(5)));
        // Past the end the fill stays pinned at 1.0, never over.
        assert_eq!(playback.progress(&item, now + Duration::from_secs(30)), 1.0);
        assert!(playback.finished(&item, now + Duration::from_secs(30)));
    }

    #[test]
    fn pause_freezes_elapsed_until_resume() {
        let item = playback_item(StoryViewerKind::Photo, 5, None);
        let t0 = Instant::now();
        let mut playback = StoryPlayback::default();
        playback.start(t0);
        playback.set_paused(true, t0 + Duration::from_secs(2));
        // The clock runs on while paused: elapsed must stay frozen at 2s.
        assert_eq!(
            playback.elapsed(&item, t0 + Duration::from_secs(100)),
            Duration::from_secs(2)
        );
        assert!(!playback.finished(&item, t0 + Duration::from_secs(100)));
        // Resuming shifts the anchor: 3s after resume == 5s of display time.
        playback.set_paused(false, t0 + Duration::from_secs(100));
        assert!(playback.finished(&item, t0 + Duration::from_secs(103)));
        assert!(!playback.finished(&item, t0 + Duration::from_secs(102)));
    }

    #[test]
    fn restart_resets_the_clock() {
        let item = playback_item(StoryViewerKind::Photo, 5, None);
        let t0 = Instant::now();
        let mut playback = StoryPlayback::default();
        playback.start(t0);
        let t1 = t0 + Duration::from_secs(3);
        playback.start(t1);
        assert_eq!(playback.elapsed(&item, t1), Duration::ZERO);
        assert!(!playback.finished(&item, t1 + Duration::from_secs(4)));
    }

    #[test]
    fn stop_clears_and_never_finishes() {
        let item = playback_item(StoryViewerKind::Photo, 5, None);
        let t0 = Instant::now();
        let mut playback = StoryPlayback::default();
        playback.start(t0);
        playback.stop();
        assert!(!playback.is_running());
        assert_eq!(
            playback.elapsed(&item, t0 + Duration::from_secs(30)),
            Duration::ZERO
        );
        assert!(!playback.finished(&item, t0 + Duration::from_secs(30)));
    }
}
