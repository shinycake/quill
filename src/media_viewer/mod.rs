//! Phase 4.5: fullscreen media viewer.
//!
//! Pure state machine (no GPUI): which chat media item is open and where it
//! sits in the chat's media list. The UI layer builds items from session
//! history, resolves display paths through the existing file/download
//! machinery (`usable_path` + `sandboxed_display_path`), and triggers
//! `downloadFile` when nothing viewable is local yet.
//!
//! Scope: photos, videos and animations (GIFs), and files (`files`): a
//! picture sent as a file shows like a photo, other files as a card.
//! Stickers, voice notes, and audio never open the viewer. Spoiler media opens once
//! revealed. Secret media is left out of the chat's list, so paging never
//! uncovers it; a photo or video with a self-destruct timer opens alone
//! when it is clicked (`timed_viewer_item`), as in Telegram Desktop.
#![deny(clippy::indexing_slicing)]

use crate::ids::{ChatId, FileId, MessageId};
use crate::state::HistoryMessage;
use crate::telegram::envelope::{MessageActions, MessageContent, PhotoContent, ServiceAction};
use crate::text::TextEntity;
use crate::voice::format_voice_duration;

mod boost;
mod controls;
mod files;
mod group_thumbs;
mod items;
pub mod pip;
mod profile;
mod quality;
mod save;
mod swipe;
mod transform;
mod ttl;
mod wheel;

pub use boost::{
    BOOST_STALE_MS, BoostPress, BoostRelease, BoostState, SPEED_BOOST_HOLD_MS, SPEED_BOOST_RATE,
    WAITING_SHOW_DELAY_MS, waiting_shown, waiting_wait,
};
pub use controls::{
    VIEWER_FADE_MS, VIEWER_HIDE_MS, VIEWER_SEEK_LONG_SECS, VIEWER_SEEK_SECS, VIEWER_SHOW_MS,
    VIEWER_WAIT_HIDE_MS, ViewerKeyAction, ViewerKeyMods, ViewerVideoStart, controls_fade_ms,
    controls_hide_wait_ms, controls_should_hide, decide_viewer_video_start, seek_target_secs,
    viewer_delete_gate, viewer_key_action,
};
pub use files::{
    PICTURE_FILE_AUTO_DOWNLOAD_MAX, READ_BYTES_LIMIT, collect_file_items, file_viewer_item,
    is_image_file, opens_as_picture, picture_format_label, shows_as_photo,
};
pub use group_thumbs::{
    GROUP_STRIP_PADDING, GROUP_THUMB_HEIGHT, GROUP_THUMB_SLIDE_MS, GROUP_THUMB_WIDTH_MAX,
    GroupStrip, GroupThumb, ThumbFrame, ViewerGroup, group_strip, strip_frames, thumb_file_ids,
};
pub use items::{
    collect_media_items, poll_option_viewer_item, profile_photo_items, timed_viewer_item,
};
pub use profile::{PhotoHeaderOwner, profile_photo_header};
pub use quality::{VideoQualityOption, build_quality_options, current_quality, pick_quality};
pub use save::{
    downloads_dir, os_downloads_dir, save_media_to_downloads, save_media_to_downloads_in_dir,
    saved_note,
};
pub use swipe::{
    SwipeAction, SwipeDir, SwipeIndicator, SwipeInput, SwipeNav, VIEWER_SWIPE_THRESHOLD_PX,
};
pub use transform::{
    VIEWER_MAX_ZOOM_LEVEL, VIEWER_WHEEL_MAX_NOTCHES, VIEWER_WHEEL_NOTCH_PX, VIEWER_ZOOM_MAX,
    VIEWER_ZOOM_MIN, VIEWER_ZOOM_STEP, ViewerOrientation, ViewerZoom, ZoomLevels, fit_within,
    flip_rgba, orient_rgba, pinch_zoom_factor, rotate_rgba_quarter_turns, wheel_zoom_factor,
    zoom_level_scale,
};
pub use ttl::{ViewerTtl, disappearing_label, seconds_tiny, viewer_ttl};
pub use wheel::{
    VIEWER_TRACKPAD_GESTURE_GAP, VIEWER_TRACKPAD_STEP_PX, WheelAction, WheelInput, WheelNav,
    WheelSource,
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
    /// A picture sent as a file: shown, zoomed, turned and saved like a
    /// photo, but listed with the chat's files (tdesktop "File 3 of 7").
    ImageFile,
    /// Any other file in that list: a card with its name, size and a
    /// download / open action (tdesktop's document bubble).
    File,
}

impl MediaViewerKind {
    pub fn label(&self) -> &'static str {
        match self {
            MediaViewerKind::Photo => "Photo",
            MediaViewerKind::Video => "Video",
            MediaViewerKind::Animation => "GIF",
            MediaViewerKind::ImageFile | MediaViewerKind::File => "File",
        }
    }

    /// A still picture: zoom, rotate, copy and save act on it.
    pub fn is_picture(self) -> bool {
        matches!(self, MediaViewerKind::Photo | MediaViewerKind::ImageFile)
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
    /// B10: a user's, group's or channel's photos, newest first
    /// (`state::PhotoGallery`). Items carry the `chatPhoto.id` as their
    /// message id and the group or channel as their chat (none for a
    /// user), so the message-bound actions (forward, show in chat, album
    /// pin) do not apply; delete, set as main and report follow
    /// `Session::photo_actions`.
    Profile,
    /// The picture or clip of one poll option. The item carries the poll's
    /// chat and message, but it is not a message of its own: forward, delete
    /// and the other message actions do not apply.
    Poll,
}

/// Items left before the loaded edge at which the viewer asks for the
/// next older Shared Media page.
pub const VIEWER_PRELOAD_AHEAD: usize = 6;

/// Profile photos the viewer keeps loaded past the current one: fewer and
/// it asks for the next page (tdesktop `kIdsLimit`, the slice it builds
/// around the open photo, which also feeds the thumbnail strip).
pub const VIEWER_PROFILE_PHOTOS_AROUND: usize = 48;

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
    /// Video and GIF: the file's MIME type, which tells an `image/gif`
    /// file from an MP4 GIF (`viewer_is_gif_file`). `None` for photos.
    pub mime_type: Option<String>,
    /// Video only: `messageVideo.start_timestamp`, the second the sender
    /// marked as the start (0 for GIFs). `None` for photos.
    pub start_timestamp: Option<i32>,
    pub caption: String,
    pub caption_entities: Vec<TextEntity>,
    /// Video duration (`0:12`), shown when no visual is local.
    pub duration_label: Option<String>,
    /// Natural pixel size (photo's largest size / video frame) when known:
    /// the viewer sizes the media to fit its frame from this.
    pub natural_size: Option<(i32, i32)>,
    /// Video only: the renditions the quality picker offers, tallest
    /// first. Empty when the video has a single quality.
    pub quality_options: Vec<VideoQualityOption>,
    /// Files only: the document's name (the card and the save name).
    pub file_name: Option<String>,
    /// The album the item belongs to, which shows as a thumbnail strip.
    pub group: Option<ViewerGroup>,
}

impl MediaViewerItem {
    /// A video profile photo: a photo with a clip, which plays muted in a
    /// loop over the still with no controls (tdesktop
    /// `PhotoData::videoCanBePlayed`, streamed with `options.loop`).
    pub fn is_video_photo(&self) -> bool {
        self.kind == MediaViewerKind::Photo && self.play_file_id.is_some()
    }

    /// Has a clip the viewer plays.
    pub fn plays_clip(&self) -> bool {
        self.kind.is_playable() || self.is_video_photo()
    }

    /// Its clip restarts by itself, muted, with no transport controls.
    pub fn loops(&self) -> bool {
        self.kind.loops() || self.is_video_photo()
    }
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
    /// The item the last step (arrow, wheel, swipe or thumb) left, which
    /// the album strip slides from.
    arrived_from: Option<usize>,
    /// Steps taken since opening, to key the strip's slide.
    steps: u64,
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

    /// B10: open over a user's, group's or channel's photos (newest
    /// first) on `index`. `total` counts the photos not loaded yet.
    pub fn open_profile(items: Vec<MediaViewerItem>, index: usize, total: usize) -> Self {
        let mut viewer = Self::open(items, index);
        viewer.source = ViewerSource::Profile;
        viewer.total = total;
        viewer
    }

    /// Profile photos: the viewer is close enough to the end of the loaded
    /// photos that the next page should be requested.
    pub fn wants_more_photos(&self) -> bool {
        self.is_open()
            && self.source == ViewerSource::Profile
            && self.index + VIEWER_PROFILE_PHOTOS_AROUND >= self.items.len()
            && self.total > self.items.len()
    }

    /// Profile photos: take the gallery as it now stands (newest first).
    /// When it starts with the photos already shown, the rest is appended
    /// and the total updated; any other change is left for the next open.
    /// Returns how many were added.
    pub fn extend_photos(&mut self, gallery: Vec<MediaViewerItem>, total: usize) -> usize {
        if self.source != ViewerSource::Profile || !self.is_open() {
            return 0;
        }
        let prefix = self.items.len() <= gallery.len()
            && self
                .items
                .iter()
                .zip(&gallery)
                .all(|(shown, now)| shown.message_id == now.message_id);
        if !prefix {
            return 0;
        }
        self.total = total;
        let added: Vec<MediaViewerItem> = gallery.into_iter().skip(self.items.len()).collect();
        let count = added.len();
        self.items.extend(added);
        count
    }

    /// Open on the media of one poll option.
    pub fn open_poll(item: MediaViewerItem) -> Self {
        let mut viewer = Self::open(vec![item], 0);
        viewer.source = ViewerSource::Poll;
        viewer
    }

    pub fn source(&self) -> ViewerSource {
        self.source
    }

    pub fn close(&mut self) {
        *self = Self::default();
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
        let Some(first) = self.items.first().map(|item| item.message_id) else {
            return 0;
        };
        let older: Vec<MediaViewerItem> = all_oldest_first
            .into_iter()
            .filter(|item| item.message_id < first)
            .collect();
        let added = older.len();
        if added > 0 {
            self.items.splice(0..0, older);
            self.index += added;
            self.arrived_from = self.arrived_from.map(|from| from + added);
        }
        added
    }

    pub fn prev(&mut self) {
        if let Some(index) = self.index.checked_sub(1) {
            self.go_to(index);
        }
    }

    pub fn next(&mut self) {
        self.go_to(self.index.saturating_add(1));
    }

    /// Step to the item at `index` (a thumb of the album strip, or a
    /// neighbour); out of range or the current one does nothing.
    pub fn go_to(&mut self, index: usize) {
        if index < self.items.len() && index != self.index {
            self.arrived_from = Some(self.index);
            self.index = index;
            self.steps = self.steps.wrapping_add(1);
        }
    }

    /// The current item's album strip laid out in `available` points
    /// (`None` outside an album or for an item alone in it).
    pub fn group_strip(&self, available: f32) -> Option<GroupStrip> {
        group_strip(&self.items, self.index, available)
    }

    /// The strip as it stood on the item the last step left, which the
    /// current strip slides from (`None` before any step).
    pub fn group_strip_before(&self, available: f32) -> Option<GroupStrip> {
        group_strip(&self.items, self.arrived_from?, available)
    }

    /// The files the strip laid out in `available` points still has to
    /// fetch: the small size of each profile photo it shows (tdesktop's
    /// `GroupThumbs::Thumb` asks for `PhotoSize::Thumbnail`). An album's
    /// thumbs draw what its chat already loaded, so it asks for none.
    pub fn strip_downloads(&self, available: f32) -> Vec<FileId> {
        let Some(strip) = self.group_strip(available) else {
            return Vec::new();
        };
        if strip.group != ViewerGroup::ProfilePhotos {
            return Vec::new();
        }
        strip
            .thumbs
            .iter()
            .filter_map(|thumb| self.items.get(strip.start.saturating_add(thumb.position)))
            .filter_map(|item| thumb_file_ids(item).first().copied())
            .filter(|id| id.0 != 0)
            .collect()
    }

    /// Changes with every step, to key the strip's slide.
    pub fn steps(&self) -> u64 {
        self.steps
    }

    pub fn current(&self) -> Option<&MediaViewerItem> {
        self.items.get(self.index)
    }

    /// The items paged over, oldest first.
    pub fn items(&self) -> &[MediaViewerItem] {
        &self.items
    }

    /// Which item of its message the current one is (0 for a single item;
    /// the album position in a bought paid-media album).
    pub fn current_offset(&self) -> usize {
        let Some(current) = self.current() else {
            return 0;
        };
        self.items.get(..self.index).map_or(0, |before| {
            before
                .iter()
                .rev()
                .take_while(|item| item.message_id == current.message_id)
                .count()
        })
    }

    /// 1-based `(position, total)` for the "Photo 2 of 5" header. Over a
    /// Shared Media list the total is the tab's count and the position
    /// counts the older items that are not loaded yet.
    pub fn position(&self) -> Option<(usize, usize)> {
        let total = self.total.max(self.items.len());
        // Shared Media lists oldest first, so the unloaded items come
        // before; profile photos list newest first, so they come after.
        let unloaded = match self.source {
            ViewerSource::Profile => 0,
            _ => total - self.items.len(),
        };
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
        self.arrived_from = None;
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
            animation: None,
        };
        let photos = [photo(901, 11, 12, 800, 600), photo(902, 13, 14, 0, 0)];
        let items = profile_photo_items(&photos, ChatId(0));
        assert_eq!(items[0].message_id, MessageId(901));
        assert_eq!(items[0].download_file_id, FileId(12));
        assert_eq!(items[0].display_file_ids, vec![FileId(12), FileId(11)]);
        assert_eq!(items[0].natural_size, Some((800, 600)));
        assert_eq!(items[1].natural_size, None);
        let mut viewer = MediaViewer::open_profile(items, 0, 2);
        assert_eq!(viewer.source(), ViewerSource::Profile);
        assert_eq!(viewer.position(), Some((1, 2)));
        assert!(!viewer.wants_older());
        assert!(!viewer.wants_more_photos());
        viewer.next();
        assert_eq!(viewer.current().map(|i| i.message_id), Some(MessageId(902)));
        viewer.close();
        assert_eq!(viewer.source(), ViewerSource::Chat);
    }

    #[test]
    fn a_video_photo_loops_its_clip_over_the_still() {
        let photo = |id, animation| crate::state::ProfilePhoto {
            id,
            added_date: 0,
            thumb_file_id: 11,
            full_file_id: 12,
            width: 640,
            height: 640,
            animation,
        };
        let clip = crate::telegram::envelope::ChatPhotoAnimation {
            file_id: FileId(19),
            start_ms: 1500,
            small_file_id: None,
        };
        let items = profile_photo_items(&[photo(1, Some(clip)), photo(2, None)], ChatId(0));
        let (video, still) = (&items[0], &items[1]);
        assert_eq!(video.kind, MediaViewerKind::Photo);
        assert_eq!(video.play_file_id, Some(FileId(19)));
        assert_eq!(video.display_file_ids, vec![FileId(12), FileId(11)]);
        assert!(video.is_video_photo() && video.plays_clip() && video.loops());
        assert_eq!(
            controls::decide_viewer_video_start(video, false),
            controls::ViewerVideoStart::ParkDownload
        );
        assert!(!still.is_video_photo() && !still.plays_clip() && !still.loops());
        assert_eq!(
            controls::decide_viewer_video_start(still, true),
            controls::ViewerVideoStart::Nothing
        );
    }

    fn profile_items(ids: std::ops::Range<i64>) -> Vec<MediaViewerItem> {
        let photos: Vec<crate::state::ProfilePhoto> = ids
            .map(|id| crate::state::ProfilePhoto {
                id,
                added_date: 0,
                thumb_file_id: 1,
                full_file_id: 2,
                width: 0,
                height: 0,
                animation: None,
            })
            .collect();
        profile_photo_items(&photos, ChatId(-100))
    }

    #[test]
    fn profile_viewer_counts_the_unloaded_photos_after_the_loaded_ones() {
        // 140 photos, the newest 100 loaded.
        let mut viewer = MediaViewer::open_profile(profile_items(0..100), 0, 140);
        assert_eq!(viewer.position(), Some((1, 140)));
        assert!(!viewer.wants_more_photos(), "far from the loaded end");
        for _ in 0..(100 - VIEWER_PROFILE_PHOTOS_AROUND - 1) {
            viewer.next();
        }
        assert!(!viewer.wants_more_photos(), "48 still loaded past it");
        viewer.next();
        assert_eq!(
            viewer.position(),
            Some((101 - VIEWER_PROFILE_PHOTOS_AROUND, 140))
        );
        assert!(viewer.wants_more_photos(), "within the preload margin");
        // The next page lands: appended after the shown ones.
        assert_eq!(viewer.extend_photos(profile_items(0..140), 140), 40);
        assert_eq!(viewer.len(), 140);
        assert_eq!(
            viewer.position(),
            Some((101 - VIEWER_PROFILE_PHOTOS_AROUND, 140))
        );
        assert!(!viewer.wants_more_photos(), "everything is loaded");
        assert_eq!(viewer.current().map(|i| i.chat_id), Some(ChatId(-100)));
    }

    #[test]
    fn profile_viewer_ignores_a_gallery_that_changed_its_start() {
        let mut viewer = MediaViewer::open_profile(profile_items(0..3), 1, 5);
        assert_eq!(viewer.extend_photos(profile_items(1..6), 5), 0);
        assert_eq!(viewer.len(), 3);
        assert_eq!(viewer.position(), Some((2, 5)));
        // A shared media viewer never takes profile pages.
        let mut shared = MediaViewer::open_shared(profile_items(0..3), 0, 9);
        assert_eq!(shared.extend_photos(profile_items(0..6), 6), 0);
        assert!(!shared.wants_more_photos());
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
