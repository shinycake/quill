//! Building viewer items from chat history and profile photos.

use super::*;
use crate::telegram::envelope::PollOptionMedia;

/// B10: viewer items for a profile photo gallery. The item's `message_id`
/// is the `chatPhoto.id` ("Set as main photo" reads it back) and its chat
/// the group or channel the photos belong to (`ChatId(0)` for a user);
/// both sizes are display candidates, the largest first.
pub fn profile_photo_items<'a>(
    photos: impl IntoIterator<Item = &'a crate::state::ProfilePhoto>,
    chat_id: ChatId,
) -> Vec<MediaViewerItem> {
    photos
        .into_iter()
        .map(|photo| MediaViewerItem {
            group: Some(ViewerGroup::ProfilePhotos),
            file_name: None,
            chat_id,
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
            quality_options: Vec::new(),
            natural_size: (photo.width > 0 && photo.height > 0)
                .then_some((photo.width, photo.height)),
        })
        .collect()
}

pub fn collect_media_items(messages: &[HistoryMessage]) -> Vec<MediaViewerItem> {
    messages
        .iter()
        .flat_map(|message| match paid_album_items(message) {
            // A bought paid-media album pages through its items, all under
            // the one message id.
            Some(contents) => contents
                .iter()
                .filter_map(|content| {
                    let mut part = message.clone();
                    part.content = content.clone();
                    media_viewer_item(&part).map(|item| MediaViewerItem {
                        group: Some(ViewerGroup::PaidMedia(message.id)),
                        ..item
                    })
                })
                .collect::<Vec<_>>(),
            None => media_viewer_item(message).into_iter().collect(),
        })
        .collect()
}

/// The bought items of a paid-media album message.
fn paid_album_items(message: &HistoryMessage) -> Option<&[MessageContent]> {
    match &message.content {
        MessageContent::Action(action) => match action.as_ref() {
            ServiceAction::PaidMedia { unlocked, .. } if !unlocked.is_empty() => {
                Some(unlocked.as_slice())
            }
            _ => None,
        },
        _ => None,
    }
}

fn photo_viewer_item(message: &HistoryMessage, photo: &PhotoContent) -> Option<MediaViewerItem> {
    photo_item(message.chat_id, message.id, photo)
}

fn photo_item(
    chat_id: ChatId,
    message_id: MessageId,
    photo: &PhotoContent,
) -> Option<MediaViewerItem> {
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
        group: None,
        file_name: None,
        chat_id,
        message_id,
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
        quality_options: Vec::new(),
        natural_size: photo
            .largest_size()
            .map(|size| (size.width, size.height))
            .filter(|(w, h)| *w > 0 && *h > 0),
    })
}

/// The viewer item of a photo or video with a self-destruct timer, which
/// opens alone (tdesktop shows timed media without paging to its
/// neighbours, so no other timer starts by browsing). View-once media has
/// none: TDLib deletes it the moment it is opened, so the viewer would have
/// nothing left to show.
pub fn timed_viewer_item(message: &HistoryMessage) -> Option<MediaViewerItem> {
    let timed = message.self_destruct.is_some_and(|sd| {
        matches!(
            sd.kind,
            crate::telegram::envelope::SelfDestructKind::Timer { .. }
        )
    });
    if !timed
        || !matches!(
            message.content,
            MessageContent::Photo(_) | MessageContent::Video(_)
        )
    {
        return None;
    }
    viewer_item_for(message, true)
}

/// The viewer item for the picture or clip of a poll option, which opens
/// over the poll's message (`ViewerSource::Poll`). Stickers, links and
/// places have none: they open elsewhere.
pub fn poll_option_viewer_item(
    chat_id: ChatId,
    message_id: MessageId,
    media: &PollOptionMedia,
) -> Option<MediaViewerItem> {
    match media {
        PollOptionMedia::Photo(photo) => photo_item(chat_id, message_id, photo),
        PollOptionMedia::Video(video) => {
            let thumb = video.thumb_file_id.filter(|id| id.0 != 0);
            let download = thumb.unwrap_or(video.file_id);
            if download.0 == 0 {
                return None;
            }
            Some(MediaViewerItem {
                group: None,
                file_name: None,
                chat_id,
                message_id,
                kind: MediaViewerKind::Video,
                display_file_ids: thumb.into_iter().collect(),
                download_file_id: download,
                play_file_id: Some(video.file_id).filter(|id| id.0 != 0),
                duration_secs: Some(video.duration.max(0)),
                mime_type: Some(video.mime_type.clone()),
                start_timestamp: Some(0),
                caption: String::new(),
                caption_entities: Vec::new(),
                duration_label: Some(format_voice_duration(video.duration)),
                quality_options: Vec::new(),
                natural_size: (video.width > 0 && video.height > 0)
                    .then_some((video.width, video.height)),
            })
        }
        PollOptionMedia::Animation(animation) => {
            let thumb = animation.thumb_file_id.filter(|id| id.0 != 0);
            let download = thumb.unwrap_or(animation.file_id);
            if download.0 == 0 {
                return None;
            }
            Some(MediaViewerItem {
                group: None,
                file_name: None,
                chat_id,
                message_id,
                kind: MediaViewerKind::Animation,
                display_file_ids: thumb.into_iter().collect(),
                download_file_id: download,
                play_file_id: Some(animation.file_id).filter(|id| id.0 != 0),
                duration_secs: Some(animation.duration.max(0)),
                mime_type: Some(animation.mime_type.clone()),
                start_timestamp: Some(0),
                caption: String::new(),
                caption_entities: Vec::new(),
                duration_label: Some(format_voice_duration(animation.duration)),
                quality_options: Vec::new(),
                natural_size: (animation.width > 0 && animation.height > 0)
                    .then_some((animation.width, animation.height)),
            })
        }
        PollOptionMedia::Sticker(_)
        | PollOptionMedia::Link { .. }
        | PollOptionMedia::Location(_)
        | PollOptionMedia::Venue { .. } => None,
    }
}

fn media_viewer_item(message: &HistoryMessage) -> Option<MediaViewerItem> {
    viewer_item_for(message, false).map(|item| MediaViewerItem {
        group: album_group(message),
        ..item
    })
}

/// The album a message is part of (`media_album_id`, 0 for none).
pub(super) fn album_group(message: &HistoryMessage) -> Option<ViewerGroup> {
    (message.media_album_id != 0).then_some(ViewerGroup::Album(message.media_album_id))
}

fn viewer_item_for(message: &HistoryMessage, allow_secret: bool) -> Option<MediaViewerItem> {
    match &message.content {
        // Spoilers are viewable once revealed (the cover takes the click
        // before that), as in Telegram Desktop; secret media only through
        // `timed_viewer_item`.
        MessageContent::Photo(photo) if allow_secret || !photo.is_secret => {
            photo_viewer_item(message, photo)
        }
        // A chat-photo change or a suggested profile photo opens in the
        // viewer like any photo.
        MessageContent::Action(action) => match action.as_ref() {
            ServiceAction::ChatPhoto {
                photo: Some(photo), ..
            }
            | ServiceAction::SuggestProfilePhoto {
                photo: Some(photo), ..
            } => photo_viewer_item(message, photo),
            _ => None,
        },
        MessageContent::Video(video) if allow_secret || !video.is_secret => {
            let thumb = video.thumb_file_id.filter(|id| id.0 != 0);
            let download = thumb.unwrap_or(video.file_id);
            if download.0 == 0 {
                return None;
            }
            Some(MediaViewerItem {
                group: None,
                file_name: None,
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
                quality_options: build_quality_options(
                    video.file_id,
                    video.width,
                    video.height,
                    &video.alternatives,
                ),
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
                group: None,
                file_name: None,
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
                quality_options: Vec::new(),
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
    use crate::media_viewer::test_support::test_message;
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
                minithumbnail: None,
                thumb_still: true,
                alternatives: Vec::new(),
            }),
        )
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
    fn timed_photos_and_videos_open_alone_and_view_once_never() {
        use crate::telegram::envelope::{MessageSelfDestruct, SelfDestructKind};
        let sd = |kind| {
            Some(MessageSelfDestruct {
                kind,
                expires_in_ms: 0,
                fetched_at_ms: 0,
            })
        };
        let mut photo = photo_message(7, 5, vec![(70, 100, 100)], "");
        if let MessageContent::Photo(p) = &mut photo.content {
            p.is_secret = true;
        }
        // No timer, no item.
        assert!(timed_viewer_item(&photo).is_none());
        photo.self_destruct = sd(SelfDestructKind::Timer { secs: 10 });
        let item = timed_viewer_item(&photo).expect("timed photo");
        assert_eq!(item.message_id, MessageId(5));
        // Paging over the chat's media never reaches it.
        assert!(collect_media_items(std::slice::from_ref(&photo)).is_empty());
        // A timed video opens too.
        let mut video = video_message(7, 6, 71, Some(72));
        if let MessageContent::Video(v) = &mut video.content {
            v.is_secret = true;
        }
        video.self_destruct = sd(SelfDestructKind::Timer { secs: 5 });
        assert_eq!(
            timed_viewer_item(&video).map(|item| item.kind),
            Some(MediaViewerKind::Video)
        );
        // View-once media is deleted by TDLib when opened: no item.
        photo.self_destruct = sd(SelfDestructKind::Immediately);
        assert!(timed_viewer_item(&photo).is_none());
    }

    #[test]
    fn document_never_opens_viewer() {
        let messages = vec![test_message(
            7,
            8,
            MessageContent::Document(crate::telegram::envelope::DocumentContent {
                thumbnail: None,
                minithumbnail: None,
                size: 0,
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
                minithumbnail: None,
                thumb_still: true,
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
        // An animation starts like a video: download, or play.
        assert_eq!(
            decide_viewer_video_start(gif, false),
            ViewerVideoStart::ParkDownload
        );
        assert_eq!(
            decide_viewer_video_start(gif, true),
            ViewerVideoStart::PlayNow
        );
    }

    #[test]
    fn album_items_carry_their_album_and_loose_ones_none() {
        let mut first = photo_message(7, 1, vec![(60, 800, 600)], "");
        let mut second = video_message(7, 2, 50, Some(51));
        first.media_album_id = 77;
        second.media_album_id = 77;
        let loose = photo_message(7, 3, vec![(62, 800, 600)], "");
        let items = collect_media_items(&[first, second, loose]);
        let groups: Vec<_> = items.iter().map(|item| item.group).collect();
        assert_eq!(
            groups,
            vec![
                Some(ViewerGroup::Album(77)),
                Some(ViewerGroup::Album(77)),
                None
            ]
        );
    }
}
