//! message media attachments: photo/video/voice/sticker/location/dice rendering.

use super::app::QuillApp;
use super::pressable::PressableDiv;
use super::*;
use gpui_kit::component::button::*;
use gpui_kit::component::slider::Slider;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::{ChatId, FileId, MessageId};
use quill::local_path::sandboxed_display_path;
use quill::media_viewer::MediaViewerItem;
use quill::state::Session;
use quill::story_viewer::StoryViewerItem;
use quill::telegram::client::copy_and_parse;
use quill::telegram::envelope::{ParsedFile, SpeechRecognition};
use quill::voice::{self, format_voice_duration};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
/// `ReadyLocation` fixture (Phase 4.3): open a dedicated "Demo places" chat
/// (id 16) and inject four messages through the normal reducer — a plain
/// `messageLocation` (coordinates + accuracy), a `messageLiveLocation`
/// (live period / expires / heading / proximity alert), a `messageVenue`
/// (title + address + provider), and a `messageContact` (name + phone +
/// vCard + user_id). All data is synthetic; no live Telegram.
pub(super) fn apply_ready_location(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let chat_id = 16;
    let chat_json = format!(
        r#"{{"@type":"updateNewChat","chat":{{"id":{chat_id},"title":"Demo places","type":{{"@type":"chatTypePrivate","user_id":{chat_id}}},"unread_count":0}}}}"#
    );
    let position_json = format!(
        r#"{{"@type":"updateChatPosition","chat_id":{chat_id},"position":{{"@type":"chatPosition","list":{{"@type":"chatListMain"}},"order":"50","is_pinned":false}}}}"#
    );
    for json in [chat_json, position_json] {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
    session.open_chat(ChatId(chat_id));

    let message = |message_id: i32, content: &str| {
        format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":{message_id},"chat_id":{chat_id},"is_outgoing":false,"content":{content}}}}}"#
        )
    };

    // Plain location: San Francisco, ±15 m accuracy.
    let location = message(
        108,
        r#"{"@type":"messageLocation","location":{"@type":"location","latitude":37.7749,"longitude":-122.4194,"horizontal_accuracy":15}}"#,
    );
    // Live location: Paris, 15-minute live period, 10 minutes left,
    // heading 90°, 500 m proximity alert.
    let live_location = message(
        109,
        r#"{"@type":"messageLiveLocation","location":{"@type":"liveLocation","location":{"@type":"location","latitude":48.8566,"longitude":2.3522,"horizontal_accuracy":0},"live_period":900,"heading":90,"proximity_alert_radius":500},"expires_in":600}"#,
    );
    // Venue: Ferry Building, via foursquare.
    let venue = message(
        110,
        r#"{"@type":"messageVenue","venue":{"@type":"venue","location":{"@type":"location","latitude":37.7955,"longitude":-122.3937,"horizontal_accuracy":0},"title":"Ferry Building","address":"1 Ferry Building, San Francisco","provider":"foursquare","id":"4a1a2b3c","type":"Food"}}"#,
    );
    // Contact: Ada Lovelace with a vCard and a known Telegram user id.
    let contact = message(
        111,
        r#"{"@type":"messageContact","contact":{"@type":"contact","phone_number":"+14155550123","first_name":"Ada","last_name":"Lovelace","vcard":"BEGIN:VCARD\nFN:Ada Lovelace\nEND:VCARD","user_id":123456789}}"#,
    );

    for json in [location, live_location, venue, contact] {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
}

/// Phase 4.4: inject a "Demo dice" chat with three `messageDice` rolls —
/// an incoming 🎲 = 4, an outgoing 🎲 = 6, and an incoming 🎯 = 5.
/// All data is synthetic; no live Telegram.
pub(super) fn apply_ready_dice(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let chat_id = 17;
    let chat_json = format!(
        r#"{{"@type":"updateNewChat","chat":{{"id":{chat_id},"title":"Demo dice","type":{{"@type":"chatTypePrivate","user_id":{chat_id}}},"unread_count":0}}}}"#
    );
    let position_json = format!(
        r#"{{"@type":"updateChatPosition","chat_id":{chat_id},"position":{{"@type":"chatPosition","list":{{"@type":"chatListMain"}},"order":"50","is_pinned":false}}}}"#
    );
    for json in [chat_json, position_json] {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
    session.open_chat(ChatId(chat_id));

    let message = |message_id: i32, outgoing: bool, content: &str| {
        format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":{message_id},"chat_id":{chat_id},"is_outgoing":{outgoing},"content":{content}}}}}"#
        )
    };

    // Incoming 🎲 = 4.
    let roll_one = message(
        112,
        false,
        r#"{"@type":"messageDice","emoji":"🎲","value":4,"success_animation_frame_number":0}"#,
    );
    // Outgoing 🎲 = 6.
    let roll_two = message(
        113,
        true,
        r#"{"@type":"messageDice","emoji":"🎲","value":6,"success_animation_frame_number":0}"#,
    );
    // Incoming 🎯 = 5.
    let roll_three = message(
        114,
        false,
        r#"{"@type":"messageDice","emoji":"🎯","value":5,"success_animation_frame_number":0}"#,
    );

    for json in [roll_one, roll_two, roll_three] {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
}

pub(super) fn photo_display_path(
    photo: &quill::telegram::envelope::PhotoContent,
    files: &HashMap<i32, ParsedFile>,
    roots: &[PathBuf],
) -> Option<PathBuf> {
    let mut ids = Vec::new();
    if let Some(size) = photo.thumb_size() {
        ids.push(size.file_id);
    }
    if let Some(size) = photo.largest_size() {
        ids.push(size.file_id);
    }
    for id in ids {
        if let Some(path) = files.get(&id.0).and_then(|f| f.usable_path())
            && let Some(safe) = sandboxed_display_path(path, roots)
        {
            return Some(safe);
        }
    }
    for size in &photo.sizes {
        if let Some(path) = files.get(&size.file_id.0).and_then(|f| f.usable_path())
            && let Some(safe) = sandboxed_display_path(path, roots)
        {
            return Some(safe);
        }
    }
    None
}

/// Phase 4.5: resolve the current viewer item's visual — first local
/// display candidate inside the media allowlist roots, same sandbox rule as
/// history rows (`sandboxed_display_path`).
pub(super) fn viewer_display_path(
    item: &MediaViewerItem,
    files: &HashMap<i32, ParsedFile>,
    roots: &[PathBuf],
) -> Option<PathBuf> {
    item.display_file_ids.iter().find_map(|id| {
        files
            .get(&id.0)
            .and_then(|file| file.usable_path())
            .and_then(|path| sandboxed_display_path(path, roots))
    })
}

pub(super) fn story_viewer_display_path(
    item: &StoryViewerItem,
    files: &HashMap<i32, ParsedFile>,
    roots: &[PathBuf],
) -> Option<PathBuf> {
    item.display_file_ids.iter().find_map(|id| {
        files
            .get(&id.0)
            .and_then(|file| file.usable_path())
            .and_then(|path| sandboxed_display_path(path, roots))
    })
}

pub(super) fn file_is_downloading(
    file_id: FileId,
    files: &HashMap<i32, ParsedFile>,
    downloading: &std::collections::HashSet<i32>,
) -> bool {
    downloading.contains(&file_id.0)
        || files
            .get(&file_id.0)
            .is_some_and(|f| f.local.is_downloading_active)
}

pub(super) fn photo_attachment(
    row_id: u64,
    photo: &quill::telegram::envelope::PhotoContent,
    files: &HashMap<i32, ParsedFile>,
    downloading: &std::collections::HashSet<i32>,
    media_roots: &[PathBuf],
    sponsored: Option<(ChatId, i64)>,
    // Phase 4.5: `(chat_id, message_id)` when a click should open the
    // fullscreen viewer (history rows only; album tiles and sponsored rows
    // pass `None`).
    viewer: Option<(ChatId, MessageId)>,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let open_id = photo.open_file_id().unwrap_or(FileId(0));
    if !photo.is_secret
        && !photo.has_spoiler
        && let Some(path) = photo_display_path(photo, files, media_roots)
    {
        return img(path)
            .id(("photo-img", row_id))
            .mt_2()
            .w(px(240.))
            .h(px(140.))
            .rounded_md()
            .object_fit(ObjectFit::Cover)
            .when_some(viewer, |this, (chat_id, message_id)| {
                this.cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.open_media_viewer(chat_id, message_id, cx);
                    }))
            })
            .with_fallback(|| {
                div()
                    .w(px(240.))
                    .h(px(140.))
                    .rounded_md()
                    .bg(fill_muted())
                    .flex()
                    .items_center()
                    .justify_center()
                    .child("Photo")
                    .into_any_element()
            })
            .into_any_element();
    }
    let (w, h) = photo
        .largest_size()
        .or_else(|| photo.thumb_size())
        .map(|s| (s.width, s.height))
        .unwrap_or((0, 0));
    let downloading_now = file_is_downloading(open_id, files, downloading);
    let ready = files
        .get(&open_id.0)
        .and_then(|f| f.usable_path())
        .is_some();
    let status = if photo.is_secret || photo.has_spoiler {
        photo.placeholder_label(downloading_now, ready)
    } else if downloading_now {
        "Photo — downloading…".into()
    } else if w > 0 && h > 0 {
        format!("Photo {w}×{h} — not downloaded")
    } else {
        "Photo — not downloaded".into()
    };
    let viewable = !photo.is_secret && !photo.has_spoiler;
    let viewer_open = viewable.then_some(viewer).flatten();
    let has_viewer_open = viewer_open.is_some();
    div()
        .id(("photo-ph", row_id))
        .mt_2()
        .w(px(240.))
        .h(px(88.))
        .rounded_md()
        .bg(fill_muted())
        .flex()
        .items_center()
        .justify_center()
        // Phase 4.5: viewable photos open the viewer (it triggers the
        // download when needed); spoiler photos keep the old
        // click-to-download placeholder, secret photos stay inert.
        .when_some(viewer_open, |this, (chat_id, message_id)| {
            this.cursor_pointer()
                .pressable(cx.theme())
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.open_media_viewer(chat_id, message_id, cx);
                }))
        })
        .when(
            !has_viewer_open && photo.click_requests_download(),
            |this| {
                this.cursor_pointer()
                    .pressable(cx.theme())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.request_media_download(open_id, sponsored, cx);
                    }))
            },
        )
        .child(div().text_xs().text_color(text_bright()).child(status))
        .into_any_element()
}

pub(super) fn animation_attachment(
    message_id: MessageId,
    animation: &quill::telegram::envelope::AnimationContent,
    files: &HashMap<i32, ParsedFile>,
    downloading: &std::collections::HashSet<i32>,
    media_roots: &[PathBuf],
    playing: bool,
    frame: Option<&std::path::Path>,
    sponsored: Option<(ChatId, i64)>,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let row_id = message_id.0 as u64;
    let play_id = animation.play_file_id().unwrap_or(FileId(0));
    let thumb_id = animation.thumb_file_id().unwrap_or(FileId(0));
    let mime = animation.mime_type.clone();
    let play_label = if playing { "Pause" } else { "Play" };
    let visual = if playing {
        frame.and_then(|path| sandboxed_display_path(&path.to_string_lossy(), media_roots))
    } else {
        None
    };
    let visual = visual.or_else(|| {
        [thumb_id, play_id].into_iter().find_map(|id| {
            if id.0 == 0 {
                return None;
            }
            files
                .get(&id.0)
                .and_then(|file| file.usable_path())
                .and_then(|path| sandboxed_display_path(path, media_roots))
        })
    });
    let downloading_now = file_is_downloading(play_id, files, downloading)
        || file_is_downloading(thumb_id, files, downloading);
    let blocked = animation.is_secret || animation.has_spoiler;
    let picture = if !blocked && let Some(path) = visual {
        img(path)
            .id(("gif-img", row_id))
            .w(px(240.))
            .h(px(140.))
            .rounded_md()
            .object_fit(ObjectFit::Cover)
            .with_fallback(|| {
                div()
                    .w(px(240.))
                    .h(px(140.))
                    .rounded_md()
                    .bg(accent_strong())
                    .into_any_element()
            })
            .into_any_element()
    } else {
        let label = if blocked {
            "GIF".to_string()
        } else if downloading_now {
            "GIF — downloading…".into()
        } else if animation.width > 0 && animation.height > 0 {
            format!(
                "GIF {}×{} — not downloaded",
                animation.width, animation.height
            )
        } else {
            "GIF — not downloaded".into()
        };
        div()
            .id(("gif-ph", row_id))
            .w(px(240.))
            .h(px(140.))
            .rounded_md()
            .bg(accent_strong())
            .flex()
            .items_center()
            .justify_center()
            .child(div().text_xs().text_color(text_on_fill()).child(label))
            .into_any_element()
    };
    div()
        .id(("gif-row", row_id))
        .mt_2()
        .flex()
        .flex_col()
        .gap_1()
        .child(
            div().relative().child(picture).child(
                div()
                    .absolute()
                    .top_1()
                    .left_1()
                    .px_1()
                    .rounded_sm()
                    .bg(bg_deep())
                    .text_xs()
                    .text_color(text_bright())
                    .child(if playing { "GIF · playing" } else { "GIF" }),
            ),
        )
        .child(
            Button::new(format!("gif-play-{row_id}"))
                .label(play_label)
                .on_click(cx.listener(move |this, _, _, cx| {
                    if blocked {
                        return;
                    }
                    if let Some((chat_id, sponsored_id)) = sponsored {
                        this.click_sponsored_message(chat_id, sponsored_id, true, cx);
                    }
                    this.toggle_animation_playback(message_id, play_id, mime.clone(), cx);
                })),
        )
        .into_any_element()
}

pub(super) fn video_attachment(
    message_id: MessageId,
    video: &quill::telegram::envelope::VideoContent,
    files: &HashMap<i32, ParsedFile>,
    downloading: &std::collections::HashSet<i32>,
    media_roots: &[PathBuf],
    playing: bool,
    frame: Option<&std::path::Path>,
    sponsored: Option<(ChatId, i64)>,
    // Phase 4.5: `(chat_id, message_id)` when a click should open the
    // fullscreen viewer (history rows only; sponsored rows pass `None`).
    // Secret/spoiler videos never get the handler.
    viewer: Option<(ChatId, MessageId)>,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let row_id = message_id.0 as u64;
    let play_id = video.play_file_id().unwrap_or(FileId(0));
    let thumb_id = video.thumb_file_id().unwrap_or(FileId(0));
    let mime = video.mime_type.clone();
    let start_timestamp = video.start_timestamp;
    let play_label = if playing { "Pause" } else { "Play" };
    let duration = format_voice_duration(video.duration);
    let visual = if playing {
        frame.and_then(|path| sandboxed_display_path(&path.to_string_lossy(), media_roots))
    } else {
        None
    };
    let visual = visual.or_else(|| {
        [thumb_id, play_id].into_iter().find_map(|id| {
            if id.0 == 0 {
                return None;
            }
            files
                .get(&id.0)
                .and_then(|file| file.usable_path())
                .and_then(|path| sandboxed_display_path(path, media_roots))
        })
    });
    let downloading_now = file_is_downloading(play_id, files, downloading)
        || file_is_downloading(thumb_id, files, downloading);
    let blocked = video.is_secret || video.has_spoiler;
    let picture = if !blocked && let Some(path) = visual {
        img(path)
            .id(("video-img", row_id))
            .w(px(240.))
            .h(px(140.))
            .rounded_md()
            .object_fit(ObjectFit::Cover)
            .with_fallback(|| {
                div()
                    .w(px(240.))
                    .h(px(140.))
                    .rounded_md()
                    .bg(success_bg())
                    .into_any_element()
            })
            .into_any_element()
    } else {
        let label = if blocked {
            "Video".to_string()
        } else if downloading_now {
            "Video — downloading…".into()
        } else if video.width > 0 && video.height > 0 {
            format!("Video {}×{} — not downloaded", video.width, video.height)
        } else {
            "Video — not downloaded".into()
        };
        div()
            .id(("video-ph", row_id))
            .w(px(240.))
            .h(px(140.))
            .rounded_md()
            .bg(success_bg())
            .flex()
            .items_center()
            .justify_center()
            .child(div().text_xs().text_color(text_on_fill()).child(label))
            .into_any_element()
    };
    div()
        .id(("video-row", row_id))
        .mt_2()
        .flex()
        .flex_col()
        .gap_1()
        .child({
            // Phase 4.5: clicking the visual opens the viewer (the viewer
            // triggers the download when nothing is local yet). The
            // Play/Pause button below keeps its own handler.
            let viewer_open = (!blocked).then_some(viewer).flatten();
            div()
                .id(("video-visual", row_id))
                .relative()
                .when_some(viewer_open, |this, (chat_id, message_id)| {
                    this.cursor_pointer()
                        .pressable(cx.theme())
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.open_media_viewer(chat_id, message_id, cx);
                        }))
                })
                .child(picture)
                .child(
                    div()
                        .absolute()
                        .top_1()
                        .left_1()
                        .px_1()
                        .rounded_sm()
                        .bg(bg_deep())
                        .text_xs()
                        .text_color(text_bright())
                        .child(if playing { "Video · playing" } else { "Video" }),
                )
                .child(
                    div()
                        .absolute()
                        .bottom_1()
                        .left_1()
                        .px_1()
                        .rounded_sm()
                        .bg(bg_deep())
                        .text_xs()
                        .text_color(text_bright())
                        .child(duration),
                )
        })
        .child(
            Button::new(format!("video-play-{row_id}"))
                .label(play_label)
                .on_click(cx.listener(move |this, _, _, cx| {
                    if blocked {
                        return;
                    }
                    if let Some((chat_id, sponsored_id)) = sponsored {
                        this.click_sponsored_message(chat_id, sponsored_id, true, cx);
                    }
                    this.toggle_video_playback(
                        message_id,
                        play_id,
                        mime.clone(),
                        start_timestamp,
                        None,
                        cx,
                    );
                })),
        )
        .into_any_element()
}

/// Round video note. tdesktop paints `history/view/media` round video as a
/// circle (JPEG thumb, duration, play). Quill uses the same ffmpeg frames as
/// `messageVideo`, clipped to a circle. Diameter on screen is fixed; schema
/// `length` is the sender's pixel size, shown when the file is not local yet.
/// MED2: the transcription row under a voice/video note. When TDLib has
/// delivered a `speechRecognitionResult` (via `updateMessageContent`), the
/// transcript shows; otherwise the row offers a real `recognizeSpeech`
/// request — pending/error states are shown honestly, never faked.
pub(super) fn transcription_row(
    chat_id: ChatId,
    message_id: MessageId,
    transcription: &Option<SpeechRecognition>,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    match transcription {
        None => div()
            .child(
                Button::new(format!("transcribe-{row}", row = message_id.0))
                    .label("Transcribe")
                    .tooltip("Send speech-recognition request to Telegram")
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.request_transcription(chat_id, message_id, cx);
                    })),
            )
            .into_any_element(),
        Some(SpeechRecognition::Pending { partial_text }) => div()
            .text_xs()
            .text_color(text_muted())
            .child(if partial_text.is_empty() {
                "Transcribing…".to_string()
            } else {
                format!("Transcribing… {partial_text}")
            })
            .into_any_element(),
        Some(SpeechRecognition::Text { text }) => div()
            .text_xs()
            .text_color(text_bright())
            .child(format!("“{text}”"))
            .into_any_element(),
        Some(SpeechRecognition::Error { message }) => div()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .text_xs()
                    .text_color(danger())
                    .child(format!("Transcription failed: {message}")),
            )
            .child(
                Button::new(format!("transcribe-retry-{row}", row = message_id.0))
                    .label("Retry")
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.request_transcription(chat_id, message_id, cx);
                    })),
            )
            .into_any_element(),
    }
}

pub(super) fn video_note_attachment(
    chat_id: ChatId,
    message_id: MessageId,
    outgoing: bool,
    note: &quill::telegram::envelope::VideoNoteContent,
    files: &HashMap<i32, ParsedFile>,
    downloading: &std::collections::HashSet<i32>,
    media_roots: &[PathBuf],
    playing: bool,
    frame: Option<&std::path::Path>,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let row_id = message_id.0 as u64;
    let play_id = note.play_file_id().unwrap_or(FileId(0));
    let thumb_id = note.thumb_file_id().unwrap_or(FileId(0));
    let play_label = if playing { "Pause" } else { "Play" };
    let duration = format_voice_duration(note.duration);
    let visual = if playing {
        frame.and_then(|path| sandboxed_display_path(&path.to_string_lossy(), media_roots))
    } else {
        None
    };
    let visual = visual.or_else(|| {
        [thumb_id, play_id].into_iter().find_map(|id| {
            if id.0 == 0 {
                return None;
            }
            files
                .get(&id.0)
                .and_then(|file| file.usable_path())
                .and_then(|path| sandboxed_display_path(path, media_roots))
        })
    });
    let downloading_now = file_is_downloading(play_id, files, downloading)
        || file_is_downloading(thumb_id, files, downloading);
    let blocked = note.is_secret;
    let unseen = !outgoing && !note.is_viewed && !playing;
    let badge = if playing {
        "Video note · playing"
    } else if unseen {
        "New · Video note"
    } else {
        "Video note"
    };
    let ring = if playing {
        success()
    } else if unseen {
        accent()
    } else {
        text_muted()
    };
    let picture = if !blocked && let Some(path) = visual {
        img(path)
            .id(("video-note-img", row_id))
            .size(px(200.))
            .rounded(px(100.))
            .object_fit(ObjectFit::Cover)
            .with_fallback(|| {
                div()
                    .size(px(200.))
                    .rounded(px(100.))
                    .bg(success_bg())
                    .into_any_element()
            })
            .into_any_element()
    } else {
        let label = if blocked {
            "Video note".to_string()
        } else if downloading_now {
            "Video note — downloading…".into()
        } else if note.length > 0 {
            format!("Video note {} — not downloaded", note.length)
        } else {
            "Video note — not downloaded".into()
        };
        div()
            .id(("video-note-ph", row_id))
            .size(px(200.))
            .rounded(px(100.))
            .bg(success_bg())
            .flex()
            .items_center()
            .justify_center()
            .px_2()
            .child(
                div()
                    .text_xs()
                    .text_center()
                    .text_color(text_on_fill())
                    .child(label),
            )
            .into_any_element()
    };
    let viewed = note.is_viewed;
    div()
        .id(("video-note", row_id))
        .mt_2()
        .flex()
        .flex_col()
        .gap_1()
        .child(
            div()
                .relative()
                .size(px(200.))
                .rounded(px(100.))
                .overflow_hidden()
                .border_2()
                .border_color(ring)
                .child(picture)
                .child(
                    div()
                        .absolute()
                        .top(px(72.))
                        .left(px(16.))
                        .w(px(168.))
                        .flex()
                        .justify_center()
                        .child(
                            div()
                                .px_1()
                                .rounded_sm()
                                .bg(bg_deep())
                                .text_xs()
                                .text_color(text_bright())
                                .child(badge),
                        ),
                )
                .child(
                    div()
                        .absolute()
                        .bottom(px(28.))
                        .left(px(16.))
                        .w(px(168.))
                        .flex()
                        .justify_center()
                        .child(
                            div()
                                .px_1()
                                .rounded_sm()
                                .bg(bg_deep())
                                .text_xs()
                                .text_color(text_bright())
                                .child(duration),
                        ),
                ),
        )
        .child(
            Button::new(format!("video-note-play-{row_id}"))
                .label(play_label)
                .on_click(cx.listener(move |this, _, _, cx| {
                    if blocked {
                        return;
                    }
                    this.toggle_video_playback(
                        message_id,
                        play_id,
                        "video/mp4".into(),
                        0,
                        if viewed { None } else { Some(chat_id) },
                        cx,
                    );
                })),
        )
        // MED2: transcription under the play button (schema 1.8.67
        // `speechRecognitionResult` on `videoNote`).
        .child(transcription_row(
            chat_id,
            message_id,
            &note.transcription,
            cx,
        ))
        .into_any_element()
}

pub(super) fn sticker_attachment(
    row_id: u64,
    sticker: &quill::telegram::envelope::StickerContent,
    files: &HashMap<i32, ParsedFile>,
    downloading: &std::collections::HashSet<i32>,
    media_roots: &[PathBuf],
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let display_id = sticker.display_file_id().unwrap_or(FileId(0));
    let fallback_label = sticker_label(sticker);
    if let Some(path) = files
        .get(&display_id.0)
        .and_then(|file| file.usable_path())
        .and_then(|path| sandboxed_display_path(path, media_roots))
    {
        let fallback_label = fallback_label.clone();
        return img(path)
            .id(("sticker-img", row_id))
            .mt_2()
            .w(px(128.))
            .h(px(128.))
            .object_fit(ObjectFit::Contain)
            .with_fallback(move || {
                div()
                    .w(px(128.))
                    .h(px(128.))
                    .rounded_md()
                    .bg(fill_muted())
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(fallback_label.clone())
                    .into_any_element()
            })
            .into_any_element();
    }
    let downloading_now = file_is_downloading(display_id, files, downloading);
    let label = if downloading_now {
        format!("{} — downloading…", sticker_label(sticker))
    } else if display_id.0 == 0 {
        sticker_label(sticker)
    } else {
        format!("{} — not downloaded", sticker_label(sticker))
    };
    div()
        .id(("sticker-ph", row_id))
        .mt_2()
        .w(px(128.))
        .h(px(88.))
        .rounded_md()
        .bg(fill_muted())
        .flex()
        .items_center()
        .justify_center()
        .when(display_id.0 != 0, |this| {
            this.cursor_pointer()
                .pressable(cx.theme())
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.request_media_download(display_id, None, cx);
                }))
        })
        .child(div().text_xs().text_color(text_bright()).child(label))
        .into_any_element()
}

pub(super) fn sticker_label(sticker: &quill::telegram::envelope::StickerContent) -> String {
    if sticker.emoji.is_empty() {
        "Sticker".into()
    } else {
        format!("Sticker {}", sticker.emoji)
    }
}

pub(super) fn waveform_row(row_key: u64, bars: &[u8]) -> impl IntoElement {
    let mut row = div()
        .id(("waveform", row_key))
        .flex()
        .items_end()
        .gap_0()
        .h(px(28.));
    let shown: Vec<u8> = if bars.is_empty() {
        vec![6, 10, 14, 8, 12]
    } else {
        bars.iter().copied().take(48).collect()
    };
    for (index, bar) in shown.into_iter().enumerate() {
        let h = 4.0 + f32::from(bar.min(31)) * 0.7;
        row = row.child(
            div()
                .id(("wave-bar", row_key * 64 + index as u64))
                .w(px(3.))
                .h(px(h))
                .rounded_sm()
                .bg(accent()),
        );
    }
    row
}

/// Phase 4.6 seek bar (tdesktop-style): the interactive gpui-component
/// `Slider` on the active row — click-to-seek and drag, with the UI layer
/// restarting ffplay at the released offset via `-ss` — and a static
/// track + fill on every other audio/voice row.
pub(super) fn seek_bar_element(row_key: u64, seek: &SeekBarView) -> AnyElement {
    if let Some(slider) = &seek.slider {
        div()
            .id(("seek-bar", row_key))
            .w_full()
            .child(Slider::new(slider).bg(accent()).text_color(text_on_fill()))
            .into_any_element()
    } else {
        div()
            .id(("seek-bar", row_key))
            .w_full()
            .h(px(6.))
            .rounded_full()
            .bg(border())
            .child(
                div()
                    .h_full()
                    .w(relative(seek.fraction() as f32))
                    .rounded_full()
                    .bg(accent()),
            )
            .into_any_element()
    }
}

pub(super) fn voice_note_row(
    chat_id: ChatId,
    message_id: MessageId,
    outgoing: bool,
    note: &quill::telegram::envelope::VoiceNoteContent,
    files: &HashMap<i32, ParsedFile>,
    downloading: &std::collections::HashSet<i32>,
    seek: &SeekBarView,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let file_id = note.file_id;
    let ready = files
        .get(&file_id.0)
        .and_then(|file| file.usable_path())
        .is_some();
    let downloading_now = file_is_downloading(file_id, files, downloading);
    let bars = voice::waveform_bars_from_bytes(&note.waveform);
    let listened = note.is_listened;
    let note_duration = f64::from(note.duration);
    let active = seek.slider.is_some();
    let play_label = if seek.is_playing {
        "Pause"
    } else if downloading_now {
        "Downloading"
    } else {
        "Play"
    };
    // Phase 4.6: the active row shows elapsed / total (tdesktop-style).
    let total = format_voice_duration(note.duration);
    let mut meta = if active {
        format!(
            "{} / {total}",
            format_voice_duration(seek.display_secs as i32)
        )
    } else {
        total
    };
    if !outgoing && !note.is_listened && !active {
        meta = format!("New · {meta}");
    }
    if !ready && !downloading_now {
        meta = format!("{meta} · not downloaded");
    } else if downloading_now {
        meta = format!("{meta} · downloading…");
    } else if seek.is_playing {
        meta = format!("Playing · {meta}");
    } else if active {
        meta = format!("{meta} · paused");
    }
    div()
        .id(("voice-note", message_id.0 as u64))
        .mt_2()
        .px_3()
        .py_2()
        .rounded_md()
        .border_1()
        .border_color(if active { success() } else { text_muted() })
        .bg(bg_subtle())
        .flex()
        .flex_col()
        .gap_1()
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    Button::new(format!("voice-play-{}", message_id.0))
                        .label(play_label)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.toggle_voice_playback(
                                chat_id,
                                message_id,
                                file_id,
                                listened,
                                note_duration,
                                cx,
                            );
                        })),
                )
                .child(
                    div()
                        .text_sm()
                        .font_medium()
                        .text_color(text_bright())
                        .child("Voice message"),
                )
                .child(div().text_xs().text_color(text_bright()).child(meta)),
        )
        .child(waveform_row(message_id.0 as u64, &bars))
        .child(seek_bar_element(message_id.0 as u64, seek))
        // MED2: transcription (schema 1.8.67 `speechRecognitionResult`
        // on `voiceNote`).
        .child(transcription_row(
            chat_id,
            message_id,
            &note.transcription,
            cx,
        ))
        // MED1: speed + mute on the active row.
        .when(active, |this| {
            this.child(row_playback_controls(
                message_id.0 as u64,
                "voice",
                seek,
                cx,
            ))
        })
        .into_any_element()
}

pub(super) fn audio_row(
    message_id: MessageId,
    audio: &quill::telegram::envelope::AudioContent,
    files: &HashMap<i32, ParsedFile>,
    downloading: &std::collections::HashSet<i32>,
    media_roots: &[PathBuf],
    seek: &SeekBarView,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let file_id = audio.file_id;
    let ready = files
        .get(&file_id.0)
        .and_then(|file| file.usable_path())
        .is_some();
    let downloading_now = file_is_downloading(file_id, files, downloading);
    let audio_duration = f64::from(audio.duration);
    let active = seek.slider.is_some();
    let play_label = if seek.is_playing {
        "Pause"
    } else if downloading_now {
        "Downloading"
    } else {
        "Play"
    };
    let title = if !audio.title.is_empty() {
        audio.title.clone()
    } else if !audio.file_name.is_empty() {
        audio.file_name.clone()
    } else {
        "Audio".to_string()
    };
    // Phase 4.6: the active row shows elapsed / total (tdesktop-style).
    let total = voice::format_voice_duration(audio.duration);
    let duration_label = if active {
        format!(
            "{} / {total}",
            voice::format_voice_duration(seek.display_secs as i32)
        )
    } else {
        total
    };
    let mut meta = duration_label;
    if !audio.performer.is_empty() {
        meta = format!("{} · {meta}", audio.performer);
    }
    if seek.is_playing {
        meta = format!("Playing · {meta}");
    } else if downloading_now {
        meta = format!("{meta} · downloading…");
    } else if !ready {
        meta = format!("{meta} · not downloaded");
    } else if active {
        meta = format!("{meta} · paused");
    }
    let cover_id = audio.cover_file_id().unwrap_or(FileId(0));
    let cover = files
        .get(&cover_id.0)
        .and_then(|file| file.usable_path())
        .and_then(|path| sandboxed_display_path(path, media_roots));
    let cover_box = if let Some(path) = cover {
        img(path)
            .id(("audio-cover", message_id.0 as u64))
            .w(px(56.))
            .h(px(56.))
            .rounded_md()
            .object_fit(ObjectFit::Cover)
            .with_fallback(|| {
                div()
                    .w(px(56.))
                    .h(px(56.))
                    .rounded_md()
                    .bg(fill_muted())
                    .flex()
                    .items_center()
                    .justify_center()
                    .child("Audio")
                    .into_any_element()
            })
            .into_any_element()
    } else {
        div()
            .id(("audio-cover-ph", message_id.0 as u64))
            .w(px(56.))
            .h(px(56.))
            .rounded_md()
            .bg(fill_muted())
            .flex()
            .items_center()
            .justify_center()
            .child(div().text_xs().text_color(text_bright()).child("Audio"))
            .into_any_element()
    };
    div()
        .id(("audio", message_id.0 as u64))
        .mt_2()
        .px_3()
        .py_2()
        .rounded_md()
        .border_1()
        .border_color(if active { success() } else { text_muted() })
        .bg(bg_subtle())
        .flex()
        .items_center()
        .gap_3()
        .child(cover_box)
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .gap_1()
                .child(
                    div()
                        .text_sm()
                        .font_medium()
                        .text_color(text_bright())
                        .child(title),
                )
                .child(div().text_xs().text_color(text_primary()).child(meta))
                .child(seek_bar_element(message_id.0 as u64, seek))
                .child(
                    Button::new(format!("audio-play-{}", message_id.0))
                        .label(play_label)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.toggle_audio_playback(message_id, file_id, audio_duration, cx);
                        })),
                )
                // MED1: speed + mute on the active row.
                .when(active, |this| {
                    this.child(row_playback_controls(
                        message_id.0 as u64,
                        "audio",
                        seek,
                        cx,
                    ))
                }),
        )
        .into_any_element()
}

/// MED3: document row. Primary click per state (TGX: tapping downloading
/// media cancels it): ready → open with the system viewer; downloading →
/// cancel (`cancelDownloadFile`); failed / not downloaded → download
/// (retry). A second action row offers "Show in folder" (ready), "Cancel"
/// (downloading), "Retry" (failed). Progress comes from
/// `file.download_progress()` — `updateFile`'s `downloaded_size` over the
/// known total (TGX `TD.getFileProgress` semantics).
pub(super) fn document_chip(
    row_id: u64,
    doc: &quill::telegram::envelope::DocumentContent,
    files: &HashMap<i32, ParsedFile>,
    downloading: &std::collections::HashSet<i32>,
    failed: &std::collections::HashSet<i32>,
    sponsored: Option<(ChatId, i64)>,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let file_id = doc.file_id;
    let file = files.get(&file_id.0);
    let ready = file.and_then(|f| f.usable_path()).is_some();
    let downloading_now = file_is_downloading(file_id, files, downloading);
    let failed_now = !ready && !downloading_now && failed.contains(&file_id.0);
    let progress = file.and_then(|f| f.download_progress());
    let size = file.map(|f| f.display_size()).unwrap_or(0);
    let size_label = format_bytes(size);
    let state = if ready {
        "ready".to_string()
    } else if downloading_now {
        match progress {
            Some(p) => format!("downloading… {}%", (p * 100.0).round() as i32),
            None => "downloading…".to_string(),
        }
    } else if failed_now {
        "download failed".to_string()
    } else {
        "not downloaded".to_string()
    };
    let mut detail = doc.mime_type.clone();
    if !size_label.is_empty() {
        if !detail.is_empty() {
            detail.push_str(" · ");
        }
        detail.push_str(&size_label);
    }
    if !detail.is_empty() {
        detail.push_str(" · ");
    }
    detail.push_str(&state);
    let name = if doc.file_name.is_empty() {
        "Document".to_string()
    } else {
        doc.file_name.clone()
    };
    let action_label = if ready {
        Some("Show in folder")
    } else if downloading_now {
        Some("Cancel")
    } else if failed_now {
        Some("Retry")
    } else {
        None
    };
    div()
        .id(("doc-chip", row_id))
        .mt_2()
        .px_3()
        .py_2()
        .rounded_md()
        .border_1()
        .border_color(text_muted())
        .bg(bg_subtle())
        .child(
            div()
                .id(("doc-chip-name", row_id))
                .cursor_pointer()
                .pressable(cx.theme())
                .child(div().text_sm().font_medium().child(name))
                .on_click(cx.listener(move |this, _, _, cx| {
                    if ready {
                        this.open_downloaded_file(file_id, cx);
                    } else if downloading_now {
                        this.cancel_media_download(file_id, cx);
                    } else {
                        this.request_media_download(file_id, sponsored, cx);
                    }
                })),
        )
        .child(div().text_xs().text_color(text_primary()).child(detail))
        .when(downloading_now, |this| {
            this.child(
                div()
                    .id(("doc-progress", row_id))
                    .w_full()
                    .h(px(4.))
                    .mt_1()
                    .rounded_full()
                    .bg(border())
                    .child(
                        div()
                            .h_full()
                            .w(relative(progress.unwrap_or(0.0)))
                            .rounded_full()
                            .bg(accent()),
                    ),
            )
        })
        .when_some(action_label, |this, label| {
            this.child(
                div()
                    .id(("doc-action", row_id))
                    .cursor_pointer()
                    .pressable(cx.theme())
                    .mt_1()
                    .text_xs()
                    .text_color(accent())
                    .child(label)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if ready {
                            this.reveal_downloaded_file(file_id, cx);
                        } else if downloading_now {
                            this.cancel_media_download(file_id, cx);
                        } else {
                            this.request_media_download(file_id, sponsored, cx);
                        }
                    })),
            )
        })
        .into_any_element()
}

/// MED3: display name for a downloads-manager row — the document's
/// `file_name` when the file belongs to a known message, else the local
/// path's file name, else a plain "File {id}" fallback.
pub(super) fn download_display_name(session: &Session, file_id: i32) -> String {
    for history in session.histories.values() {
        for message in history.messages.values() {
            if let quill::telegram::envelope::MessageContent::Document(doc) = &message.content
                && doc.file_id.0 == file_id
            {
                return if doc.file_name.is_empty() {
                    "Document".to_string()
                } else {
                    doc.file_name.clone()
                };
            }
        }
    }
    session
        .files
        .get(&file_id)
        .and_then(|f| f.usable_path())
        .and_then(|p| std::path::Path::new(p).file_name())
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| format!("File {file_id}"))
}

pub(super) fn format_bytes(n: i64) -> String {
    if n <= 0 {
        String::new()
    } else if n < 1024 {
        format!("{n} B")
    } else if n < 1024 * 1024 {
        format!("{} KB", n / 1024)
    } else {
        format!("{:.1} MB", n as f64 / (1024.0 * 1024.0))
    }
}

/// Phase 4.3: `messageLocation` / `messageLiveLocation` row. A static map
/// placeholder chip (no live tiles): pin glyph, coordinate line, live
/// status when the message is a live location, and a tappable "Open map"
/// link. The link opens an OpenStreetMap URL through
/// `platform::open_external_url` (https only, scheme-gated — no `geo:`
/// or `tel:` schemes). Live re-rendering is out of scope: the
/// live-period/expires state is a static snapshot from parse time.
pub(super) fn location_row(
    row_id: u64,
    location: &quill::telegram::envelope::GeoLocation,
    live: Option<&quill::telegram::envelope::LiveLocationState>,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let url = location.open_street_map_url();
    let header = if live.is_some() {
        "📍 Live location"
    } else {
        "📍 Location"
    };
    let mut body = div()
        .id(("location-row", row_id))
        .flex()
        .flex_col()
        .gap_1()
        .mt_2()
        .px_3()
        .py_2()
        .rounded_md()
        .border_1()
        .border_color(text_muted())
        .bg(bg_subtle())
        .child(div().text_sm().font_medium().child(header))
        .child(
            div()
                .text_xs()
                .text_color(text_primary())
                .child(location.coords_label()),
        );
    if let Some(live) = live {
        body = body.child(
            div()
                .text_xs()
                .text_color(accent())
                .child(live.status_label()),
        );
    } else if location.accuracy_m > 0 {
        body = body.child(
            div()
                .text_xs()
                .text_color(text_muted())
                .child(format!("accuracy ±{} m", location.accuracy_m)),
        );
    }
    body.child(
        div()
            .id(("location-open-map", row_id))
            .text_sm()
            .text_color(accent())
            .cursor_pointer()
            .pressable(cx.theme())
            .on_click(cx.listener(move |this, _, _, cx| {
                this.open_message_url(&url, cx);
            }))
            .child("🗺 Open map"),
    )
    .into_any_element()
}

/// Phase 4.3: `messageVenue` row — venue title, address, optional provider
/// subtitle, and a tappable "Open map" link on the venue's coordinates
/// (same OpenStreetMap handling as `location_row`). The provider `id` /
/// `type` are not kept in the model (see `VenueContent`).
pub(super) fn venue_row(
    row_id: u64,
    venue: &quill::telegram::envelope::VenueContent,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let url = venue.location.open_street_map_url();
    let title = if venue.title.is_empty() {
        "Venue".to_string()
    } else {
        venue.title.clone()
    };
    let mut body = div()
        .id(("venue-row", row_id))
        .flex()
        .flex_col()
        .gap_1()
        .mt_2()
        .px_3()
        .py_2()
        .rounded_md()
        .border_1()
        .border_color(text_muted())
        .bg(bg_subtle())
        .child(div().text_sm().font_medium().child(format!("📍 {title}")));
    if !venue.address.is_empty() {
        body = body.child(
            div()
                .text_xs()
                .text_color(text_primary())
                .child(venue.address.clone()),
        );
    }
    let subtitle = if venue.provider.is_empty() {
        venue.location.coords_label()
    } else {
        format!("{} · via {}", venue.location.coords_label(), venue.provider)
    };
    body.child(div().text_xs().text_color(text_muted()).child(subtitle))
        .child(
            div()
                .id(("venue-open-map", row_id))
                .text_sm()
                .text_color(accent())
                .cursor_pointer()
                .pressable(cx.theme())
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.open_message_url(&url, cx);
                }))
                .child("🗺 Open map"),
        )
        .into_any_element()
}

/// Phase 4.3: `messageContact` row — display name, phone number, and a
/// subtle "Telegram user" note when `user_id` is known. The phone number
/// is display-only: tapping it must not dial (`tel:` URLs are refused by
/// `open_external_url`'s scheme gate anyway). The vCard is kept in the
/// model but not rendered; there is no profile deep-link yet.
pub(super) fn contact_row(
    row_id: u64,
    contact: &quill::telegram::envelope::ContactContent,
) -> AnyElement {
    let name = contact.display_name();
    let mut body = div()
        .id(("contact-row", row_id))
        .flex()
        .flex_col()
        .gap_1()
        .mt_2()
        .px_3()
        .py_2()
        .rounded_md()
        .border_1()
        .border_color(text_muted())
        .bg(bg_subtle())
        .child(div().text_sm().font_medium().child(format!(
            "👤 {}",
            if name.is_empty() { "Contact" } else { &name }
        )));
    if !contact.phone_number.is_empty() {
        body = body.child(
            div()
                .text_xs()
                .text_color(text_primary())
                .child(contact.phone_number.clone()),
        );
    }
    if contact.user_id != 0 {
        body = body.child(
            div()
                .text_xs()
                .text_color(text_muted())
                .child("Telegram user"),
        );
    }
    body.into_any_element()
}

/// Phase 4.4: `messageDice` row — the dice emoji rendered large plus the
/// rolled value, tdesktop-style. Static only: the `DiceStickers` roll
/// animation (and `success_animation_frame_number`) is out of scope for
/// this slice; the face glyph stands in for the final animation frame.
pub(super) fn dice_row(row_id: u64, dice: &quill::telegram::envelope::DiceContent) -> AnyElement {
    div()
        .id(("dice-row", row_id))
        .flex()
        .flex_col()
        .items_center()
        .gap_1()
        .mt_2()
        .px_3()
        .py_3()
        .rounded_md()
        .border_1()
        .border_color(text_muted())
        .bg(bg_subtle())
        .child(div().text_size(px(64.0)).child(dice.face().to_string()))
        .child(
            div()
                .text_sm()
                .font_medium()
                .child(format!("Rolled {}", dice.value)),
        )
        .into_any_element()
}
