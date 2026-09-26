//! Phase 4.5: fullscreen media viewer.
//!
//! Pure state machine (no GPUI): which chat media item is open and where it
//! sits in the chat's media list. The UI layer builds items from session
//! history, resolves display paths through the existing file/download
//! machinery (`usable_path` + `sandboxed_display_path`), and triggers
//! `downloadFile` when nothing viewable is local yet.
//!
//! Scope: photos and videos only. Documents, animations (GIFs), stickers,
//! voice notes, and audio never open the viewer. Secret and spoiler media are
//! excluded too — the viewer is a full-bleed surface and must not bypass
//! their hidden-until-revealed contract.

use crate::ids::{ChatId, FileId, MessageId};
use crate::state::HistoryMessage;
use crate::telegram::envelope::MessageContent;
use crate::text::TextEntity;
use crate::voice::format_voice_duration;

/// Media kinds the viewer opens this slice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaViewerKind {
    Photo,
    Video,
}

impl MediaViewerKind {
    pub fn label(&self) -> &'static str {
        match self {
            MediaViewerKind::Photo => "Photo",
            MediaViewerKind::Video => "Video",
        }
    }
}

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
}

/// Viewer state: the open chat's media list plus the current position.
/// Empty items = closed.
#[derive(Debug, Clone, Default)]
pub struct MediaViewer {
    items: Vec<MediaViewerItem>,
    index: usize,
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

    pub fn current(&self) -> Option<&MediaViewerItem> {
        self.items.get(self.index)
    }

    /// 1-based `(position, total)` for the "Photo 2 of 5" header.
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

/// Parity slice 5: viewer video start decision (pure, testable).
///
/// `clip_local` is whether the clip file (`play_file_id`) is downloaded;
/// `frames_ready` is whether decoded frames for this file are already
/// cached. The UI layer (`maybe_autoplay_viewer_video`,
/// `resume_pending_viewer_video`, `toggle_viewer_video`) routes through
/// this so every start path agrees: a local clip without cached frames
/// must go through extraction, never straight to playback with an empty
/// frame cache.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewerVideoStart {
    /// Clip local and frames cached: start playback immediately.
    PlayNow,
    /// Clip local but no cached frames: extract frames (async) first.
    ExtractFrames,
    /// Clip not downloaded: park the request, download, resume later.
    ParkDownload,
    /// Not a video item, or no playable file: do nothing.
    Nothing,
}

pub fn decide_viewer_video_start(
    item: &MediaViewerItem,
    clip_local: bool,
    frames_ready: bool,
) -> ViewerVideoStart {
    if item.kind != MediaViewerKind::Video || item.play_file_id.is_none() {
        return ViewerVideoStart::Nothing;
    }
    if !clip_local {
        return ViewerVideoStart::ParkDownload;
    }
    if frames_ready {
        ViewerVideoStart::PlayNow
    } else {
        ViewerVideoStart::ExtractFrames
    }
}

/// Zoom/pan state for the viewer visual (pure, no GPUI). Zoom is a fit-scale
/// factor (`1.0` = contain); pan is the visual's top-left offset in px at the
/// current zoom, clamped so the image can never leave the frame entirely.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewerZoom {
    pub zoom: f32,
    pub pan: (f32, f32),
}

/// Wheel-zoom step factor and zoom limits.
pub const VIEWER_ZOOM_STEP: f32 = 1.15;
pub const VIEWER_ZOOM_MIN: f32 = 1.0;
pub const VIEWER_ZOOM_MAX: f32 = 8.0;

impl Default for ViewerZoom {
    fn default() -> Self {
        Self {
            zoom: VIEWER_ZOOM_MIN,
            pan: (0.0, 0.0),
        }
    }
}

impl ViewerZoom {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// `true` when the visual is magnified (pan is meaningful).
    pub fn is_zoomed(&self) -> bool {
        self.zoom > VIEWER_ZOOM_MIN
    }

    /// Step the zoom in (`zoom_in = true`) or out, keeping the frame center
    /// fixed. `frame` is the `(width, height)` of the visual container.
    pub fn step(&mut self, zoom_in: bool, frame: (f32, f32)) {
        let old = self.zoom;
        let new = if zoom_in {
            old * VIEWER_ZOOM_STEP
        } else {
            old / VIEWER_ZOOM_STEP
        }
        .clamp(VIEWER_ZOOM_MIN, VIEWER_ZOOM_MAX);
        if (new - old).abs() < f32::EPSILON {
            return;
        }
        let ratio = new / old;
        let (cx, cy) = (frame.0 / 2.0, frame.1 / 2.0);
        self.zoom = new;
        self.pan = (
            cx - (cx - self.pan.0) * ratio,
            cy - (cy - self.pan.1) * ratio,
        );
        self.clamp_pan(frame);
    }

    /// Drag-pan by a mouse delta in px. Ignored at fit zoom (nothing to pan).
    pub fn pan_by(&mut self, dx: f32, dy: f32, frame: (f32, f32)) {
        if !self.is_zoomed() {
            return;
        }
        self.pan = (self.pan.0 + dx, self.pan.1 + dy);
        self.clamp_pan(frame);
    }

    fn clamp_pan(&mut self, frame: (f32, f32)) {
        if !self.is_zoomed() {
            self.pan = (0.0, 0.0);
            return;
        }
        let (min_x, min_y) = (frame.0 - frame.0 * self.zoom, frame.1 - frame.1 * self.zoom);
        self.pan = (self.pan.0.clamp(min_x, 0.0), self.pan.1.clamp(min_y, 0.0));
    }
}

/// Collect the openable photo/video messages from a chat's ordered history,
/// oldest first. Documents, animations, stickers, voice, audio, and
/// secret/spoiler media are excluded (see module docs).
pub fn collect_media_items(messages: &[HistoryMessage]) -> Vec<MediaViewerItem> {
    messages.iter().filter_map(media_viewer_item).collect()
}

fn media_viewer_item(message: &HistoryMessage) -> Option<MediaViewerItem> {
    match &message.content {
        MessageContent::Photo(photo) if !photo.is_secret && !photo.has_spoiler => {
            let largest = photo.open_file_id()?;
            let mut display = vec![largest];
            if let Some(thumb) = photo.thumb_size()
                && thumb.file_id != largest
            {
                display.push(thumb.file_id);
            }
            for size in &photo.sizes {
                if !display.contains(&size.file_id) {
                    display.push(size.file_id);
                }
            }
            Some(MediaViewerItem {
                chat_id: message.chat_id,
                message_id: message.id,
                kind: MediaViewerKind::Photo,
                display_file_ids: display,
                download_file_id: largest,
                play_file_id: None,
                duration_secs: None,
                mime_type: None,
                start_timestamp: None,
                caption: photo.caption.clone(),
                caption_entities: photo.caption_entities.clone(),
                duration_label: None,
            })
        }
        MessageContent::Video(video) if !video.is_secret && !video.has_spoiler => {
            let thumb = video.thumb_file_id.filter(|id| id.0 != 0);
            let download = thumb.unwrap_or(video.file_id);
            if download.0 == 0 {
                return None;
            }
            Some(MediaViewerItem {
                chat_id: message.chat_id,
                message_id: message.id,
                kind: MediaViewerKind::Video,
                display_file_ids: thumb.into_iter().collect(),
                download_file_id: download,
                play_file_id: video.play_file_id(),
                duration_secs: Some(video.duration.max(0)),
                mime_type: Some(video.mime_type.clone()),
                start_timestamp: Some(video.start_timestamp),
                caption: video.caption.clone(),
                caption_entities: video.caption_entities.clone(),
                duration_label: Some(format_voice_duration(video.duration)),
            })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::telegram::envelope::{PhotoContent, PhotoSizeView, TextContent, VideoContent};

    fn photo_message(
        chat: i64,
        id: i64,
        sizes: Vec<(i32, i32, i32)>,
        caption: &str,
    ) -> HistoryMessage {
        test_message(
            chat,
            id,
            MessageContent::Photo(PhotoContent {
                caption: caption.to_string(),
                caption_entities: Vec::new(),
                sizes: sizes
                    .into_iter()
                    .map(|(file_id, width, height)| PhotoSizeView {
                        type_name: "x".to_string(),
                        width,
                        height,
                        file_id: FileId(file_id),
                    })
                    .collect(),
                is_secret: false,
                has_spoiler: false,
            }),
        )
    }

    fn video_message(chat: i64, id: i64, file_id: i32, thumb: Option<i32>) -> HistoryMessage {
        test_message(
            chat,
            id,
            MessageContent::Video(VideoContent {
                duration: 72,
                width: 640,
                height: 480,
                file_name: "clip.mp4".to_string(),
                mime_type: "video/mp4".to_string(),
                caption: String::new(),
                caption_entities: Vec::new(),
                show_caption_above_media: false,
                has_spoiler: false,
                is_secret: false,
                start_timestamp: 0,
                supports_streaming: false,
                has_stickers: false,
                file_id: FileId(file_id),
                thumb_file_id: thumb.map(FileId),
                thumb_width: 320,
                thumb_height: 240,
            }),
        )
    }

    /// `SearchMessageHit` construction in `media_viewer.rs` (test fixture).
    fn test_message(chat: i64, id: i64, content: MessageContent) -> HistoryMessage {
        HistoryMessage {
            id: MessageId(id),
            chat_id: ChatId(chat),
            is_outgoing: false,
            content,
            pending: false,
            reply_to: None,
            forward_info: None,
            interaction_info: None,
            is_pinned: false,
            media_album_id: 0,
            reply_markup: None,
            self_destruct: None,
        }
    }

    fn item(kind: MediaViewerKind, message_id: i64) -> MediaViewerItem {
        MediaViewerItem {
            chat_id: ChatId(7),
            message_id: MessageId(message_id),
            kind,
            display_file_ids: vec![FileId(1)],
            download_file_id: FileId(1),
            play_file_id: None,
            duration_secs: None,
            mime_type: None,
            start_timestamp: None,
            caption: String::new(),
            caption_entities: Vec::new(),
            duration_label: None,
        }
    }

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

    fn video_item_with_clip() -> MediaViewerItem {
        let mut item = item(MediaViewerKind::Video, 20);
        item.play_file_id = Some(FileId(96));
        item.duration_secs = Some(12);
        item.mime_type = Some("video/mp4".to_string());
        item.start_timestamp = Some(0);
        item
    }

    #[test]
    fn decide_start_downloaded_clip_without_frames_extracts() {
        // Blocking-1 regression: a clip that just finished downloading has
        // no cached frames yet — playback must go through extraction, never
        // straight to play with an empty frame cache.
        let item = video_item_with_clip();
        assert_eq!(
            decide_viewer_video_start(&item, true, false),
            ViewerVideoStart::ExtractFrames
        );
    }

    #[test]
    fn decide_start_cached_frames_play_now() {
        let item = video_item_with_clip();
        assert_eq!(
            decide_viewer_video_start(&item, true, true),
            ViewerVideoStart::PlayNow
        );
    }

    #[test]
    fn decide_start_undownloaded_clip_parks_for_download() {
        let item = video_item_with_clip();
        assert_eq!(
            decide_viewer_video_start(&item, false, false),
            ViewerVideoStart::ParkDownload
        );
        // Cached frames for another file don't help an undownloaded clip.
        assert_eq!(
            decide_viewer_video_start(&item, false, true),
            ViewerVideoStart::ParkDownload
        );
    }

    #[test]
    fn decide_start_photo_or_missing_clip_does_nothing() {
        let photo = item(MediaViewerKind::Photo, 10);
        assert_eq!(
            decide_viewer_video_start(&photo, true, false),
            ViewerVideoStart::Nothing
        );
        let no_clip = item(MediaViewerKind::Video, 21);
        assert_eq!(
            decide_viewer_video_start(&no_clip, true, false),
            ViewerVideoStart::Nothing
        );
    }

    #[test]
    fn collect_keeps_photos_and_videos_in_order() {
        let messages = vec![
            test_message(7, 1, MessageContent::Text(TextContent::plain("hello"))),
            video_message(7, 2, 50, Some(51)),
            photo_message(7, 3, vec![(60, 800, 600), (61, 320, 240)], "hi"),
            test_message(
                7,
                4,
                MessageContent::Unsupported {
                    type_name: "x".into(),
                },
            ),
        ];
        let items = collect_media_items(&messages);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].message_id, MessageId(2));
        assert_eq!(items[0].kind, MediaViewerKind::Video);
        assert_eq!(items[1].message_id, MessageId(3));
        assert_eq!(items[1].kind, MediaViewerKind::Photo);
    }

    #[test]
    fn photo_item_prefers_largest_and_downloads_it() {
        let messages = vec![photo_message(
            7,
            3,
            vec![(61, 320, 240), (60, 800, 600)],
            "cap",
        )];
        let items = collect_media_items(&messages);
        assert_eq!(items.len(), 1);
        let item = &items[0];
        assert_eq!(item.download_file_id, FileId(60));
        assert_eq!(item.display_file_ids[0], FileId(60));
        assert!(item.display_file_ids.contains(&FileId(61)));
        assert_eq!(item.caption, "cap");
        assert_eq!(item.duration_label, None);
    }

    #[test]
    fn video_item_downloads_thumb_when_present() {
        let messages = vec![video_message(7, 2, 50, Some(51))];
        let items = collect_media_items(&messages);
        assert_eq!(items.len(), 1);
        let item = &items[0];
        assert_eq!(item.display_file_ids, vec![FileId(51)]);
        assert_eq!(item.download_file_id, FileId(51));
        assert_eq!(item.duration_label.as_deref(), Some("1:12"));
    }

    #[test]
    fn video_item_carries_play_file_and_duration_secs() {
        let messages = vec![video_message(7, 2, 50, Some(51))];
        let items = collect_media_items(&messages);
        assert_eq!(items.len(), 1);
        let item = &items[0];
        assert_eq!(item.play_file_id, Some(FileId(50)));
        assert_eq!(item.duration_secs, Some(72));
        assert_eq!(item.mime_type.as_deref(), Some("video/mp4"));
        assert_eq!(item.start_timestamp, Some(0));
    }

    #[test]
    fn video_without_thumb_downloads_video_file() {
        let messages = vec![video_message(7, 2, 50, None)];
        let items = collect_media_items(&messages);
        assert_eq!(items.len(), 1);
        assert!(items[0].display_file_ids.is_empty());
        assert_eq!(items[0].download_file_id, FileId(50));
        assert_eq!(items[0].play_file_id, Some(FileId(50)));
    }

    #[test]
    fn photo_item_has_no_play_file_or_duration() {
        let messages = vec![photo_message(7, 3, vec![(60, 800, 600)], "")];
        let items = collect_media_items(&messages);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].play_file_id, None);
        assert_eq!(items[0].duration_secs, None);
        assert_eq!(items[0].mime_type, None);
        assert_eq!(items[0].start_timestamp, None);
    }

    #[test]
    fn zoom_defaults_to_fit_and_steps_around_center() {
        let mut zoom = ViewerZoom::new();
        assert!(!zoom.is_zoomed());
        zoom.step(true, (720.0, 480.0));
        assert!(zoom.is_zoomed());
        assert!((zoom.zoom - VIEWER_ZOOM_STEP).abs() < 1e-6);
        // Center stays fixed: pan moves the visual's top-left up-left.
        assert!((zoom.pan.0 - (360.0 - 360.0 * VIEWER_ZOOM_STEP)).abs() < 1e-3);
        assert!((zoom.pan.1 - (240.0 - 240.0 * VIEWER_ZOOM_STEP)).abs() < 1e-3);
    }

    #[test]
    fn zoom_clamps_to_min_and_max() {
        let mut zoom = ViewerZoom::new();
        zoom.step(false, (720.0, 480.0));
        assert_eq!(zoom.zoom, VIEWER_ZOOM_MIN);
        for _ in 0..40 {
            zoom.step(true, (720.0, 480.0));
        }
        assert_eq!(zoom.zoom, VIEWER_ZOOM_MAX);
        for _ in 0..40 {
            zoom.step(false, (720.0, 480.0));
        }
        assert_eq!(zoom.zoom, VIEWER_ZOOM_MIN);
        assert_eq!(zoom.pan, (0.0, 0.0));
    }

    #[test]
    fn pan_clamps_inside_frame_and_ignores_fit_zoom() {
        let mut zoom = ViewerZoom::new();
        zoom.pan_by(50.0, 50.0, (720.0, 480.0));
        assert_eq!(zoom.pan, (0.0, 0.0));
        zoom.step(true, (720.0, 480.0));
        zoom.pan_by(10_000.0, -10_000.0, (720.0, 480.0));
        assert_eq!(zoom.pan.0, 0.0);
        assert_eq!(zoom.pan.1, 480.0 - 480.0 * zoom.zoom);
        zoom.reset();
        assert!(!zoom.is_zoomed());
        assert_eq!(zoom.pan, (0.0, 0.0));
    }

    #[test]
    fn secret_and_spoiler_media_are_excluded() {
        let mut secret = photo_message(7, 5, vec![(70, 100, 100)], "");
        if let MessageContent::Photo(photo) = &mut secret.content {
            photo.is_secret = true;
        }
        let mut spoiler = video_message(7, 6, 71, Some(72));
        if let MessageContent::Video(video) = &mut spoiler.content {
            video.has_spoiler = true;
        }
        let items = collect_media_items(&[secret, spoiler]);
        assert!(items.is_empty());
    }

    #[test]
    fn document_never_opens_viewer() {
        let messages = vec![test_message(
            7,
            8,
            MessageContent::Document(crate::telegram::envelope::DocumentContent {
                file_name: "notes.txt".to_string(),
                mime_type: "text/plain".to_string(),
                caption: String::new(),
                caption_entities: Vec::new(),
                file_id: FileId(80),
            }),
        )];
        assert!(collect_media_items(&messages).is_empty());
    }
}
