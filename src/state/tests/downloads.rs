//! State reducer tests: downloads.
use super::common::*;
use super::*;
use crate::telegram::envelope::MediaPayload;

#[test]
fn sound_download_error_drops_pending_playback() {
    let (mut session, _sink) = session();
    session.settings.sound_file_ids.insert(91, 7);
    session.settings.pending_sound_downloads.insert(7);
    session.upsert_file(
        ParsedFile {
            id: FileId(91),
            size: 0,
            expected_size: 100,
            local: LocalFileState {
                path: String::new(),
                can_be_downloaded: true,
                is_downloading_active: false,
                is_downloading_completed: false,
                downloaded_size: 0,
            },
        },
        true,
    );
    assert!(!session.settings.pending_sound_downloads.contains(&7));
    assert!(session.settings.pending_sound_plays.is_empty());
    // The file→sound mapping itself stays (the list refetch prunes it).
    assert_eq!(session.settings.sound_file_ids.get(&91), Some(&7));
}

#[test]
fn video_thumb_auto_downloads_secret_video_does_not() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.open_chat(ChatId(1));
    let thumb = media_file_json(41, "", false);
    let clip = media_file_json(42, "", false);
    let secret_thumb = media_file_json(43, "", false);
    let secret_clip = media_file_json(44, "", false);
    let open = format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":20,"chat_id":1,"is_outgoing":false,"content":{{"@type":"messageVideo","video":{{"@type":"video","duration":8,"width":320,"height":180,"file_name":"a.mp4","mime_type":"video/mp4","has_stickers":false,"supports_streaming":true,"minithumbnail":null,"thumbnail":{{"@type":"thumbnail","format":{{"@type":"thumbnailFormatJpeg"}},"width":120,"height":68,"file":{thumb}}},"video":{clip}}},"alternative_videos":[],"storyboards":[],"cover":null,"start_timestamp":0,"caption":{{"@type":"formattedText","text":"clip","entities":[]}},"show_caption_above_media":false,"has_spoiler":false,"is_secret":false}}}}}}"#
    );
    let secret = format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":21,"chat_id":1,"is_outgoing":false,"content":{{"@type":"messageVideo","video":{{"@type":"video","duration":1,"width":100,"height":100,"file_name":"s.mp4","mime_type":"video/mp4","has_stickers":false,"supports_streaming":false,"minithumbnail":null,"thumbnail":{{"@type":"thumbnail","format":{{"@type":"thumbnailFormatJpeg"}},"width":40,"height":40,"file":{secret_thumb}}},"video":{secret_clip}}},"alternative_videos":[],"storyboards":[],"cover":null,"start_timestamp":0,"caption":{{"@type":"formattedText","text":"","entities":[]}},"show_caption_above_media":false,"has_spoiler":false,"is_secret":true}}}}}}"#
    );
    apply_json(&mut session, &seq, &sink, &open);
    apply_json(&mut session, &seq, &sink, &secret);
    assert_eq!(session.thumb_file_ids_to_download(), vec![FileId(41)]);
    assert!(session.should_download(FileId(42)));
    assert!(session.should_download(FileId(43)));
    assert_eq!(
        session
            .histories
            .get(&1)
            .unwrap()
            .messages
            .get(&20)
            .unwrap()
            .content
            .preview(),
        "clip"
    );
}

#[test]
fn video_note_thumb_auto_downloads_secret_note_does_not() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.open_chat(ChatId(1));
    let thumb = media_file_json(51, "", false);
    let clip = media_file_json(52, "", false);
    let secret_thumb = media_file_json(53, "", false);
    let secret_clip = media_file_json(54, "", false);
    let open = format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":30,"chat_id":1,"is_outgoing":false,"content":{{"@type":"messageVideoNote","video_note":{{"@type":"videoNote","duration":6,"waveform":"","length":240,"minithumbnail":null,"thumbnail":{{"@type":"thumbnail","format":{{"@type":"thumbnailFormatJpeg"}},"width":90,"height":90,"file":{thumb}}},"speech_recognition_result":null,"video":{clip}}},"is_viewed":false,"is_secret":false}}}}}}"#
    );
    let secret = format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":31,"chat_id":1,"is_outgoing":false,"content":{{"@type":"messageVideoNote","video_note":{{"@type":"videoNote","duration":1,"waveform":"","length":200,"minithumbnail":null,"thumbnail":{{"@type":"thumbnail","format":{{"@type":"thumbnailFormatJpeg"}},"width":40,"height":40,"file":{secret_thumb}}},"speech_recognition_result":null,"video":{secret_clip}}},"is_viewed":false,"is_secret":true}}}}}}"#
    );
    apply_json(&mut session, &seq, &sink, &open);
    apply_json(&mut session, &seq, &sink, &secret);
    assert_eq!(session.thumb_file_ids_to_download(), vec![FileId(51)]);
    assert!(session.should_download(FileId(52)));
    assert!(session.should_download(FileId(53)));
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateMessageContentOpened","chat_id":1,"message_id":30}"#,
    );
    let content = &session
        .histories
        .get(&1)
        .unwrap()
        .messages
        .get(&30)
        .unwrap()
        .content;
    let crate::telegram::envelope::MessageContent::VideoNote(note) = content else {
        panic!("{content:?}");
    };
    assert!(note.is_viewed);
    assert_eq!(note.length, 240);
    assert_eq!(content.preview(), "Video note");
}

#[test]
fn audio_cover_auto_downloads_track_does_not() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.open_chat(ChatId(1));
    let cover = media_file_json(61, "", false);
    let track = media_file_json(62, "", false);
    let external = media_file_json(63, "", false);
    let open = format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":40,"chat_id":1,"is_outgoing":false,"content":{{"@type":"messageAudio","audio":{{"@type":"audio","duration":90,"title":"Night Drive","performer":"Ada","file_name":"night.mp3","mime_type":"audio/mpeg","album_cover_minithumbnail":null,"album_cover_thumbnail":{{"@type":"thumbnail","format":{{"@type":"thumbnailFormatJpeg"}},"width":90,"height":90,"file":{cover}}},"external_album_covers":[{{"@type":"thumbnail","format":{{"@type":"thumbnailFormatJpeg"}},"width":40,"height":40,"file":{external}}}],"audio":{track}}},"caption":{{"@type":"formattedText","text":"","entities":[]}}}}}}}}"#
    );
    apply_json(&mut session, &seq, &sink, &open);
    assert_eq!(session.thumb_file_ids_to_download(), vec![FileId(61)]);
    assert!(session.should_download(FileId(62)));
    let fallback = format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":41,"chat_id":1,"is_outgoing":false,"content":{{"@type":"messageAudio","audio":{{"@type":"audio","duration":10,"title":"","performer":"","file_name":"b.mp3","mime_type":"audio/mpeg","album_cover_minithumbnail":null,"album_cover_thumbnail":null,"external_album_covers":[{{"@type":"thumbnail","format":{{"@type":"thumbnailFormatJpeg"}},"width":40,"height":40,"file":{external}}}],"audio":{track}}},"caption":{{"@type":"formattedText","text":"","entities":[]}}}}}}}}"#
    );
    apply_json(&mut session, &seq, &sink, &fallback);
    let ids = session.thumb_file_ids_to_download();
    assert!(ids.contains(&FileId(61)));
    assert!(ids.contains(&FileId(63)));
    assert!(!ids.contains(&FileId(62)));
    assert_eq!(
        session
            .histories
            .get(&1)
            .unwrap()
            .messages
            .get(&40)
            .unwrap()
            .content
            .preview(),
        "Night Drive"
    );
}

#[test]
fn download_error_unsticks_in_flight_file() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let extra = session.request_download(FileId(4));
    session.begin_download(FileId(4));
    assert!(!session.should_download(FileId(4)));
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","code":400,"message":"CANARY_FILE_ERR","@extra":"{}"}}"#,
            extra.0
        ),
    );
    assert!(session.should_download(FileId(4)));
    assert!(!session.downloading.contains(&4));
    assert!(!sink.rendered().contains("CANARY_FILE_ERR"));
}

#[test]
fn download_unsticks_after_file_extra_then_idle_update_or_error() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);

    let extra = session.request_download(FileId(4));
    session.begin_download(FileId(4));
    apply_json(
        &mut session,
        &seq,
        &sink,
        &file_reply_json(extra.0, 4, true),
    );
    assert!(session.requests.take(extra).is_none());
    assert!(!session.should_download(FileId(4)));
    assert!(session.downloading.contains(&4));
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"updateFile","file":{}}}"#,
            media_file_json(4, "", false)
        ),
    );
    assert!(session.should_download(FileId(4)));
    assert!(!session.downloading.contains(&4));

    let extra = session.request_download(FileId(5));
    session.begin_download(FileId(5));
    apply_json(
        &mut session,
        &seq,
        &sink,
        &file_reply_json(extra.0, 5, true),
    );
    assert!(!session.should_download(FileId(5)));
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","code":400,"message":"CANARY_FILE_ERR2","@extra":"{}"}}"#,
            extra.0
        ),
    );
    assert!(session.should_download(FileId(5)));
    assert!(!session.downloading.contains(&5));
    assert!(!sink.rendered().contains("CANARY_FILE_ERR2"));
}

#[test]
fn auto_download_gate_respects_data_saver_and_chat_kind() {
    use crate::settings::{AUTO_DOWNLOAD_FILE, AUTO_DOWNLOAD_PHOTO, AUTO_DOWNLOAD_VIDEO};
    let (mut session, _sink) = session();
    // Unknown kind → private bucket; defaults allow photos.
    let mut chat = placeholder_chat(ChatId(13));
    session.chats.insert(13, chat.clone());
    assert!(session.auto_download_allowed(ChatId(13), AUTO_DOWNLOAD_PHOTO));
    assert!(!session.auto_download_allowed(ChatId(13), AUTO_DOWNLOAD_VIDEO));
    // Channel bucket is independent.
    chat.kind = ChatKind::Supergroup {
        supergroup_id: 1,
        is_channel: true,
    };
    session.chats.insert(13, chat);
    session.settings.media_prefs.auto_download_channels = AUTO_DOWNLOAD_VIDEO;
    assert!(!session.auto_download_allowed(ChatId(13), AUTO_DOWNLOAD_PHOTO));
    assert!(session.auto_download_allowed(ChatId(13), AUTO_DOWNLOAD_VIDEO));
    // Data saver pauses everything, regardless of bucket.
    session.settings.media_prefs.data_saver = true;
    assert!(!session.auto_download_allowed(ChatId(13), AUTO_DOWNLOAD_VIDEO));
    session.settings.media_prefs.data_saver = false;
    // Groups bucket: basic groups.
    let mut group = placeholder_chat(ChatId(14));
    group.kind = ChatKind::BasicGroup { basic_group_id: 2 };
    session.chats.insert(14, group);
    session.settings.media_prefs.auto_download_groups = AUTO_DOWNLOAD_FILE;
    assert!(session.auto_download_allowed(ChatId(14), AUTO_DOWNLOAD_FILE));
    assert!(!session.auto_download_allowed(ChatId(14), AUTO_DOWNLOAD_PHOTO));
}

#[test]
fn auto_download_media_ids_follow_per_type_flags() {
    // MED3: full-media auto-download honors the per-media-type flags —
    // voice on by default (TGX 0x63), video/file off; data saver and
    // spoiler/secret suppress everything.
    use crate::settings::{AUTO_DOWNLOAD_FILE, AUTO_DOWNLOAD_VIDEO};
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.open_chat(ChatId(1));
    // Voice note (file 4), video (file 5), document (file 9).
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":20,"chat_id":1,"is_outgoing":false,"content":{{"@type":"messageVoiceNote","voice_note":{{"@type":"voiceNote","duration":12,"mime_type":"audio/ogg","voice":{}}},"caption":{{"@type":"formattedText","text":"","entities":[]}},"is_listened":false}}}}}}"#,
            media_file_json(4, "", false),
        ),
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":21,"chat_id":1,"is_outgoing":false,"content":{{"@type":"messageVideo","video":{{"@type":"video","duration":10,"width":320,"height":240,"file_name":"v.mp4","mime_type":"video/mp4","has_stickers":false,"video":{}}},"caption":{{"@type":"formattedText","text":"","entities":[]}},"has_spoiler":false,"is_secret":false}}}}}}"#,
            media_file_json(5, "", false),
        ),
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":22,"chat_id":1,"is_outgoing":false,"content":{{"@type":"messageDocument","document":{{"@type":"document","file_name":"d.bin","mime_type":"application/octet-stream","document":{}}},"caption":{{"@type":"formattedText","text":"","entities":[]}}}}}}}}"#,
            media_file_json(9, "", false),
        ),
    );
    // Defaults: voice auto-downloads; video and file do not.
    assert_eq!(session.auto_download_media_file_ids(), vec![FileId(4)]);
    // Enabling video+file for private chats picks them up.
    session.settings.media_prefs.auto_download_private |= AUTO_DOWNLOAD_VIDEO | AUTO_DOWNLOAD_FILE;
    assert_eq!(
        session.auto_download_media_file_ids(),
        vec![FileId(4), FileId(5), FileId(9)]
    );
    // Data saver suppresses all automatic media.
    session.settings.media_prefs.data_saver = true;
    assert!(session.auto_download_media_file_ids().is_empty());
    session.settings.media_prefs.data_saver = false;
    // A spoiler video is never auto-downloaded.
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":23,"chat_id":1,"is_outgoing":false,"content":{{"@type":"messageVideo","video":{{"@type":"video","duration":10,"width":320,"height":240,"file_name":"s.mp4","mime_type":"video/mp4","has_stickers":false,"video":{}}},"caption":{{"@type":"formattedText","text":"","entities":[]}},"has_spoiler":true,"is_secret":false}}}}}}"#,
            media_file_json(6, "", false),
        ),
    );
    assert_eq!(
        session.auto_download_media_file_ids(),
        vec![FileId(4), FileId(5), FileId(9)]
    );
}

#[test]
fn auto_download_skips_files_over_size_cap() {
    // MED3 review: full-media auto-download skips files whose known
    // size exceeds `AUTO_DOWNLOAD_MAX_BYTES` (TGX
    // `canAutomaticallyDownload` download limit, WiFi default 50 MiB).
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.open_chat(ChatId(1));
    // Voice note (file 4), auto-downloaded by default (TGX 0x63).
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":20,"chat_id":1,"is_outgoing":false,"content":{{"@type":"messageVoiceNote","voice_note":{{"@type":"voiceNote","duration":12,"mime_type":"audio/ogg","voice":{}}},"caption":{{"@type":"formattedText","text":"","entities":[]}},"is_listened":false}}}}}}"#,
            media_file_json(4, "", false),
        ),
    );
    assert_eq!(session.auto_download_media_file_ids(), vec![FileId(4)]);
    // File 4 balloons past the cap: skipped from here on.
    session.files.get_mut(&4).unwrap().size = 100 * 1024 * 1024;
    assert!(session.auto_download_media_file_ids().is_empty());
}

#[test]
fn completed_user_downloads_land_in_recent_list() {
    let (mut session, _sink) = session();
    let completed = |id: i32| ParsedFile {
        id: FileId(id),
        size: 100,
        expected_size: 100,
        local: LocalFileState {
            path: format!("/tmp/{id}.bin"),
            can_be_downloaded: true,
            is_downloading_active: false,
            is_downloading_completed: true,
            downloaded_size: 100,
        },
    };
    // User-initiated download completing → recorded.
    session.begin_download(FileId(7));
    session.user_downloads.insert(7);
    session.upsert_file(completed(7), true);
    // Automatic thumb completing → not recorded.
    session.begin_download(FileId(8));
    session.upsert_file(completed(8), true);
    assert_eq!(
        session
            .completed_downloads
            .iter()
            .copied()
            .collect::<Vec<_>>(),
        vec![7]
    );
    assert!(!session.downloading.contains(&7));
    assert!(!session.user_downloads.contains(&7));
    // begin_download clears a recorded failure (retry path).
    session.failed_downloads.insert(9);
    session.begin_download(FileId(9));
    assert!(!session.failed_downloads.contains(&9));
}

#[test]
fn stalled_user_download_marks_failed_but_cancel_does_not() {
    // MED3: an `updateFile` that takes a user download active → idle
    // without completing is a stall — record the failure so the row
    // offers Retry. An explicit cancel (`abort_download` drops the id
    // from `user_downloads`) must not be mislabeled as a failure.
    fn update_file(id: i32, active: bool) -> String {
        format!(
            r#"{{"@type":"updateFile","file":{{"@type":"file","id":{id},"size":24,"expected_size":24,"local":{{"@type":"localFile","path":"","can_be_downloaded":true,"can_be_deleted":false,"is_downloading_active":{active},"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0}},"remote":{{"@type":"remoteFile","id":"r","unique_id":"u","is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":24}}}}}}"#,
        )
    }
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    // Stalled: in-flight, then idle without completing.
    session.begin_download(FileId(12));
    session.user_downloads.insert(12);
    apply_json(&mut session, &seq, &sink, &update_file(12, true));
    assert!(!session.failed_downloads.contains(&12));
    apply_json(&mut session, &seq, &sink, &update_file(12, false));
    assert!(session.failed_downloads.contains(&12));
    assert!(!session.downloading.contains(&12));
    assert!(!session.user_downloads.contains(&12));
    // Cancelled: abort first, then the idle echo arrives.
    session.begin_download(FileId(13));
    session.user_downloads.insert(13);
    session.abort_download(FileId(13));
    apply_json(&mut session, &seq, &sink, &update_file(13, true));
    apply_json(&mut session, &seq, &sink, &update_file(13, false));
    assert!(!session.failed_downloads.contains(&13));
}

#[test]
fn update_file_download_tracks_pause_and_completion() {
    // Slice media-downloads-pause: `updateFileDownload` is the list API's
    // pause/completion channel — it sets/clears `paused_downloads` for
    // user-initiated downloads, and completion records the recent download
    // and unsticks everything (mirroring the `updateFile` path).
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.begin_download(FileId(31));
    session.user_downloads.insert(31);
    let update = |paused: bool, complete: i32| {
        format!(
            r#"{{"@type":"updateFileDownload","file_id":31,"complete_date":{complete},"is_paused":{paused},"counts":{{"@type":"downloadedFileCounts","being_downloaded":1,"recently_downloaded":0}}}}"#,
        )
    };
    apply_json(&mut session, &seq, &sink, &update(true, 0));
    assert!(session.paused_downloads.contains(&31));
    assert!(session.user_downloads.contains(&31));
    assert!(session.downloading.contains(&31));
    apply_json(&mut session, &seq, &sink, &update(false, 0));
    assert!(!session.paused_downloads.contains(&31));
    assert!(session.user_downloads.contains(&31));
    // Completion: recent list + full unstick (paused cleared too).
    session.paused_downloads.insert(31);
    apply_json(&mut session, &seq, &sink, &update(false, 1723456789));
    assert!(session.completed_downloads.contains(&31));
    assert!(!session.user_downloads.contains(&31));
    assert!(!session.downloading.contains(&31));
    assert!(!session.paused_downloads.contains(&31));
}

#[test]
fn update_file_download_ignores_non_user_downloads() {
    // Pause state is only tracked for user-initiated (listed) downloads —
    // an automatic download's `updateFileDownload` must not enter
    // `paused_downloads`, and a completion for a non-user file records
    // nothing in the recent list.
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.begin_download(FileId(32));
    let update = |paused: bool, complete: i32| {
        format!(
            r#"{{"@type":"updateFileDownload","file_id":32,"complete_date":{complete},"is_paused":{paused},"counts":{{"@type":"downloadedFileCounts","being_downloaded":1,"recently_downloaded":0}}}}"#,
        )
    };
    apply_json(&mut session, &seq, &sink, &update(true, 0));
    assert!(!session.paused_downloads.contains(&32));
    apply_json(&mut session, &seq, &sink, &update(false, 1723456789));
    assert!(!session.completed_downloads.contains(&32));
}

#[test]
fn download_file_error_marks_failed_download() {
    // A `downloadFile` error response unsticks the download and records
    // the failure so the row can offer an honest retry — but only for
    // user-initiated downloads; an automatic (auto-download) error must
    // not surface in the Failed section.
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let extra = session.request_download(FileId(11));
    session.begin_download(FileId(11));
    session.user_downloads.insert(11);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":500,"message":"CANARY download failed"}}"#,
            extra.0
        ),
    );
    assert!(!session.downloading.contains(&11));
    assert!(session.failed_downloads.contains(&11));
    // Automatic download: unstuck, but not recorded as failed.
    let extra = session.request_download(FileId(12));
    session.begin_download(FileId(12));
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":500,"message":"CANARY download failed"}}"#,
            extra.0
        ),
    );
    assert!(!session.downloading.contains(&12));
    assert!(!session.failed_downloads.contains(&12));
}

#[test]
fn nested_file_snapshots_cannot_erase_downloaded_media() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let complete = media_file_json(91, "/tmp/quill-regression-photo.jpg", true);
    let idle = media_file_json(91, "", false);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(r#"{{"@type":"updateFile","file":{complete}}}"#),
    );
    let stale: crate::telegram::envelope::ParsedFile =
        match crate::telegram::envelope::parse_envelope(&format!(
            r#"{{"@type":"updateFile","file":{idle}}}"#
        ))
        .unwrap()
        .payload
        {
            crate::telegram::envelope::EnvelopePayload::Media(MediaPayload::UpdateFile(file)) => {
                file
            }
            _ => unreachable!(),
        };
    session.remember_files(&[stale]);
    assert_eq!(
        session.file(FileId(91)).unwrap().usable_path(),
        Some("/tmp/quill-regression-photo.jpg")
    );
    assert!(!session.should_download(FileId(91)));
    // Explicit cache eviction remains authoritative.
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(r#"{{"@type":"updateFile","file":{idle}}}"#),
    );
    assert!(session.file(FileId(91)).unwrap().usable_path().is_none());
    assert!(session.should_download(FileId(91)));
}

#[test]
fn paused_download_survives_idle_file_updates_in_either_order() {
    for pause_first in [true, false] {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        session.begin_download(FileId(31));
        session.user_downloads.insert(31);
        let pause =
            r#"{"@type":"updateFileDownload","file_id":31,"complete_date":0,"is_paused":true}"#;
        if pause_first {
            apply_json(&mut session, &seq, &sink, pause);
        }
        session.upsert_file(
            ParsedFile {
                id: FileId(31),
                size: 100,
                expected_size: 100,
                local: LocalFileState {
                    path: String::new(),
                    can_be_downloaded: true,
                    is_downloading_active: false,
                    is_downloading_completed: false,
                    downloaded_size: 25,
                },
            },
            true,
        );
        if !pause_first {
            apply_json(&mut session, &seq, &sink, pause);
        }
        assert!(session.user_downloads.contains(&31));
        assert!(session.downloading.contains(&31));
        assert!(session.paused_downloads.contains(&31));
        assert!(!session.failed_downloads.contains(&31));
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateFileDownload","file_id":31,"complete_date":0,"is_paused":false}"#,
        );
        assert!(session.user_downloads.contains(&31));
        assert!(!session.paused_downloads.contains(&31));
    }
}
