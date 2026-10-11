//! message history: loading, rows, albums, skeletons.

use super::app::{PaneMode, QuillApp};
use super::bubble_header::{forward_header_line, reply_header_strip, via_bot_line};
use super::demo::{demo_file_json, demo_media_allowlist, demo_thumb_png_path};
use super::message_checklist::checklist_body;
use super::message_games::game_card;
use super::message_media::{
    ContactCardState, animation_attachment, audio_row, contact_row, dice_row, document_chip,
    location_row, paid_media_card, photo_attachment, sticker_attachment, venue_row,
    video_attachment, video_note_attachment, voice_note_row,
};
use super::message_media::{
    MediaCorners, MediaFrameKind, file_is_downloading, media_content_width, media_frame,
    photo_display_path, single_media_width, spoiler_cover,
};
use super::message_payments::{
    inline_keyboard, invoice_body, payment_received_row, payment_success_row,
};
use super::message_poll::poll_body;
use super::message_text::{
    FooterMeta, caption_above_media, message_chrome, message_footer_meta, message_rich_block,
    message_text_block, rich_text_reserving,
};
use super::nested_click::SwallowPress;
use super::pressable::PressableDiv;
use super::synthetic::{BubbleLook, footer_reserve, session_bubble_quoted, session_bubble_rich};
use super::*;
use gpui_kit::component::button::*;
use gpui_kit::component::skeleton::Skeleton;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::FileId;
use quill::ids::{ChatId, MessageId};
use quill::local_path::sandboxed_display_path;
use quill::state::{HistoryMessage, OutboxReceipt, Session, unix_ms_now};
use quill::telegram::client::copy_and_parse;
use quill::telegram::envelope::{MessageContent, ParsedFile, effective_content};
use quill::text::TextEntity;
use quill::voice::format_voice_duration;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
/// kit Phase 3: per-render shared inputs for message-history rows.
#[derive(Default)]
pub(super) struct HistoryShared {
    pub(super) media_roots: Vec<PathBuf>,
}

pub(super) fn apply_ready_albums(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let wide = demo_file_json(71, &demo_thumb_png_path(), true);
    let left = demo_file_json(
        72,
        &demo_media_allowlist()
            .join("demo-gif-1.png")
            .to_string_lossy(),
        true,
    );
    let right = demo_file_json(
        73,
        &demo_media_allowlist()
            .join("demo-gif-2.png")
            .to_string_lossy(),
        true,
    );
    let own_photo = demo_file_json(74, &demo_thumb_png_path(), true);
    let own_clip = demo_file_json(
        75,
        &demo_media_allowlist()
            .join("demo-clip.mp4")
            .to_string_lossy(),
        true,
    );
    let own_thumb = demo_file_json(76, &demo_thumb_png_path(), true);
    let received_a = format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":801,"chat_id":11,"is_outgoing":false,"date":1790632320,"media_album_id":"77001","content":{{"@type":"messagePhoto","photo":{{"@type":"photo","has_stickers":false,"sizes":[{{"@type":"photoSize","type":"m","photo":{wide},"width":640,"height":200,"progressive_sizes":[]}}]}},"caption":{{"@type":"formattedText","text":"","entities":[]}},"has_spoiler":false,"is_secret":false}}}}}}"#
    );
    let received_b = format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":802,"chat_id":11,"is_outgoing":false,"date":1790632320,"media_album_id":"77001","content":{{"@type":"messagePhoto","photo":{{"@type":"photo","has_stickers":false,"sizes":[{{"@type":"photoSize","type":"m","photo":{left},"width":200,"height":200,"progressive_sizes":[]}}]}},"caption":{{"@type":"formattedText","text":"","entities":[]}},"has_spoiler":false,"is_secret":false}}}}}}"#
    );
    let received_c = format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":803,"chat_id":11,"is_outgoing":false,"date":1790632320,"media_album_id":"77001","content":{{"@type":"messagePhoto","photo":{{"@type":"photo","has_stickers":false,"sizes":[{{"@type":"photoSize","type":"m","photo":{right},"width":200,"height":220,"progressive_sizes":[]}}]}},"caption":{{"@type":"formattedText","text":"From Ada","entities":[]}},"has_spoiler":false,"is_secret":false}}}}}}"#
    );
    let sent_photo = format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":811,"chat_id":11,"is_outgoing":true,"date":1790632440,"media_album_id":"77002","content":{{"@type":"messagePhoto","photo":{{"@type":"photo","has_stickers":false,"sizes":[{{"@type":"photoSize","type":"m","photo":{own_photo},"width":240,"height":200,"progressive_sizes":[]}}]}},"caption":{{"@type":"formattedText","text":"","entities":[]}},"has_spoiler":false,"is_secret":false}}}}}}"#
    );
    let sent_video = format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":812,"chat_id":11,"is_outgoing":true,"date":1790632440,"media_album_id":"77002","content":{{"@type":"messageVideo","video":{{"@type":"video","duration":1,"width":320,"height":180,"file_name":"demo-clip.mp4","mime_type":"video/mp4","has_stickers":false,"supports_streaming":true,"minithumbnail":null,"thumbnail":{{"@type":"thumbnail","format":{{"@type":"thumbnailFormatJpeg"}},"width":240,"height":140,"file":{own_thumb}}},"video":{own_clip}}},"alternative_videos":[],"storyboards":[],"cover":null,"start_timestamp":0,"caption":{{"@type":"formattedText","text":"Sent album","entities":[]}},"show_caption_above_media":false,"has_spoiler":false,"is_secret":false}}}}}}"#
    );
    let drop_seed = r#"{"@type":"updateDeleteMessages","chat_id":11,"message_ids":[101,102,103],"is_permanent":true,"from_cache":false}"#;
    for json in [
        received_a,
        received_b,
        received_c,
        sent_photo,
        sent_video,
        drop_seed.to_string(),
    ] {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
}

/// kit Phase 9: loading placeholders for the message history — a few
/// alternating kit `Skeleton` bubbles while the first history batch is
/// in flight.
pub(super) fn history_skeleton() -> impl IntoElement {
    div()
        .id("history-skeleton")
        .flex()
        .flex_col()
        .gap_3()
        .p_4()
        .child(Skeleton::new().w(px(220.)).h(px(52.)).rounded_lg())
        .child(
            div()
                .flex()
                .justify_end()
                .child(Skeleton::new().w(px(180.)).h(px(40.)).rounded_lg()),
        )
        .child(
            Skeleton::new()
                .w(px(260.))
                .h(px(52.))
                .rounded_lg()
                .secondary(),
        )
        .child(
            div().flex().justify_end().child(
                Skeleton::new()
                    .w(px(140.))
                    .h(px(36.))
                    .rounded_lg()
                    .secondary(),
            ),
        )
}

/// A sender avatar's click target (tdesktop `Element::fromLink`): opens
/// the sender's profile. Built beside the row, wrapped around the kit
/// avatar by [`AvatarLink::wrap`].
struct AvatarLink {
    id: u64,
    on_click: Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>,
}

impl AvatarLink {}

/// Link for a row's avatar; `None` for spacers (rows continuing a sender
/// run), outgoing rows and senders TDLib did not name.
fn avatar_link(
    sender_avatar: &Option<(String, Option<PathBuf>)>,
    message: &HistoryMessage,
    cx: &mut Context<QuillApp>,
) -> Option<AvatarLink> {
    let (name, _) = sender_avatar.as_ref()?;
    if name.is_empty() {
        return None;
    }
    let sender = message.sender?;
    Some(AvatarLink {
        id: message.id.0 as u64,
        on_click: Box::new(cx.listener(move |this, _, window, cx| {
            this.open_avatar_profile(sender, window, cx);
        })),
    })
}

pub(super) fn album_history_row(
    album_id: i64,
    messages: &[&HistoryMessage],
    files: &HashMap<i32, ParsedFile>,
    downloading: &std::collections::HashSet<i32>,
    media_roots: &[PathBuf],
    // kit Phase 4: per-row chrome (sender header / outbox receipt /
    // avatar) computed at row-build time so the virtualized row doesn't
    // carry the chat.
    sender: Option<SenderLabel>,
    receipt: OutboxReceipt,
    sender_avatar: Option<(String, Option<PathBuf>)>,
    // Settings → Appearance: font size + bubble/plain style.
    look: BubbleLook,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let Some(first) = messages.first() else {
        return div().into_any_element();
    };
    let sizes: Vec<(i32, i32)> = messages
        .iter()
        .map(|message| quill::album::album_pixel_size(&message.content))
        .collect();
    let layout = quill::album::layout_media_group(
        &sizes,
        quill::album::ALBUM_MAX_WIDTH,
        quill::album::ALBUM_MIN_WIDTH,
        quill::album::ALBUM_SPACING,
    );
    let (box_w, box_h) = quill::album::layout_bounds(&layout);
    let mut mosaic = div()
        .id(("album", album_id as u64))
        .relative()
        .w(px(box_w as f32))
        .h(px(box_h as f32))
        .overflow_hidden();
    let album_has_caption = messages.iter().any(|message| match &message.content {
        MessageContent::Photo(photo) => !photo.caption.is_empty(),
        MessageContent::Video(video) => !video.caption.is_empty(),
        _ => false,
    });
    let corner_base = MediaCorners::in_bubble(
        cx,
        first.is_outgoing,
        look.plain,
        true,
        sender.is_none(),
        !album_has_caption,
    );
    for (index, message) in messages.iter().enumerate() {
        let Some(part) = layout.get(index) else {
            continue;
        };
        let tile = album_tile(
            message,
            part,
            (box_w, box_h),
            corner_base,
            files,
            downloading,
            media_roots,
            cx,
        );
        mosaic = mosaic.child(tile);
    }
    let caption = messages.iter().rev().find_map(|message| {
        let text = match &message.content {
            MessageContent::Photo(photo) => photo.caption.clone(),
            MessageContent::Video(video) => video.caption.clone(),
            _ => String::new(),
        };
        if text.is_empty() { None } else { Some(text) }
    });
    // Media-led like single photos: the mosaic sits on a thin inset and,
    // without a caption, the time rides on the picture.
    let has_caption = caption.is_some();
    let avatar_link = avatar_link(&sender_avatar, first, cx);
    let mut chrome = message_chrome(sender, receipt, sender_avatar, first.date, first.pending);
    if let Some(link) = avatar_link
        && let Some(avatar) = chrome.avatar.take()
    {
        chrome.avatar = Some(link.wrap(avatar));
    }
    chrome.media_led = true;
    chrome.media_width = Some(media_content_width(px(box_w as f32), px(0.)));
    chrome.footer_overlay = !has_caption;
    chrome.actions = Some(message_actions_button(first.chat_id, first.id, cx).into_any_element());
    let extra = div()
        .id(("album-extra", album_id as u64))
        .flex()
        .flex_col()
        .gap_1()
        .child(mosaic)
        .when_some(caption, |this, text| {
            this.child(div().px_2().py_1().text_sm().child(text))
        })
        .into_any_element();
    session_bubble_quoted(
        album_id as u64,
        chrome,
        String::new(),
        first.is_outgoing,
        Some(extra),
        None,
        // Settings → Appearance: font size + bubble/plain style.
        look,
    )
}

/// A round button shown beside a bubble on hover.
fn corner_button(
    id: String,
    icon: gpui_kit::assets::IconName,
    label: &'static str,
    cx: &mut Context<QuillApp>,
) -> Button {
    Button::new(id)
        .icon(icon)
        .xsmall()
        .rounded_full()
        .custom(
            ButtonCustomVariant::new(cx)
                .color(cx.theme().background.opacity(0.85))
                .foreground(cx.theme().foreground)
                .hover(cx.theme().background),
        )
        .tooltip(label)
        .accessibility_label(label)
}

/// The hover buttons beside a bubble: reply and react (Settings, "Reply
/// button on messages" and "Reaction button on messages"; tdesktop
/// `cornerReply` and `cornerReaction`), then the "…" button. Returns them
/// with the room they take.
fn message_corner_actions(
    chat_id: ChatId,
    message_id: MessageId,
    message: &HistoryMessage,
    session: Option<&Session>,
    cx: &mut Context<QuillApp>,
) -> (AnyElement, Pixels) {
    let more = message_actions_button(chat_id, message_id, cx);
    let (reply, react) = quill::corner_buttons::visible(
        session.map(|s| {
            (
                s.settings.media_prefs.corner_reply,
                s.settings.media_prefs.corner_reaction,
            )
        }),
        !message.pending && message.id.0 > 0,
        message.can_react(),
    );
    if !reply && !react {
        return (
            more.into_any_element(),
            px(quill::corner_buttons::span(false, false)),
        );
    }
    let mut row = div().flex().items_center().gap_1();
    if reply {
        row = row.child(
            corner_button(
                format!("message-reply-{}", message_id.0),
                gpui_kit::assets::IconName::Reply,
                "Reply",
                cx,
            )
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                this.begin_reply_from_message(chat_id, message_id, window, cx);
            })),
        );
    }
    if react {
        row = row.child(
            corner_button(
                format!("message-react-{}", message_id.0),
                gpui_kit::assets::IconName::FaceSlightlySmilingPlus,
                "React",
                cx,
            )
            .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                // The menu leads with the reaction strip.
                this.open_message_menu(
                    MessageMenuState {
                        chat_id,
                        message_id,
                        position: event.position(),
                    },
                    window,
                    cx,
                );
            })),
        );
    }
    let span = quill::corner_buttons::span(reply, react);
    (row.child(more).into_any_element(), px(span))
}

/// Round "…" button shown beside a bubble on hover; opens the message menu.
fn message_actions_button(
    chat_id: ChatId,
    message_id: MessageId,
    cx: &mut Context<QuillApp>,
) -> Button {
    corner_button(
        format!("message-actions-{}", message_id.0),
        gpui_kit::assets::IconName::Ellipsis,
        "Message actions",
        cx,
    )
    .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
        this.open_message_menu(
            MessageMenuState {
                chat_id,
                message_id,
                position: event.position(),
            },
            window,
            cx,
        );
    }))
}

pub(super) fn album_tile(
    message: &HistoryMessage,
    part: &quill::album::AlbumRect,
    mosaic: (i32, i32),
    mosaic_corners: MediaCorners,
    files: &HashMap<i32, ParsedFile>,
    downloading: &std::collections::HashSet<i32>,
    media_roots: &[PathBuf],
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let row_id = message.id.0 as u64;
    let corners = mosaic_corners.tile(
        part.x <= 0,
        part.y <= 0,
        part.x + part.width >= mosaic.0,
        part.y + part.height >= mosaic.1,
    );
    let chat_id = message.chat_id;
    let message_id = message.id;
    // Parity slice 5: tiles open the fullscreen media viewer. The Play
    // button inside video tiles keeps its history-row playback — its
    // handler stops propagation, so the tile click never double-fires
    // (kit buttons do not).
    let frame = div()
        .id(("album-tile", row_id))
        .absolute()
        .left(px(part.x as f32))
        .top(px(part.y as f32))
        .w(px(part.width as f32))
        .h(px(part.height as f32))
        .map(|this| corners.round(this))
        .overflow_hidden()
        .role(gpui_kit::Role::Button)
        .aria_label("Open album media")
        .tab_index(0)
        .cursor_pointer()
        .on_click(cx.listener(move |this, _, _, cx| {
            this.open_media_viewer(chat_id, message_id, cx);
        }));
    match &message.content {
        MessageContent::Photo(photo) => {
            if let Some(path) = photo_display_path(photo, files, media_roots) {
                frame
                    .child(
                        img(super::image_budget::sized_media(
                            &path,
                            (px(part.width as f32), px(part.height as f32)),
                            photo
                                .largest_size()
                                .or_else(|| photo.thumb_size())
                                .map(|size| (size.width, size.height)),
                            super::image_budget::Fit::Cover,
                        ))
                        .id(("album-photo", row_id))
                        .w(px(part.width as f32))
                        .h(px(part.height as f32))
                        .aspect_ratio(part.width.max(1) as f32 / part.height.max(1) as f32)
                        .map(|this| corners.round(this))
                        .object_fit(ObjectFit::Cover)
                        .with_fallback(move || {
                            div()
                                .size_full()
                                .map(|this| corners.round(this))
                                .bg(fill_muted())
                                .flex()
                                .items_center()
                                .justify_center()
                                .child("Photo")
                                .into_any_element()
                        }),
                    )
                    .into_any_element()
            } else {
                let open_id = photo.open_file_id().unwrap_or(FileId(0));
                let downloading_now = file_is_downloading(open_id, files, downloading);
                frame
                    .bg(fill_muted())
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        div()
                            .text_xs()
                            .text_color(text_bright())
                            .child(if downloading_now {
                                "Photo — downloading…"
                            } else {
                                "Photo"
                            }),
                    )
                    .into_any_element()
            }
        }
        MessageContent::Video(video) => {
            let play_id = video.play_file_id().unwrap_or(FileId(0));
            let thumb_id = video.thumb_file_id().unwrap_or(FileId(0));
            let mime = video.mime_type.clone();
            let start_timestamp = video.start_timestamp;
            let message_id = message.id;
            let visual = [thumb_id, play_id].into_iter().find_map(|id| {
                if id.0 == 0 {
                    return None;
                }
                files
                    .get(&id.0)
                    .and_then(|file| file.usable_path())
                    .and_then(|path| sandboxed_display_path(path, media_roots))
            });
            let duration = format_voice_duration(video.duration);
            let picture = if let Some(path) = visual {
                img(super::image_budget::sized_media(
                    &path,
                    (px(part.width as f32), px(part.height as f32)),
                    Some((video.width, video.height)),
                    super::image_budget::Fit::Cover,
                ))
                .id(("album-video", row_id))
                .w(px(part.width as f32))
                .h(px(part.height as f32))
                .aspect_ratio(part.width.max(1) as f32 / part.height.max(1) as f32)
                .map(|this| corners.round(this))
                .object_fit(ObjectFit::Cover)
                .with_fallback(move || {
                    div()
                        .size_full()
                        .map(|this| corners.round(this))
                        .bg(success_bg())
                        .into_any_element()
                })
                .into_any_element()
            } else {
                div()
                    .size_full()
                    .map(|this| corners.round(this))
                    .bg(success_bg())
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(div().text_xs().text_color(text_on_fill()).child("Video"))
                    .into_any_element()
            };
            frame
                .child(picture)
                .child(
                    div()
                        .absolute()
                        .bottom(px(4.))
                        .left(px(4.))
                        .px_1()
                        .rounded_sm()
                        .bg(bg_tile())
                        .text_xs()
                        .text_color(text_bright())
                        .child(format!("Video · {duration}")),
                )
                .child(
                    Button::new(format!("album-play-{row_id}"))
                        .label("Play")
                        .ghost()
                        // Kit buttons let the press bubble: without this the
                        // tile's own click also opens the viewer.
                        .swallow_press()
                        .on_click(cx.listener(move |this, _, _, cx| {
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
        _ => frame.into_any_element(),
    }
}

/// A service message ("X joined", "pinned a message", timers, screenshots)
/// as a centered translucent pill, Telegram Desktop style
/// (`msgServicePadding`, semibold) and matching the day separators.
pub(super) fn service_pill(
    id: impl Into<ElementId>,
    text: impl Into<SharedString>,
    cx: &App,
) -> Stateful<Div> {
    div().id(id).flex().justify_center().py_1().child(
        div()
            .max_w_full()
            .px_3()
            .py_0p5()
            .rounded_xl()
            .bg(cx.theme().secondary.opacity(0.85))
            .text_xs()
            .font_medium()
            .text_center()
            .text_color(cx.theme().secondary_foreground)
            .child(text.into()),
    )
}

/// The downloaded map tile of a place, if TDLib has produced one.
fn map_tile_path(
    session: Option<&Session>,
    location: &quill::telegram::envelope::GeoLocation,
    files: &HashMap<i32, ParsedFile>,
    media_roots: &[PathBuf],
) -> Option<PathBuf> {
    let file_id = session?.media.map_thumbs.file_id(location)?;
    files
        .get(&file_id)
        .and_then(|file| file.usable_path())
        .and_then(|path| sandboxed_display_path(path, media_roots))
}

impl QuillApp {}

mod session_history_row;
mod wrap;
#[allow(unused_imports)]
pub use session_history_row::*;
mod custom_emoji_chip_glyph;
mod load_older_action;
#[allow(unused_imports)]
pub use custom_emoji_chip_glyph::*;
