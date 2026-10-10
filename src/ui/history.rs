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

impl AvatarLink {
    fn wrap(self, avatar: AnyElement) -> AnyElement {
        div()
            .id(("sender-avatar", self.id))
            .size(px(32.))
            .flex_none()
            .cursor_pointer()
            .child(avatar)
            .on_click(self.on_click)
            .into_any_element()
    }
}

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
        session.map(|s| (s.media_prefs.corner_reply, s.media_prefs.corner_reaction)),
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
    let file_id = session?.map_thumbs.file_id(location)?;
    files
        .get(&file_id)
        .and_then(|file| file.usable_path())
        .and_then(|path| sandboxed_display_path(path, media_roots))
}

pub(super) fn session_history_row(
    message: &HistoryMessage,
    files: &HashMap<i32, ParsedFile>,
    downloading: &std::collections::HashSet<i32>,
    failed: &std::collections::HashSet<i32>,
    media_roots: &[PathBuf],
    // kit Phase 4: per-row chrome (sender header / outbox receipt /
    // avatar) computed at row-build time so the virtualized row doesn't
    // carry the chat.
    sender: Option<SenderLabel>,
    receipt: OutboxReceipt,
    sender_avatar: Option<(String, Option<PathBuf>)>,
    reply_header: Option<quill::state::ReplyHeader>,
    forward_header: Option<quill::state::ForwardHeader>,
    via_bot: Option<String>,
    // Seek-bar view for audio/voice rows (`None` for other content).
    seek_bar: Option<SeekBarView>,
    animation_playing: bool,
    animation_frame: Option<Arc<RenderImage>>,
    sticker_frame: Option<super::sticker_playback::AnimatedVisual>,
    // A slot machine's five layers, bottom to top (empty for other rows).
    dice_layers: Vec<Option<super::sticker_playback::AnimatedVisual>>,
    // Decoded animations of the message's custom emoji (by custom emoji id).
    animated_emoji: HashMap<i64, super::sticker_playback::AnimatedVisual>,
    video_playing: bool,
    video_frame: Option<PathBuf>,
    // The row's clip playing inline (muted autoplay), if any.
    inline: Option<super::inline_video::InlineFrame>,
    revealed: &std::collections::HashSet<(i64, u64, u64, bool)>,
    // Phase B4: whether the row's chat is a secret chat — selects the
    // "Self-destruct" vs "Auto-delete" service-row wording.
    _is_secret: bool,
    // Phase C2i: session for `messageCall` peer resolution ("Call
    // again" only for 1:1 chats).
    session: Option<&Session>,
    // Settings → Appearance: font size + bubble/plain style.
    look: BubbleLook,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    // Stickers, round video messages and emoji-only messages (the ones
    // drawn big) stand on their own, without a bubble, as in Telegram
    // Desktop. Their time then sits on the chat background, so it takes
    // the theme's text color instead of the bubble's.
    let emoji_only = match &message.content {
        MessageContent::Text(text) => {
            text.link_preview.is_none()
                && session.is_none_or(|s| s.media_prefs.big_emoji)
                && quill::emoji_catalog::big_emoji_count_with_entities(&text.text, &text.entities)
                    .is_some()
        }
        _ => false,
    };
    let look = if emoji_only
        || matches!(
            message.content,
            MessageContent::Sticker(_) | MessageContent::VideoNote(_)
        ) {
        BubbleLook {
            plain: true,
            text: cx.theme().foreground,
            ..look
        }
    } else {
        look
    };
    // Service actions (members, pins, gifts, topics…), timer changes and
    // screenshots: Telegram Desktop's wording as a centered pill — no
    // bubble, no reply/react/edit/delete controls.
    if super::service_row::is_service_row(&message.content) {
        return super::service_row::service_message_row(message, session, files, media_roots, cx);
    }
    // Slice G9: community service rows (`messageChatAddedToCommunity`,
    // `messageChatRemovedFromCommunity`, `messageChatJoinFromCommunity`,
    // schema 1.8.67 lines 5353–5363) — centered neutral notices, no
    // bubble, no reply/react/edit/delete controls. Wording is TGX
    // verbatim (strings.xml: ActionChatAddedToCommunity{,Unknown} /
    // ActionChatRemovedFromCommunity / group_user_join_from_community*).
    // The community name resolves from the session's `updateCommunity`
    // cache; unknown communities fall back to the nameless TGX forms.
    let community_name = |community_id: i64| {
        session.and_then(|s| s.communities.get(&community_id).map(|c| c.name.clone()))
    };
    let community_service_row = |text: String| {
        service_pill(("community-service-row", message.id.0 as u64), text, cx).into_any_element()
    };
    if let MessageContent::ChatAddedToCommunity { community_id } = &message.content {
        let text = match community_name(*community_id) {
            Some(name) => format!("This chat was added to community \"{name}\""),
            None => "This chat was added to community".to_string(),
        };
        return community_service_row(text);
    }
    if matches!(message.content, MessageContent::ChatRemovedFromCommunity) {
        return community_service_row("This chat was removed from community".to_string());
    }
    if let MessageContent::ChatJoinFromCommunity { community_id } = &message.content {
        let community = community_name(*community_id);
        let text = match (message.is_outgoing, community) {
            (true, Some(name)) => {
                format!("You joined the group from the community \"{name}\"")
            }
            (true, None) => "You joined the group from the community".to_string(),
            (false, Some(name)) => format!(
                "{} joined the group from the community \"{name}\"",
                sender
                    .as_ref()
                    .map_or("Someone", |label| label.name.as_str())
            ),
            (false, None) => format!(
                "{} joined the group from the community",
                sender
                    .as_ref()
                    .map_or("Someone", |label| label.name.as_str())
            ),
        };
        return community_service_row(text);
    }
    // Phase C2f: `messageGroupCall` invitation service row (schema
    // 1.8.67, line 5288) — incoming and pending: Accept / Decline.
    if let MessageContent::GroupCallInvitation {
        is_active,
        was_missed,
        is_video,
        ..
    } = &message.content
    {
        return QuillApp::group_call_invitation_row(
            message,
            *is_active,
            *was_missed,
            *is_video,
            cx,
        )
        .into_any_element();
    }
    // Phase C2i: `messageCall` service row (schema 1.8.67, line 5277)
    // — reason-aware label + "Call again" for 1:1 chats.
    if let MessageContent::Call {
        is_video,
        discard_reason,
        duration,
    } = &message.content
    {
        return QuillApp::call_message_row(
            message,
            *is_video,
            discard_reason,
            *duration,
            session,
            cx,
        )
        .into_any_element();
    }
    let on_fill = message.is_outgoing && !look.plain;
    let quote = reply_header.map(|header| {
        let thumb = QuillApp::reply_thumb_path(&header, files, media_roots);
        let pattern = QuillApp::reply_pattern_path(&header, session, files, media_roots);
        reply_header_strip(message.id, header, thumb, pattern, on_fill, cx)
    });
    let has_forward = forward_header.is_some();
    let forward_strip = forward_header.map(|header| {
        let original = (header.original_date > 0 && message.forward_info.is_some()).then(|| {
            format!(
                "Original: {}",
                super::message_text::format_unix_date_time(header.original_date.into())
            )
        });
        forward_header_line(message.id, header, via_bot.clone(), original, on_fill, cx)
    });
    let via_strip = via_bot
        .filter(|_| !has_forward)
        .map(|bot| via_bot_line(message.id, bot, on_fill));
    let header_parts: Vec<AnyElement> = [via_strip, forward_strip, quote]
        .into_iter()
        .flatten()
        .collect();
    let header = match header_parts.len() {
        0 => None,
        1 => header_parts.into_iter().next(),
        _ => Some(
            div()
                .id(("row-headers", message.id.0 as u64))
                .flex()
                .flex_col()
                .children(header_parts)
                .into_any_element(),
        ),
    };
    let chat_id = message.chat_id;
    let message_id = message.id;
    let (more_btn, actions_span) =
        message_corner_actions(chat_id, message_id, message, session, cx);
    // Broadcast posts (Phase 2.2): eye glyph + compact view count, like the
    // official clients' post footer. Renders whenever views exist; only
    // channel posts carry a view count in practice.
    // Channel post views and the author signature ride in the footer row
    // with the time ("Demo Admin · 👁 12.4K · 16:44").
    let views = message
        .interaction_info
        .as_ref()
        .map(|info| info.view_count)
        .filter(|&count| count > 0);
    // Phase D2: author signature (`message.author_signature`, schema 1.8.67
    // lines 3155/3165). The official clients show it under channel posts
    // and anonymous admin messages. Suppressed when the message is
    // forwarded — `forward_from_strip` already attributes the signature
    // there, and a second line would double-attribute.
    let signature = message
        .author_signature
        .clone()
        .filter(|_| message.forward_info.is_none());
    let footer_meta = FooterMeta::of(message, receipt, views, signature.clone());
    let chips = message.reaction_chips();
    // Telegram Desktop shows who reacted (small avatars) instead of a
    // count when there are at most three known reactors, outside channels.
    let in_channel = session
        .and_then(|s| s.chats.get(&message.chat_id.0))
        .is_some_and(|chat| {
            matches!(
                chat.kind,
                quill::telegram::envelope::ChatKind::Supergroup {
                    is_channel: true,
                    ..
                }
            )
        });
    // Saved Messages reactions are tags: Telegram Desktop shows just the
    // tag's emoji, without a count or who reacted.
    let are_tags = message
        .interaction_info
        .as_ref()
        .and_then(|info| info.reactions.as_ref())
        .is_some_and(|reactions| reactions.are_tags);
    let can_list_reactors = message
        .interaction_info
        .as_ref()
        .and_then(|info| info.reactions.as_ref())
        .is_some_and(|reactions| reactions.can_get_added_reactions);
    let has_chips = !chips.is_empty();
    // Channel posts open their comments, group messages their replies
    // (tdesktop `paintCommentsButton`). Inside the open thread itself the
    // bar would only point back at the view.
    let in_thread = session.is_some_and(|s| s.thread_for_chat(message.chat_id).is_some());
    let in_group = session
        .and_then(|s| s.chats.get(&message.chat_id.0))
        .is_some_and(|chat| {
            matches!(
                chat.kind,
                quill::telegram::envelope::ChatKind::BasicGroup { .. }
                    | quill::telegram::envelope::ChatKind::Supergroup {
                        is_channel: false,
                        ..
                    }
            )
        });
    let mut reply_bar_el = message
        .interaction_info
        .as_ref()
        .and_then(|info| info.reply_info.as_ref())
        .filter(|_| !in_thread && message.id.0 > 0 && !message.pending)
        .and_then(|info| {
            let kind = if in_channel {
                super::threads::ReplyBarKind::Comments
            } else if in_group && info.reply_count > 0 {
                super::threads::ReplyBarKind::Replies
            } else {
                return None;
            };
            Some(super::threads::reply_bar(
                message.chat_id,
                message.id,
                kind,
                info,
                message.is_outgoing,
                super::threads::replier_avatars(info, session, media_roots),
                cx,
            ))
        });
    let chip_row = (!chips.is_empty()).then(|| {
        let mut row = div()
            .id(("reaction-chips", message_id.0 as u64))
            .flex()
            .flex_wrap()
            .items_center()
            .gap_1()
            .mt_1();
        for (index, chip) in chips.into_iter().enumerate() {
            let Some(choice) = quill::state::ReactionChoice::from_type(&chip.reaction_type) else {
                continue;
            };
            let count = chip.total_count;
            let chosen = chip.is_chosen;
            let reactors: Vec<(String, Option<PathBuf>)> =
                if !in_channel && count <= 3 && chip.recent_senders.len() == count as usize {
                    {
                        chip.recent_senders
                            .iter()
                            .map(|sender| reactor_avatar(sender, session, media_roots))
                            .collect()
                    }
                } else {
                    Default::default()
                };
            let glyph: AnyElement = match &choice {
                quill::state::ReactionChoice::Emoji(emoji) => div()
                    .child(super::reactions::emoji_presentation(emoji))
                    .into_any_element(),
                quill::state::ReactionChoice::CustomEmoji(id) => {
                    custom_emoji_chip_glyph(*id, session, files, media_roots)
                }
            };
            let label = match &choice {
                quill::state::ReactionChoice::Emoji(emoji) => format!("{emoji} {count}"),
                quill::state::ReactionChoice::CustomEmoji(_) => format!("Custom emoji {count}"),
            };
            let tag_type = chip.reaction_type.clone();
            // Hover names who reacted; right-click opens the full list.
            let hover_text = (!are_tags).then(|| {
                let names: Vec<String> = chip
                    .recent_senders
                    .iter()
                    .map(|sender| reactor_avatar(sender, session, media_roots).0)
                    .collect();
                quill::reaction_who::tooltip_text(&names, count)
            });
            let list_type = quill::reaction_who::chip_opens_list(can_list_reactors, are_tags)
                .then(|| chip.reaction_type.clone());
            let chip_el = div()
                .id(("reaction-chip", message_id.0 as u64 * 64 + index as u64))
                .when_some(hover_text.filter(|text| !text.is_empty()), |this, text| {
                    this.tooltip(move |window, cx| {
                        gpui_kit::component::tooltip::Tooltip::new(text.clone()).build(window, cx)
                    })
                })
                .when_some(list_type, |this, reaction| {
                    this.on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                            cx.stop_propagation();
                            this.open_reactors_menu(
                                MessageMenuState {
                                    chat_id,
                                    message_id,
                                    position: event.position,
                                },
                                reaction.clone(),
                                window,
                                cx,
                            );
                        }),
                    )
                })
                .flex()
                .items_center()
                .gap_1()
                .px_2()
                .py_1()
                .rounded_full()
                .text_xs()
                .role(gpui_kit::Role::Button)
                .aria_label(label)
                .tab_index(0)
                .cursor_pointer()
                .pressable(cx.theme())
                .when(chosen, |this| {
                    this.bg(accent_strong())
                        .text_color(text_on_fill())
                        .border_1()
                        .border_color(accent())
                })
                .when(!chosen, |this| {
                    this.bg(bg_subtle())
                        .text_color(text_primary())
                        .border_1()
                        .border_color(text_muted())
                })
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.toggle_reaction(chat_id, message_id, choice.clone(), cx);
                }))
                .child(glyph)
                .map(|this| {
                    if are_tags {
                        this
                    } else if reactors.is_empty() {
                        this.child(count.to_string())
                    } else {
                        // Overlapping 18 px avatars of who reacted.
                        this.child(div().flex().items_center().children(
                            reactors.iter().enumerate().map(|(ix, (name, photo))| {
                                div()
                                    .when(ix > 0, |this| this.ml(px(-5.)))
                                    .rounded_full()
                                    .border_1()
                                    .border_color(bg_subtle())
                                    .child(super::message_text::kit_avatar_element(
                                        name,
                                        photo.as_deref(),
                                        px(18.),
                                    ))
                            }),
                        ))
                    }
                });
            // A Saved Messages tag has its own menu (Filter by Tag, Add or
            // Edit Name, Remove Tag).
            row = row.child(if are_tags {
                let named = session.is_some_and(|s| {
                    s.saved
                        .tags
                        .iter()
                        .any(|entry| entry.tag == tag_type && !entry.label.is_empty())
                });
                let premium = session
                    .and_then(|s| s.my_user_id.and_then(|id| s.user(id)))
                    .is_some_and(|user| user.is_premium);
                super::saved_sublists::tag_chip_menu(
                    chip_el,
                    super::saved_sublists::TagChipMenu {
                        owner: cx.entity().downgrade(),
                        tag: tag_type,
                        choice: quill::state::ReactionChoice::from_type(&chip.reaction_type),
                        chat_id,
                        message_id,
                        named,
                        premium,
                    },
                )
            } else {
                chip_el.into_any_element()
            });
        }
        // Telegram Desktop keeps the time on the reactions' line.
        if let Some(footer) = message_footer_meta(&footer_meta) {
            row = row.child(div().ml_auto().pl_2().child(footer));
        }
        row
    });
    // A spoiler stays covered until clicked; once revealed, it renders as
    // the plain media it is.
    let media_revealed =
        revealed.contains(&(message.chat_id.0, message.id.0 as u64, u64::MAX, false));
    // Media corners follow the bubble (Telegram Desktop `BubbleRounding`): a
    // picture that leads the bubble takes its radius on the edges that are
    // free of the sender name, a caption above/below, badges or reactions.
    let corners = {
        let media_led = header.is_none()
            && matches!(
                effective_content(&message.content, message.ephemeral.as_ref()),
                MessageContent::Photo(_) | MessageContent::Video(_) | MessageContent::Animation(_)
            );
        let has_caption = match &message.content {
            MessageContent::Photo(photo) => !photo.caption.is_empty(),
            MessageContent::Video(video) => !video.caption.is_empty(),
            MessageContent::Animation(animation) => !animation.caption.is_empty(),
            _ => false,
        };
        let caption_above = has_caption && caption_above_media(&message.content);
        let caption_below = has_caption && !caption_above;
        let badges = message.self_destruct_badge(unix_ms_now()).is_some()
            || message.auto_delete_chip(unix_ms_now()).is_some();
        MediaCorners::in_bubble(
            cx,
            message.is_outgoing,
            look.plain,
            media_led,
            sender.is_none() && !caption_above,
            !caption_below && !badges && !has_chips,
        )
    };
    let extra_media = match effective_content(&message.content, message.ephemeral.as_ref()) {
        MessageContent::Photo(photo) if photo.has_spoiler && !media_revealed => {
            let (frame_w, frame_h) = photo
                .largest_size()
                .or_else(|| photo.thumb_size())
                .map(|size| media_frame(MediaFrameKind::Photo, size.width, size.height))
                .unwrap_or_else(|| media_frame(MediaFrameKind::Photo, 0, 0));
            Some(spoiler_cover(
                message.id.0 as u64,
                message.chat_id,
                message.id,
                photo.minithumbnail.as_ref(),
                photo.open_file_id(),
                frame_w,
                frame_h,
                corners,
                cx,
            ))
        }
        MessageContent::Video(video) if video.has_spoiler && !media_revealed => {
            let (frame_w, frame_h) = media_frame(MediaFrameKind::Video, video.width, video.height);
            Some(spoiler_cover(message.id.0 as u64, message.chat_id, message.id, None, None, frame_w, frame_h, corners, cx))
        }
        MessageContent::Animation(animation) if animation.has_spoiler && !media_revealed => {
            let (frame_w, frame_h) = media_frame(MediaFrameKind::Gif, animation.width, animation.height);
            Some(spoiler_cover(message.id.0 as u64, message.chat_id, message.id, None, None, frame_w, frame_h, corners, cx))
        }
        MessageContent::Photo(photo) => {
            let unveiled;
            let photo = if photo.has_spoiler {
                unveiled = quill::telegram::envelope::PhotoContent {
                    has_spoiler: false,
                    ..photo.clone()
                };
                &unveiled
            } else {
                photo
            };
            Some(photo_attachment(
                message.id.0 as u64,
                photo,
                files,
                downloading,
                media_roots,
                None,
                Some((message.chat_id, message.id)),
                corners,
                cx,
            ))
        }
        MessageContent::Document(doc) => Some(document_chip(
            message.id.0 as u64,
            doc,
            message.is_outgoing,
            files,
            downloading,
            failed,
            // Slice media-downloads-pause: the pause toggle only appears
            // for user-initiated (listed) downloads.
            session
                .filter(|s| s.user_downloads.contains(&doc.file_id.0))
                .map(|s| s.paused_downloads.contains(&doc.file_id.0)),
            None,
            cx,
        )),
        MessageContent::Sticker(sticker) => {
            let attachment = sticker_attachment(
                message.id.0 as u64,
                sticker,
                files,
                downloading,
                media_roots,
                sticker_frame,
                cx,
            );
            // A tap opens the sticker's set (tdesktop `StickerSetBox`).
            let set_id = sticker.set_id;
            Some(if set_id != 0 && !message.pending {
                div()
                    .id(("sticker-open-set", message.id.0 as u64))
                    .cursor_pointer()
                    .role(gpui_kit::Role::Button)
                    .aria_label("Open sticker set")
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.view_message_sticker_set(set_id, cx);
                    }))
                    .child(attachment)
                    .into_any_element()
            } else {
                attachment
            })
        }
        MessageContent::VoiceNote(note) => Some(voice_note_row(
            message.chat_id,
            message.id,
            message.is_outgoing,
            note,
            files,
            downloading,
            seek_bar.as_ref().expect("voice row always has a seek view"),
            cx,
        )),
        MessageContent::Audio(audio) => Some(audio_row(
            message.id,
            message.is_outgoing,
            audio,
            files,
            downloading,
            media_roots,
            seek_bar.as_ref().expect("audio row always has a seek view"),
            cx,
        )),
        MessageContent::Animation(animation) => Some(animation_attachment(
            message.id,
            animation,
            files,
            downloading,
            media_roots,
            animation_playing,
            animation_frame,
            inline,
            None,
            Some((message.chat_id, message.id)),
            corners,
            cx,
        )),
        MessageContent::Video(video) => Some(video_attachment(
            message.id,
            video,
            files,
            downloading,
            media_roots,
            video_playing,
            video_frame.as_deref(),
            inline,
            None,
            Some((message.chat_id, message.id)),
            corners,
            cx,
        )),
        MessageContent::VideoNote(note) => Some(video_note_attachment(
            message.chat_id,
            message.id,
            message.is_outgoing,
            note,
            files,
            downloading,
            media_roots,
            video_playing,
            video_frame.as_deref(),
            inline,
            cx,
        )),
        MessageContent::Poll(poll) => Some(poll_body(message.chat_id, message.id, poll, cx)),
        // B15: checklist card (tasks, done marks, completer names).
        MessageContent::Checklist(checklist) => Some(checklist_body(
            message.chat_id,
            message.id,
            &checklist.list,
            session,
            cx,
        )),
        // Slice P1: invoice card + payment receipt rows.
        MessageContent::Invoice(invoice) => {
            Some(invoice_body(message.chat_id, message.id, invoice, cx))
        }
        MessageContent::PaymentSuccessful(success) => Some(payment_success_row(success, cx)),
        MessageContent::PaymentReceived(received) => Some(payment_received_row(received, cx)),
        MessageContent::Location(location) => Some(location_row(
            message.id.0 as u64,
            &location.location,
            location.live.as_ref(),
            location
                .live
                .is_some_and(|live| {
                    live.can_stop_at(message.is_outgoing, quill::local_time::now_unix())
                })
                .then_some((message.chat_id, message.id)),
            map_tile_path(session, &location.location, files, media_roots),
            cx,
        )),
        MessageContent::Venue(venue) => Some(venue_row(
            message.id.0 as u64,
            venue,
            map_tile_path(session, &venue.location, files, media_roots),
            cx,
        )),
        MessageContent::Contact(contact) => Some(contact_row(
            message.id.0 as u64,
            contact,
            ContactCardState::of(contact.user_id, session),
            session
                .and_then(|s| s.user_photo_path(contact.user_id))
                .and_then(|path| sandboxed_display_path(path, media_roots)),
            cx,
        )),
        MessageContent::Dice(dice) => Some(dice_row(
            message.id.0 as u64,
            dice,
            files,
            downloading,
            media_roots,
            sticker_frame,
            dice_layers,
            cx,
        )),
        MessageContent::Action(_) if super::service_row::locked_paid_media(&message.content).is_some() => {
            super::service_row::locked_paid_media(&message.content)
                .map(|(stars, locked, caption)| paid_media_card(message.id.0 as u64, stars, locked, caption))
        }
        MessageContent::Action(_)
        | MessageContent::Text(_)
        | MessageContent::RichMessage(_)
        | MessageContent::Game(_)
        | MessageContent::GroupCallInvitation { .. }
        | MessageContent::Call { .. }
        | MessageContent::ChatTtlChanged { .. }
        | MessageContent::ScreenshotTaken
        // Slice C2k: community service rows render no extra media.
        | MessageContent::ChatAddedToCommunity { .. }
        | MessageContent::ChatRemovedFromCommunity
        // Slice G9: the join-from-community service row renders no extra
        // media either (the name ships in the centered row text).
        | MessageContent::ChatJoinFromCommunity { .. } => None,
        MessageContent::Unsupported { .. } => Some(
            div().id(("unsupported-update-card", message.id.0 as u64))
                .flex().flex_col().gap_2().p_3()
                .child(div().id(("unsupported-message-label", message.id.0 as u64))
                    .role(Role::Label)
                    .aria_label("Quill cannot display this message. A newer release may support it.")
                    .child("Quill cannot display this message. A newer release may support it."))
                .child(Button::new(("unsupported-update", message.id.0 as u64))
                    .label("Get latest Quill")
                    .on_click(cx.listener(|_, _, _, cx| cx.open_url(quill::updater::RELEASES_URL))))
                .into_any_element(),
        ),
    };
    let fast_buttons = session.is_some_and(|session| {
        session
            .fast_button_target(message.chat_id)
            .is_some_and(|target| {
                target.message_id == message.id && quill::fast_buttons::is_enabled(target.bot_id)
            })
    });
    let keyboard = inline_keyboard(message, fast_buttons, cx);
    // Phase B3: self-destruct timer badge (`message.self_destruct_type` /
    // `message.self_destruct_in`, schema 1.8.67 lines 3146–3147). The
    // countdown decays locally against `unix_ms_now()` (same pattern as
    // the Phase A1 slow-mode countdown); TDLib removes the row via
    // `updateDeleteMessages` when the timer fires. The 1-second render
    // tick (see `ensure_self_destruct_tick`) keeps this fresh.
    let outgoing = message.is_outgoing;
    let self_destruct_badge = message.self_destruct_badge(unix_ms_now()).map(|label| {
        div()
            .id(("self-destruct-badge", message.id.0 as u64))
            .mt_1()
            .text_xs()
            // Outgoing bubbles are accent-filled: inherit their text
            // color like the time footer instead of warning orange.
            .map(|this| {
                if outgoing {
                    this.opacity(0.8)
                } else {
                    this.text_color(warning_text())
                }
            })
            .child(label)
    });
    // Phase B4: auto-delete countdown chip (`message.auto_delete_in`,
    // schema 1.8.67 line 3148) — "🗑 59m left", decaying on the same
    // 1-second tick as the self-destruct badge.
    let auto_delete_chip = message.auto_delete_chip(unix_ms_now()).map(|label| {
        div()
            .id(("auto-delete-chip", message.id.0 as u64))
            .mt_1()
            .text_xs()
            // Outgoing bubbles are accent-filled: inherit their text
            // color like the time footer instead of warning orange.
            .map(|this| {
                if outgoing {
                    this.opacity(0.8)
                } else {
                    this.text_color(warning_text())
                }
            })
            .child(label)
    });
    // A time footer exists and nothing renders after the body/caption: the
    // footer can share its last line instead of taking a row of its own.
    let tail_empty = keyboard.is_none()
        && self_destruct_badge.is_none()
        && auto_delete_chip.is_none()
        && views.is_none()
        && signature.is_none()
        && reply_bar_el.is_none()
        && chip_row.is_none();
    // A message ending in a right-to-left line takes its time on a line of
    // its own (Telegram Desktop): that line ends at the bubble's left.
    let ends_rtl = match effective_content(&message.content, message.ephemeral.as_ref()) {
        MessageContent::Text(text) => quill::text::last_line_is_rtl(&text.text),
        MessageContent::Photo(photo) => quill::text::last_line_is_rtl(&photo.caption),
        MessageContent::Document(doc) => quill::text::last_line_is_rtl(&doc.caption),
        MessageContent::Animation(animation) => quill::text::last_line_is_rtl(&animation.caption),
        MessageContent::Video(video) => quill::text::last_line_is_rtl(&video.caption),
        MessageContent::VoiceNote(note) => quill::text::last_line_is_rtl(&note.caption),
        MessageContent::Audio(audio) => quill::text::last_line_is_rtl(&audio.caption),
        _ => false,
    };
    let reserve_footer = tail_empty && message.date > 0 && !ends_rtl;
    // M2: ephemeral content replaces the regular content for rendering
    // (bot-built flows show the ephemeral variant to the current user).
    let text_body = match effective_content(&message.content, message.ephemeral.as_ref()) {
        MessageContent::Text(text) => Some(message_text_block(
            (message.chat_id.0, message.id.0 as u64),
            text,
            files,
            downloading,
            media_roots,
            session
                .as_ref()
                .map(|s| s.emoji.custom_emoji_stickers.as_slice())
                .unwrap_or(&[]),
            &animated_emoji,
            revealed,
            // Settings → Appearance: message font size.
            look.font,
            session.is_none_or(|s| s.media_prefs.big_emoji),
            reserve_footer.then(|| footer_meta.reserve(footer_reserve(message.is_outgoing))),
            cx,
        )),
        // Slice bots-games: the game card is the message's primary
        // content (thumbnail/title/text/description + Play/Scores).
        MessageContent::Game(game) => Some(game_card(
            message.chat_id,
            message.id,
            game,
            session,
            files,
            downloading,
            media_roots,
            revealed,
            look.font,
            cx,
        )),
        _ => None,
    };
    // MED4: caption element + position (`show_caption_above_media`,
    // schema 1.8.67 lines 6117/6128). Above → bubble body; below →
    // inside `extra` right after the media (the previous code always
    // rendered the caption above the media).
    let caption_above_el: Option<AnyElement>;
    let caption_below_el: Option<AnyElement>;
    let caption_reserved: bool;
    {
        let caption: Option<(&str, &[TextEntity])> = match &message.content {
            MessageContent::Photo(photo) => (!photo.caption.is_empty())
                .then_some((photo.caption.as_str(), photo.caption_entities.as_slice())),
            MessageContent::Document(doc) => (!doc.caption.is_empty())
                .then_some((doc.caption.as_str(), doc.caption_entities.as_slice())),
            MessageContent::Animation(animation) => (!animation.caption.is_empty()).then_some((
                animation.caption.as_str(),
                animation.caption_entities.as_slice(),
            )),
            MessageContent::Video(video) => (!video.caption.is_empty())
                .then_some((video.caption.as_str(), video.caption_entities.as_slice())),
            MessageContent::VoiceNote(note) => (!note.caption.is_empty())
                .then_some((note.caption.as_str(), note.caption_entities.as_slice())),
            MessageContent::Audio(audio) => (!audio.caption.is_empty())
                .then_some((audio.caption.as_str(), audio.caption_entities.as_slice())),
            _ => None,
        };
        let below = !caption_above_media(&message.content);
        let el = caption.map(|(caption_text, caption_entities)| {
            rich_text_reserving(
                caption_text,
                caption_entities,
                (message.chat_id.0, message.id.0 as u64),
                true,
                revealed,
                // Settings → Appearance: caption follows the message font size.
                look.font,
                // Captions don't resolve custom emoji in this slice (text fallback).
                &HashMap::new(),
                &HashMap::new(),
                (below && reserve_footer)
                    .then(|| footer_meta.reserve(footer_reserve(message.is_outgoing))),
                cx,
            )
        });
        caption_reserved = below && reserve_footer && el.is_some();
        if caption_above_media(&message.content) {
            caption_above_el = el;
            caption_below_el = None;
        } else {
            caption_above_el = None;
            caption_below_el = el;
        }
    }
    // A just-revealed spoiler: its cover fades out over the media.
    let reveal_key = (message.chat_id.0, message.id.0 as u64, u64::MAX, false);
    let extra_media = match (
        extra_media,
        media_revealed
            .then(|| super::spoiler_fx::reveal_fade(reveal_key))
            .flatten(),
    ) {
        (Some(media), Some(cover_opacity)) => {
            // Built outside the animation layer: the cover fades under an
            // opacity the layer can't apply, so the slice draws its specks.
            let cover = super::anim_layer::with_layer(None, || {
                match effective_content(&message.content, message.ephemeral.as_ref()) {
                    MessageContent::Photo(photo) if photo.has_spoiler => {
                        let (frame_w, frame_h) = photo
                            .largest_size()
                            .or_else(|| photo.thumb_size())
                            .map(|size| media_frame(MediaFrameKind::Photo, size.width, size.height))
                            .unwrap_or_else(|| media_frame(MediaFrameKind::Photo, 0, 0));
                        Some(spoiler_cover(
                            message.id.0 as u64,
                            message.chat_id,
                            message.id,
                            photo.minithumbnail.as_ref(),
                            None,
                            frame_w,
                            frame_h,
                            corners,
                            cx,
                        ))
                    }
                    MessageContent::Video(video) if video.has_spoiler => {
                        let (frame_w, frame_h) =
                            media_frame(MediaFrameKind::Video, video.width, video.height);
                        Some(spoiler_cover(
                            message.id.0 as u64,
                            message.chat_id,
                            message.id,
                            None,
                            None,
                            frame_w,
                            frame_h,
                            corners,
                            cx,
                        ))
                    }
                    MessageContent::Animation(animation) if animation.has_spoiler => {
                        let (frame_w, frame_h) =
                            media_frame(MediaFrameKind::Gif, animation.width, animation.height);
                        Some(spoiler_cover(
                            message.id.0 as u64,
                            message.chat_id,
                            message.id,
                            None,
                            None,
                            frame_w,
                            frame_h,
                            corners,
                            cx,
                        ))
                    }
                    _ => None,
                }
            });
            Some(match cover {
                Some(cover) => div()
                    .relative()
                    .child(media)
                    .child(
                        div()
                            .absolute()
                            .top_0()
                            .left_0()
                            .opacity(cover_opacity)
                            .child(cover),
                    )
                    .into_any_element(),
                None => media,
            })
        }
        (media, _) => media,
    };
    // Photo/GIF/video bubbles without a reply or forward header: the
    // picture sits on a thin inset and a caption gets its own padding.
    let media_led = header.is_none()
        && extra_media.is_some()
        && matches!(
            effective_content(&message.content, message.ephemeral.as_ref()),
            MessageContent::Photo(_) | MessageContent::Video(_) | MessageContent::Animation(_)
        );
    // Media with nothing under it shows its time on the picture.
    let footer_overlay = media_led && caption_below_el.is_none() && reserve_footer;
    let caption_below_el = caption_below_el.map(|caption| {
        if media_led {
            div().px_2().py_1().child(caption).into_any_element()
        } else {
            caption
        }
    });
    let extra_media = extra_media.map(|media| {
        if media_led {
            media
        } else {
            div().mt_2().child(media).into_any_element()
        }
    });
    let extra_media_present = extra_media.is_some();
    let extra_is_empty =
        extra_media.is_none() && caption_below_el.is_none() && tail_empty && keyboard.is_none();
    #[allow(clippy::some_filter)]
    let extra = Some(
        div()
            .id(("bubble-extra", message.id.0 as u64))
            .flex()
            .flex_col()
            .when_some(extra_media, |this, media| this.child(media))
            // MED4: caption below the media.
            .when_some(caption_below_el, |this, el| this.child(el))
            .when_some(self_destruct_badge, |this, badge| {
                this.child(badge.when(media_led, |badge| badge.px_2()))
            })
            .when_some(auto_delete_chip, |this, chip| {
                this.child(chip.when(media_led, |chip| chip.px_2()))
            })
            .when_some(keyboard, |this, keyboard| this.child(keyboard))
            .when_some(chip_row, |this, chips| this.child(chips))
            .into_any_element(),
    )
    .filter(|_| !extra_is_empty);
    // kit Phase 4: the shared kit-shell chrome, built fresh per divergent
    // branch below (`MessageChrome` isn't `Clone` — it holds elements).
    // The text body reserved trailing footer space (mirrors the condition
    // in `message_text_block`: a link card below the text ends the bubble).
    let text_reserved = reserve_footer
        && matches!(
            effective_content(&message.content, message.ephemeral.as_ref()),
            MessageContent::Text(text)
                if !text.link_preview.as_ref().is_some_and(|preview| {
                    !preview.show_above_text && preview.has_card()
                })
        );
    // The leading picture / video / GIF decides the bubble's width; the
    // footer only widens it when the media is tiny.
    let media_width = (extra_media_present && !emoji_only)
        .then(|| {
            single_media_width(effective_content(
                &message.content,
                message.ephemeral.as_ref(),
            ))
        })
        .flatten()
        .map(|width| {
            media_content_width(
                width,
                footer_meta.reserve(footer_reserve(message.is_outgoing)),
            )
        });
    let mut more_btn = Some(more_btn);
    let mut avatar_link = avatar_link(&sender_avatar, message, cx);
    let mut chrome = |footer_inline: bool| {
        let mut chrome = message_chrome(
            sender.clone(),
            receipt,
            sender_avatar.clone(),
            message.date,
            message.pending,
        );
        // A spacer (no link) keeps its slot: taking it would drop the gutter.
        if let Some(link) = avatar_link.take()
            && let Some(avatar) = chrome.avatar.take()
        {
            chrome.avatar = Some(link.wrap(avatar));
        }
        chrome.footer = message_footer_meta(&footer_meta);
        if has_chips {
            // The reaction row carries the time.
            chrome.footer = None;
        }
        chrome.footer_inline = footer_inline;
        chrome.footer_overlay = footer_overlay;
        if footer_overlay && chrome.footer.is_some() {
            let meta = footer_meta.clone();
            chrome.footer_rebuild = Some(std::rc::Rc::new(move || message_footer_meta(&meta)));
        }
        chrome.media_led = media_led;
        chrome.media_width = media_width;
        chrome.actions = more_btn.take();
        chrome.actions_span = actions_span;
        chrome.bottom_bar = reply_bar_el.take();
        chrome
    };
    // M2: `messageRichMessage` (schema 1.8.67, line 5143) renders its
    // `pageBlock*` list as a block stack; ephemeral content wins here too.
    let rich_body = match effective_content(&message.content, message.ephemeral.as_ref()) {
        MessageContent::RichMessage(rich) => Some(message_rich_block(
            (message.chat_id.0, message.id.0 as u64),
            message.chat_id,
            message.id,
            rich,
            revealed,
            // Settings → Appearance: message font size.
            look.font,
            cx,
        )),
        _ => None,
    };
    if let Some(rich_body) = rich_body {
        return session_bubble_rich(
            message.id.0 as u64,
            chrome(false),
            message.is_outgoing,
            rich_body,
            extra,
            header,
            // Settings → Appearance: font size + bubble/plain style.
            look,
        );
    }
    if let Some(text_body) = text_body {
        return session_bubble_rich(
            message.id.0 as u64,
            chrome(text_reserved && extra_is_empty),
            message.is_outgoing,
            text_body,
            extra,
            header,
            // Settings → Appearance: font size + bubble/plain style.
            look,
        );
    }
    // MED4: caption above the media → bubble body. Caption below the
    // media already rides inside `extra` (after the media), so it falls
    // through to the quoted fallback with an empty body.
    if let Some(caption_el) = caption_above_el {
        return session_bubble_rich(
            message.id.0 as u64,
            chrome(false),
            message.is_outgoing,
            caption_el,
            extra,
            header,
            // Settings → Appearance: font size + bubble/plain style.
            look,
        );
    }
    session_bubble_quoted(
        message.id.0 as u64,
        chrome(caption_reserved),
        String::new(),
        message.is_outgoing,
        extra,
        header,
        // Settings → Appearance: font size + bubble/plain style.
        look,
    )
}

impl QuillApp {
    pub(super) fn load_older_action(&mut self, cx: &mut Context<Self>) {
        match self.pane_mode() {
            PaneMode::Ready => {
                if self.live.is_some() {
                    // Phase 5.1: a topic view pages its own history.
                    let topic_open = self
                        .live
                        .as_ref()
                        .and_then(|live| live.driver.session.open_topic)
                        .is_some();
                    let result = if self.thread_active() {
                        self.live
                            .as_mut()
                            .expect("live")
                            .driver
                            .fetch_thread_history()
                    } else if topic_open {
                        self.live
                            .as_mut()
                            .expect("live")
                            .driver
                            .fetch_topic_history()
                    } else {
                        self.live.as_mut().expect("live").driver.fetch_history()
                    };
                    self.status_note = match result {
                        Ok(Some(_)) => "loading older messages".into(),
                        Ok(None) => "no older messages to load".into(),
                        Err(_) => "could not load history".into(),
                    };
                }
                cx.notify();
            }
            PaneMode::Synthetic => {
                self.chat.update(cx, |chat, cx| chat.prepend_older(cx));
            }
            PaneMode::Connecting => {}
        }
    }
}

/// A custom-emoji reaction's image inside a chip; its fallback emoji (or a
/// blank disc) until the sticker resolves and downloads.
fn custom_emoji_chip_glyph(
    id: i64,
    session: Option<&Session>,
    files: &HashMap<i32, ParsedFile>,
    media_roots: &[PathBuf],
) -> AnyElement {
    let size = super::reactions::CHIP_GLYPH;
    let sticker = session.and_then(|s| {
        s.emoji
            .custom_emoji_stickers
            .iter()
            .find(|item| item.custom_emoji_id == Some(id))
    });
    let path = sticker
        .and_then(|item| item.display_file_id())
        .and_then(|file| files.get(&file.0))
        .and_then(|file| file.usable_path())
        .and_then(|path| sandboxed_display_path(path, media_roots));
    match (path, sticker) {
        (Some(path), _) => img(path)
            .size(px(size))
            .aspect_square()
            .object_fit(ObjectFit::Contain)
            .into_any_element(),
        (None, Some(item)) if !item.emoji.is_empty() => {
            div().child(item.emoji.clone()).into_any_element()
        }
        _ => div()
            .size(px(size))
            .rounded_full()
            .bg(text_muted().opacity(0.3))
            .into_any_element(),
    }
}

/// A reactor's avatar: name and (sandboxed) photo for a user or chat.
pub(super) fn reactor_avatar(
    sender: &quill::telegram::envelope::MessageSender,
    session: Option<&Session>,
    media_roots: &[PathBuf],
) -> (String, Option<PathBuf>) {
    use quill::telegram::envelope::MessageSender;
    let Some(session) = session else {
        return (String::new(), None);
    };
    let (name, photo) = match sender {
        MessageSender::User { user_id } => (
            session
                .user(*user_id)
                .map(|u| u.display_name())
                .unwrap_or_default(),
            session.user_photo_path(*user_id),
        ),
        MessageSender::Chat { chat_id } => (
            session
                .chats
                .get(chat_id)
                .map(|c| c.title.clone())
                .unwrap_or_default(),
            session.chat_photo_path(ChatId(*chat_id)),
        ),
    };
    let photo = photo.and_then(|path| sandboxed_display_path(path, media_roots));
    (name, photo)
}
