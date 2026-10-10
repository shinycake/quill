//! Building viewer items from chat history and profile photos.

use super::*;

/// B10: viewer items for a profile photo gallery. The item's `message_id`
/// is the `chatPhoto.id` ("Set as main photo" reads it back); both sizes
/// are display candidates, the largest first.
pub fn profile_photo_items(photos: &[crate::state::ProfilePhoto]) -> Vec<MediaViewerItem> {
    photos
        .iter()
        .map(|photo| MediaViewerItem {
            chat_id: ChatId(0),
            message_id: MessageId(photo.id),
            kind: MediaViewerKind::Photo,
            display_file_ids: vec![FileId(photo.full_file_id), FileId(photo.thumb_file_id)],
            download_file_id: FileId(photo.full_file_id),
            play_file_id: None,
            duration_secs: None,
            mime_type: None,
            start_timestamp: None,
            caption: String::new(),
            caption_entities: Vec::new(),
            duration_label: None,
            natural_size: (photo.width > 0 && photo.height > 0)
                .then_some((photo.width, photo.height)),
        })
        .collect()
}

pub fn collect_media_items(messages: &[HistoryMessage]) -> Vec<MediaViewerItem> {
    messages.iter().filter_map(media_viewer_item).collect()
}

fn photo_viewer_item(message: &HistoryMessage, photo: &PhotoContent) -> Option<MediaViewerItem> {
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
        natural_size: photo
            .largest_size()
            .map(|size| (size.width, size.height))
            .filter(|(w, h)| *w > 0 && *h > 0),
    })
}

fn media_viewer_item(message: &HistoryMessage) -> Option<MediaViewerItem> {
    match &message.content {
        // Spoilers are viewable once revealed (the cover takes the click
        // before that), as in Telegram Desktop; secret media never.
        MessageContent::Photo(photo) if !photo.is_secret => photo_viewer_item(message, photo),
        // A chat-photo change or a suggested profile photo opens in the
        // viewer like any photo.
        MessageContent::Action(action) => match action.as_ref() {
            ServiceAction::ChatPhoto { photo: Some(photo) }
            | ServiceAction::SuggestProfilePhoto {
                photo: Some(photo), ..
            } => photo_viewer_item(message, photo),
            _ => None,
        },
        MessageContent::Video(video) if !video.is_secret => {
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
                natural_size: (video.width > 0 && video.height > 0)
                    .then_some((video.width, video.height)),
            })
        }
        // GIFs open the viewer too (secret ones never; spoilers once revealed).
        MessageContent::Animation(animation) if !animation.is_secret => {
            let thumb = animation.thumb_file_id();
            let download = thumb.unwrap_or(animation.file_id);
            if download.0 == 0 {
                return None;
            }
            Some(MediaViewerItem {
                chat_id: message.chat_id,
                message_id: message.id,
                kind: MediaViewerKind::Animation,
                display_file_ids: thumb.into_iter().collect(),
                download_file_id: download,
                play_file_id: animation.play_file_id(),
                duration_secs: Some(animation.duration.max(0)),
                mime_type: Some(animation.mime_type.clone()),
                start_timestamp: Some(0),
                caption: animation.caption.clone(),
                caption_entities: animation.caption_entities.clone(),
                duration_label: Some(format_voice_duration(animation.duration)),
                natural_size: (animation.width > 0 && animation.height > 0)
                    .then_some((animation.width, animation.height)),
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
                has_stickers: false,
                caption: caption.to_string(),
                caption_entities: Vec::new(),
                show_caption_above_media: false,
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
                minithumbnail: None,
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
            sender: None,
            id: MessageId(id),
            chat_id: ChatId(chat),
            is_outgoing: false,
            date: 0,
            content,
            pending: false,
            reply_to: None,
            forward_info: None,
            extras: Default::default(),
            interaction_info: None,
            is_pinned: false,
            media_album_id: 0,
            reply_markup: None,
            self_destruct: None,
            auto_delete: None,
            author_signature: None,
            failed: false,
            can_retry: false,
            ephemeral: None,
        }
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
    fn secret_media_is_excluded_but_spoilers_open_once_revealed() {
        let mut secret = photo_message(7, 5, vec![(70, 100, 100)], "");
        if let MessageContent::Photo(photo) = &mut secret.content {
            photo.is_secret = true;
        }
        let mut spoiler = video_message(7, 6, 71, Some(72));
        if let MessageContent::Video(video) = &mut spoiler.content {
            video.has_spoiler = true;
        }
        let items = collect_media_items(&[secret, spoiler]);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].message_id, MessageId(6));
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

    fn animation_message(id: i64, file: i32, secret: bool) -> HistoryMessage {
        test_message(
            7,
            id,
            MessageContent::Animation(crate::telegram::envelope::AnimationContent {
                duration: 3,
                width: 320,
                height: 240,
                file_name: "loop.mp4".into(),
                mime_type: "video/mp4".into(),
                caption: "looping".into(),
                caption_entities: Vec::new(),
                show_caption_above_media: false,
                has_spoiler: false,
                is_secret: secret,
                file_id: FileId(file),
                thumb_file_id: Some(FileId(file + 1)),
                thumb_width: 160,
                thumb_height: 120,
            }),
        )
    }

    #[test]
    fn animations_open_the_viewer_as_looping_clips() {
        let items = collect_media_items(&[
            animation_message(1, 10, false),
            animation_message(2, 20, true),
        ]);
        assert_eq!(items.len(), 1, "secret animations never open");
        let gif = &items[0];
        assert_eq!(gif.kind, MediaViewerKind::Animation);
        assert_eq!(gif.kind.label(), "GIF");
        assert!(gif.kind.is_playable() && gif.kind.loops());
        assert!(MediaViewerKind::Video.is_playable() && !MediaViewerKind::Video.loops());
        assert!(!MediaViewerKind::Photo.is_playable());
        assert_eq!(gif.play_file_id, Some(FileId(10)));
        assert_eq!(gif.display_file_ids, vec![FileId(11)]);
        assert_eq!(gif.natural_size, Some((320, 240)));
        // An animation starts like a video: download, extract, or play.
        assert_eq!(
            decide_viewer_video_start(gif, false, false),
            ViewerVideoStart::ParkDownload
        );
        assert_eq!(
            decide_viewer_video_start(gif, true, false),
            ViewerVideoStart::ExtractFrames
        );
        assert_eq!(
            decide_viewer_video_start(gif, true, true),
            ViewerVideoStart::PlayNow
        );
    }
}
