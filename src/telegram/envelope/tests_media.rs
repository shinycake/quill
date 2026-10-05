use super::*;
use crate::ids::FileId;
use base64::Engine;

pub(crate) fn local_file_json(id: i32, path: &str, completed: bool, can_download: bool) -> String {
    format!(
        r#"{{"@type":"file","id":{id},"size":12,"expected_size":12,"local":{{"@type":"localFile","path":{path},"can_be_downloaded":{can_download},"can_be_deleted":false,"is_downloading_active":false,"is_downloading_completed":{completed},"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0}},"remote":{{"@type":"remoteFile","id":"CANARY_REMOTE_ID","unique_id":"u","is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":12}}}}"#,
        path = serde_json::to_string(path).unwrap(),
        can_download = can_download,
        completed = completed,
    )
}

#[test]
fn message_photo_parses_sizes_caption_and_flags() {
    let thumb = local_file_json(1, "", false, true);
    let full = local_file_json(2, "/tmp/quill-photo.jpg", true, true);
    let json = format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":9,"chat_id":4,"is_outgoing":false,"content":{{"@type":"messagePhoto","photo":{{"@type":"photo","has_stickers":false,"sizes":[{{"@type":"photoSize","type":"m","photo":{thumb},"width":320,"height":240,"progressive_sizes":[]}},{{"@type":"photoSize","type":"x","photo":{full},"width":800,"height":600,"progressive_sizes":[]}}]}},"caption":{{"@type":"formattedText","text":"CANARY_PHOTO_caption","entities":[]}},"show_caption_above_media":false,"has_spoiler":false,"is_secret":false}}}}}}"#
    );
    let env = parse_envelope(&json).unwrap();
    match &env.payload {
        EnvelopePayload::UpdateNewMessage(message) => {
            let MessageContent::Photo(photo) = &message.content else {
                panic!("{:?}", message.content);
            };
            assert_eq!(photo.caption, "CANARY_PHOTO_caption");
            assert!(!photo.is_secret);
            assert!(!photo.has_spoiler);
            assert_eq!(photo.sizes.len(), 2);
            assert_eq!(photo.thumb_size().unwrap().type_name, "m");
            assert_eq!(photo.thumb_size().unwrap().file_id.0, 1);
            assert_eq!(photo.largest_size().unwrap().file_id.0, 2);
            assert_eq!(photo.open_file_id().unwrap().0, 2);
            assert_eq!(message.files.len(), 2);
            assert_eq!(message.files[0].id.0, 1);
            assert!(message.files[0].needs_download());
            assert_eq!(message.files[1].usable_path(), Some("/tmp/quill-photo.jpg"));
        }
        other => panic!("{other:?}"),
    }
    let debug = format!("{env:?}");
    assert!(!debug.contains("CANARY_REMOTE_ID"));
}

#[test]
fn message_document_parses_name_mime_and_file() {
    let file = local_file_json(8, "", false, true);
    let json = format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":3,"chat_id":4,"is_outgoing":false,"content":{{"@type":"messageDocument","document":{{"@type":"document","file_name":"notes.txt","mime_type":"text/plain","document":{file}}},"caption":{{"@type":"formattedText","text":"CANARY_DOC_caption","entities":[]}}}}}}}}"#
    );
    let env = parse_envelope(&json).unwrap();
    match env.payload {
        EnvelopePayload::UpdateNewMessage(message) => {
            let MessageContent::Document(doc) = message.content else {
                panic!("{:?}", message.content);
            };
            assert_eq!(doc.file_name, "notes.txt");
            assert_eq!(doc.mime_type, "text/plain");
            assert_eq!(doc.caption, "CANARY_DOC_caption");
            assert_eq!(doc.file_id.0, 8);
            assert_eq!(message.files[0].id.0, 8);
            assert!(message.files[0].needs_download());
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn update_file_and_file_response_are_typed() {
    let file = local_file_json(4, "/tmp/done.bin", true, true);
    let update = parse_envelope(&format!(r#"{{"@type":"updateFile","file":{file}}}"#)).unwrap();
    match update.payload {
        EnvelopePayload::UpdateFile(parsed) => {
            assert_eq!(parsed.id.0, 4);
            assert_eq!(parsed.usable_path(), Some("/tmp/done.bin"));
        }
        other => panic!("{other:?}"),
    }
    let response = parse_envelope(
            r#"{"@type":"file","@extra":"12","id":4,"size":12,"expected_size":12,"local":{"@type":"localFile","path":"","can_be_downloaded":true,"can_be_deleted":false,"is_downloading_active":true,"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0},"remote":{"@type":"remoteFile","id":"x","unique_id":"u","is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":12}}"#,
        )
        .unwrap();
    match response.payload {
        EnvelopePayload::File(parsed) => {
            assert_eq!(parsed.id.0, 4);
            assert!(parsed.local.is_downloading_active);
            assert!(parsed.needs_download());
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(response.extra, Some(crate::ids::RequestId(12)));
}

#[test]
fn download_progress_uses_downloaded_size_over_expected_size() {
    // Schema 1.8.67 localFile :292 — `downloaded_size` is "for
    // calculating download progress"; TGX divides by expectedSize.
    let parsed = parse_envelope(
            r#"{"@type":"file","id":9,"size":0,"expected_size":100,"local":{"@type":"localFile","path":"","can_be_downloaded":true,"can_be_deleted":false,"is_downloading_active":true,"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":40,"downloaded_size":42},"remote":{"@type":"remoteFile","id":"x","unique_id":"u","is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":100}}"#,
        )
        .unwrap();
    let EnvelopePayload::File(parsed) = parsed.payload else {
        panic!("expected File");
    };
    assert_eq!(parsed.local.downloaded_size, 42);
    assert_eq!(parsed.download_progress(), Some(0.42));
    // Unknown total → no percent (avoids a bogus 0%/100%).
    let no_total = ParsedFile {
        id: FileId(10),
        size: 0,
        expected_size: 0,
        local: LocalFileState {
            path: String::new(),
            can_be_downloaded: true,
            is_downloading_active: true,
            is_downloading_completed: false,
            downloaded_size: 42,
        },
    };
    assert_eq!(no_total.download_progress(), None);
}

#[test]
fn media_schema_matches_1_8_67() {
    let schema = include_str!("../../../schema/td_api.tl");
    let photo = schema
        .lines()
        .find(|l| l.starts_with("messagePhoto "))
        .expect("messagePhoto");
    assert!(photo.contains("photo:photo"));
    assert!(photo.contains("caption:formattedText"));
    assert!(photo.contains("is_secret:Bool"));
    assert!(photo.contains("has_spoiler:Bool"));
    let document = schema
        .lines()
        .find(|l| l.starts_with("messageDocument "))
        .expect("messageDocument");
    assert!(document.contains("document:document"));
    assert!(document.contains("caption:formattedText"));
    let download = schema
        .lines()
        .find(|l| l.starts_with("downloadFile "))
        .expect("downloadFile");
    assert!(download.contains("file_id:int32"));
    assert!(download.contains("priority:int32"));
    assert!(download.contains("offset:int53"));
    assert!(download.contains("limit:int53"));
    assert!(download.contains("synchronous:Bool"));
    assert!(schema.lines().any(|l| l.starts_with("updateFile ")));
    assert!(schema.lines().any(|l| l.starts_with("localFile ")));
    assert!(schema.lines().any(|l| l.starts_with("photoSize ")));
}

#[test]
fn message_sticker_keeps_webp_thumb_and_file() {
    let sticker_file = local_file_json(41, "", false, true);
    let thumb = local_file_json(42, "/tmp/sticker.webp", true, true);
    let json = format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":8,"chat_id":11,"is_outgoing":false,"content":{{"@type":"messageSticker","is_premium":false,"sticker":{{"@type":"sticker","id":"9001","set_id":"77","width":512,"height":512,"emoji":"😀","format":{{"@type":"stickerFormatWebp"}},"full_type":{{"@type":"stickerFullTypeRegular","premium_animation":null}},"thumbnail":{{"@type":"thumbnail","format":{{"@type":"thumbnailFormatWebp"}},"width":128,"height":128,"file":{thumb}}},"sticker":{sticker_file}}}}}}}}}"#
    );
    let env = parse_envelope(&json).unwrap();
    match env.payload {
        EnvelopePayload::UpdateNewMessage(message) => {
            let MessageContent::Sticker(sticker) = &message.content else {
                panic!("{:?}", message.content);
            };
            assert_eq!(sticker.emoji, "😀");
            assert_eq!(sticker.file_id, FileId(41));
            assert_eq!(sticker.thumb_file_id, Some(FileId(42)));
            assert_eq!(sticker.display_file_id(), Some(FileId(41)));
            assert_eq!(sticker.format, StickerFormat::Webp);
            assert!(message.files.iter().any(|file| file.id == FileId(42)));
            assert_eq!(message.content.preview(), "😀");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn sticker_sets_and_sticker_set_parse_1_8_67() {
    let sets = parse_envelope(
            r#"{"@type":"stickerSets","total_count":1,"sets":[{"@type":"stickerSetInfo","id":"77","title":"Demo","name":"DemoStickers","thumbnail":null,"thumbnail_outline":null,"is_owned":false,"is_installed":true,"is_archived":false,"is_official":true,"sticker_type":{"@type":"stickerTypeRegular"},"needs_repainting":false,"is_allowed_as_chat_emoji_status":false,"is_viewed":true,"size":2,"covers":[]}]}"#,
        )
        .unwrap();
    match sets.payload {
        EnvelopePayload::StickerSets { total_count, sets } => {
            assert_eq!(total_count, 1);
            assert_eq!(sets[0].id, 77);
            assert!(sets[0].is_installed);
            assert!(sets[0].is_official);
            assert_eq!(sets[0].title, "Demo");
        }
        other => panic!("{other:?}"),
    }
    let file = local_file_json(41, "/tmp/s.webp", true, true);
    let set = parse_envelope(&format!(
            r#"{{"@type":"stickerSet","id":"77","title":"Demo","name":"DemoStickers","thumbnail":null,"thumbnail_outline":null,"is_owned":false,"is_installed":true,"is_archived":false,"is_official":true,"sticker_type":{{"@type":"stickerTypeRegular"}},"needs_repainting":false,"is_allowed_as_chat_emoji_status":false,"is_viewed":true,"stickers":[{{"@type":"sticker","id":"9001","set_id":"77","width":512,"height":512,"emoji":"😀","format":{{"@type":"stickerFormatTgs"}},"full_type":{{"@type":"stickerFullTypeRegular","premium_animation":null}},"thumbnail":null,"sticker":{file}}}],"emojis":[]}}"#
        ))
        .unwrap();
    match set.payload {
        EnvelopePayload::StickerSet { id, stickers, .. } => {
            assert_eq!(id, 77);
            assert_eq!(stickers[0].format, StickerFormat::Tgs);
            assert_eq!(stickers[0].file_id, FileId(41));
            assert!(stickers[0].thumb_file_id.is_none());
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn s8_trending_sticker_sets_and_stickers_parse_1_8_67() {
    // `trendingStickerSets` (schema 1.8.67, line 6477): the same
    // `stickerSetInfo` rows as `stickerSets`, plus `is_premium`.
    let trending = parse_envelope(
            r#"{"@type":"trendingStickerSets","total_count":2,"is_premium":true,"sets":[{"@type":"stickerSetInfo","id":"77","title":"Demo","name":"DemoStickers","thumbnail":null,"thumbnail_outline":null,"is_owned":false,"is_installed":false,"is_archived":false,"is_official":true,"sticker_type":{"@type":"stickerTypeRegular"},"needs_repainting":false,"is_allowed_as_chat_emoji_status":false,"is_viewed":false,"size":2,"covers":[]}]}"#,
        )
        .unwrap();
    match trending.payload {
        EnvelopePayload::TrendingStickerSets {
            total_count,
            sets,
            is_premium,
        } => {
            assert_eq!(total_count, 2);
            assert!(is_premium);
            assert_eq!(sets[0].id, 77);
            assert!(!sets[0].is_installed);
        }
        other => panic!("{other:?}"),
    }

    // `stickers` (schema 1.8.67, line 6432): a bare `vector<sticker>`
    // as returned by `searchStickers` / `getFavoriteStickers` /
    // `getRecentStickers`.
    let file = local_file_json(41, "/tmp/s.webp", true, true);
    let found = parse_envelope(&format!(
            r#"{{"@type":"stickers","stickers":[{{"@type":"sticker","id":"9001","set_id":"77","width":512,"height":512,"emoji":"😀","format":{{"@type":"stickerFormatTgs"}},"full_type":{{"@type":"stickerFullTypeRegular","premium_animation":null}},"thumbnail":null,"sticker":{file}}}]}}"#
        ))
        .unwrap();
    match found.payload {
        EnvelopePayload::Stickers { stickers, files } => {
            assert_eq!(stickers.len(), 1);
            assert_eq!(stickers[0].file_id, FileId(41));
            assert_eq!(stickers[0].emoji, "😀");
            assert_eq!(files.len(), 1);
        }
        other => panic!("{other:?}"),
    }
}

/// Slice S9: `inlineQueryResults` carrying `inlineQueryResultAnimation`
/// entries (schema 1.8.67, line 7658) — the summaries keep the
/// `animation` kind mapping (Loop 3's filter), and the page additionally
/// exposes parsed `AnimationItem`s + files for the GIF panel search,
/// with `next_offset` paging intact. Also `updateAnimationSearchParameters`
/// (schema line 11064) parses provider + emojis.
#[test]
fn s9_gif_search_page_parses_animation_items_1_8_67() {
    let file = local_file_json(41, "/tmp/g.gif", true, true);
    let thumb = local_file_json(42, "", false, true);
    let env = parse_envelope(&format!(
            r#"{{
                "@type": "inlineQueryResults",
                "inline_query_id": 9003,
                "button": null,
                "results": [
                    {{"@type": "inlineQueryResultAnimation", "id": "g1", "title": "cat gif",
                      "animation": {{"@type": "animation", "duration": 2, "width": 240, "height": 140,
                        "file_name": "cat.gif", "mime_type": "image/gif",
                        "animation": {file},
                        "thumbnail": {{"@type": "thumbnail", "width": 120, "height": 70, "file": {thumb}}},
                        "has_stickers": false}}}},
                    {{"@type": "inlineQueryResultArticle", "id": "a1", "title": "not a gif", "description": ""}}
                ],
                "next_offset": "50"
            }}"#
        ))
        .unwrap();
    let page = match env.payload {
        EnvelopePayload::InlineQueryResults(page) => page,
        other => panic!("unexpected {other:?}"),
    };
    assert_eq!(page.next_offset, "50");
    assert_eq!(page.results.len(), 2);
    assert_eq!(page.results[0].kind, "animation");
    assert_eq!(page.results[0].title, "cat gif");
    // Only the animation entry yields an AnimationItem (Loop 3's
    // summary shape is untouched).
    assert_eq!(page.animations.len(), 1);
    assert_eq!(page.animations[0].file_id, FileId(41));
    assert_eq!(page.animations[0].thumb_file_id, Some(FileId(42)));
    assert_eq!(page.animations[0].mime_type, "image/gif");
    assert_eq!(page.files.len(), 2);

    let env = parse_envelope(
        r#"{"@type":"updateAnimationSearchParameters","provider":"GIPHY","emojis":["😀","🐱"]}"#,
    )
    .unwrap();
    match env.payload {
        EnvelopePayload::UpdateAnimationSearchParameters { provider, emojis } => {
            assert_eq!(provider, "GIPHY");
            assert_eq!(emojis, vec!["😀".to_string(), "🐱".to_string()]);
        }
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn message_video_parses_1_8_67_fields() {
    let clip = local_file_json(33, "", false, true);
    let thumb = local_file_json(42, "/tmp/video-thumb.jpg", true, true);
    let json = format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":9,"chat_id":11,"is_outgoing":false,"content":{{"@type":"messageVideo","video":{{"@type":"video","duration":42,"width":640,"height":360,"file_name":"clip.mp4","mime_type":"video/mp4","has_stickers":false,"supports_streaming":true,"minithumbnail":null,"thumbnail":{{"@type":"thumbnail","format":{{"@type":"thumbnailFormatJpeg"}},"width":120,"height":68,"file":{thumb}}},"video":{clip}}},"alternative_videos":[],"storyboards":[],"cover":null,"start_timestamp":3,"caption":{{"@type":"formattedText","text":"see this","entities":[]}},"show_caption_above_media":false,"has_spoiler":false,"is_secret":false}}}}}}"#
    );
    let env = parse_envelope(&json).unwrap();
    let EnvelopePayload::UpdateNewMessage(message) = env.payload else {
        panic!("expected message");
    };
    let MessageContent::Video(video) = &message.content else {
        panic!("{:?}", message.content);
    };
    assert_eq!(video.duration, 42);
    assert_eq!(video.width, 640);
    assert_eq!(video.height, 360);
    assert_eq!(video.file_name, "clip.mp4");
    assert_eq!(video.mime_type, "video/mp4");
    assert!(video.supports_streaming);
    assert!(!video.has_stickers);
    assert_eq!(video.start_timestamp, 3);
    assert_eq!(video.file_id, FileId(33));
    assert_eq!(video.play_file_id(), Some(FileId(33)));
    assert_eq!(video.thumb_file_id(), Some(FileId(42)));
    assert_eq!(video.thumb_width, 120);
    assert_eq!(video.thumb_height, 68);
    assert_eq!(video.caption, "see this");
    assert_eq!(message.content.preview(), "see this");
    assert!(message.files.iter().any(|file| file.id == FileId(33)));
    assert!(message.files.iter().any(|file| file.id == FileId(42)));
}

#[test]
fn message_video_note_parses_1_8_67_fields() {
    let waveform = base64::engine::general_purpose::STANDARD.encode([0xF8, 0x02]);
    let clip = local_file_json(33, "", false, true);
    let thumb = local_file_json(42, "/tmp/note-thumb.jpg", true, true);
    let json = format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":9,"chat_id":11,"is_outgoing":false,"content":{{"@type":"messageVideoNote","video_note":{{"@type":"videoNote","duration":8,"waveform":"{waveform}","length":240,"minithumbnail":null,"thumbnail":{{"@type":"thumbnail","format":{{"@type":"thumbnailFormatJpeg"}},"width":120,"height":120,"file":{thumb}}},"speech_recognition_result":null,"video":{clip}}},"is_viewed":false,"is_secret":false}}}}}}"#
    );
    let env = parse_envelope(&json).unwrap();
    let EnvelopePayload::UpdateNewMessage(message) = env.payload else {
        panic!("expected message");
    };
    let MessageContent::VideoNote(note) = &message.content else {
        panic!("{:?}", message.content);
    };
    assert_eq!(note.duration, 8);
    assert_eq!(note.length, 240);
    assert_eq!(note.waveform, vec![0xF8, 0x02]);
    assert!(!note.is_viewed);
    assert!(!note.is_secret);
    assert_eq!(note.file_id, FileId(33));
    assert_eq!(note.play_file_id(), Some(FileId(33)));
    assert_eq!(note.thumb_file_id(), Some(FileId(42)));
    assert_eq!(note.thumb_width, 120);
    assert_eq!(note.thumb_height, 120);
    assert_eq!(message.content.preview(), "Video note");
    assert!(message.files.iter().any(|file| file.id == FileId(33)));
    assert!(message.files.iter().any(|file| file.id == FileId(42)));
}

#[test]
fn message_animation_and_saved_list_parse_1_8_67() {
    let clip = local_file_json(33, "", false, true);
    let thumb = local_file_json(42, "/tmp/gif-thumb.jpg", true, true);
    let json = format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":8,"chat_id":11,"is_outgoing":false,"content":{{"@type":"messageAnimation","animation":{{"@type":"animation","duration":2,"width":240,"height":140,"file_name":"wave.mp4","mime_type":"video/mp4","has_stickers":false,"minithumbnail":null,"thumbnail":{{"@type":"thumbnail","format":{{"@type":"thumbnailFormatJpeg"}},"width":120,"height":70,"file":{thumb}}},"animation":{clip}}},"caption":{{"@type":"formattedText","text":"loop","entities":[]}},"show_caption_above_media":false,"has_spoiler":false,"is_secret":false}}}}}}"#
    );
    let env = parse_envelope(&json).unwrap();
    let EnvelopePayload::UpdateNewMessage(message) = env.payload else {
        panic!("expected message");
    };
    let MessageContent::Animation(animation) = &message.content else {
        panic!("{:?}", message.content);
    };
    assert_eq!(animation.mime_type, "video/mp4");
    assert_eq!(animation.file_id, FileId(33));
    assert_eq!(animation.thumb_file_id(), Some(FileId(42)));
    assert_eq!(animation.caption, "loop");
    assert_eq!(message.content.preview(), "loop");
    let saved = parse_envelope(
            r#"{"@type":"animations","animations":[{"@type":"animation","duration":1,"width":10,"height":10,"file_name":"a.mp4","mime_type":"video/mp4","has_stickers":false,"minithumbnail":null,"thumbnail":null,"animation":{"@type":"file","id":33,"size":1,"expected_size":1,"local":{"@type":"localFile","path":"","can_be_downloaded":true,"can_be_deleted":false,"is_downloading_active":false,"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0},"remote":{"@type":"remoteFile","id":"r","unique_id":"u","is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":1}}}]}"#,
        )
        .unwrap();
    match saved.payload {
        EnvelopePayload::Animations { animations, .. } => {
            assert_eq!(animations.len(), 1);
            assert_eq!(animations[0].file_id, FileId(33));
            assert!(animations[0].thumb_file_id.is_none());
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn secret_photo_is_flagged() {
    let file = local_file_json(1, "", false, true);
    let json = format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":1,"chat_id":1,"is_outgoing":false,"content":{{"@type":"messagePhoto","photo":{{"@type":"photo","has_stickers":false,"sizes":[{{"@type":"photoSize","type":"m","photo":{file},"width":100,"height":80,"progressive_sizes":[]}}]}},"caption":{{"@type":"formattedText","text":"","entities":[]}},"has_spoiler":true,"is_secret":true}}}}}}"#
    );
    let env = parse_envelope(&json).unwrap();
    match env.payload {
        EnvelopePayload::UpdateNewMessage(message) => {
            let MessageContent::Photo(photo) = message.content else {
                panic!("{:?}", message.content);
            };
            assert!(photo.is_secret);
            assert!(photo.has_spoiler);
            assert!(!photo.click_requests_download());
            assert_eq!(
                photo.placeholder_label(false, false),
                "Secret photo — not downloaded"
            );
            assert_eq!(
                photo.placeholder_label(true, false),
                "Secret photo — downloading…"
            );
            assert_eq!(photo.placeholder_label(false, true), "Secret photo — ready");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn spoiler_placeholder_follows_file_state_and_may_download() {
    let file = local_file_json(1, "", false, true);
    let json = format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":1,"chat_id":1,"is_outgoing":false,"content":{{"@type":"messagePhoto","photo":{{"@type":"photo","has_stickers":false,"sizes":[{{"@type":"photoSize","type":"m","photo":{file},"width":100,"height":80,"progressive_sizes":[]}}]}},"caption":{{"@type":"formattedText","text":"","entities":[]}},"has_spoiler":true,"is_secret":false}}}}}}"#
    );
    let env = parse_envelope(&json).unwrap();
    match env.payload {
        EnvelopePayload::UpdateNewMessage(message) => {
            let MessageContent::Photo(photo) = message.content else {
                panic!("{:?}", message.content);
            };
            assert!(photo.click_requests_download());
            assert_eq!(
                photo.placeholder_label(false, false),
                "Photo (spoiler) — not downloaded"
            );
            assert_eq!(
                photo.placeholder_label(true, false),
                "Photo (spoiler) — downloading…"
            );
            assert_eq!(
                photo.placeholder_label(false, true),
                "Photo (spoiler) — ready"
            );
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn message_voice_note_keeps_duration_waveform_and_listened() {
    let waveform = base64::engine::general_purpose::STANDARD.encode([0xF8, 0x02]);
    let json = format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":8,"chat_id":11,"is_outgoing":false,"content":{{"@type":"messageVoiceNote","voice_note":{{"@type":"voiceNote","duration":12,"waveform":"{waveform}","mime_type":"audio/ogg","speech_recognition_result":null,"voice":{{"@type":"file","id":4,"size":9,"expected_size":9,"local":{{"@type":"localFile","path":"","can_be_downloaded":true,"can_be_deleted":false,"is_downloading_active":false,"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0}},"remote":{{"@type":"remoteFile","id":"CANARY_REMOTE","unique_id":"u","is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":9}}}}}},"caption":{{"@type":"formattedText","text":"","entities":[]}},"is_listened":false}}}}}}"#
    );
    let env = parse_envelope(&json).unwrap();
    let EnvelopePayload::UpdateNewMessage(message) = env.payload else {
        panic!("voice note");
    };
    let MessageContent::VoiceNote(note) = &message.content else {
        panic!("content");
    };
    assert_eq!(note.duration, 12);
    assert_eq!(note.mime_type, "audio/ogg");
    assert!(!note.is_listened);
    assert_eq!(note.file_id, FileId(4));
    assert_eq!(note.waveform, vec![0xF8, 0x02]);
    assert_eq!(message.content.preview(), "Voice message");
    assert_eq!(message.files[0].id, FileId(4));
    let opened =
        parse_envelope(r#"{"@type":"updateMessageContentOpened","chat_id":11,"message_id":8}"#)
            .unwrap();
    match opened.payload {
        EnvelopePayload::UpdateMessageContentOpened {
            chat_id,
            message_id,
        } => {
            assert_eq!(chat_id.0, 11);
            assert_eq!(message_id.0, 8);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn speech_recognition_result_shapes_parse() {
    // MED2: what the parser is ultimately validating — the three
    // TDLib `SpeechRecognitionResult` shapes land on the enum, null
    // and unknown types stay `None` (never a fake result).
    let pending = serde_json::json!({
        "@type": "speechRecognitionResultPending",
        "partial_text": "hel"
    });
    assert_eq!(
        parse_speech_recognition(Some(&pending)),
        Some(SpeechRecognition::Pending {
            partial_text: "hel".into()
        })
    );
    let text = serde_json::json!({
        "@type": "speechRecognitionResultText",
        "text": "hello world"
    });
    assert_eq!(
        parse_speech_recognition(Some(&text)),
        Some(SpeechRecognition::Text {
            text: "hello world".into()
        })
    );
    let error = serde_json::json!({
        "@type": "speechRecognitionResultError",
        "error": { "@type": "error", "code": 400, "message": "SPEECH_RECOGNITION_TOO_MANY" }
    });
    assert_eq!(
        parse_speech_recognition(Some(&error)),
        Some(SpeechRecognition::Error {
            message: "SPEECH_RECOGNITION_TOO_MANY".into()
        })
    );
    assert_eq!(
        parse_speech_recognition(Some(&serde_json::Value::Null)),
        None
    );
    assert_eq!(parse_speech_recognition(None), None);
    let unknown = serde_json::json!({ "@type": "speechRecognitionResultFuture" });
    assert_eq!(parse_speech_recognition(Some(&unknown)), None);
}

#[test]
fn voice_note_transcription_text_parses_end_to_end() {
    let json = r#"{"@type":"updateNewMessage","message":{"id":8,"chat_id":11,"is_outgoing":false,"content":{"@type":"messageVoiceNote","voice_note":{"@type":"voiceNote","duration":12,"waveform":"","mime_type":"audio/ogg","speech_recognition_result":{"@type":"speechRecognitionResultText","text":"buy milk"},"voice":{"@type":"file","id":4,"size":9,"expected_size":9,"local":{"@type":"localFile","path":"","can_be_downloaded":true,"can_be_deleted":false,"is_downloading_active":false,"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0},"remote":{"@type":"remoteFile","id":"r","unique_id":"u","is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":9}}}},"caption":{"@type":"formattedText","text":"","entities":[]},"is_listened":false}}"#;
    let env = parse_envelope(json).unwrap();
    let EnvelopePayload::UpdateNewMessage(message) = env.payload else {
        panic!("voice note");
    };
    let MessageContent::VoiceNote(note) = &message.content else {
        panic!("content");
    };
    assert_eq!(
        note.transcription,
        Some(SpeechRecognition::Text {
            text: "buy milk".into()
        })
    );
}

#[test]
fn message_audio_parses_1_8_67_fields() {
    let mini = base64::engine::general_purpose::STANDARD.encode([0xFF, 0xD8, 0xFF]);
    let track = local_file_json(7, "", false, true);
    let cover = local_file_json(8, "/tmp/cover.jpg", true, true);
    let external = local_file_json(9, "", false, true);
    let json = format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":12,"chat_id":11,"is_outgoing":false,"content":{{"@type":"messageAudio","audio":{{"@type":"audio","duration":214,"title":"Night Drive","performer":"Ada","file_name":"night.mp3","mime_type":"audio/mpeg","album_cover_minithumbnail":{{"@type":"minithumbnail","width":8,"height":8,"data":"{mini}"}},"album_cover_thumbnail":{{"@type":"thumbnail","format":{{"@type":"thumbnailFormatJpeg"}},"width":90,"height":90,"file":{cover}}},"external_album_covers":[{{"@type":"thumbnail","format":{{"@type":"thumbnailFormatJpeg"}},"width":320,"height":320,"file":{external}}}],"audio":{track}}},"caption":{{"@type":"formattedText","text":"from the album","entities":[]}}}}}}}}"#
    );
    let env = parse_envelope(&json).unwrap();
    let EnvelopePayload::UpdateNewMessage(message) = env.payload else {
        panic!("expected message");
    };
    let MessageContent::Audio(audio) = &message.content else {
        panic!("{:?}", message.content);
    };
    assert_eq!(audio.duration, 214);
    assert_eq!(audio.title, "Night Drive");
    assert_eq!(audio.performer, "Ada");
    assert_eq!(audio.file_name, "night.mp3");
    assert_eq!(audio.mime_type, "audio/mpeg");
    assert_eq!(audio.caption, "from the album");
    assert_eq!(audio.file_id, FileId(7));
    assert_eq!(audio.play_file_id(), Some(FileId(7)));
    let mini_thumb = audio.album_cover_minithumbnail.as_ref().expect("mini");
    assert_eq!(mini_thumb.width, 8);
    assert_eq!(mini_thumb.data, vec![0xFF, 0xD8, 0xFF]);
    let cover_thumb = audio.album_cover_thumbnail.as_ref().expect("cover");
    assert_eq!(cover_thumb.file_id, FileId(8));
    assert_eq!(cover_thumb.width, 90);
    assert_eq!(audio.external_album_covers.len(), 1);
    assert_eq!(audio.external_album_covers[0].file_id, FileId(9));
    assert_eq!(audio.cover_file_id(), Some(FileId(8)));
    assert_eq!(message.content.preview(), "from the album");
    assert!(message.files.iter().any(|file| file.id == FileId(7)));
    assert!(message.files.iter().any(|file| file.id == FileId(8)));
    assert!(message.files.iter().any(|file| file.id == FileId(9)));
}

#[test]
fn standalone_animated_emoji_keeps_readable_content() {
    let env = parse_envelope(r#"{"@type":"updateNewMessage","message":{"id":9,"chat_id":4,"content":{"@type":"messageAnimatedEmoji","emoji":"🥰","animated_emoji":null}}}"#).unwrap();
    let EnvelopePayload::UpdateNewMessage(message) = env.payload else {
        panic!("Expected message")
    };
    assert!(matches!(message.content, MessageContent::Text(_)));
    assert_eq!(message.content.preview(), "🥰");
}

#[test]
fn ordinary_service_messages_keep_readable_content() {
    for (kind, expected) in [
        ("messageChatChangeTitle", "Chat renamed to New title"),
        ("messageChatChangePhoto", "Chat photo changed"),
        ("messagePinMessage", "A message was pinned"),
        ("messageCustomServiceAction", "Welcome"),
    ] {
        let value = serde_json::json!({"@type":kind,"title":"New title","text":"Welcome"});
        let (content, _) = parse_content(Some(&value));
        assert_eq!(content, MessageContent::Service(expected.into()));
        assert_eq!(content.preview(), expected);
    }
}
