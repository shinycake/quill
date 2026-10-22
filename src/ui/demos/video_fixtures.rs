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
        r#"{{"@type":"animations","@extra":"{}","animations":[{}]}}"#,
        extra.0,
        saved_gifs(&thumb, &local_clip, &pending).join(",")
    );
    if let Some(owned) = copy_and_parse(&saved, seq, &dyn_sink) {
        session.apply(owned);
    }
    // `updateAnimationSearchParameters`' suggested searches.
    session.stickers.gifs.provider_emojis = ["👍", "😘", "😍", "😡", "🥳", "😂", "😮", "🙄", "😎"]
        .map(String::from)
        .to_vec();
}

/// The saved GIFs of the GIF panel demo, like a real collection: mixed
/// shapes, most with an MP4 preview loop for a thumbnail (as Telegram
/// sends them), some with a still JPEG, one with only its blurred
/// minithumbnail and one with nothing downloaded yet.
fn saved_gifs(thumb: &str, local_clip: &str, pending: &str) -> Vec<String> {
    let fixture = |name: &str| {
        demo_media_allowlist()
            .join(name)
            .to_string_lossy()
            .into_owned()
    };
    let mini = format!(
        r#"{{"@type":"minithumbnail","width":40,"height":30,"data":"{}"}}"#,
        crate::ui::demo::DEMO_MINITHUMB
    );
    let thumbnail = |format: &str, width: i32, height: i32, file: &str| {
        format!(
            r#"{{"@type":"thumbnail","format":{{"@type":"{format}"}},"width":{width},"height":{height},"file":{file}}}"#
        )
    };
    let animation = |name: &str, width: i32, height: i32, mini: &str, thumb: &str, clip: &str| {
        format!(
            r#"{{"@type":"animation","duration":2,"width":{width},"height":{height},"file_name":"{name}","mime_type":"video/mp4","has_stickers":false,"minithumbnail":{mini},"thumbnail":{thumb},"animation":{clip}}}"#
        )
    };
    let loop_file = |id: i32, name: &str| demo_file_json(id, &fixture(name), true);
    let gifs = [
        (
            "demo-gif.gif",
            240,
            140,
            "null".to_string(),
            thumbnail("thumbnailFormatJpeg", 240, 140, thumb),
            local_clip.to_string(),
        ),
        (
            "steve-carell-no.mp4",
            384,
            216,
            mini.clone(),
            thumbnail(
                "thumbnailFormatMpeg4",
                192,
                108,
                &loop_file(64, "demo-gif-loop-wide.mp4"),
            ),
            demo_file_json(74, "", false),
        ),
        (
            "corgi-smirk.mp4",
            240,
            240,
            "null".to_string(),
            thumbnail(
                "thumbnailFormatMpeg4",
                120,
                120,
                &loop_file(65, "demo-gif-loop-square.mp4"),
            ),
            demo_file_json(75, "", false),
        ),
        (
            "коза.mp4",
            192,
            256,
            "null".to_string(),
            thumbnail(
                "thumbnailFormatMpeg4",
                96,
                128,
                &loop_file(66, "demo-gif-loop-tall.mp4"),
            ),
            demo_file_json(76, "", false),
        ),
        (
            "sunset-pan.mp4",
            400,
            200,
            "null".to_string(),
            thumbnail(
                "thumbnailFormatJpeg",
                200,
                100,
                &loop_file(67, "demo-gif-still-wide.jpg"),
            ),
            demo_file_json(77, "", false),
        ),
        (
            "waiting-for-thumbnail.mp4",
            320,
            240,
            mini,
            "null".to_string(),
            demo_file_json(78, "", false),
        ),
        (
            "mountain-loop.mp4",
            320,
            240,
            "null".to_string(),
            thumbnail(
                "thumbnailFormatJpeg",
                160,
                120,
                &loop_file(68, "demo-gif-still-photo.jpg"),
            ),
            demo_file_json(79, "", false),
        ),
        (
            "saved.mp4",
            240,
            140,
            "null".to_string(),
            "null".to_string(),
            pending.to_string(),
        ),
        (
            "dance-loop.mp4",
            480,
            270,
            "null".to_string(),
            thumbnail(
                "thumbnailFormatMpeg4",
                192,
                108,
                &loop_file(69, "demo-gif-loop-wide.mp4"),
            ),
            demo_file_json(80, "", false),
        ),
    ];
    gifs.iter()
        .map(|(name, width, height, mini, thumb, clip)| {
            animation(name, *width, *height, mini, thumb, clip)
        })
        .collect()
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
    let custom = demo_viewer_clip();
    let ((width, height), clip_path) = custom.clone().unwrap_or_else(|| {
        let path = demo_media_allowlist().join("demo-clip-12s.mp4");
        ((320, 180), path.to_string_lossy().into_owned())
    });
    // A clip from elsewhere plays only as one you sent (`playable_clip_path`).
    let outgoing = custom.is_some();
    let thumb_path = demo_thumb_png_path();
    let clip = demo_file_json(96, &clip_path, true);
    let thumb = demo_file_json(97, &thumb_path, true);
    let json = format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":204,"chat_id":11,"is_outgoing":{outgoing},"content":{{"@type":"messageVideo","video":{{"@type":"video","duration":12,"width":{width},"height":{height},"file_name":"demo-clip-12s.mp4","mime_type":"video/mp4","has_stickers":false,"supports_streaming":true,"minithumbnail":null,"thumbnail":{{"@type":"thumbnail","format":{{"@type":"thumbnailFormatJpeg"}},"width":240,"height":140,"file":{thumb}}},"video":{clip}}},"alternative_videos":[],"storyboards":[],"cover":null,"start_timestamp":0,"caption":{{"@type":"formattedText","text":"Demo clip with a longer caption that wraps across several lines in the viewer, the way a real message caption often does when someone writes a paragraph under a video, so the bottom bar has to make room for it","entities":[]}},"show_caption_above_media":false,"has_spoiler":false,"is_secret":false}}}}}}"#
    );
    if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
        session.apply(owned);
    }
}

/// `QUILL_DEMO_VIEWER_CLIP=<w>x<h>:<path>`: the viewer demos play that
/// clip (a bright, dark or vertical picture for a capture) instead of the
/// 12 s test pattern.
fn demo_viewer_clip() -> Option<((u32, u32), String)> {
    let value = std::env::var("QUILL_DEMO_VIEWER_CLIP").ok()?;
    let (size, path) = value.split_once(':')?;
    let (width, height) = size.split_once('x')?;
    Some((
        (width.parse().ok()?, height.parse().ok()?),
        path.to_string(),
    ))
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
