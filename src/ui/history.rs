//! message history: loading, rows, albums, skeletons.

use super::app::{PaneMode, QuillApp};
use super::demo::{demo_file_json, demo_media_allowlist, demo_thumb_png_path};
use super::forward::forward_from_strip;
use super::message_games::game_card;
use super::message_media::{
    animation_attachment, audio_row, contact_row, dice_row, document_chip, location_row,
    photo_attachment, sticker_attachment, venue_row, video_attachment, video_note_attachment,
    voice_note_row,
};
use super::message_media::{file_is_downloading, photo_display_path};
use super::message_payments::{
    inline_keyboard, invoice_body, payment_received_row, payment_success_row,
};
use super::message_poll::poll_body;
use super::message_text::{
    caption_above_media, message_chrome, message_rich_block, message_text_block, reply_quote_strip,
    rich_text_reserving,
};
use super::pressable::PressableDiv;
use super::statistics::format_view_count;
use super::synthetic::{BubbleLook, footer_reserve, session_bubble_quoted, session_bubble_rich};
use super::*;
use gpui_kit::component::button::*;
use gpui_kit::component::skeleton::Skeleton;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::composer::ComposerReplyTo;
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::FileId;
use quill::local_path::sandboxed_display_path;
use quill::state::{HistoryMessage, OutboxReceipt, Session, unix_ms_now};
use quill::telegram::client::copy_and_parse;
use quill::telegram::envelope::{
    MessageContent, ParsedFile, chat_ttl_service_label, effective_content,
};
use quill::text::TextEntity;
use quill::voice::format_voice_duration;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
/// kit Phase 3: per-render shared inputs for message-history rows.
#[derive(Default)]
pub(super) struct HistoryShared {
    pub(super) files: HashMap<i32, ParsedFile>,
    pub(super) downloading: std::collections::HashSet<i32>,
    pub(super) failed: std::collections::HashSet<i32>,
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
        .overflow_hidden()
        .rounded_md();
    for (index, message) in messages.iter().enumerate() {
        let Some(part) = layout.get(index) else {
            continue;
        };
        let tile = album_tile(message, part, files, downloading, media_roots, cx);
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
    let reply_target = ComposerReplyTo::new(
        first.chat_id,
        first.id,
        caption.clone().unwrap_or_else(|| "Album".into()),
    );
    let reply_btn = Button::new(format!("reply-album-{}", album_id))
        .label("Reply")
        .ghost()
        .on_click(cx.listener(move |this, _, window, cx| {
            this.begin_reply_to(reply_target.clone(), window, cx);
        }));
    let extra = div()
        .id(("album-extra", album_id as u64))
        .flex()
        .flex_col()
        .gap_1()
        .child(mosaic)
        .when_some(caption, |this, text| {
            this.child(div().text_sm().child(text))
        })
        .child(reply_btn)
        .into_any_element();
    session_bubble_quoted(
        album_id as u64,
        message_chrome(sender, receipt, sender_avatar, first.date, first.pending),
        String::new(),
        first.is_outgoing,
        Some(extra),
        None,
        // Settings → Appearance: font size + bubble/plain style.
        look,
    )
}

pub(super) fn album_tile(
    message: &HistoryMessage,
    part: &quill::album::AlbumRect,
    files: &HashMap<i32, ParsedFile>,
    downloading: &std::collections::HashSet<i32>,
    media_roots: &[PathBuf],
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let row_id = message.id.0 as u64;
    let chat_id = message.chat_id;
    let message_id = message.id;
    // Parity slice 5: tiles open the fullscreen media viewer. The Play
    // button inside video tiles keeps its history-row playback — the
    // component button stops propagation on mouse-down, so the tile click
    // never double-fires.
    let frame = div()
        .id(("album-tile", row_id))
        .absolute()
        .left(px(part.x as f32))
        .top(px(part.y as f32))
        .w(px(part.width as f32))
        .h(px(part.height as f32))
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
                        img(path)
                            .id(("album-photo", row_id))
                            .w(px(part.width as f32))
                            .h(px(part.height as f32))
                            .object_fit(ObjectFit::Cover)
                            .with_fallback(|| {
                                div()
                                    .size_full()
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
                img(path)
                    .id(("album-video", row_id))
                    .w(px(part.width as f32))
                    .h(px(part.height as f32))
                    .object_fit(ObjectFit::Cover)
                    .with_fallback(|| div().size_full().bg(success_bg()).into_any_element())
                    .into_any_element()
            } else {
                div()
                    .size_full()
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
    quote_preview: Option<String>,
    forward_from: Option<String>,
    // Seek-bar view for audio/voice rows (`None` for other content).
    seek_bar: Option<SeekBarView>,
    animation_playing: bool,
    animation_frame: Option<Arc<RenderImage>>,
    sticker_frame: Option<Arc<RenderImage>>,
    video_playing: bool,
    video_frame: Option<PathBuf>,
    revealed: &std::collections::HashSet<(i64, u64, u64, bool)>,
    // Phase B4: whether the row's chat is a secret chat — selects the
    // "Self-destruct" vs "Auto-delete" service-row wording.
    is_secret: bool,
    // Phase C2i: session for `messageCall` peer resolution ("Call
    // again" only for 1:1 chats).
    session: Option<&Session>,
    // Settings → Appearance: font size + bubble/plain style.
    look: BubbleLook,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let look = if matches!(message.content, MessageContent::Sticker(_)) {
        BubbleLook {
            plain: true,
            ..look
        }
    } else {
        look
    };
    // Phase B4: timer-change service rows (`messageChatSetMessageAutoDeleteTime`,
    // schema 1.8.67 line 5387) render as a centered neutral notice — no
    // bubble, no reply/react/edit/delete controls.
    if let MessageContent::Service(text) = &message.content {
        return div()
            .id(("service-message", message.id.0 as u64))
            .flex()
            .justify_center()
            .py_1()
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(text.clone()),
            )
            .into_any_element();
    }
    if let MessageContent::ChatTtlChanged { secs } = &message.content {
        return div()
            .id(("ttl-service-row", message.id.0 as u64))
            .flex()
            .justify_center()
            .py_1()
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(chat_ttl_service_label(*secs, is_secret)),
            )
            .into_any_element();
    }
    // Phase S1: `messageScreenshotTaken` service row (schema 1.8.67,
    // line 5375) — centered neutral notice, attributed via
    // `message.is_outgoing` (TGX `YouTookAScreenshot` /
    // `XTookAScreenshot`).
    if matches!(message.content, MessageContent::ScreenshotTaken) {
        let text = if message.is_outgoing {
            "You took a screenshot".to_string()
        } else {
            format!(
                "{} took a screenshot",
                sender
                    .as_ref()
                    .map_or("Someone", |label| label.name.as_str())
            )
        };
        return div()
            .id(("screenshot-service-row", message.id.0 as u64))
            .flex()
            .justify_center()
            .py_1()
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(text),
            )
            .into_any_element();
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
        div()
            .id(("community-service-row", message.id.0 as u64))
            .flex()
            .justify_center()
            .py_1()
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(text),
            )
            .into_any_element()
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
    let quote = message.reply_to.as_ref().and_then(|reply| {
        let preview = quote_preview.clone()?;
        Some(reply_quote_strip(message.id, reply.message_id, preview, cx))
    });
    let forward_strip = forward_from.map(|label| forward_from_strip(message.id, label));
    let header = match (forward_strip, quote) {
        (Some(fwd), Some(reply)) => Some(
            div()
                .id(("row-headers", message.id.0 as u64))
                .flex()
                .flex_col()
                .child(fwd)
                .child(reply)
                .into_any_element(),
        ),
        (Some(fwd), None) => Some(fwd),
        (None, Some(reply)) => Some(reply),
        (None, None) => None,
    };
    let chat_id = message.chat_id;
    let message_id = message.id;
    let more_btn = Button::new(format!("message-actions-{}", message_id.0))
        .icon(IconName::Ellipsis)
        .xsmall()
        .rounded_full()
        .custom(
            ButtonCustomVariant::new(cx)
                .color(cx.theme().background.opacity(0.85))
                .foreground(cx.theme().foreground)
                .hover(cx.theme().background),
        )
        .tooltip("Message actions")
        .accessibility_label("Message actions")
        .on_click(cx.listener(move |this, event: &ClickEvent, _, cx| {
            this.message_menu = Some(MessageMenuState {
                chat_id,
                message_id,
                position: event.position(),
            });
            cx.notify();
        }));
    // Broadcast posts (Phase 2.2): eye glyph + compact view count, like the
    // official clients' post footer. Renders whenever views exist; only
    // channel posts carry a view count in practice.
    let views_footer = message
        .interaction_info
        .as_ref()
        .map(|info| info.view_count)
        .filter(|&count| count > 0)
        .map(|count| {
            div()
                .id(("row-views", message_id.0 as u64))
                .mt_1()
                .text_xs()
                .opacity(0.75)
                .child(format!("👁 {}", format_view_count(count)))
        });
    // Phase D2: author signature (`message.author_signature`, schema 1.8.67
    // lines 3155/3165). The official clients show it under channel posts
    // and anonymous admin messages. Suppressed when the message is
    // forwarded — `forward_from_strip` already attributes the signature
    // there, and a second line would double-attribute.
    let author_signature_line = message
        .author_signature
        .as_ref()
        .filter(|_| message.forward_info.is_none())
        .map(|signature| {
            div()
                .id(("row-signature", message_id.0 as u64))
                .mt_1()
                .text_xs()
                // Inside the bubble fill: white on outgoing, kit muted otherwise.
                .text_color(if message.is_outgoing {
                    text_on_fill().into()
                } else {
                    cx.theme().muted_foreground
                })
                .child(signature.clone())
        });
    let chips = message.emoji_reaction_chips();
    let chip_row = (!chips.is_empty()).then(|| {
        let mut row = div()
            .id(("reaction-chips", message_id.0 as u64))
            .flex()
            .flex_wrap()
            .gap_1()
            .mt_1();
        for (index, chip) in chips.into_iter().enumerate() {
            let Some(label) = chip.chip_label() else {
                continue;
            };
            let emoji = chip.reaction_type.emoji_text().unwrap_or("").to_string();
            let chosen = chip.is_chosen;
            row = row.child(
                div()
                    .id(("reaction-chip", message_id.0 as u64 * 64 + index as u64))
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .text_xs()
                    .role(gpui_kit::Role::Button)
                    .aria_label(label.clone())
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
                        this.toggle_emoji_reaction(chat_id, message_id, emoji.clone(), cx);
                    }))
                    .child(label),
            );
        }
        row
    });
    let extra_media = match effective_content(&message.content, message.ephemeral.as_ref()) {
        MessageContent::Photo(photo) => Some(photo_attachment(
            message.id.0 as u64,
            photo,
            files,
            downloading,
            media_roots,
            None,
            Some((message.chat_id, message.id)),
            cx,
        )),
        MessageContent::Document(doc) => Some(document_chip(
            message.id.0 as u64,
            doc,
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
        MessageContent::Sticker(sticker) => Some(sticker_attachment(
            message.id.0 as u64,
            sticker,
            files,
            downloading,
            media_roots,
            sticker_frame,
            cx,
        )),
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
            None,
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
            None,
            Some((message.chat_id, message.id)),
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
            cx,
        )),
        MessageContent::Poll(poll) => Some(poll_body(message.chat_id, message.id, poll, cx)),
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
            cx,
        )),
        MessageContent::Venue(venue) => Some(venue_row(message.id.0 as u64, venue, cx)),
        MessageContent::Contact(contact) => Some(contact_row(message.id.0 as u64, contact)),
        MessageContent::Dice(dice) => Some(dice_row(message.id.0 as u64, dice)),
        MessageContent::Service(_)
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
        MessageContent::Unsupported { type_name } if matches!(type_name.as_str(), "messageExpiredPhoto" | "messageExpiredVideo" | "messageExpiredVideoNote" | "messageExpiredVoiceNote") => Some(
            div().id(("expired-message-label", message.id.0 as u64)).role(Role::Label)
                .aria_label("This message has expired.").child("This message has expired.").into_any_element(),
        ),
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
    let keyboard = inline_keyboard(message, cx);
    // Phase B3: self-destruct timer badge (`message.self_destruct_type` /
    // `message.self_destruct_in`, schema 1.8.67 lines 3146–3147). The
    // countdown decays locally against `unix_ms_now()` (same pattern as
    // the Phase A1 slow-mode countdown); TDLib removes the row via
    // `updateDeleteMessages` when the timer fires. The 1-second render
    // tick (see `ensure_self_destruct_tick`) keeps this fresh.
    let self_destruct_badge = message.self_destruct_badge(unix_ms_now()).map(|label| {
        div()
            .id(("self-destruct-badge", message.id.0 as u64))
            .mt_1()
            .text_xs()
            .text_color(warning_text())
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
            .text_color(warning_text())
            .child(label)
    });
    // A time footer exists and nothing renders after the body/caption: the
    // footer can share its last line instead of taking a row of its own.
    let tail_empty = keyboard.is_none()
        && self_destruct_badge.is_none()
        && auto_delete_chip.is_none()
        && views_footer.is_none()
        && author_signature_line.is_none()
        && chip_row.is_none();
    let reserve_footer = tail_empty && message.date > 0;
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
            revealed,
            // Settings → Appearance: message font size.
            look.font,
            session.is_none_or(|s| s.media_prefs.big_emoji),
            reserve_footer.then(|| footer_reserve(message.is_outgoing)),
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
                (below && reserve_footer).then(|| footer_reserve(message.is_outgoing)),
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
            div().px_2().pt_1().child(caption).into_any_element()
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
    let extra_is_empty =
        extra_media.is_none() && caption_below_el.is_none() && tail_empty && keyboard.is_none();
    let extra = Some(
        div()
            .id(("bubble-extra", message.id.0 as u64))
            .flex()
            .flex_col()
            .when_some(extra_media, |this, media| this.child(media))
            // MED4: caption below the media.
            .when_some(caption_below_el, |this, el| this.child(el))
            .when_some(self_destruct_badge, |this, badge| this.child(badge))
            .when_some(auto_delete_chip, |this, chip| this.child(chip))
            .when_some(keyboard, |this, keyboard| this.child(keyboard))
            .when_some(views_footer, |this, footer| this.child(footer))
            .when_some(author_signature_line, |this, line| this.child(line))
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
    let mut more_btn = Some(more_btn);
    let mut chrome = |footer_inline: bool| {
        let mut chrome = message_chrome(
            sender.clone(),
            receipt,
            sender_avatar.clone(),
            message.date,
            message.pending,
        );
        chrome.footer_inline = footer_inline;
        chrome.footer_overlay = footer_overlay;
        chrome.media_led = media_led;
        chrome.actions = more_btn.take().map(IntoElement::into_any_element);
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
                    let result = if topic_open {
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
