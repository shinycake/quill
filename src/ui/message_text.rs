//! message text rendering: rich text, entities, link previews, reply strips.

use super::app::QuillApp;
use super::demo::{demo_file_json, demo_thumb_png_path};
use super::message_media::{file_is_downloading, photo_display_path};
use super::message_payments::inline_keyboard_button;
use super::pressable::PressableDiv;
use super::*;
use gpui_kit::component::avatar::Avatar;
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::{ChatId, FileId, MessageId};
use quill::local_path::sandboxed_display_path;
use quill::rich::RichBlock;
use quill::state::{OutboxReceipt, Session, message_time_hhmm};
use quill::telegram::client::copy_and_parse;
use quill::telegram::envelope::{MessageContent, MessageInteractionInfo, ParsedFile, StickerItem};
use quill::text::{
    TextEntity, TextEntityKind, TextRun, collapsed_quote_len, quote_collapses, styled_runs,
    utf8_to_utf16_offset,
};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use synthetic::MessageChrome;
/// Slice G1: public username editor (`setSupergroupUsername`, schema
/// 1.8.67, line 15136). Empty clears the username.
/// Slice G1: coarse client-side emoji check for custom titles
/// (`setChatMemberTag` rejects emoji, schema 1.8.67 line 13597). The
/// server is authoritative; this only gives a friendlier error before
/// the request goes out.
pub(super) fn looks_like_emoji(c: char) -> bool {
    matches!(
        c as u32,
        0x200D | 0xFE00..=0xFE0F | 0x2190..=0x21FF | 0x2300..=0x23FF | 0x25A0..=0x25FF
            | 0x2600..=0x27BF | 0x2B00..=0x2BFF | 0x1F000..=0x1FAFF
    )
}

pub(super) fn apply_ready_link_preview(
    session: &mut Session,
    sink: &Arc<MemorySink>,
    seq: &AtomicU64,
) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let thumb = demo_file_json(41, &demo_thumb_png_path(), true);
    let body = "see https://example.com/story";
    let url_at = body.find("https").unwrap();
    let url_len = "https://example.com/story".len();
    let text_json = serde_json::to_string(body).unwrap();
    let json = format!(
        r#"{{"@type":"updateMessageContent","chat_id":11,"message_id":101,"new_content":{{"@type":"messageText","text":{{"@type":"formattedText","text":{text_json},"entities":[{{"@type":"textEntity","offset":{url_at},"length":{url_len},"type":{{"@type":"textEntityTypeUrl"}}}}]}},"link_preview":{{"@type":"linkPreview","url":"https://example.com/story","display_url":"example.com","site_name":"Example","title":"A short story","description":{{"@type":"formattedText","text":"Telegram-style link preview for a private chat.","entities":[]}},"author":"","type":{{"@type":"linkPreviewTypeArticle","photo":{{"@type":"photo","has_stickers":false,"minithumbnail":null,"sizes":[{{"@type":"photoSize","type":"m","photo":{thumb},"width":90,"height":90,"progressive_sizes":[]}}]}}}},"has_large_media":true,"show_large_media":false,"show_media_above_description":false,"skip_confirmation":true,"show_above_text":false,"instant_view_version":0}},"link_preview_options":null}}}}"#
    );
    if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
        session.apply(owned);
    }
}

/// MED4: `ReadyPreviewCards` fixture — one message with an embedded
/// video player preview (play badge + duration), one with an album
/// preview (thumbnail strip). Both carry `instant_view_version > 0` so
/// the tap path is honest.
pub(super) fn apply_ready_preview_cards(
    session: &mut Session,
    sink: &Arc<MemorySink>,
    seq: &AtomicU64,
) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let thumb = demo_file_json(61, &demo_thumb_png_path(), true);
    let mk_text = |body: &str, url: &str| {
        let url_at = body.find("https").unwrap();
        let url_len = url.len();
        let text_json = serde_json::to_string(body).unwrap();
        format!(
            r#"{{"@type":"formattedText","text":{text_json},"entities":[{{"@type":"textEntity","offset":{url_at},"length":{url_len},"type":{{"@type":"textEntityTypeUrl"}}}}]}}"#,
        )
    };
    let embedded = format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":102,"chat_id":11,"is_outgoing":false,"date":1700000000,"content":{{"@type":"messageText","text":{text},"link_preview":{{"@type":"linkPreview","url":"https://video.example/watch","display_url":"video.example","site_name":"Vids","title":"Clip","description":{{"@type":"formattedText","text":"","entities":[]}},"author":"","type":{{"@type":"linkPreviewTypeEmbeddedVideoPlayer","url":"https://video.example/embed/1","thumbnail":{{"@type":"photo","has_stickers":false,"minithumbnail":null,"sizes":[{{"@type":"photoSize","type":"m","photo":{thumb},"width":90,"height":90,"progressive_sizes":[]}}]}},"duration":95,"width":640,"height":360}},"has_large_media":false,"show_large_media":false,"show_media_above_description":false,"skip_confirmation":true,"show_above_text":false,"instant_view_version":2}},"link_preview_options":null}}}}}}"#,
        text = mk_text(
            "watch https://video.example/watch",
            "https://video.example/watch"
        ),
    );
    let album = format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":103,"chat_id":11,"is_outgoing":false,"date":1700000001,"content":{{"@type":"messageText","text":{text},"link_preview":{{"@type":"linkPreview","url":"https://example.com/album","display_url":"example.com","site_name":"","title":"Album","description":{{"@type":"formattedText","text":"","entities":[]}},"author":"","type":{{"@type":"linkPreviewTypeAlbum","media":[{{"@type":"linkPreviewAlbumMediaPhoto","photo":{{"@type":"photo","has_stickers":false,"minithumbnail":null,"sizes":[{{"@type":"photoSize","type":"m","photo":{thumb},"width":90,"height":90,"progressive_sizes":[]}}]}}}}],"caption":""}},"has_large_media":false,"show_large_media":false,"show_media_above_description":false,"skip_confirmation":false,"show_above_text":false,"instant_view_version":0}},"link_preview_options":null}}}}}}"#,
        text = mk_text(
            "pics https://example.com/album",
            "https://example.com/album"
        ),
    );
    for json in [embedded, album] {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
}

/// MED4: `ReadyCaptionPosition` fixture — two photo messages, one with
/// `show_caption_above_media: true`, one false.
pub(super) fn apply_ready_caption_position(
    session: &mut Session,
    sink: &Arc<MemorySink>,
    seq: &AtomicU64,
) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let file = demo_file_json(71, &demo_thumb_png_path(), true);
    for (id, above, caption) in [
        (104, true, "caption above the photo"),
        (105, false, "caption below the photo"),
    ] {
        let caption_json = serde_json::to_string(caption).unwrap();
        let json = format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":{id},"chat_id":11,"is_outgoing":false,"date":1700000000,"content":{{"@type":"messagePhoto","photo":{{"@type":"photo","has_stickers":false,"sizes":[{{"@type":"photoSize","type":"m","photo":{file},"width":240,"height":160,"progressive_sizes":[]}}]}},"caption":{{"@type":"formattedText","text":{caption_json},"entities":[]}},"show_caption_above_media":{above},"has_spoiler":false,"is_secret":false}}}}}}"#,
        );
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
}

/// `ReadyTextEntities` fixture (Phase 4.1): open a dedicated "Demo entities"
/// chat (id 14) holding a `messageText` with mixed entities
/// (bold/italic/underline/strikethrough/spoiler/code/preCode with a `rust`
/// language, incl. a nested bold-inside-italic run) plus a `messagePhoto`
/// whose caption carries bold + link entities — all through the normal
/// reducer, no live Telegram. Offsets are computed in UTF-16 code units via
/// the same helper the parser uses.
pub(super) fn apply_ready_text_entities(
    session: &mut Session,
    sink: &Arc<MemorySink>,
    seq: &AtomicU64,
) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let thumb = demo_file_json(51, &demo_thumb_png_path(), true);
    // A dedicated chat keeps the screenshot focused: just the two fixture
    // messages, both visible without scrolling.
    let chat_id = 14;
    let chat_json = format!(
        r#"{{"@type":"updateNewChat","chat":{{"id":{chat_id},"title":"Demo entities","type":{{"@type":"chatTypePrivate","user_id":{chat_id}}},"unread_count":0}}}}"#
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

    let body = "Bold italic bolditalic underline strike secret code https://example.com\nfn demo() {\n  42\n}";
    let span = |text: &str, needle: &str| -> (i32, i32) {
        let start = text.find(needle).expect("demo needle");
        let utf16_start = utf8_to_utf16_offset(text, start).expect("demo offset");
        let utf16_end = utf8_to_utf16_offset(text, start + needle.len()).expect("demo offset");
        (utf16_start, utf16_end - utf16_start)
    };
    let ent = |text: &str, needle: &str, type_json: &str| -> String {
        let (offset, length) = span(text, needle);
        format!(
            r#"{{"@type":"textEntity","offset":{offset},"length":{length},"type":{type_json}}}"#
        )
    };
    let bold = r#"{"@type":"textEntityTypeBold"}"#;
    let entities = [
        ent(body, "Bold", bold),
        ent(body, "italic", r#"{"@type":"textEntityTypeItalic"}"#),
        // Nested: bold inside italic renders bold italic.
        ent(body, "bolditalic", bold),
        ent(body, "bolditalic", r#"{"@type":"textEntityTypeItalic"}"#),
        ent(body, "underline", r#"{"@type":"textEntityTypeUnderline"}"#),
        ent(body, "strike", r#"{"@type":"textEntityTypeStrikethrough"}"#),
        ent(body, "secret", r#"{"@type":"textEntityTypeSpoiler"}"#),
        ent(body, "code", r#"{"@type":"textEntityTypeCode"}"#),
        ent(
            body,
            "https://example.com",
            r#"{"@type":"textEntityTypeUrl"}"#,
        ),
        ent(
            body,
            "fn demo() {\n  42\n}",
            r#"{"@type":"textEntityTypePreCode","language":"rust"}"#,
        ),
    ]
    .join(",");
    let text_json = serde_json::to_string(body).unwrap();
    let text_message = format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":104,"chat_id":14,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":{text_json},"entities":[{entities}]}}}}}}}}"#
    );

    let caption = "A captioned photo: bold caption and a link https://example.com/pic";
    let caption_entities = [
        ent(caption, "bold caption", bold),
        ent(
            caption,
            "https://example.com/pic",
            r#"{"@type":"textEntityTypeUrl"}"#,
        ),
    ]
    .join(",");
    let caption_json = serde_json::to_string(caption).unwrap();
    let photo_message = format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":105,"chat_id":14,"is_outgoing":false,"content":{{"@type":"messagePhoto","photo":{{"@type":"photo","has_stickers":false,"sizes":[{{"@type":"photoSize","type":"m","photo":{thumb},"width":240,"height":160,"progressive_sizes":[]}}]}},"caption":{{"@type":"formattedText","text":{caption_json},"entities":[{caption_entities}]}},"has_spoiler":false,"is_secret":false}}}}}}"#
    );

    for json in [text_message, photo_message] {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
}

pub(super) fn apply_ready_blockquote_expandable(
    session: &mut Session,
    sink: &Arc<MemorySink>,
    seq: &AtomicU64,
) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    // Dedicated chat so the collapsed long quote + Show more affordance are
    // visible without scrolling past other Ready fixtures.
    let chat_id = 15;
    let chat_json = format!(
        r#"{{"@type":"updateNewChat","chat":{{"id":{chat_id},"title":"Demo blockquotes","type":{{"@type":"chatTypePrivate","user_id":{chat_id}}},"unread_count":0}}}}"#
    );
    let position_json = format!(
        r#"{{"@type":"updateChatPosition","chat_id":{chat_id},"position":{{"@type":"chatPosition","list":{{"@type":"chatListMain"}},"order":"49","is_pinned":false}}}}"#
    );
    for json in [chat_json, position_json] {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
    session.open_chat(ChatId(chat_id));

    let short = "Short quote stays fully visible.";
    let long = "Line one of a long block quote.\nLine two of a long block quote.\nLine three of a long block quote.\nLine four — past the collapse threshold.\nLine five — Show more reveals the rest.";
    let ent = |text: &str, type_name: &str| -> String {
        let utf16_len = text.encode_utf16().count();
        format!(
            r#"{{"@type":"textEntity","offset":0,"length":{utf16_len},"type":{{"@type":"{type_name}"}}}}"#
        )
    };
    let short_json = serde_json::to_string(short).unwrap();
    let long_json = serde_json::to_string(long).unwrap();
    let short_msg = format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":201,"chat_id":{chat_id},"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":{short_json},"entities":[{}]}}}}}}}}"#,
        ent(short, "textEntityTypeBlockQuote")
    );
    let long_msg = format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":202,"chat_id":{chat_id},"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":{long_json},"entities":[{}]}}}}}}}}"#,
        ent(long, "textEntityTypeExpandableBlockQuote")
    );
    for json in [short_msg, long_msg] {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
}

pub(super) fn interaction_info_update_json(
    chat_id: ChatId,
    message_id: MessageId,
    info: &MessageInteractionInfo,
) -> String {
    let reactions = match &info.reactions {
        Some(list) if !list.reactions.is_empty() => {
            let items: Vec<String> = list
                .reactions
                .iter()
                .filter_map(|reaction| {
                    let emoji = reaction.reaction_type.emoji_text()?;
                    Some(format!(
                        r#"{{"@type":"messageReaction","type":{{"@type":"reactionTypeEmoji","emoji":{}}},"total_count":{},"is_chosen":{},"used_sender_id":null,"recent_sender_ids":[]}}"#,
                        serde_json::to_string(emoji).unwrap_or_else(|_| "\"\"".into()),
                        reaction.total_count,
                        reaction.is_chosen
                    ))
                })
                .collect();
            format!(
                r#"{{"@type":"messageReactions","reactions":[{}],"are_tags":false,"paid_reactors":[],"can_get_added_reactions":false}}"#,
                items.join(",")
            )
        }
        _ => "null".into(),
    };
    format!(
        r#"{{"@type":"updateMessageInteractionInfo","chat_id":{},"message_id":{},"interaction_info":{{"@type":"messageInteractionInfo","view_count":{},"forward_count":{},"reply_info":null,"reactions":{reactions}}}}}"#,
        chat_id.0, message_id.0, info.view_count, info.forward_count
    )
}

/// MED4: `show_caption_above_media` (TDLib 1.8.67, `schema/td_api.tl:6117`
/// photo / `:6128` video / animation). Documents / audio / voice notes
/// have no such field — their captions always render below.
pub(super) fn caption_above_media(content: &MessageContent) -> bool {
    match content {
        MessageContent::Photo(photo) => photo.show_caption_above_media,
        MessageContent::Video(video) => video.show_caption_above_media,
        MessageContent::Animation(animation) => animation.show_caption_above_media,
        _ => false,
    }
}

/// kit Phase 4: in-bubble footer — `HH:MM` plus the outgoing delivery
/// state (`…` while pending, `✓` sent, `✓✓` read). Incoming rows show
/// just the time. `None` when the message carries no date (nothing to
/// stamp) — the footer hides instead of inventing a time.
pub(super) fn message_footer(
    date: i32,
    pending: bool,
    receipt: OutboxReceipt,
) -> Option<AnyElement> {
    let time = message_time_hhmm(date)?;
    let marks = if pending {
        "…"
    } else {
        match receipt {
            OutboxReceipt::Read => "✓✓",
            OutboxReceipt::Sent => "✓",
            OutboxReceipt::None => "",
        }
    };
    let status = if pending {
        "sending"
    } else {
        match receipt {
            OutboxReceipt::Read => "read",
            OutboxReceipt::Sent => "sent",
            OutboxReceipt::None => "",
        }
    };
    let accessible = format!("{time} {status}");
    let text = if marks.is_empty() {
        time
    } else {
        format!("{time} {marks}")
    };
    Some(
        div()
            .id("message-time")
            .role(Role::Label)
            .aria_label(accessible)
            .text_xs()
            .opacity(0.7)
            .child(text)
            .into_any_element(),
    )
}

/// kit Phase 4: kit `Avatar` — photo when available, otherwise the kit's
/// initials + theme fallback colors. Shared by the message avatar slot
/// and the `initials_avatar` / `chat_avatar` helpers below.
pub(super) fn kit_avatar_element(
    name: &str,
    photo: Option<&std::path::Path>,
    size: Pixels,
) -> AnyElement {
    let mut avatar = Avatar::new().name(name).with_size(size);
    if let Some(path) = photo {
        avatar = avatar.src(path.to_path_buf());
    }
    avatar.into_any_element()
}

/// kit Phase 4: builds the shared kit-shell chrome (sender header /
/// avatar slot / footer) from row inputs.
pub(super) fn message_chrome(
    sender: Option<String>,
    receipt: OutboxReceipt,
    sender_avatar: Option<(String, Option<PathBuf>)>,
    date: i32,
    pending: bool,
) -> MessageChrome {
    MessageChrome {
        sender: sender.map(SharedString::from),
        avatar: sender_avatar
            .map(|(name, photo)| kit_avatar_element(&name, photo.as_deref(), px(32.))),
        footer: message_footer(date, pending, receipt),
    }
}

/// Monospace family for `code` / `pre` entity runs (Phase 4.1). The generic
/// family resolves through the platform font stack (fontconfig on Linux).
pub(super) const MONO_FONT: &str = "monospace";

/// Custom emoji ids in `entities` → downloaded sticker image paths, via the
/// EmojiPanel's `getCustomEmojiStickers` cache (thumbnail first, else static
/// WEBP — `StickerItem::display_file_id`). Ids without a resolved, downloaded
/// sticker are absent; the renderer falls back to the span text.
fn custom_emoji_paths(
    entities: &[TextEntity],
    stickers: &[StickerItem],
    files: &HashMap<i32, ParsedFile>,
    media_roots: &[PathBuf],
) -> HashMap<i64, PathBuf> {
    let mut out = HashMap::new();
    for entity in entities {
        let TextEntityKind::CustomEmoji { custom_emoji_id } = entity.kind else {
            continue;
        };
        if out.contains_key(&custom_emoji_id) {
            continue;
        }
        let path = stickers
            .iter()
            .find(|s| s.custom_emoji_id == Some(custom_emoji_id))
            .and_then(|s| s.display_file_id())
            .and_then(|id| files.get(&id.0))
            .and_then(|f| f.usable_path())
            .and_then(|path| sandboxed_display_path(path, media_roots));
        if let Some(path) = path {
            out.insert(custom_emoji_id, path);
        }
    }
    out
}

/// Paint one entity run with the Phase 4.1 styles (bold/italic/…, spoiler,
/// code/pre, links). Shared by plain runs and quote-block runs.
/// `emoji_paths` maps resolved custom emoji ids to sticker images; a run
/// carrying an id renders the image inline (1.25× the text size) and falls
/// back to its text while unresolved.
fn paint_text_run(
    run: &TextRun,
    index: usize,
    msg_key: (i64, u64),
    is_caption: bool,
    revealed: &std::collections::HashSet<(i64, u64, u64, bool)>,
    emoji_paths: &HashMap<i64, PathBuf>,
    // Settings → Appearance: message font size.
    font: Pixels,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let run_id = format!(
        "msg-run-{}-{}-{}-{index}",
        msg_key.0, msg_key.1, is_caption as u64
    );
    let style = &run.style;
    let revealed =
        !style.spoiler || revealed.contains(&(msg_key.0, msg_key.1, index as u64, is_caption));
    if !revealed {
        let key = (msg_key.0, msg_key.1, index as u64, is_caption);
        return div()
            .id(run_id)
            .role(Role::Button)
            .aria_label("Reveal spoiler")
            .bg(fill_muted())
            .rounded_sm()
            .px_1()
            .text_color(fill_muted())
            .cursor_pointer()
            .pressable(cx.theme())
            .on_click(cx.listener(move |this, _, _, cx| {
                this.spoiler_revealed.insert(key);
                cx.notify();
            }))
            .child(run.text.clone())
            .into_any_element();
    }
    if let Some(id) = run.custom_emoji_id
        && let Some(path) = emoji_paths.get(&id)
    {
        let edge = font * 1.25;
        let fallback_text = run.text.clone();
        let image = img(path.clone())
            .id(format!("{run_id}-emoji"))
            .w(edge)
            .h(edge)
            .object_fit(ObjectFit::Contain)
            .with_fallback(move || div().child(fallback_text.clone()).into_any_element());
        // A custom emoji inside a link keeps the link (role + click); the
        // image itself carries no text styling.
        if let Some(href) = run.href.clone() {
            return div()
                .id(run_id)
                .role(Role::Link)
                .aria_label(run.text.clone())
                .cursor_pointer()
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.open_message_url(&href, cx);
                }))
                .child(image)
                .into_any_element();
        }
        return image.into_any_element();
    }
    let mut el = div().id(run_id).when(!run.text.trim().is_empty(), |el| {
        el.role(if run.href.is_some() {
            Role::Link
        } else {
            Role::Label
        })
        .aria_label(run.text.clone())
    });
    if style.bold {
        el = el.font_weight(FontWeight::BOLD);
    }
    if style.italic {
        el = el.italic();
    }
    if style.underline {
        el = el.underline();
    }
    if style.strikethrough {
        el = el.line_through();
    }
    if style.code || style.pre {
        el = el.font_family(MONO_FONT);
    }
    if style.pre {
        el = el.w_full().bg(bg_code()).rounded_md().px_2().py_1().my_1();
    } else if style.code {
        el = el.bg(fill_muted()).rounded_sm().px_1();
    }
    if let Some(href) = run.href.clone() {
        el = el
            .text_color(accent_info())
            .underline()
            .cursor_pointer()
            .on_click(cx.listener(move |this, _, _, cx| {
                this.open_message_url(&href, cx);
            }));
    }
    el.child(run.text.clone()).into_any_element()
}

/// One blockquote block: quote styling (accent bar, like `reply_quote_strip`)
/// with long quotes collapsed to `QUOTE_COLLAPSE_LINES` lines and a kit
/// "Show more"/"Show less" affordance. `start` is the group's first run
/// index; expansion state shares the spoiler `revealed` set and its key
/// scheme instead of threading a second set through every history call site.
fn quote_block(
    group: &[TextRun],
    start: usize,
    msg_key: (i64, u64),
    is_caption: bool,
    revealed: &std::collections::HashSet<(i64, u64, u64, bool)>,
    emoji_paths: &HashMap<i64, PathBuf>,
    // Settings → Appearance: message font size.
    font: Pixels,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let quote_text: String = group.iter().map(|run| run.text.as_str()).collect();
    // Keep quote expansion separate from spoiler visibility at the same run.
    let key = (msg_key.0, msg_key.1, (start as u64) | (1 << 63), is_caption);
    let expanded = revealed.contains(&key);
    let collapsible = quote_collapses(&quote_text);
    let mut runs_row = div()
        .id(format!("msg-quote-runs-{}-{start}", msg_key.1))
        .flex()
        .flex_wrap()
        .gap_0();
    if collapsible && !expanded {
        // Truncate to the first QUOTE_COLLAPSE_LINES lines, keeping each
        // run's own styling on the visible portion.
        let mut remaining = collapsed_quote_len(&quote_text);
        for (offset, run) in group.iter().enumerate() {
            if remaining == 0 {
                break;
            }
            let shown_len = remaining.min(run.text.len());
            remaining -= shown_len;
            let mut shown_run = run.clone();
            shown_run.text.truncate(shown_len);
            runs_row = runs_row.child(paint_text_run(
                &shown_run,
                start + offset,
                msg_key,
                is_caption,
                revealed,
                emoji_paths,
                font,
                cx,
            ));
        }
    } else {
        for (offset, run) in group.iter().enumerate() {
            runs_row = runs_row.child(paint_text_run(
                run,
                start + offset,
                msg_key,
                is_caption,
                revealed,
                emoji_paths,
                font,
                cx,
            ));
        }
    }
    let mut block = div()
        .id(format!("msg-quote-{}-{start}", msg_key.1))
        .w_full()
        .mt_1()
        .mb_1()
        .px_2()
        .py_1()
        .rounded_md()
        .border_l_2()
        .border_color(accent())
        .bg(bg_canvas())
        .child(runs_row);
    if collapsible {
        let label = if expanded { "Show less" } else { "Show more" };
        block = block.child(
            Button::new(format!("msg-quote-toggle-{}-{start}", msg_key.1))
                .label(label)
                .ghost()
                .on_click(cx.listener(move |this, _, _, cx| {
                    if this.spoiler_revealed.contains(&key) {
                        this.spoiler_revealed.remove(&key);
                    } else {
                        this.spoiler_revealed.insert(key);
                    }
                    cx.notify();
                })),
        );
    }
    block.into_any_element()
}

/// Render message text or a caption with text-entity styling (Phase 4.1).
///
/// Runs come from `styled_runs`, so nested entities combine additively
/// (bold inside italic renders bold italic). `code` is a monospace chip,
/// `pre` a full-width monospace block, and spoilers render as opaque blocks
/// until tapped — reveal state lives on the app, keyed by
/// `(row_id, run index, is_caption)`. Links keep their existing
/// accent-color + open-on-click behavior. Consecutive quote runs render as
/// one blockquote block, collapsing past `QUOTE_COLLAPSE_LINES` lines.
pub(super) fn rich_text_line(
    text: &str,
    entities: &[TextEntity],
    msg_key: (i64, u64),
    is_caption: bool,
    revealed: &std::collections::HashSet<(i64, u64, u64, bool)>,
    // Settings → Appearance: message font size (was hardcoded text_sm).
    font: Pixels,
    // Resolved custom emoji sticker images (id → path); empty when none.
    emoji_paths: &HashMap<i64, PathBuf>,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let runs = styled_runs(text, entities);
    let mut line = div()
        .id(("msg-rich-text", msg_key.1 * 2 + is_caption as u64))
        .text_size(font)
        .flex()
        .flex_wrap()
        .gap_0();
    let mut index = 0;
    while index < runs.len() {
        let run = &runs[index];
        if run.text.is_empty() {
            index += 1;
            continue;
        }
        if !run.style.quote {
            line = line.child(paint_text_run(
                run,
                index,
                msg_key,
                is_caption,
                revealed,
                emoji_paths,
                font,
                cx,
            ));
            index += 1;
            continue;
        }
        // Consecutive quote runs form one blockquote: nested styles (e.g.
        // bold inside the quote) split the runs but stay one visual block.
        let start = index;
        let mut end = start;
        while end < runs.len() && runs[end].style.quote {
            end += 1;
        }
        line = line.child(quote_block(
            &runs[start..end],
            start,
            msg_key,
            is_caption,
            revealed,
            emoji_paths,
            font,
            cx,
        ));
        index = end;
    }
    line.into_any_element()
}

/// Phase 4.1: plain/link message text (with optional link-preview card).
/// `msg_key` is (chat id, message id); the spoiler-reveal lookup needs the
/// chat id because message ids are only unique within a chat.
pub(super) fn message_text_block(
    msg_key: (i64, u64),
    text: &quill::telegram::envelope::TextContent,
    files: &HashMap<i32, ParsedFile>,
    downloading: &std::collections::HashSet<i32>,
    media_roots: &[PathBuf],
    // Resolved custom emoji stickers for inline rendering (EmojiPanel cache).
    custom_emoji: &[StickerItem],
    revealed: &std::collections::HashSet<(i64, u64, u64, bool)>,
    // Settings → Appearance: message font size.
    font: Pixels,
    big_emoji: bool,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let row_id = msg_key.1;
    let font = if big_emoji && text.entities.is_empty() && text.link_preview.is_none() {
        match quill::emoji_catalog::big_emoji_count(&text.text) {
            Some(1) => font.max(px(40.)),
            Some(2) => font.max(px(36.)),
            Some(3) => font.max(px(32.)),
            _ => font,
        }
    } else {
        font
    };
    let line = rich_text_line(
        &text.text,
        &text.entities,
        msg_key,
        false,
        revealed,
        font,
        &custom_emoji_paths(&text.entities, custom_emoji, files, media_roots),
        cx,
    );
    let card = text.link_preview.as_ref().and_then(|preview| {
        preview
            .has_card()
            .then(|| link_preview_card(row_id, preview, files, downloading, media_roots, font, cx))
    });
    let above = text
        .link_preview
        .as_ref()
        .is_some_and(|preview| preview.show_above_text);
    let has_text = !text.text.is_empty();
    let mut block = div().id(("msg-text-block", row_id)).flex().flex_col();
    if above {
        block = block.when_some(card, |this, card| this.child(card));
        if has_text {
            block = block.child(line);
        }
    } else {
        if has_text {
            block = block.child(line);
        }
        block = block.when_some(card, |this, card| this.child(card));
    }
    block.into_any_element()
}

pub(super) fn link_preview_card(
    row_id: u64,
    preview: &quill::telegram::envelope::LinkPreview,
    files: &HashMap<i32, ParsedFile>,
    downloading: &std::collections::HashSet<i32>,
    media_roots: &[PathBuf],
    // Settings → Appearance: message font size.
    font: Pixels,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let site_empty = preview.site_name.is_empty();
    let title_empty = preview.title.is_empty();
    let description_empty = preview.description.is_empty();
    let site = preview.site_name.clone();
    let title = preview.title.clone();
    let description = preview.description.clone();
    let display = if preview.display_url.is_empty() {
        preview.url.clone()
    } else {
        preview.display_url.clone()
    };
    let thumb = preview.photo.as_ref().map(|photo| {
        preview_thumb(
            row_id,
            photo,
            preview.show_large_media,
            files,
            downloading,
            media_roots,
        )
    });
    // MED4: embedded players get a play/duration badge over the
    // thumbnail (schema:4434/:4443/:4452). Tap opens the embed URL in
    // the browser — inline playback is out of slice (DECISIONS.md MED4).
    let thumb = match &preview.kind {
        quill::telegram::envelope::LinkPreviewKind::EmbeddedPlayer {
            duration_secs,
            audio,
            ..
        } => thumb.map(|thumb| {
            let badge = if *duration_secs > 0 {
                format!(
                    "{} {}:{:02}",
                    if *audio { "♪" } else { "▶" },
                    duration_secs / 60,
                    duration_secs % 60
                )
            } else if *audio {
                "♪".to_string()
            } else {
                "▶".to_string()
            };
            div()
                .relative()
                .child(thumb)
                .child(
                    div()
                        .absolute()
                        .bottom_1()
                        .right_1()
                        .px_1()
                        .rounded_sm()
                        .bg(bg_black())
                        .text_xs()
                        .text_color(text_on_fill())
                        .child(badge),
                )
                .into_any_element()
        }),
        _ => thumb,
    };
    // MED4: album previews (`linkPreviewTypeAlbum`, schema:4392) show a
    // strip of up to 4 thumbnails under the card copy.
    let album_strip: Option<AnyElement> = match &preview.kind {
        quill::telegram::envelope::LinkPreviewKind::Album { thumbnails }
            if !thumbnails.is_empty() =>
        {
            let mut strip = div()
                .id(("link-preview-album", row_id))
                .flex()
                .gap_1()
                .mt_1();
            for (index, thumb_photo) in thumbnails.iter().enumerate() {
                strip = strip.child(preview_thumb(
                    row_id * 100 + index as u64,
                    thumb_photo,
                    false,
                    files,
                    downloading,
                    media_roots,
                ));
            }
            Some(strip.into_any_element())
        }
        _ => None,
    };
    // Settings → Appearance: the card copy scales with the message
    // font size — the title keeps body size, the meta lines stay one
    // step smaller (12px vs 14px at the default).
    let small = font * (12.0 / 14.0);
    let mut copy = div()
        .id(("link-preview-copy", row_id))
        .flex()
        .flex_col()
        .min_w_0()
        .gap_0();
    if !site_empty {
        copy = copy.child(
            div()
                .text_size(small)
                .font_medium()
                .text_color(accent())
                .child(site),
        );
    }
    if !title_empty {
        copy = copy.child(div().text_size(font).font_medium().child(title));
    }
    if !description_empty {
        copy = copy.child(
            div()
                .text_size(small)
                .text_color(text_primary())
                .child(description),
        );
    }
    if site_empty && title_empty && description_empty && !display.is_empty() {
        copy = copy.child(div().text_size(small).text_color(accent()).child(display));
    }
    let body = if preview.show_large_media {
        let mut column = div()
            .id(("link-preview-large", row_id))
            .flex()
            .flex_col()
            .gap_1();
        if preview.show_media_above_description {
            column = column.when_some(thumb, |this, thumb| this.child(thumb));
            column = column.child(copy);
        } else {
            column = column.child(copy);
            column = column.when_some(thumb, |this, thumb| this.child(thumb));
        }
        column
            .when_some(album_strip, |this, strip| this.child(strip))
            .into_any_element()
    } else {
        div()
            .id(("link-preview-small", row_id))
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .flex()
                    .items_start()
                    .gap_2()
                    .child(copy.flex_1())
                    .when_some(thumb, |this, thumb| this.child(thumb)),
            )
            .when_some(album_strip, |this, strip| this.child(strip))
            .into_any_element()
    };
    let preview_for_tap = preview.clone();
    div()
        .id(("link-preview", row_id))
        .mt_2()
        .px_2()
        .py_1()
        .rounded_md()
        .border_l_2()
        .border_color(accent())
        .bg(bg_canvas())
        .cursor_pointer()
        .pressable(cx.theme())
        .on_click(cx.listener(move |this, _, _, cx| {
            // MED4: `instant_view_version > 0` (schema:4570) opens the IV
            // reader (mode-gated); otherwise the browser. Embedded
            // players open their embed URL instead of the page URL.
            this.open_preview_url(&preview_for_tap, cx);
        }))
        .child(body)
        .into_any_element()
}

/// M2: render a `messageRichMessage` (schema 1.8.67, line 5143) as a stack
/// of `pageBlock*` elements. Styled text reuses `rich_text_line` (M1
/// entities); inline buttons and button rows reuse
/// `inline_keyboard_button` (URL / callback / switchInline / copy
/// handlers — the same honest tap behavior as reply-markup keyboards).
/// A partial message (`is_full == false`, schema line 123) offers a
/// "Load full message" button that fetches the rest via
/// `getFullRichMessage` instead of pretending to be complete.
pub(super) fn message_rich_block(
    msg_key: (i64, u64),
    chat_id: ChatId,
    message_id: MessageId,
    rich: &quill::telegram::envelope::RichMessageContent,
    revealed: &std::collections::HashSet<(i64, u64, u64, bool)>,
    // Settings → Appearance: message font size.
    font: Pixels,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let row_id = msg_key.1;
    let mut stack = div()
        .id(format!("msg-rich-block-{row_id}"))
        .flex()
        .flex_col()
        .gap_2();
    for (index, block) in rich.blocks.iter().enumerate() {
        if let Some(child) = rich_block_element(
            index, block, msg_key, chat_id, message_id, revealed, font, cx,
        ) {
            stack = stack.child(child);
        }
    }
    if !rich.is_full {
        stack = stack.child(
            Button::new(format!("rich-load-full-{row_id}"))
                .label("Load full message")
                .ghost()
                .on_click(cx.listener(move |this, _, _, cx| {
                    if let Some(live) = this.live.as_mut() {
                        match live.driver.fetch_full_rich_message(chat_id, message_id) {
                            Ok(_) => this.status_note = "loading full message…".into(),
                            Err(_) => this.status_note = "could not load full message".into(),
                        }
                        cx.notify();
                    }
                })),
        );
    }
    stack.into_any_element()
}

/// M2: inline photo/video rich block — an emoji tile with the caption,
/// mirroring the document tile above.
fn rich_media_tile(
    row_id: u64,
    index: usize,
    kind: &str,
    emoji: &str,
    caption: &str,
) -> AnyElement {
    // No caption → label the tile with the media kind instead of an empty row.
    let label = if caption.is_empty() {
        match kind {
            "photo" => "Photo",
            "video" => "Video",
            _ => kind,
        }
        .to_string()
    } else {
        caption.to_string()
    };
    div()
        .id(format!("rich-{kind}-{row_id}-{index}"))
        .flex()
        .flex_col()
        .gap_1()
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(div().text_sm().child(emoji.to_string()))
                .child(div().text_sm().font_medium().child(label)),
        )
        .into_any_element()
}

/// M2: one `RichBlock` as an element. `None` for invisible/unsupported
/// blocks (`pageBlockAnchor`, unknown types) — parsed, never rendered as
/// fake content.
#[allow(clippy::too_many_arguments)]
pub(super) fn rich_block_element(
    index: usize,
    block: &RichBlock,
    msg_key: (i64, u64),
    chat_id: ChatId,
    message_id: MessageId,
    revealed: &std::collections::HashSet<(i64, u64, u64, bool)>,
    // Settings → Appearance: message font size.
    font: Pixels,
    cx: &mut Context<QuillApp>,
) -> Option<AnyElement> {
    let row_id = msg_key.1;
    match block {
        RichBlock::Paragraph {
            text,
            entities,
            buttons,
        } => {
            let mut col = div()
                .id(format!("rich-para-{row_id}-{index}"))
                .flex()
                .flex_col()
                .gap_1();
            if !text.is_empty() {
                col = col.child(rich_text_line(
                    text,
                    entities,
                    msg_key,
                    false,
                    revealed,
                    font,
                    // Instant View richText* has no custom emoji entities
                    // (richTextCustomEmoji degrades to alternative_text).
                    &HashMap::new(),
                    cx,
                ));
            }
            for (button_index, button) in buttons.iter().enumerate() {
                col = col.child(
                    div()
                        .id(format!("rich-para-btn-{row_id}-{index}-{button_index}"))
                        .flex()
                        .child(
                            inline_keyboard_button(
                                chat_id,
                                message_id,
                                index,
                                button_index,
                                button,
                                None,
                                cx,
                            )
                            .flex_1(),
                        ),
                );
            }
            Some(col.into_any_element())
        }
        RichBlock::Heading {
            level,
            text,
            entities,
        } => {
            let heading = div()
                .id(format!("rich-heading-{row_id}-{index}"))
                .font_semibold();
            let heading = match level {
                1 => heading.text_xl(),
                2 => heading.text_lg(),
                _ => heading.text_base(),
            };
            Some(
                heading
                    .child(rich_text_line(
                        text,
                        entities,
                        msg_key,
                        false,
                        revealed,
                        font,
                        // Instant View richText* has no custom emoji entities.
                        &HashMap::new(),
                        cx,
                    ))
                    .into_any_element(),
            )
        }
        RichBlock::List { ordered, items } => {
            let mut col = div()
                .id(format!("rich-list-{row_id}-{index}"))
                .flex()
                .flex_col()
                .gap_1();
            for (item_index, item) in items.iter().enumerate() {
                let marker = match item.checked {
                    Some(true) => "☑",
                    Some(false) => "☐",
                    None if *ordered => &format!("{}.", item_index + 1),
                    None => "•",
                };
                col = col.child(
                    div()
                        .id(format!("rich-list-item-{row_id}-{index}-{item_index}"))
                        .flex()
                        .gap_2()
                        .child(div().text_sm().child(marker.to_string()))
                        .child(div().text_sm().child(item.text.clone())),
                );
            }
            Some(col.into_any_element())
        }
        RichBlock::Collapsible { header, body, .. } => Some(
            div()
                .id(format!("rich-details-{row_id}-{index}"))
                .flex()
                .flex_col()
                .gap_1()
                .child(div().text_sm().font_semibold().child(format!("▾ {header}")))
                .child(
                    div()
                        .ml_4()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(body.clone()),
                )
                .into_any_element(),
        ),
        RichBlock::Document {
            file_name, caption, ..
        } => {
            let mut col = div()
                .id(format!("rich-document-{row_id}-{index}"))
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(div().text_sm().child("📄"))
                        .child(div().text_sm().font_medium().child(file_name.clone())),
                );
            if !caption.is_empty() {
                col = col.child(div().text_xs().child(caption.clone()));
            }
            Some(col.into_any_element())
        }
        // Inline photos/videos mirror the document tile: emoji + caption.
        // (Thumbnails need `media_roots` plumbing through both render call
        // sites — future work, not needed for parity here.)
        RichBlock::Photo { caption, .. } => {
            Some(rich_media_tile(row_id, index, "photo", "📷", caption))
        }
        RichBlock::Video { caption, .. } => {
            Some(rich_media_tile(row_id, index, "video", "🎬", caption))
        }
        RichBlock::Table { rows } => {
            // Compact bordered grid: hairline dividers, tight cell padding,
            // header row (row 0 — matches rich.rs serialize `is_header`)
            // in semibold on a muted band.
            let border = cx.theme().border;
            let mut table = div()
                .id(format!("rich-table-{row_id}-{index}"))
                .flex()
                .flex_col()
                .border_1()
                .border_color(border)
                .rounded_md()
                .overflow_hidden();
            for (row_index, row) in rows.iter().enumerate() {
                let mut line = div()
                    .id(format!("rich-table-row-{row_id}-{index}-{row_index}"))
                    .flex();
                if row_index > 0 {
                    line = line.border_t_1().border_color(border);
                }
                for (cell_index, cell) in row.iter().enumerate() {
                    let mut cell_div = div()
                        .id(format!(
                            "rich-table-cell-{row_id}-{index}-{row_index}-{cell_index}"
                        ))
                        .flex_1()
                        .px_2()
                        .py_1()
                        .text_sm()
                        .child(cell.clone());
                    if cell_index > 0 {
                        cell_div = cell_div.border_l_1().border_color(border);
                    }
                    if row_index == 0 {
                        cell_div = cell_div.font_semibold().bg(fill_muted());
                    }
                    line = line.child(cell_div);
                }
                table = table.child(line);
            }
            Some(table.into_any_element())
        }
        RichBlock::ButtonRow { buttons } => {
            let mut line = div()
                .id(format!("rich-button-row-{row_id}-{index}"))
                .flex()
                .gap_1();
            for (button_index, button) in buttons.iter().enumerate() {
                line = line.child(
                    inline_keyboard_button(
                        chat_id,
                        message_id,
                        index,
                        button_index,
                        button,
                        None,
                        cx,
                    )
                    .flex_1(),
                );
            }
            Some(line.into_any_element())
        }
        RichBlock::Divider => Some(
            div()
                .id(format!("rich-divider-{row_id}-{index}"))
                .h_px()
                .w_full()
                .bg(cx.theme().border)
                .into_any_element(),
        ),
        RichBlock::Empty | RichBlock::Unsupported { .. } => None,
    }
}

pub(super) fn preview_thumb(
    row_id: u64,
    photo: &quill::telegram::envelope::PhotoContent,
    large: bool,
    files: &HashMap<i32, ParsedFile>,
    downloading: &std::collections::HashSet<i32>,
    media_roots: &[PathBuf],
) -> AnyElement {
    let (w, h) = if large {
        (px(240.), px(140.))
    } else {
        (px(72.), px(72.))
    };
    if let Some(path) = photo_display_path(photo, files, media_roots) {
        return img(path)
            .id(("link-preview-img", row_id))
            .w(w)
            .h(h)
            .rounded_md()
            .object_fit(ObjectFit::Cover)
            .flex_shrink_0()
            .with_fallback(move || {
                div()
                    .w(w)
                    .h(h)
                    .rounded_md()
                    .bg(fill_muted())
                    .into_any_element()
            })
            .into_any_element();
    }
    let file_id = photo
        .thumb_size()
        .map(|size| size.file_id)
        .unwrap_or(FileId(0));
    let label = if file_is_downloading(file_id, files, downloading) {
        "…"
    } else {
        "Preview"
    };
    div()
        .id(("link-preview-ph", row_id))
        .w(w)
        .h(h)
        .rounded_md()
        .bg(fill_muted())
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .child(div().text_xs().text_color(text_bright()).child(label))
        .into_any_element()
}

/// M1: `YYYY-MM-DD HH:MM` (UTC) from a unix timestamp, stdlib only — no
/// chrono dependency for one label. UTC is stated explicitly rather than
/// pretending at a local timezone the stdlib cannot compute.
pub(super) fn format_unix_date_time(unix: i64) -> String {
    // Days since epoch → civil date (Howard Hinnant's algorithm).
    let days = unix.div_euclid(86400);
    let secs_of_day = unix.rem_euclid(86400);
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z.rem_euclid(146097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if m <= 2 { y + 1 } else { y };
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02} UTC",
        year,
        m,
        d,
        secs_of_day / 3600,
        (secs_of_day % 3600) / 60
    )
}

pub(super) fn reply_quote_strip(
    row_id: MessageId,
    target_id: MessageId,
    preview: String,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    div()
        .id(("reply-quote", row_id.0 as u64))
        .mt_1()
        .mb_1()
        .px_2()
        .py_1()
        .rounded_md()
        .border_l_2()
        .border_color(accent())
        .bg(bg_canvas())
        .cursor_pointer()
        .pressable(cx.theme())
        .on_click(cx.listener(move |this, _, _, cx| {
            this.jump_to_replied_message(target_id, cx);
        }))
        .child(
            div()
                .text_xs()
                .font_medium()
                .text_color(accent())
                .child("Reply"),
        )
        .child(div().text_xs().text_color(text_primary()).child(preview))
        .into_any_element()
}
