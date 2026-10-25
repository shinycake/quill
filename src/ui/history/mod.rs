//! message history: loading, rows, albums, skeletons.
#![deny(clippy::indexing_slicing)]

use super::app::{PaneMode, QuillApp};
use super::bubble_header::{forward_header_line, reply_header_strip, via_bot_line};
use super::demo::{demo_file_json, demo_media_allowlist, demo_thumb_png_path};
use super::message_checklist::checklist_body;
use super::message_games::game_card;
use super::message_media::{
    ContactCardState, action_card, animation_attachment, audio_row, call_card, contact_row,
    dice_row, document_chip, location_row, photo_attachment, picture_file_attachment,
    sticker_attachment, venue_row, video_attachment, video_note_attachment, voice_note_row,
};
use super::message_media::{
    DocOrigin, MediaCorners, fading_spoiler, leads_with_media, media_content_width,
    single_media_frame, single_media_width, spoiler_cover,
};
use super::message_payments::{
    inline_keyboard, invoice_body, payment_received_row, payment_success_row,
};
use super::message_poll::session_poll_body as poll_row;
use super::message_text::{
    FooterMeta, caption_above_media, message_chrome, message_footer_meta, message_rich_block,
    message_text_block, rich_text_reserving,
};
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
use quill::lang::{keys, tr};
use quill::local_path::sandboxed_display_path;
use quill::state::{HistoryMessage, OutboxReceipt, Session, unix_ms_now};
use quill::telegram::client::copy_and_parse;
use quill::telegram::envelope::{MessageContent, effective_content};
use quill::text::TextEntity;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
/// kit Phase 3: per-render shared inputs for message-history rows.
#[derive(Default)]
pub(super) struct HistoryShared {
    pub(super) media_roots: std::rc::Rc<[PathBuf]>,
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
    files: &quill::state::FileMap,
    downloading: &quill::state::DownloadIntents,
    media_roots: &[PathBuf],
    // kit Phase 4: per-row chrome (sender header / outbox receipt /
    // avatar) computed at row-build time so the virtualized row doesn't
    // carry the chat.
    sender: Option<SenderLabel>,
    sender_status: Option<AnyElement>,
    receipt: OutboxReceipt,
    sender_avatar: Option<(String, Option<PathBuf>)>,
    // Settings → Appearance: font size + bubble/plain style.
    look: BubbleLook,
    session: Option<&Session>,
    // Revealed spoilers: tiles keep their cover until clicked.
    revealed: &std::collections::HashSet<(i64, u64, u64, bool)>,
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
    // A narrow history scales the mosaic down (`GroupedMedia::countCurrentSize`).
    let media_max =
        bubble_width::BubbleRoom::new(look.history, sender_avatar.is_some(), look.plain)
            .media(true);
    let layout = quill::album::scale_layout(&layout, media_max, quill::album::ALBUM_SPACING);
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
    // Without a caption or a sender name the mosaic stands on its own
    // (`GroupedMedia::computeNeedBubble`; `quill::media_bubble`).
    let bubbleless = !look.plain
        && !quill::media_bubble::MediaSurroundings {
            caption: album_has_caption,
            sender_name: sender.is_some(),
            ..Default::default()
        }
        .needs_bubble();
    let corner_base = if bubbleless {
        MediaCorners::bare(
            cx,
            quill::chat_wide::has_right_layout(first.is_outgoing, look.wide),
            look.joined_above,
        )
    } else {
        MediaCorners::in_bubble(
            cx,
            first.is_outgoing,
            look.plain,
            true,
            sender.is_none(),
            !album_has_caption,
        )
    };
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
            0,
            super::forward_drag_source::drag_for_media(session, message),
            super::spoiler_fx::media_cover(
                &message.content,
                super::spoiler_fx::media_key(message.chat_id, message.id),
                revealed,
            ),
            cx,
        );
        mosaic = mosaic.child(tile);
    }
    let caption = messages.iter().rev().find_map(|message| {
        let (text, entities) = match &message.content {
            MessageContent::Photo(photo) => (&photo.caption, &photo.caption_entities),
            MessageContent::Video(video) => (&video.caption, &video.caption_entities),
            _ => return None,
        };
        if text.is_empty() {
            None
        } else {
            Some((message.chat_id, message.id, text.clone(), entities.clone()))
        }
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
    chrome.sender_status = sender_status;
    chrome.media_led = true;
    chrome.bubbleless = bubbleless;
    chrome.media_width = Some(media_content_width(px(box_w as f32), px(0.), media_max));
    chrome.footer_overlay = !has_caption;
    chrome.actions = Some(message_actions_button(first.chat_id, first.id, cx));
    let emoji: HashMap<i64, ImageSource> = HashMap::new();
    let layered: HashMap<i64, super::anim_layer::LayeredClip> = HashMap::new();
    let extra = div()
        .id(("album-extra", album_id as u64))
        .flex()
        .flex_col()
        .gap_1()
        .child(mosaic)
        .when_some(caption, |this, (chat_id, message_id, text, entities)| {
            // Same paragraph path as a single photo. A plain string is
            // GPUI's left-to-right layout, so a Hebrew caption reads
            // backwards.
            this.child(
                div()
                    .w(px(box_w as f32))
                    .px_2()
                    .py_1()
                    .child(rich_text_reserving(
                        &text,
                        &entities,
                        (chat_id.0, message_id.0 as u64),
                        true,
                        revealed,
                        look.font,
                        &emoji,
                        &layered,
                        None,
                        cx,
                    )),
            )
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

/// A round button shown beside a bubble on hover. It is built and turned
/// into an element here, so the row that holds it never keeps a `Button`
/// (7 KB per builder step in a debug build) in its own frame
/// (docs/decisions/main-thread-stack.md).
fn corner_button(
    id: String,
    icon: gpui_kit::assets::IconName,
    label: &'static str,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
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
        .on_click(on_click)
        .into_any_element()
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
    let fast_share =
        fast_share::facts(message, session).and_then(quill::corner_buttons::fast_share);
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
    if !reply && !react && fast_share.is_none() {
        return (more, px(quill::corner_buttons::span(false, false, false)));
    }
    let mut row = div().flex().items_center().gap_1();
    if let Some(what) = fast_share {
        row = row.child(fast_share::button(
            chat_id,
            message_id,
            message.pending,
            what,
            cx,
        ));
    }
    if reply {
        row = row.child(corner_button(
            format!("message-reply-{}", message_id.0),
            gpui_kit::assets::IconName::Reply,
            tr(keys::Q_MEDIA_REPLY),
            cx.listener(move |this, _: &ClickEvent, window, cx| {
                this.begin_reply_from_message(chat_id, message_id, window, cx);
            }),
            cx,
        ));
    }
    if react {
        row = row.child(corner_button(
            format!("message-react-{}", message_id.0),
            gpui_kit::assets::IconName::FaceSlightlySmilingPlus,
            tr(keys::Q_MEDIA_REACT_2),
            cx.listener(move |this, event: &ClickEvent, window, cx| {
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
            }),
            cx,
        ));
    }
    let span = quill::corner_buttons::span(fast_share.is_some(), reply, react);
    (row.child(more).into_any_element(), px(span))
}

/// Round "…" button shown beside a bubble on hover; opens the message menu.
fn message_actions_button(
    chat_id: ChatId,
    message_id: MessageId,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    corner_button(
        format!("message-actions-{}", message_id.0),
        gpui_kit::assets::IconName::Ellipsis,
        tr(keys::Q_MEDIA_MESSAGE_ACTIONS),
        cx.listener(move |this, event: &ClickEvent, window, cx| {
            this.open_message_menu(
                MessageMenuState {
                    chat_id,
                    message_id,
                    position: event.position(),
                },
                window,
                cx,
            );
        }),
        cx,
    )
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
pub(in crate::ui) fn map_tile_path(
    session: Option<&Session>,
    location: &quill::telegram::envelope::GeoLocation,
    files: &quill::state::FileMap,
    media_roots: &[PathBuf],
) -> Option<PathBuf> {
    let file_id = session?.media.map_thumbs.file_id(location)?;
    files
        .get(&file_id)
        .and_then(|file| file.usable_path())
        .and_then(|path| sandboxed_display_path(path, media_roots))
}

impl QuillApp {}

pub(in crate::ui) mod bubble_width;
mod reaction_chip_row;
pub(in crate::ui) mod text_measure;
use reaction_chip_row::reaction_chip_row;
mod album_tile;
pub(in crate::ui) use album_tile::album_tile;
mod community_rows;
mod fast_share;
mod session_history_row;
mod wrap;
#[allow(unused_imports)]
pub use session_history_row::*;
mod custom_emoji_chip_glyph;
mod fact_check_block;
mod load_older_action;
pub(in crate::ui) mod suggested_post_block;
#[allow(unused_imports)]
pub use custom_emoji_chip_glyph::*;
