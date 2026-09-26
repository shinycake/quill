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
        }
    }

    fn item(kind: MediaViewerKind, message_id: i64) -> MediaViewerItem {
        MediaViewerItem {
            chat_id: ChatId(7),
            message_id: MessageId(message_id),
            kind,
            display_file_ids: vec![FileId(1)],
            download_file_id: FileId(1),
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
    fn video_without_thumb_downloads_video_file() {
        let messages = vec![video_message(7, 2, 50, None)];
        let items = collect_media_items(&messages);
        assert_eq!(items.len(), 1);
        assert!(items[0].display_file_ids.is_empty());
        assert_eq!(items[0].download_file_id, FileId(50));
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
