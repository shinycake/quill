//! message media attachments: photo/video/voice/sticker/location/dice rendering.

use super::app::QuillApp;
use super::pressable::PressableDiv;
use super::*;
use gpui_kit::component::progress::ProgressCircle;
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
    // Sharpest downloaded size first: the bubble frame is up to 360pt
    // wide (720px on Retina), well past the ≤320px "m" thumbnail.
    let mut ids = Vec::new();
    if let Some(size) = photo.largest_size() {
        ids.push(size.file_id);
    }
    if let Some(size) = photo.thumb_size() {
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

/// Display frame for a photo/GIF/video in a bubble: the media's aspect
/// ratio fitted into at most 360×400 (and at least 120 on the short
/// side), so pictures are never cropped to a fixed strip and the
/// not-yet-downloaded placeholder already has the final size — the row
/// keeps its height when the file arrives.
pub(super) fn media_frame(width: i32, height: i32) -> (Pixels, Pixels) {
    const MAX_W: f32 = 360.;
    const MAX_H: f32 = 400.;
    const MIN_SIDE: f32 = 120.;
    if width <= 0 || height <= 0 {
        return (px(260.), px(180.));
    }
    let (w, h) = (width as f32, height as f32);
    let scale = (MAX_W / w).min(MAX_H / h);
    let (w, h) = (w * scale, h * scale);
    // Very wide/tall media: keep a usable short side (cropped by Cover).
    (px(w.max(MIN_SIDE)), px(h.max(MIN_SIDE)))
}

/// Hover group for a media frame: the pause disc shows only on hover
/// while the clip plays.
const MEDIA_VISUAL_GROUP: &str = "media-visual";

/// What the round status disc centered on a photo/video/GIF frame shows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum MediaDisc {
    Download,
    /// Downloading — the fraction once TDLib knows the total size.
    Progress(Option<f32>),
    Play,
    Pause,
}

/// Telegram-style status disc: a dark translucent circle holding a
/// download arrow, a progress ring, or play/pause.
pub(super) fn media_disc(id: impl Into<ElementId>, state: MediaDisc) -> Stateful<Div> {
    let inner: AnyElement = match state {
        MediaDisc::Progress(fraction) => ProgressCircle::new("media-disc-ring")
            .size(px(34.))
            .color(gpui_kit::white())
            .loading(fraction.is_none())
            .value(fraction.unwrap_or(0.) * 100.)
            .accessibility_label("Downloading")
            .into_any_element(),
        MediaDisc::Download | MediaDisc::Play | MediaDisc::Pause => {
            let icon = match state {
                MediaDisc::Download => gpui_kit::assets::IconName::ArrowDown,
                MediaDisc::Pause => gpui_kit::assets::IconName::Pause,
                _ => gpui_kit::assets::IconName::Play,
            };
            Icon::new(icon)
                .size(px(22.))
                .text_color(gpui_kit::white())
                .into_any_element()
        }
    };
    div()
        .id(id)
        .size(px(48.))
        .flex_none()
        .rounded_full()
        .bg(gpui_kit::black().opacity(0.5))
        .flex()
        .items_center()
        .justify_center()
        .child(inner)
}

/// Download fraction of `file_id`, when TDLib reported a total.
fn download_fraction(file_id: FileId, files: &HashMap<i32, ParsedFile>) -> Option<f32> {
    files
        .get(&file_id.0)
        .and_then(ParsedFile::download_progress)
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
    let (frame_w, frame_h) = photo
        .largest_size()
        .or_else(|| photo.thumb_size())
        .map(|size| media_frame(size.width, size.height))
        .unwrap_or_else(|| media_frame(0, 0));
    if !photo.is_secret
        && !photo.has_spoiler
        && let Some(path) = photo_display_path(photo, files, media_roots)
    {
        return img(path)
            .id(("photo-img", row_id))
            .w(frame_w)
            .h(frame_h)
            .aspect_ratio(frame_w / frame_h)
            .rounded_md()
            .object_fit(ObjectFit::Cover)
            .when_some(viewer, |this, (chat_id, message_id)| {
                this.role(gpui_kit::Role::Button)
                    .aria_label("Open photo")
                    .tab_index(0)
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.open_media_viewer(chat_id, message_id, cx);
                    }))
            })
            .with_fallback(move || {
                div()
                    .w(frame_w)
                    .h(frame_h)
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
    let downloading_now = file_is_downloading(open_id, files, downloading);
    let ready = files
        .get(&open_id.0)
        .and_then(|f| f.usable_path())
        .is_some();
    // Secret and spoiler photos keep a text label; the rest show a disc.
    let status = photo.placeholder_label(downloading_now, ready);
    let viewable = !photo.is_secret && !photo.has_spoiler;
    let viewer_open = viewable.then_some(viewer).flatten();
    let has_viewer_open = viewer_open.is_some();
    // The inline minithumbnail, scaled to the frame, previews the picture
    // (soft, like a blur) while the real size downloads. Secret and
    // spoiler photos never reveal it.
    let preview = viewable
        .then_some(photo.minithumbnail.as_ref())
        .flatten()
        .filter(|mini| !mini.data.is_empty())
        .map(|mini| {
            img(ImageSource::Image(Arc::new(gpui_kit::Image::from_bytes(
                gpui_kit::ImageFormat::Jpeg,
                mini.data.clone(),
            ))))
            .absolute()
            .inset_0()
            .size_full()
            .object_fit(ObjectFit::Cover)
        });
    let has_preview = preview.is_some();
    div()
        .id(("photo-ph", row_id))
        .relative()
        .overflow_hidden()
        .w(frame_w)
        .h(frame_h)
        .rounded_md()
        .bg(fill_muted())
        .flex()
        .items_center()
        .justify_center()
        .children(preview)
        // Phase 4.5: viewable photos open the viewer (it triggers the
        // download when needed); spoiler photos keep the old
        // click-to-download placeholder, secret photos stay inert.
        .when_some(viewer_open, |this, (chat_id, message_id)| {
            this.role(gpui_kit::Role::Button)
                .aria_label("Open photo")
                .tab_index(0)
                .cursor_pointer()
                .pressable(cx.theme())
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.open_media_viewer(chat_id, message_id, cx);
                }))
        })
        .when(
            !has_viewer_open && photo.click_requests_download(),
            |this| {
                this.role(gpui_kit::Role::Button)
                    .aria_label("Download photo")
                    .tab_index(0)
                    .cursor_pointer()
                    .pressable(cx.theme())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.request_media_download(open_id, sponsored, cx);
                    }))
            },
        )
        .map(|this| {
            if viewable {
                this.child(media_disc(
                    ("photo-disc", row_id),
                    if downloading_now {
                        MediaDisc::Progress(download_fraction(open_id, files))
                    } else {
                        MediaDisc::Download
                    },
                ))
            } else {
                this.child(
                    div()
                        .text_xs()
                        .when(has_preview, |this| {
                            // Over the preview: a legible pill.
                            this.px_2()
                                .py_0p5()
                                .rounded_full()
                                .bg(gpui_kit::black().opacity(0.45))
                                .text_color(gpui_kit::white())
                        })
                        .when(!has_preview, |this| this.text_color(text_bright()))
                        .child(status),
                )
            }
        })
        .into_any_element()
}

pub(super) fn animation_attachment(
    message_id: MessageId,
    animation: &quill::telegram::envelope::AnimationContent,
    files: &HashMap<i32, ParsedFile>,
    downloading: &std::collections::HashSet<i32>,
    media_roots: &[PathBuf],
    playing: bool,
    frame: Option<Arc<RenderImage>>,
    inline: Option<super::inline_video::InlineFrame>,
    sponsored: Option<(ChatId, i64)>,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let row_id = message_id.0 as u64;
    let play_id = animation.play_file_id().unwrap_or(FileId(0));
    let thumb_id = animation.thumb_file_id().unwrap_or(FileId(0));
    let mime = animation.mime_type.clone();
    let play_label = if playing { "Pause" } else { "Play" };
    let visual = if playing {
        frame.map(ImageSource::Render)
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
                .map(ImageSource::from)
        })
    });
    let downloading_now = file_is_downloading(play_id, files, downloading)
        || file_is_downloading(thumb_id, files, downloading);
    let blocked = animation.is_secret || animation.has_spoiler;
    let (frame_w, frame_h) = media_frame(animation.width, animation.height);
    let live = inline.is_some();
    let picture = if let Some(inline) = inline {
        inline_surface(inline, frame_w, frame_h)
    } else if !blocked && let Some(path) = visual {
        img(path)
            .id(("gif-img", row_id))
            .w(frame_w)
            .h(frame_h)
            .aspect_ratio(frame_w / frame_h)
            .rounded_md()
            .object_fit(ObjectFit::Cover)
            .with_fallback(move || {
                div()
                    .w(frame_w)
                    .h(frame_h)
                    .rounded_md()
                    .bg(accent_strong())
                    .into_any_element()
            })
            .into_any_element()
    } else {
        div()
            .id(("gif-ph", row_id))
            .w(frame_w)
            .h(frame_h)
            .rounded_md()
            .bg(fill_muted())
            .into_any_element()
    };
    let gif_disc = if downloading_now && !playing {
        MediaDisc::Progress(download_fraction(play_id, files))
    } else if playing {
        MediaDisc::Pause
    } else {
        MediaDisc::Play
    };
    div()
        .id(("gif-row", row_id))
        .flex()
        .flex_col()
        .gap_1()
        .child(
            div()
                .id(("gif-visual", row_id))
                .relative()
                .group(MEDIA_VISUAL_GROUP)
                .child(picture)
                .child(
                    div()
                        .absolute()
                        .top_1()
                        .left_1()
                        .px_1p5()
                        .rounded_md()
                        .bg(gpui_kit::black().opacity(0.5))
                        .text_xs()
                        .text_color(gpui_kit::white())
                        .child("GIF"),
                )
                // Autoplaying GIFs show no play control (Telegram Desktop).
                .when(!blocked && !live, |this| {
                    this.child(
                        div()
                            .absolute()
                            .inset_0()
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(
                                media_disc(("gif-disc", row_id), gif_disc)
                                    .role(gpui_kit::Role::Button)
                                    .aria_label(play_label)
                                    .cursor_pointer()
                                    .when(playing, |disc| {
                                        disc.invisible()
                                            .group_hover(MEDIA_VISUAL_GROUP, |s| s.visible())
                                    })
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        cx.stop_propagation();
                                        if let Some((chat_id, sponsored_id)) = sponsored {
                                            this.click_sponsored_message(
                                                chat_id,
                                                sponsored_id,
                                                true,
                                                cx,
                                            );
                                        }
                                        this.toggle_animation_playback(
                                            message_id,
                                            play_id,
                                            mime.clone(),
                                            cx,
                                        );
                                    })),
                            ),
                    )
                }),
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
    inline: Option<super::inline_video::InlineFrame>,
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
    // Autoplaying: the time left and a muted mark (Telegram Desktop).
    let live_remaining = inline.as_ref().map(|inline| inline.remaining_secs);
    let duration = match live_remaining {
        Some(Some(left)) => format_voice_duration(left.ceil() as i32),
        _ => format_voice_duration(video.duration),
    };
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
    let (frame_w, frame_h) = media_frame(video.width, video.height);
    let live = inline.is_some();
    let picture = if let Some(inline) = inline {
        inline_surface(inline, frame_w, frame_h)
    } else if !blocked && let Some(path) = visual {
        img(path)
            .id(("video-img", row_id))
            .w(frame_w)
            .h(frame_h)
            .aspect_ratio(frame_w / frame_h)
            .rounded_md()
            .object_fit(ObjectFit::Cover)
            .with_fallback(move || {
                div()
                    .w(frame_w)
                    .h(frame_h)
                    .rounded_md()
                    .bg(success_bg())
                    .into_any_element()
            })
            .into_any_element()
    } else {
        div()
            .id(("video-ph", row_id))
            .w(frame_w)
            .h(frame_h)
            .rounded_md()
            .bg(fill_muted())
            .flex()
            .items_center()
            .justify_center()
            .when(blocked, |this| {
                this.child(div().text_xs().text_color(text_muted()).child("Video"))
            })
            .into_any_element()
    };
    let video_disc = if downloading_now && !playing {
        MediaDisc::Progress(download_fraction(play_id, files))
    } else if playing {
        MediaDisc::Pause
    } else {
        MediaDisc::Play
    };
    div()
        .id(("video-row", row_id))
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
                    this.role(gpui_kit::Role::Button)
                        .aria_label("Open video")
                        .tab_index(0)
                        .cursor_pointer()
                        .pressable(cx.theme())
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.open_media_viewer(chat_id, message_id, cx);
                        }))
                })
                .group(MEDIA_VISUAL_GROUP)
                .child(picture)
                .child(
                    div()
                        .absolute()
                        .bottom_1()
                        .left_1()
                        .px_1p5()
                        .rounded_md()
                        .bg(gpui_kit::black().opacity(0.5))
                        .text_xs()
                        .text_color(gpui_kit::white())
                        .flex()
                        .items_center()
                        .gap_1()
                        .child(duration)
                        .when(live, |this| {
                            this.child(
                                Icon::new(gpui_kit::assets::IconName::VolumeX)
                                    .size(px(12.))
                                    .text_color(gpui_kit::white()),
                            )
                        }),
                )
                .when(!blocked && !live, |this| {
                    this.child(
                        div()
                            .absolute()
                            .inset_0()
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(
                                media_disc(("video-disc", row_id), video_disc)
                                    .role(gpui_kit::Role::Button)
                                    .aria_label(play_label)
                                    .cursor_pointer()
                                    .when(playing, |disc| {
                                        disc.invisible()
                                            .group_hover(MEDIA_VISUAL_GROUP, |s| s.visible())
                                    })
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        // The frame around the disc opens the viewer.
                                        cx.stop_propagation();
                                        if let Some((chat_id, sponsored_id)) = sponsored {
                                            this.click_sponsored_message(
                                                chat_id,
                                                sponsored_id,
                                                true,
                                                cx,
                                            );
                                        }
                                        // With the native player, videos play in
                                        // the media viewer (Telegram Desktop's
                                        // behavior); inline frames otherwise.
                                        if super::native_video::SUPPORTED
                                            && let Some((chat_id, message_id)) = viewer
                                        {
                                            this.open_media_viewer(chat_id, message_id, cx);
                                            return;
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
                            ),
                    )
                })
        })
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
    accent: Hsla,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let row = message_id.0 as u64;
    match transcription {
        None => div()
            .child(
                inline_link(("transcribe", row), "Transcribe", accent)
                    .tooltip(|window, cx| {
                        gpui_kit::component::tooltip::Tooltip::new(
                            "Send speech-recognition request to Telegram",
                        )
                        .build(window, cx)
                    })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.request_transcription(chat_id, message_id, cx);
                    })),
            )
            .into_any_element(),
        Some(SpeechRecognition::Pending { partial_text }) => div()
            .text_xs()
            .opacity(0.7)
            .child(if partial_text.is_empty() {
                "Transcribing…".to_string()
            } else {
                format!("Transcribing… {partial_text}")
            })
            .into_any_element(),
        Some(SpeechRecognition::Text { text }) => div()
            .text_sm()
            .child(format!("“{text}”"))
            .into_any_element(),
        Some(SpeechRecognition::Error { message }) => div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_2()
            .text_xs()
            .child(
                div()
                    .opacity(0.7)
                    .child(format!("Transcription failed: {message}")),
            )
            .child(
                inline_link(("transcribe-retry", row), "Retry", accent).on_click(cx.listener(
                    move |this, _, _, cx| {
                        this.request_transcription(chat_id, message_id, cx);
                    },
                )),
            )
            .into_any_element(),
    }
}

/// Round video message diameter.
const VIDEO_NOTE_DIAMETER: f32 = 220.;

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
    inline: Option<super::inline_video::InlineFrame>,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let row_id = message_id.0 as u64;
    let play_id = note.play_file_id().unwrap_or(FileId(0));
    let thumb_id = note.thumb_file_id().unwrap_or(FileId(0));
    // Playing with sound: the time left (Telegram Desktop).
    let duration = match inline.as_ref() {
        Some(inline) if inline.sound => {
            format_voice_duration(inline.remaining_secs.unwrap_or(0.0).ceil() as i32)
        }
        _ => format_voice_duration(note.duration),
    };
    let live = inline.is_some();
    let sounding = inline.as_ref().is_some_and(|inline| inline.sound);
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
    let unseen = !outgoing && !note.is_viewed && !playing && !sounding;
    let has_visual = !blocked && (live || visual.is_some());
    let picture = if let Some(inline) = inline {
        round_inline_surface(inline)
    } else {
        match visual.filter(|_| !blocked) {
            // GPUI clips overflow to rectangles: the image and the placeholder
            // round themselves.
            Some(path) => img(path)
                .id(("video-note-img", row_id))
                .size_full()
                .rounded_full()
                .object_fit(ObjectFit::Cover)
                .with_fallback(|| {
                    div()
                        .size_full()
                        .rounded_full()
                        .bg(fill_muted())
                        .into_any_element()
                })
                .into_any_element(),
            None => div()
                .size_full()
                .rounded_full()
                .bg(fill_muted())
                .flex()
                .items_center()
                .justify_center()
                .when(blocked, |this| {
                    this.child(
                        div()
                            .text_xs()
                            .text_color(text_muted())
                            .child("Video message"),
                    )
                })
                .into_any_element(),
        }
    };
    let disc = if downloading_now && !has_visual {
        MediaDisc::Progress(download_fraction(play_id, files))
    } else if playing {
        MediaDisc::Pause
    } else {
        MediaDisc::Play
    };
    let viewed = note.is_viewed;
    let toggle = move |this: &mut QuillApp, cx: &mut Context<QuillApp>| {
        if blocked {
            return;
        }
        // A muted inline loop: play it once with sound (Telegram Desktop).
        if live {
            if this
                .inline_videos
                .borrow_mut()
                .toggle_sound(chat_id.0, message_id.0)
            {
                if !viewed {
                    this.mark_voice_opened(chat_id, message_id);
                }
                cx.notify();
            }
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
    };
    let diameter = px(VIDEO_NOTE_DIAMETER);
    div()
        .id(("video-note", row_id))
        .flex()
        .flex_col()
        .gap_1()
        .when(outgoing, |this| this.items_end())
        .child(
            div()
                .id(("video-note-circle", row_id))
                .relative()
                .size(diameter)
                .rounded_full()
                .overflow_hidden()
                .group(MEDIA_VISUAL_GROUP)
                // An unseen incoming note carries an accent ring.
                .when(unseen && !live, |this| {
                    this.border_2().border_color(cx.theme().primary)
                })
                .role(gpui_kit::Role::Button)
                .aria_label(if playing || sounding {
                    "Pause video message"
                } else {
                    "Play video message"
                })
                .tab_index(0)
                .cursor_pointer()
                .on_click(cx.listener(move |this, _, _, cx| toggle(this, cx)))
                .child(picture)
                // Over the video's mask, so the ring stays visible.
                .when(unseen && live, |this| {
                    this.child(
                        div()
                            .absolute()
                            .inset_0()
                            .rounded_full()
                            .border_2()
                            .border_color(cx.theme().primary),
                    )
                })
                // A looping round video shows no play control.
                .when(!blocked && !live, |this| {
                    this.child(
                        div()
                            .absolute()
                            .inset_0()
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(media_disc(("video-note-disc", row_id), disc).when(
                                playing,
                                |disc| {
                                    disc.invisible()
                                        .group_hover(MEDIA_VISUAL_GROUP, |s| s.visible())
                                },
                            )),
                    )
                })
                .child(
                    div()
                        .absolute()
                        .bottom(px(14.))
                        .left_0()
                        .right_0()
                        .flex()
                        .justify_center()
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_1()
                                .px_1p5()
                                .rounded_md()
                                .bg(gpui_kit::black().opacity(0.5))
                                .text_xs()
                                .text_color(gpui_kit::white())
                                .child(duration)
                                .when(unseen, |this| {
                                    this.child(
                                        div().size(px(5.)).rounded_full().bg(gpui_kit::white()),
                                    )
                                }),
                        ),
                ),
        )
        // MED2: transcription under the play button (schema 1.8.67
        // `speechRecognitionResult` on `videoNote`).
        .child(transcription_row(
            chat_id,
            message_id,
            &note.transcription,
            cx.theme().primary,
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
    animated: Option<Arc<RenderImage>>,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    if let Some(image) = animated {
        return img(image)
            .id(("sticker-animated", row_id))
            .mt_2()
            .w(px(128.))
            .h(px(128.))
            .aspect_ratio(px(128.) / px(128.))
            .object_fit(ObjectFit::Contain)
            .into_any_element();
    }
    let display_id = sticker.display_file_id().unwrap_or(sticker.file_id);
    let fallback_label = sticker_label(sticker);
    if let Some(path) = [Some(display_id), sticker.thumb_file_id]
        .into_iter()
        .flatten()
        .filter_map(|id| files.get(&id.0).and_then(|file| file.usable_path()))
        .find_map(|path| sandboxed_display_path(path, media_roots))
    {
        let fallback_label = fallback_label.clone();
        return img(path)
            .id(("sticker-img", row_id))
            .mt_2()
            .w(px(128.))
            .h(px(128.))
            .aspect_ratio(px(128.) / px(128.))
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
    // Until the image lands: the sticker's emoji, faded, in a box the size
    // of the sticker itself so the row doesn't jump when it arrives.
    let downloading_now = file_is_downloading(display_id, files, downloading);
    let label = if downloading_now {
        format!("{} — downloading", sticker_label(sticker))
    } else {
        sticker_label(sticker)
    };
    let glyph = if sticker.emoji.is_empty() {
        "🙂".to_string()
    } else {
        sticker.emoji.clone()
    };
    div()
        .id(("sticker-ph", row_id))
        .mt_2()
        .size(px(128.))
        .flex_none()
        .rounded_lg()
        .bg(fill_muted().opacity(0.5))
        .flex()
        .items_center()
        .justify_center()
        .role(gpui_kit::Role::Image)
        .aria_label(label)
        .when(display_id.0 != 0 && !downloading_now, |this| {
            this.role(gpui_kit::Role::Button)
                .tab_index(0)
                .cursor_pointer()
                .pressable(cx.theme())
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.request_media_download(display_id, None, cx);
                }))
        })
        .child(
            div()
                .text_size(px(56.))
                .opacity(if downloading_now { 0.35 } else { 0.6 })
                .child(glyph),
        )
        .into_any_element()
}

pub(super) fn sticker_label(sticker: &quill::telegram::envelope::StickerContent) -> String {
    if sticker.emoji.is_empty() {
        "Sticker".into()
    } else {
        format!("Sticker {}", sticker.emoji)
    }
}

/// Voice waveform bars; the first `played` fraction is drawn solid, the
/// rest faded, so the waveform doubles as the progress indicator.
pub(super) fn waveform_row(
    row_key: u64,
    bars: &[u8],
    color: Hsla,
    played: f32,
) -> impl IntoElement {
    let mut row = div()
        .id(("waveform", row_key))
        .flex()
        .items_center()
        .gap(px(2.))
        .h(px(24.));
    let shown: Vec<u8> = if bars.is_empty() {
        vec![6, 10, 14, 8, 12]
    } else {
        bars.iter().copied().take(48).collect()
    };
    let count = shown.len().max(1) as f32;
    for (index, bar) in shown.into_iter().enumerate() {
        let h = 3.0 + f32::from(bar.min(31)) * 0.65;
        let lit = (index as f32 + 0.5) / count <= played;
        row = row.child(
            div()
                .id(("wave-bar", row_key * 64 + index as u64))
                .w(px(2.))
                .h(px(h))
                .rounded_full()
                .bg(if lit { color } else { color.opacity(0.35) }),
        );
    }
    row
}

/// Phase 4.6 seek bar (tdesktop-style): the interactive gpui-component
/// `Slider` on the active row — click-to-seek and drag, with the UI layer
/// restarting ffplay at the released offset via `-ss` — and a static
/// track + fill on every other audio/voice row.
pub(super) fn seek_bar_element(row_key: u64, seek: &SeekBarView, color: Hsla) -> AnyElement {
    if let Some(slider) = &seek.slider {
        div()
            .id(("seek-bar", row_key))
            .role(gpui_kit::Role::Group)
            .aria_label("Playback position")
            .w_full()
            .child(Slider::new(slider).bg(color).text_color(color))
            .into_any_element()
    } else {
        div()
            .id(("seek-bar", row_key))
            .w_full()
            .h(px(3.))
            .rounded_full()
            .bg(color.opacity(0.25))
            .child(
                div()
                    .h_full()
                    .w(relative(seek.fraction() as f32))
                    .rounded_full()
                    .bg(color),
            )
            .into_any_element()
    }
}

/// Accent color for controls drawn inside a bubble: the theme primary on
/// incoming bubbles, white on the accent-filled outgoing ones.
pub(super) fn bubble_accent(outgoing: bool, cx: &App) -> Hsla {
    if outgoing {
        gpui_kit::white()
    } else {
        cx.theme().primary
    }
}

/// A compact text action inside a bubble ("Show in folder", "Transcribe",
/// "1.5×") in the bubble accent color.
pub(super) fn inline_link(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    color: Hsla,
) -> Stateful<Div> {
    let label = label.into();
    div()
        .id(id)
        .role(gpui_kit::Role::Button)
        .aria_label(label.clone())
        .tab_index(0)
        .cursor_pointer()
        .text_xs()
        .font_medium()
        .text_color(color)
        .hover(|style| style.underline())
        .child(label)
}

/// Round accent action disc used by voice, audio and document rows: an
/// icon, optionally inside a progress ring (`ring: Some(None)` spins).
pub(super) fn action_disc(
    id: impl Into<ElementId>,
    outgoing: bool,
    icon: gpui_kit::assets::IconName,
    ring: Option<Option<f32>>,
    label: &'static str,
    cx: &App,
) -> Stateful<Div> {
    let id = id.into();
    let (bg, fg) = if outgoing {
        (gpui_kit::white().opacity(0.22), gpui_kit::white())
    } else {
        (cx.theme().primary, gpui_kit::white())
    };
    div()
        .id(id.clone())
        .relative()
        .size(px(44.))
        .flex_none()
        .rounded_full()
        .bg(bg)
        .flex()
        .items_center()
        .justify_center()
        .role(gpui_kit::Role::Button)
        .aria_label(label)
        .tab_index(0)
        .cursor_pointer()
        .when_some(ring, |this, fraction| {
            this.child(
                div().absolute().inset(px(3.)).child(
                    ProgressCircle::new(ElementId::NamedChild(Arc::new(id), "ring".into()))
                        .size_full()
                        .color(fg)
                        .loading(fraction.is_none())
                        .value(fraction.unwrap_or(0.) * 100.),
                ),
            )
        })
        .child(Icon::new(icon).size(px(20.)).text_color(fg))
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
    let downloading_now = file_is_downloading(file_id, files, downloading);
    let bars = voice::waveform_bars_from_bytes(&note.waveform);
    let listened = note.is_listened;
    let note_duration = f64::from(note.duration);
    let active = seek.slider.is_some();
    let accent = bubble_accent(outgoing, cx);
    // Phase 4.6: the active row shows elapsed / total (tdesktop-style).
    let total = format_voice_duration(note.duration);
    let meta = if downloading_now && !seek.is_playing {
        "Downloading…".to_string()
    } else if active {
        format!(
            "{} / {total}",
            format_voice_duration(seek.display_secs as i32)
        )
    } else {
        total
    };
    let unheard = !outgoing && !note.is_listened && !active;
    let (icon, label, ring) = if seek.is_playing {
        (gpui_kit::assets::IconName::Pause, "Pause", None)
    } else if downloading_now {
        (
            gpui_kit::assets::IconName::X,
            "Downloading",
            Some(download_fraction(file_id, files)),
        )
    } else {
        (gpui_kit::assets::IconName::Play, "Play voice message", None)
    };
    let row_key = message_id.0 as u64;
    div()
        .id(("voice-note", row_key))
        .mt_1()
        .flex()
        .flex_col()
        .gap_1()
        .min_w(px(220.))
        .child(
            div()
                .flex()
                .items_center()
                .gap_3()
                .child(
                    action_disc(("voice-play", row_key), outgoing, icon, ring, label, cx).on_click(
                        cx.listener(move |this, _, _, cx| {
                            this.toggle_voice_playback(
                                chat_id,
                                message_id,
                                file_id,
                                listened,
                                note_duration,
                                cx,
                            );
                        }),
                    ),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap_0p5()
                        .child(waveform_row(row_key, &bars, accent, seek.fraction() as f32))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_1()
                                .text_xs()
                                .child(div().opacity(0.7).child(meta))
                                // Unheard incoming note: a small accent dot.
                                .when(unheard, |this| {
                                    this.child(div().size(px(6.)).rounded_full().bg(accent))
                                }),
                        ),
                ),
        )
        .when(active, |this| {
            this.child(seek_bar_element(row_key, seek, accent))
                // MED1: speed + mute on the active row.
                .child(row_playback_controls(row_key, "voice", seek, accent, cx))
        })
        // MED2: transcription (schema 1.8.67 `speechRecognitionResult`
        // on `voiceNote`).
        .child(transcription_row(
            chat_id,
            message_id,
            &note.transcription,
            accent,
            cx,
        ))
        .into_any_element()
}

pub(super) fn audio_row(
    message_id: MessageId,
    outgoing: bool,
    audio: &quill::telegram::envelope::AudioContent,
    files: &HashMap<i32, ParsedFile>,
    downloading: &std::collections::HashSet<i32>,
    media_roots: &[PathBuf],
    seek: &SeekBarView,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let file_id = audio.file_id;
    let downloading_now = file_is_downloading(file_id, files, downloading);
    let audio_duration = f64::from(audio.duration);
    let active = seek.slider.is_some();
    let accent = bubble_accent(outgoing, cx);
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
    let meta = if downloading_now && !seek.is_playing {
        "Downloading…".to_string()
    } else if audio.performer.is_empty() {
        duration_label
    } else {
        format!("{} · {duration_label}", audio.performer)
    };
    let (icon, label, ring) = if seek.is_playing {
        (gpui_kit::assets::IconName::Pause, "Pause", None)
    } else if downloading_now {
        (
            gpui_kit::assets::IconName::X,
            "Downloading",
            Some(download_fraction(file_id, files)),
        )
    } else {
        (gpui_kit::assets::IconName::Play, "Play", None)
    };
    let row_key = message_id.0 as u64;
    let cover_id = audio.cover_file_id().unwrap_or(FileId(0));
    let cover = files
        .get(&cover_id.0)
        .and_then(|file| file.usable_path())
        .and_then(|path| sandboxed_display_path(path, media_roots));
    let play = cx.listener(move |this, _, _, cx| {
        this.toggle_audio_playback(message_id, file_id, audio_duration, cx);
    });
    // Album art, when there is one, carries the play glyph on a scrim;
    // otherwise the plain accent disc.
    let disc = match cover {
        Some(path) => div()
            .id(("audio-play", row_key))
            .relative()
            .size(px(44.))
            .flex_none()
            .rounded_md()
            .overflow_hidden()
            .role(gpui_kit::Role::Button)
            .aria_label(label)
            .tab_index(0)
            .cursor_pointer()
            .child(
                img(path)
                    .id(("audio-cover", row_key))
                    .size_full()
                    .object_fit(ObjectFit::Cover),
            )
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .bg(gpui_kit::black().opacity(0.35))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(Icon::new(icon).size(px(20.)).text_color(gpui_kit::white())),
            )
            .on_click(play),
        None => {
            action_disc(("audio-play", row_key), outgoing, icon, ring, label, cx).on_click(play)
        }
    };
    div()
        .id(("audio", row_key))
        .mt_1()
        .flex()
        .flex_col()
        .gap_1()
        .min_w(px(220.))
        .child(
            div().flex().items_center().gap_3().child(disc).child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap_0p5()
                    .child(div().text_sm().font_medium().truncate().child(title))
                    .child(div().text_xs().opacity(0.7).truncate().child(meta)),
            ),
        )
        .when(active, |this| {
            this.child(seek_bar_element(row_key, seek, accent))
                // MED1: speed + mute on the active row.
                .child(row_playback_controls(row_key, "audio", seek, accent, cx))
        })
        .into_any_element()
}

/// A document row (tdesktop `HistoryDocument`): a round action disc —
/// download, progress ring with cancel, open, or retry — then the name and
/// a compact meta line ("450 KB · PDF", "1.2 MB of 3.4 MB") carrying the
/// secondary actions (Show in folder, Pause/Resume).
pub(super) fn document_chip(
    row_id: u64,
    doc: &quill::telegram::envelope::DocumentContent,
    // The disc is accent-filled; on an accent outgoing bubble it switches
    // to a translucent white one.
    outgoing: bool,
    files: &HashMap<i32, ParsedFile>,
    downloading: &std::collections::HashSet<i32>,
    failed: &std::collections::HashSet<i32>,
    // Slice media-downloads-pause: `None` when the file isn't a pausable
    // (user-initiated, listed) download; `Some(paused)` otherwise.
    paused: Option<bool>,
    sponsored: Option<(ChatId, i64)>,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let file_id = doc.file_id;
    let file = files.get(&file_id.0);
    let ready = file.and_then(|f| f.usable_path()).is_some();
    let downloading_now = file_is_downloading(file_id, files, downloading);
    let paused_now = downloading_now && paused == Some(true);
    let failed_now = !ready && !downloading_now && failed.contains(&file_id.0);
    let progress = file.and_then(|f| f.download_progress());
    let size = file.map(|f| f.display_size()).unwrap_or(0);
    let size_label = format_bytes(size);
    let kind = document_kind_label(&doc.file_name, &doc.mime_type);
    let meta = if downloading_now {
        let done = file.map(|f| f.local.downloaded_size).unwrap_or(0);
        let mut text = match (done > 0, size_label.is_empty()) {
            (true, false) => format!("{} of {}", format_bytes(done), size_label),
            _ => "Downloading…".to_string(),
        };
        if paused_now {
            text.push_str(" · paused");
        }
        text
    } else if failed_now {
        "Download failed".to_string()
    } else {
        [size_label.as_str(), kind.as_str()]
            .into_iter()
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(" · ")
    };
    let name = if doc.file_name.is_empty() {
        "Document".to_string()
    } else {
        doc.file_name.clone()
    };
    let (disc_bg, disc_fg) = if outgoing {
        (gpui_kit::white().opacity(0.22), gpui_kit::white())
    } else {
        (cx.theme().primary, gpui_kit::white())
    };
    let (disc_icon, disc_label) = if ready {
        (gpui_kit::assets::IconName::File, "Open")
    } else if downloading_now {
        (gpui_kit::assets::IconName::X, "Cancel download")
    } else if failed_now {
        (gpui_kit::assets::IconName::RotateCcw, "Retry download")
    } else {
        (gpui_kit::assets::IconName::ArrowDown, "Download")
    };
    let primary_action = move |this: &mut QuillApp, cx: &mut Context<QuillApp>| {
        if ready {
            this.open_downloaded_file(file_id, cx);
        } else if downloading_now {
            this.cancel_media_download(file_id, cx);
        } else {
            this.request_media_download(file_id, sponsored, cx);
        }
    };
    let disc = div()
        .id(("doc-disc", row_id))
        .relative()
        .size(px(44.))
        .flex_none()
        .rounded_full()
        .bg(disc_bg)
        .flex()
        .items_center()
        .justify_center()
        .role(gpui_kit::Role::Button)
        .aria_label(disc_label)
        .tab_index(0)
        .cursor_pointer()
        .pressable(cx.theme())
        .when(downloading_now, |this| {
            this.child(
                div().absolute().inset(px(3.)).child(
                    ProgressCircle::new(("doc-ring", row_id))
                        .size_full()
                        .color(disc_fg)
                        .loading(progress.is_none() && !paused_now)
                        .value(progress.unwrap_or(0.) * 100.),
                ),
            )
        })
        .child(Icon::new(disc_icon).size(px(20.)).text_color(disc_fg))
        .on_click(cx.listener(move |this, _, _, cx| primary_action(this, cx)));
    let link_color = (!outgoing).then_some(cx.theme().primary);
    let link = |id: (&'static str, u64), label: &'static str| {
        div()
            .id(id)
            .when_some(link_color, |this, color| this.text_color(color))
            .role(gpui_kit::Role::Button)
            .aria_label(label)
            .tab_index(0)
            .cursor_pointer()
            .font_medium()
            .hover(|style| style.underline())
            .child(label)
    };
    // Slice media-downloads-pause: Pause/Resume only for user-initiated
    // (listed) downloads — `None` hides it.
    let pause_label = if downloading_now {
        paused.map(|is_paused| if is_paused { "Resume" } else { "Pause" })
    } else {
        None
    };
    div()
        .id(("doc-chip", row_id))
        .mt_1()
        .flex()
        .items_center()
        .gap_3()
        .min_w(px(220.))
        .child(disc)
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap_0p5()
                .child(
                    div()
                        .id(("doc-chip-name", row_id))
                        .role(gpui_kit::Role::Button)
                        .aria_label(format!("Open document {name}"))
                        .tab_index(0)
                        .cursor_pointer()
                        .text_sm()
                        .font_medium()
                        .truncate()
                        .child(name)
                        .on_click(cx.listener(move |this, _, _, cx| primary_action(this, cx))),
                )
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap_x_2()
                        .text_xs()
                        .child(div().opacity(0.7).child(meta))
                        .when(ready, |this| {
                            this.child(link(("doc-action", row_id), "Show in folder").on_click(
                                cx.listener(move |this, _, _, cx| {
                                    this.reveal_downloaded_file(file_id, cx);
                                }),
                            ))
                        })
                        .when(failed_now, |this| {
                            this.child(link(("doc-action", row_id), "Retry").on_click(cx.listener(
                                move |this, _, _, cx| {
                                    this.request_media_download(file_id, sponsored, cx);
                                },
                            )))
                        })
                        .when_some(pause_label, |this, label| {
                            this.child(link(("doc-pause", row_id), label).on_click(cx.listener(
                                move |this, _, _, cx| {
                                    if paused_now {
                                        this.resume_media_download(file_id, cx);
                                    } else {
                                        this.pause_media_download(file_id, cx);
                                    }
                                },
                            )))
                        }),
                ),
        )
        .into_any_element()
}

/// Short type tag for a document row: the file extension ("PDF", "ZIP"),
/// else the MIME subtype, else nothing.
pub(super) fn document_kind_label(file_name: &str, mime_type: &str) -> String {
    let ext = std::path::Path::new(file_name)
        .extension()
        .and_then(|ext| ext.to_str())
        .filter(|ext| !ext.is_empty() && ext.len() <= 6);
    match ext {
        Some(ext) => ext.to_ascii_uppercase(),
        None => mime_type
            .split('/')
            .nth(1)
            .filter(|sub| !sub.is_empty() && sub.len() <= 12)
            .map(str::to_ascii_uppercase)
            .unwrap_or_default(),
    }
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
            .role(gpui_kit::Role::Button)
            .aria_label("Open location in Maps")
            .tab_index(0)
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
                .role(gpui_kit::Role::Button)
                .aria_label("Open venue in Maps")
                .tab_index(0)
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

#[cfg(test)]
mod tests {
    use super::document_kind_label;

    #[test]
    fn document_kind_prefers_extension_then_mime_subtype() {
        assert_eq!(document_kind_label("report.pdf", "application/pdf"), "PDF");
        assert_eq!(document_kind_label("notes", "text/plain"), "PLAIN");
        assert_eq!(document_kind_label("", ""), "");
        assert_eq!(document_kind_label("weird.verylongext", ""), "");
    }
}

/// An autoplaying clip's current frame, cropped to the media frame.
fn inline_surface(
    inline: super::inline_video::InlineFrame,
    frame_w: Pixels,
    frame_h: Pixels,
) -> AnyElement {
    #[cfg(target_os = "macos")]
    {
        div()
            .w(frame_w)
            .h(frame_h)
            .rounded_md()
            .overflow_hidden()
            .child(
                gpui_kit::surface(inline.buffer)
                    .w(frame_w)
                    .h(frame_h)
                    .object_fit(ObjectFit::Cover),
            )
            .into_any_element()
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = inline;
        div().w(frame_w).h(frame_h).into_any_element()
    }
}

/// A round video message's current frame: the square video under a mask
/// in the history's color that leaves only the circle.
fn round_inline_surface(inline: super::inline_video::InlineFrame) -> AnyElement {
    let diameter = px(VIDEO_NOTE_DIAMETER);
    // Twice the size for Retina edges.
    let mask = super::inline_video::circle_mask(VIDEO_NOTE_DIAMETER as u32 * 2, inline.backdrop);
    #[cfg(target_os = "macos")]
    let video = gpui_kit::surface(inline.buffer)
        .size(diameter)
        .object_fit(ObjectFit::Cover)
        .into_any_element();
    #[cfg(not(target_os = "macos"))]
    let video = div().size(diameter).into_any_element();
    div()
        .relative()
        .size(diameter)
        .child(video)
        .child(
            img(ImageSource::Render(mask))
                .absolute()
                .inset_0()
                .size(diameter),
        )
        .into_any_element()
}
