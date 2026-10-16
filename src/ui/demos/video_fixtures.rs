//! Demo message fixtures for the video, video note and GIF screenshot
//! demos: local clips injected into chat 11 so the shared player (and the
//! GIF panel) have something to show. Moved out of `inline_playback.rs`,
//! which no longer plays video.
#![deny(clippy::indexing_slicing)]

use crate::ui::demo::{demo_file_json, demo_media_allowlist, demo_thumb_png_path};
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::state::{RequestPurpose, Session};
use quill::telegram::client::copy_and_parse;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

pub(in crate::ui) fn apply_ready_gifs(
    session: &mut Session,
    sink: &Arc<MemorySink>,
    seq: &AtomicU64,
) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let frame = demo_media_allowlist()
        .join("demo-gif-1.png")
        .to_string_lossy()
        .into_owned();
    let clip = demo_media_allowlist()
        .join("demo-gif.gif")
        .to_string_lossy()
        .into_owned();
    let thumb = demo_file_json(61, &frame, true);
    let pending = demo_file_json(62, "", false);
    let local_clip = demo_file_json(63, &clip, true);
    let history = format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":501,"chat_id":11,"is_outgoing":false,"content":{{"@type":"messageAnimation","animation":{{"@type":"animation","duration":1,"width":240,"height":140,"file_name":"demo-gif.gif","mime_type":"image/gif","has_stickers":false,"minithumbnail":null,"thumbnail":{{"@type":"thumbnail","format":{{"@type":"thumbnailFormatJpeg"}},"width":240,"height":140,"file":{thumb}}},"animation":{local_clip}}},"caption":{{"@type":"formattedText","text":"","entities":[]}},"show_caption_above_media":false,"has_spoiler":false,"is_secret":false}}}}}}"#
    );
    let waiting = format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":502,"chat_id":11,"is_outgoing":true,"content":{{"@type":"messageAnimation","animation":{{"@type":"animation","duration":2,"width":240,"height":140,"file_name":"saved.mp4","mime_type":"video/mp4","has_stickers":false,"minithumbnail":null,"thumbnail":null,"animation":{pending}}},"caption":{{"@type":"formattedText","text":"","entities":[]}},"show_caption_above_media":false,"has_spoiler":false,"is_secret":false}}}}}}"#
    );
    for json in [history, waiting] {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
    session.stickers.gifs.open = true;
    session.stickers.gifs.loading = true;
    let extra = session.request(RequestPurpose::GetSavedAnimations, None);
    let saved = format!(
        r#"{{"@type":"animations","@extra":"{}","animations":[{{"@type":"animation","duration":1,"width":240,"height":140,"file_name":"demo-gif.gif","mime_type":"image/gif","has_stickers":false,"minithumbnail":null,"thumbnail":{{"@type":"thumbnail","format":{{"@type":"thumbnailFormatJpeg"}},"width":240,"height":140,"file":{thumb}}},"animation":{local_clip}}},{{"@type":"animation","duration":2,"width":240,"height":140,"file_name":"saved.mp4","mime_type":"video/mp4","has_stickers":false,"minithumbnail":null,"thumbnail":null,"animation":{pending}}}]}}"#,
        extra.0
    );
    if let Some(owned) = copy_and_parse(&saved, seq, &dyn_sink) {
        session.apply(owned);
    }
}

pub(in crate::ui) fn apply_ready_video(
    session: &mut Session,
    sink: &Arc<MemorySink>,
    seq: &AtomicU64,
) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let thumb_path = demo_thumb_png_path();
    let clip_path = demo_media_allowlist()
        .join("demo-clip-12s.mp4")
        .to_string_lossy()
        .into_owned();
    let thumb = demo_file_json(71, &thumb_path, true);
    let clip = demo_file_json(72, &clip_path, true);
    let pending = demo_file_json(73, "", false);
    let playing = format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":601,"chat_id":11,"is_outgoing":false,"content":{{"@type":"messageVideo","video":{{"@type":"video","duration":12,"width":640,"height":360,"file_name":"clip.mp4","mime_type":"video/mp4","has_stickers":false,"supports_streaming":true,"minithumbnail":null,"thumbnail":{{"@type":"thumbnail","format":{{"@type":"thumbnailFormatJpeg"}},"width":240,"height":140,"file":{thumb}}},"video":{clip}}},"alternative_videos":[],"storyboards":[],"cover":null,"start_timestamp":0,"caption":{{"@type":"formattedText","text":"Beach clip","entities":[]}},"show_caption_above_media":false,"has_spoiler":false,"is_secret":false}}}}}}"#
    );
    let waiting = format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":602,"chat_id":11,"is_outgoing":true,"content":{{"@type":"messageVideo","video":{{"@type":"video","duration":3,"width":320,"height":180,"file_name":"pending.mp4","mime_type":"video/mp4","has_stickers":false,"supports_streaming":false,"minithumbnail":null,"thumbnail":null,"video":{pending}}},"alternative_videos":[],"storyboards":[],"cover":null,"start_timestamp":0,"caption":{{"@type":"formattedText","text":"","entities":[]}},"show_caption_above_media":false,"has_spoiler":false,"is_secret":false}}}}}}"#
    );
    let drop_seed = r#"{"@type":"updateDeleteMessages","chat_id":11,"message_ids":[101,102,103],"is_permanent":true,"from_cache":false}"#;
    for json in [playing, waiting, drop_seed.to_string()] {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
}

/// Demo fixture (Parity slice 5): inject a downloaded video message into
/// chat 11 of a Media-seeded demo session. The thumbnail (file 97) and the
/// full clip (file 96, `demo-clip.mp4`) are both local/completed so the
/// viewer opens straight onto playback. The 12 s duration is fixture data
/// for the screenshot — the real fixture clip is 1 s.
pub(in crate::ui) fn apply_ready_video_viewer(
    session: &mut Session,
    sink: &Arc<MemorySink>,
    seq: &AtomicU64,
) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let clip_path = demo_media_allowlist()
        .join("demo-clip-12s.mp4")
        .to_string_lossy()
        .into_owned();
    let thumb_path = demo_thumb_png_path();
    let clip = demo_file_json(96, &clip_path, true);
    let thumb = demo_file_json(97, &thumb_path, true);
    let json = format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":204,"chat_id":11,"is_outgoing":false,"content":{{"@type":"messageVideo","video":{{"@type":"video","duration":12,"width":320,"height":180,"file_name":"demo-clip-12s.mp4","mime_type":"video/mp4","has_stickers":false,"supports_streaming":true,"minithumbnail":null,"thumbnail":{{"@type":"thumbnail","format":{{"@type":"thumbnailFormatJpeg"}},"width":240,"height":140,"file":{thumb}}},"video":{clip}}},"alternative_videos":[],"storyboards":[],"cover":null,"start_timestamp":0,"caption":{{"@type":"formattedText","text":"Demo clip","entities":[]}},"show_caption_above_media":false,"has_spoiler":false,"is_secret":false}}}}}}"#
    );
    if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
        session.apply(owned);
    }
}

pub(in crate::ui) fn apply_ready_video_note(
    session: &mut Session,
    sink: &Arc<MemorySink>,
    seq: &AtomicU64,
) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let thumb_path = demo_thumb_png_path();
    let clip_path = demo_media_allowlist()
        .join("demo-clip.mp4")
        .to_string_lossy()
        .into_owned();
    let thumb = demo_file_json(91, &thumb_path, true);
    let clip = demo_file_json(92, &clip_path, true);
    let pending = demo_file_json(93, "", false);
    let playing = format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":611,"chat_id":11,"is_outgoing":false,"content":{{"@type":"messageVideoNote","video_note":{{"@type":"videoNote","duration":8,"waveform":"","length":240,"minithumbnail":null,"thumbnail":{{"@type":"thumbnail","format":{{"@type":"thumbnailFormatJpeg"}},"width":120,"height":120,"file":{thumb}}},"speech_recognition_result":null,"video":{clip}}},"is_viewed":false,"is_secret":false}}}}}}"#
    );
    let waiting = format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":612,"chat_id":11,"is_outgoing":true,"content":{{"@type":"messageVideoNote","video_note":{{"@type":"videoNote","duration":3,"waveform":"","length":240,"minithumbnail":null,"thumbnail":null,"speech_recognition_result":null,"video":{pending}}},"is_viewed":true,"is_secret":false}}}}}}"#
    );
    let drop_seed = r#"{"@type":"updateDeleteMessages","chat_id":11,"message_ids":[101,102,103],"is_permanent":true,"from_cache":false}"#;
    for json in [playing, waiting, drop_seed.to_string()] {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
}

pub(in crate::ui) fn apply_ready_video_note_send(
    session: &mut Session,
    sink: &Arc<MemorySink>,
    seq: &AtomicU64,
) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let clip_path = demo_media_allowlist()
        .join("demo-video-note.mp4")
        .to_string_lossy()
        .into_owned();
    let thumb_path = demo_thumb_png_path();
    let clip = demo_file_json(94, &clip_path, true);
    let thumb = demo_file_json(95, &thumb_path, true);
    let sent = format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":721,"chat_id":11,"is_outgoing":true,"content":{{"@type":"messageVideoNote","video_note":{{"@type":"videoNote","duration":1,"waveform":"","length":240,"minithumbnail":null,"thumbnail":{{"@type":"thumbnail","format":{{"@type":"thumbnailFormatJpeg"}},"width":240,"height":240,"file":{thumb}}},"speech_recognition_result":null,"video":{clip}}},"is_viewed":true,"is_secret":false}}}}}}"#
    );
    let drop_seed = r#"{"@type":"updateDeleteMessages","chat_id":11,"message_ids":[101,102,103],"is_permanent":true,"from_cache":false}"#;
    for json in [sent, drop_seed.to_string()] {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
}

pub(in crate::ui) fn apply_ready_video_send(
    session: &mut Session,
    sink: &Arc<MemorySink>,
    seq: &AtomicU64,
) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let clip_path = demo_media_allowlist()
        .join("demo-clip.mp4")
        .to_string_lossy()
        .into_owned();
    let thumb_path = demo_thumb_png_path();
    let clip = demo_file_json(81, &clip_path, true);
    let thumb = demo_file_json(82, &thumb_path, true);
    let sent = format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":701,"chat_id":11,"is_outgoing":true,"content":{{"@type":"messageVideo","video":{{"@type":"video","duration":1,"width":320,"height":180,"file_name":"demo-clip.mp4","mime_type":"video/mp4","has_stickers":false,"supports_streaming":true,"minithumbnail":null,"thumbnail":{{"@type":"thumbnail","format":{{"@type":"thumbnailFormatJpeg"}},"width":240,"height":140,"file":{thumb}}},"video":{clip}}},"alternative_videos":[],"storyboards":[],"cover":null,"start_timestamp":0,"caption":{{"@type":"formattedText","text":"Sent clip","entities":[]}},"show_caption_above_media":false,"has_spoiler":false,"is_secret":false}}}}}}"#
    );
    let drop_seed = r#"{"@type":"updateDeleteMessages","chat_id":11,"message_ids":[101,102,103],"is_permanent":true,"from_cache":false}"#;
    for json in [sent, drop_seed.to_string()] {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
}

/// Test fixture: an animation message `message_id` in chat 11 whose file
/// (`file_id`) is the local `demo-clip.mp4`, labelled `mime` (an MP4 GIF,
/// or a video passed off as `image/gif`).
#[cfg(test)]
#[cfg(feature = "demo-capture")]
pub(in crate::ui) fn apply_clip_animation(
    session: &mut Session,
    sink: &Arc<MemorySink>,
    seq: &AtomicU64,
    (message_id, file_id): (i64, i32),
    mime: &str,
) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let clip_path = demo_media_allowlist()
        .join("demo-clip.mp4")
        .to_string_lossy()
        .into_owned();
    let clip = demo_file_json(file_id, &clip_path, true);
    let json = format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":{message_id},"chat_id":11,"is_outgoing":false,"content":{{"@type":"messageAnimation","animation":{{"@type":"animation","duration":1,"width":320,"height":180,"file_name":"clip.mp4","mime_type":"{mime}","has_stickers":false,"minithumbnail":null,"thumbnail":null,"animation":{clip}}},"caption":{{"@type":"formattedText","text":"","entities":[]}},"show_caption_above_media":false,"has_spoiler":false,"is_secret":false}}}}}}"#
    );
    if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
        session.apply(owned);
    }
}
