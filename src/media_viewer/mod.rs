//! Phase 4.5: fullscreen media viewer.
//!
//! Pure state machine (no GPUI): which chat media item is open and where it
//! sits in the chat's media list. The UI layer builds items from session
//! history, resolves display paths through the existing file/download
//! machinery (`usable_path` + `sandboxed_display_path`), and triggers
//! `downloadFile` when nothing viewable is local yet.
//!
//! Scope: photos, videos and animations (GIFs). Documents, stickers,
//! voice notes, and audio never open the viewer. Secret and spoiler media are
//! excluded too — the viewer is a full-bleed surface and must not bypass
//! their hidden-until-revealed contract.

use crate::ids::{ChatId, FileId, MessageId};
use crate::state::HistoryMessage;
use crate::telegram::envelope::{MessageActions, MessageContent, PhotoContent, ServiceAction};
use crate::text::TextEntity;
use crate::voice::format_voice_duration;

mod controls;
mod items;
mod save;
mod transform;

pub use controls::{
    VIEWER_FADE_MS, VIEWER_SEEK_LONG_SECS, VIEWER_SEEK_SECS, VIEWER_SHOW_MS, VIEWER_WAIT_HIDE_MS,
    ViewerKeyAction, ViewerKeyMods, ViewerVideoStart, controls_hide_wait_ms, controls_should_hide,
    decide_viewer_video_start, seek_target_secs, viewer_delete_gate, viewer_key_action,
};
pub use items::{collect_media_items, profile_photo_items};
pub use save::{
    downloads_dir, os_downloads_dir, save_media_to_downloads, save_media_to_downloads_in_dir,
    saved_note,
};
pub use transform::{
    VIEWER_WHEEL_MAX_NOTCHES, VIEWER_WHEEL_NOTCH_PX, VIEWER_ZOOM_MAX, VIEWER_ZOOM_MIN,
    VIEWER_ZOOM_STEP, ViewerOrientation, ViewerZoom, fit_within, flip_rgba, orient_rgba,
    rotate_rgba_quarter_turns, wheel_zoom_factor,
};

/// Media kinds the viewer opens this slice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaViewerKind {
    Photo,
    Video,
    /// A GIF / MPEG4 animation: plays in a loop without sound or controls
    /// (tdesktop streams it with `options.loop` and no playback controls,
    /// media_view_overlay_widget.cpp:5614 and :1491).
    Animation,
}

impl MediaViewerKind {
    pub fn label(&self) -> &'static str {
        match self {
            MediaViewerKind::Photo => "Photo",
            MediaViewerKind::Video => "Video",
            MediaViewerKind::Animation => "GIF",
        }
    }

    /// Has a clip to play (video or animation).
    pub fn is_playable(self) -> bool {
        matches!(self, MediaViewerKind::Video | MediaViewerKind::Animation)
    }

    /// Restarts by itself at the end and has no transport controls.
    pub fn loops(self) -> bool {
        self == MediaViewerKind::Animation
    }
}

/// Where the viewer's item list comes from.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ViewerSource {
    /// The open chat's loaded history.
    #[default]
    Chat,
    /// A Shared Media tab (tdesktop `SharedMediaWithLastSlice`): the list
    /// grows toward older messages as the viewer pages toward its start.
    SharedMedia,
    /// B10: one user's profile photos (`getUserProfilePhotos`), newest
    /// first. Items carry the `chatPhoto.id` as their message id and no
    /// chat, so the message-bound actions (forward, delete, show in chat,
    /// album pin) do not apply.
    Profile,
}

/// Items left before the loaded edge at which the viewer asks for the
/// next older Shared Media page.
pub const VIEWER_PRELOAD_AHEAD: usize = 6;

/// One openable media item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaViewerItem {
    pub chat_id: ChatId,
    pub message_id: MessageId,
    pub kind: MediaViewerKind,
    /// Display candidates, most preferred first. Photo: largest size, thumb,
    /// then any other size. Video: the `video.thumbnail` file only — the
    /// full clip is not renderable by the image element, so it is never a
    /// display candidate.
    pub display_file_ids: Vec<FileId>,
    /// File to `downloadFile` when no display candidate is local yet. Photo:
    /// the largest size (`open_file_id`). Video: the thumbnail when the
    /// sender attached one, else the video file itself (so it lands local
    /// for the history row's Play path).
    pub download_file_id: FileId,
    /// Video only: the full clip (`video.video`) for in-viewer playback.
    /// `None` for photos. Downloading this is separate from
    /// `download_file_id` — the thumbnail can be local while the clip is not.
    pub play_file_id: Option<FileId>,
    /// Video only: `video.duration` in whole seconds, driving the playback
    /// clock and the elapsed/total label. `None` for photos.
    pub duration_secs: Option<i32>,
    /// Video only: `video.mime_type`, for frame-extraction validation.
    /// `None` for photos.
    pub mime_type: Option<String>,
    /// Video only: `messageVideo.start_timestamp` — seconds to seek before
    /// extracting playback frames. `None` for photos.
    pub start_timestamp: Option<i32>,
    pub caption: String,
    pub caption_entities: Vec<TextEntity>,
    /// Video duration (`0:12`), shown when no visual is local.
    pub duration_label: Option<String>,
    /// Natural pixel size (photo's largest size / video frame) when known:
    /// the viewer sizes the media to fit its frame from this.
    pub natural_size: Option<(i32, i32)>,
}

/// Viewer state: the open chat's media list plus the current position.
/// Empty items = closed.
#[derive(Debug, Clone, Default)]
pub struct MediaViewer {
    items: Vec<MediaViewerItem>,
    index: usize,
    source: ViewerSource,
    /// Shared Media only: the tab's total count, so the header can say
    /// "Photo 120 of 340" while only a prefix of the list is loaded.
    total: usize,
}

impl MediaViewer {
    pub fn closed() -> Self {
        Self::default()
    }

    pub fn is_open(&self) -> bool {
        !self.items.is_empty()
    }

    /// Open on `index`, clamped into range. An empty list stays closed.
    pub fn open(items: Vec<MediaViewerItem>, index: usize) -> Self {
        let index = index.min(items.len().saturating_sub(1));
        Self {
            items,
            index,
            ..Self::default()
        }
    }

    /// Open over a Shared Media list (oldest first). `total` is the tab's
    /// full count (items older than the loaded prefix are not listed yet).
    pub fn open_shared(items: Vec<MediaViewerItem>, index: usize, total: usize) -> Self {
        let mut viewer = Self::open(items, index);
        viewer.source = ViewerSource::SharedMedia;
        viewer.total = total;
        viewer
    }

    /// B10: open over a user's profile photos on `index`.
    pub fn open_profile(items: Vec<MediaViewerItem>, index: usize) -> Self {
        let mut viewer = Self::open(items, index);
        viewer.source = ViewerSource::Profile;
        viewer
    }

    pub fn source(&self) -> ViewerSource {
        self.source
    }

    pub fn close(&mut self) {
        self.items.clear();
        self.index = 0;
        self.source = ViewerSource::Chat;
        self.total = 0;
    }

    /// Whether the previous / next arrow has an item to go to.
    pub fn has_prev(&self) -> bool {
        self.is_open() && self.index > 0
    }

    pub fn has_next(&self) -> bool {
        self.is_open() && self.index + 1 < self.items.len()
    }

    /// Shared Media: the viewer is close enough to the start of the loaded
    /// list that the next older page should be requested (it is safe to
    /// ask repeatedly; the state layer drops a request while one is in
    /// flight or the list is complete).
    pub fn wants_older(&self) -> bool {
        self.is_open()
            && self.source == ViewerSource::SharedMedia
            && self.index < VIEWER_PRELOAD_AHEAD
            && self.total > self.items.len()
    }

    /// Shared Media: merge the tab's loaded list (oldest first, possibly
    /// longer than the viewer's) in. Only items older than the current
    /// first one are taken, in front, and the position follows the item
    /// the user is on. Returns how many were added.
    pub fn merge_older(&mut self, all_oldest_first: Vec<MediaViewerItem>, total: usize) -> usize {
        if self.source != ViewerSource::SharedMedia || !self.is_open() {
            return 0;
        }
        self.total = total.max(self.items.len());
        let first = self.items[0].message_id;
        let older: Vec<MediaViewerItem> = all_oldest_first
            .into_iter()
            .filter(|item| item.message_id < first)
            .collect();
        let added = older.len();
        if added > 0 {
            self.items.splice(0..0, older);
            self.index += added;
        }
        added
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

    pub fn current(&self) -> Option<&MediaViewerItem> {
        self.items.get(self.index)
    }

    /// 1-based `(position, total)` for the "Photo 2 of 5" header. Over a
    /// Shared Media list the total is the tab's count and the position
    /// counts the older items that are not loaded yet.
    pub fn position(&self) -> Option<(usize, usize)> {
        let total = self.total.max(self.items.len());
        let unloaded = total - self.items.len();
        self.is_open().then(|| (unloaded + self.index + 1, total))
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Drop items whose message is gone (deleted from the viewer or
    /// elsewhere). The current item stays current while it survives;
    /// otherwise the viewer lands on the item that followed it (the new
    /// last item when it was the last), and an emptied list closes the
    /// viewer. Returns `true` when the current item changed or the viewer
    /// closed, so the caller can reset per-item state.
    pub fn retain(&mut self, mut keep: impl FnMut(&MediaViewerItem) -> bool) -> bool {
        if self.items.is_empty() {
            return false;
        }
        let current = self.items.get(self.index).map(|item| item.message_id);
        let before = self.items.len();
        let index = self.index;
        let mut removed_before = 0;
        let mut position = 0;
        self.items.retain(|item| {
            let kept = keep(item);
            if !kept && position < index {
                removed_before += 1;
            }
            position += 1;
            kept
        });
        if self.items.len() == before {
            return false;
        }
        if self.items.is_empty() {
            self.index = 0;
            return true;
        }
        self.index = (self.index - removed_before).min(self.items.len() - 1);
        self.items.get(self.index).map(|item| item.message_id) != current
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

#[cfg(test)]
mod test_support;

#[cfg(test)]
mod tests {
    use super::test_support::item;
    use super::*;

    #[test]
    fn closed_has_no_current_or_position() {
        let viewer = MediaViewer::closed();
        assert!(!viewer.is_open());
        assert_eq!(viewer.current(), None);
        assert_eq!(viewer.position(), None);
        assert_eq!(viewer.len(), 0);
    }

    #[test]
    fn open_clamps_index_and_reports_position() {
        let items = vec![
            item(MediaViewerKind::Photo, 10),
            item(MediaViewerKind::Video, 11),
        ];
        let viewer = MediaViewer::open(items, 9);
        assert!(viewer.is_open());
        assert_eq!(viewer.current().unwrap().message_id, MessageId(11));
        assert_eq!(viewer.position(), Some((2, 2)));
    }

    #[test]
    fn open_empty_stays_closed() {
        let viewer = MediaViewer::open(Vec::new(), 0);
        assert!(!viewer.is_open());
    }

    #[test]
    fn prev_next_walk_and_clamp_at_ends() {
        let items = vec![
            item(MediaViewerKind::Photo, 10),
            item(MediaViewerKind::Photo, 11),
            item(MediaViewerKind::Photo, 12),
        ];
        let mut viewer = MediaViewer::open(items, 1);
        viewer.prev();
        assert_eq!(viewer.current().unwrap().message_id, MessageId(10));
        assert_eq!(viewer.position(), Some((1, 3)));
        viewer.prev();
        assert_eq!(viewer.current().unwrap().message_id, MessageId(10));
        viewer.next();
        viewer.next();
        assert_eq!(viewer.current().unwrap().message_id, MessageId(12));
        viewer.next();
        assert_eq!(viewer.current().unwrap().message_id, MessageId(12));
    }

    #[test]
    fn close_resets_state() {
        let mut viewer = MediaViewer::open(vec![item(MediaViewerKind::Photo, 10)], 0);
        viewer.close();
        assert!(!viewer.is_open());
        assert_eq!(viewer.current(), None);
        viewer.prev();
        viewer.next();
        assert!(!viewer.is_open());
    }

    #[test]
    fn retain_moves_to_next_item_or_closes() {
        let mk = |ids: &[i64]| {
            ids.iter()
                .map(|id| item(MediaViewerKind::Photo, *id))
                .collect::<Vec<_>>()
        };
        // Current (middle) deleted: lands on the item that followed.
        let mut viewer = MediaViewer::open(mk(&[1, 2, 3]), 1);
        assert!(viewer.retain(|i| i.message_id.0 != 2));
        assert_eq!(viewer.current().unwrap().message_id.0, 3);
        // Last deleted: lands on the new last.
        let mut viewer = MediaViewer::open(mk(&[1, 2, 3]), 2);
        assert!(viewer.retain(|i| i.message_id.0 != 3));
        assert_eq!(viewer.current().unwrap().message_id.0, 2);
        // An earlier item deleted: current stays, no reset needed.
        let mut viewer = MediaViewer::open(mk(&[1, 2, 3]), 2);
        assert!(!viewer.retain(|i| i.message_id.0 != 1));
        assert_eq!(viewer.current().unwrap().message_id.0, 3);
        assert_eq!(viewer.position(), Some((2, 2)));
        // Nothing removed.
        assert!(!viewer.retain(|_| true));
        // All gone: closed.
        assert!(viewer.retain(|_| false));
        assert!(!viewer.is_open());
    }

    fn shared_items(ids: &[i64]) -> Vec<MediaViewerItem> {
        ids.iter()
            .map(|id| item(MediaViewerKind::Photo, *id))
            .collect()
    }

    #[test]
    fn profile_viewer_pages_over_photos_and_keeps_the_photo_id() {
        let photo = |id, thumb, full, width, height| crate::state::ProfilePhoto {
            id,
            added_date: 0,
            thumb_file_id: thumb,
            full_file_id: full,
            width,
            height,
        };
        let items = profile_photo_items(&[photo(901, 11, 12, 800, 600), photo(902, 13, 14, 0, 0)]);
        assert_eq!(items[0].message_id, MessageId(901));
        assert_eq!(items[0].download_file_id, FileId(12));
        assert_eq!(items[0].display_file_ids, vec![FileId(12), FileId(11)]);
        assert_eq!(items[0].natural_size, Some((800, 600)));
        assert_eq!(items[1].natural_size, None);
        let mut viewer = MediaViewer::open_profile(items, 0);
        assert_eq!(viewer.source(), ViewerSource::Profile);
        assert_eq!(viewer.position(), Some((1, 2)));
        assert!(!viewer.wants_older());
        viewer.next();
        assert_eq!(viewer.current().map(|i| i.message_id), Some(MessageId(902)));
        viewer.close();
        assert_eq!(viewer.source(), ViewerSource::Chat);
    }

    #[test]
    fn shared_media_viewer_counts_unloaded_older_items() {
        // 340 items in the tab, the newest 4 loaded (oldest first).
        let viewer = MediaViewer::open_shared(shared_items(&[97, 98, 99, 100]), 3, 340);
        assert_eq!(viewer.source(), ViewerSource::SharedMedia);
        assert_eq!(viewer.position(), Some((340, 340)));
        let mut viewer = viewer;
        viewer.prev();
        assert_eq!(viewer.position(), Some((339, 340)));
        // A chat viewer counts only what it holds.
        let chat = MediaViewer::open(shared_items(&[1, 2, 3]), 1);
        assert_eq!(chat.source(), ViewerSource::Chat);
        assert_eq!(chat.position(), Some((2, 3)));
    }

    #[test]
    fn shared_media_viewer_asks_for_older_pages_near_the_start() {
        let ids: Vec<i64> = (50..100).collect();
        let mut viewer = MediaViewer::open_shared(shared_items(&ids), 49, 200);
        assert!(!viewer.wants_older(), "far from the loaded start");
        for _ in 0..(49 - VIEWER_PRELOAD_AHEAD + 1) {
            viewer.prev();
        }
        assert!(viewer.wants_older(), "within the preload margin");
        // Everything loaded: nothing to ask for.
        let all = MediaViewer::open_shared(shared_items(&ids), 0, ids.len());
        assert!(!all.wants_older());
        // A chat viewer never pages.
        let chat = MediaViewer::open(shared_items(&ids), 0);
        assert!(!chat.wants_older());
    }

    #[test]
    fn merge_older_prepends_and_keeps_the_current_item() {
        let mut viewer = MediaViewer::open_shared(shared_items(&[80, 90, 100]), 1, 6);
        assert_eq!(viewer.current().unwrap().message_id, MessageId(90));
        assert!(viewer.has_prev() && viewer.has_next());
        // The tab now holds three more, older than the viewer's first.
        let added = viewer.merge_older(shared_items(&[50, 60, 70, 80, 90, 100]), 6);
        assert_eq!(added, 3);
        assert_eq!(viewer.len(), 6);
        assert_eq!(viewer.current().unwrap().message_id, MessageId(90));
        assert_eq!(viewer.position(), Some((5, 6)));
        // Merging the same list again adds nothing.
        assert_eq!(
            viewer.merge_older(shared_items(&[50, 60, 70, 80, 90, 100]), 6),
            0
        );
        // The user can now page back to the new items.
        viewer.prev();
        viewer.prev();
        viewer.prev();
        assert_eq!(viewer.current().unwrap().message_id, MessageId(60));
        assert!(viewer.has_prev());
        viewer.prev();
        assert!(!viewer.has_prev());
        // A chat viewer ignores merges.
        let mut chat = MediaViewer::open(shared_items(&[80, 90]), 0);
        assert_eq!(chat.merge_older(shared_items(&[1, 2, 80, 90]), 4), 0);
        assert_eq!(chat.len(), 2);
    }

    #[test]
    fn closing_the_viewer_forgets_its_source() {
        let mut viewer = MediaViewer::open_shared(shared_items(&[1, 2]), 0, 9);
        viewer.close();
        assert_eq!(viewer.source(), ViewerSource::Chat);
        assert_eq!(viewer.position(), None);
    }
}
