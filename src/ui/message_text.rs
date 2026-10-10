//! message text rendering: rich text, entities, link previews, reply strips.

use super::anim_layer::LayeredClip;
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
use std::rc::Rc;
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
        r#"{{"@type":"updateNewMessage","message":{{"id":102,"chat_id":11,"is_outgoing":false,"date":1700000000,"content":{{"@type":"messageText","text":{text},"link_preview":{{"@type":"linkPreview","url":"https://video.example/watch","display_url":"video.example","site_name":"Vids","title":"Clip","description":{{"@type":"formattedText","text":"","entities":[]}},"author":"","type":{{"@type":"linkPreviewTypeEmbeddedVideoPlayer","url":"https://video.example/embed/1","thumbnail":{{"@type":"photo","has_stickers":false,"minithumbnail":null,"sizes":[{{"@type":"photoSize","type":"m","photo":{thumb},"width":90,"height":90,"progressive_sizes":[]}}]}},"duration":95,"width":640,"height":360}},"has_large_media":true,"show_large_media":true,"show_media_above_description":false,"skip_confirmation":true,"show_above_text":false,"instant_view_version":2}},"link_preview_options":null}}}}}}"#,
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

    // Every interactive entity: mention, mention-name, hashtag, cashtag,
    // command, email, phone, card, media timestamp, a hidden text link, a
    // look-alike domain, a bare domain, a date, and an RTL line with a
    // mention.
    let links = "Ping @durov and Ann #news $TON /start\nwrite hello@example.com or call +1 555 010 0199\ncard 4111 1111 1111 1111, jump to 1:30\nread the docs or visit https://\u{430}pple.com/login or example.com/path\nשלום @durov מה נשמע";
    let link_entities = [
        ent(links, "@durov", r#"{"@type":"textEntityTypeMention"}"#),
        ent(
            links,
            "Ann",
            r#"{"@type":"textEntityTypeMentionName","user_id":14}"#,
        ),
        ent(links, "#news", r#"{"@type":"textEntityTypeHashtag"}"#),
        ent(links, "$TON", r#"{"@type":"textEntityTypeCashtag"}"#),
        ent(links, "/start", r#"{"@type":"textEntityTypeBotCommand"}"#),
        ent(
            links,
            "hello@example.com",
            r#"{"@type":"textEntityTypeEmailAddress"}"#,
        ),
        ent(
            links,
            "+1 555 010 0199",
            r#"{"@type":"textEntityTypePhoneNumber"}"#,
        ),
        ent(
            links,
            "4111 1111 1111 1111",
            r#"{"@type":"textEntityTypeBankCardNumber"}"#,
        ),
        ent(
            links,
            "1:30",
            r#"{"@type":"textEntityTypeMediaTimestamp","media_timestamp":90}"#,
        ),
        ent(
            links,
            "the docs",
            r#"{"@type":"textEntityTypeTextUrl","url":"https://docs.example.org/guide"}"#,
        ),
        ent(
            links,
            "https://\u{430}pple.com/login",
            r#"{"@type":"textEntityTypeUrl"}"#,
        ),
        ent(
            links,
            "example.com/path",
            r#"{"@type":"textEntityTypeUrl"}"#,
        ),
    ]
    .join(",");
    let links_json = serde_json::to_string(links).unwrap();
    let links_message = format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":106,"chat_id":14,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":{links_json},"entities":[{link_entities}]}}}}}}}}"#
    );

    for json in [text_message, photo_message, links_message] {
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
/// state as Telegram Desktop draws it: a clock while sending, one check
/// when sent, the joined double check when read. Incoming rows show just
/// the time. `None` when the message carries no date (nothing to stamp) —
/// the footer hides instead of inventing a time.
pub(super) fn message_footer(
    date: i32,
    pending: bool,
    receipt: OutboxReceipt,
) -> Option<AnyElement> {
    use gpui_kit::assets::IconName;
    let time = message_time_hhmm(date)?;
    let mark = if pending {
        Some(IconName::Clock)
    } else {
        match receipt {
            OutboxReceipt::Read => Some(IconName::CheckCheck),
            OutboxReceipt::Sent => Some(IconName::Check),
            OutboxReceipt::None => None,
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
    Some(
        div()
            .id("message-time")
            .role(Role::Label)
            .aria_label(accessible)
            .flex()
            .items_center()
            .gap(px(3.))
            .text_xs()
            .opacity(0.7)
            .child(time)
            .when_some(mark, |this, icon| {
                this.child(Icon::new(icon).size(px(14.)).flex_none())
            })
            .into_any_element(),
    )
}

/// What the in-bubble footer shows besides the time and the receipt.
#[derive(Clone)]
pub(super) struct FooterMeta {
    pub(super) date: i32,
    pub(super) pending: bool,
    pub(super) receipt: OutboxReceipt,
    pub(super) views: Option<i32>,
    pub(super) signature: Option<String>,
    /// "edited" before the time (`lng_edited`).
    pub(super) edited: bool,
    /// The pin glyph of a pinned message.
    pub(super) pinned: bool,
    /// "imported" before the time (`lng_imported`).
    pub(super) imported: bool,
    /// Hover text with the full sent / edited / original dates.
    pub(super) tooltip: Option<String>,
}

impl FooterMeta {
    pub(super) fn of(
        message: &quill::state::HistoryMessage,
        receipt: OutboxReceipt,
        views: Option<i32>,
        signature: Option<String>,
    ) -> Self {
        Self {
            date: message.date,
            pending: message.pending,
            receipt,
            views,
            signature,
            edited: message.extras.edit_date > 0,
            pinned: message.is_pinned,
            imported: message.extras.import_info.is_some(),
            tooltip: quill::state::footer_tooltip(message, format_unix_date_time),
        }
    }

    /// Width the whole footer needs: `base` (time and receipt) plus the
    /// extras that precede it.
    pub(super) fn reserve(&self, base: Pixels) -> Pixels {
        let mut width = base;
        if self.edited || self.imported {
            width += px(44.);
        }
        if self.pinned {
            width += px(20.);
        }
        if self.views.is_some() {
            width += px(48.);
        }
        width
    }

    fn plain(&self) -> bool {
        self.views.is_none()
            && self.signature.is_none()
            && !self.edited
            && !self.pinned
            && !self.imported
            && self.tooltip.is_none()
    }
}

/// The time footer of a message: author signature, pin, view count (eye
/// icon), "edited" / "imported" and the time/receipt, on one row.
pub(super) fn message_footer_meta(meta: &FooterMeta) -> Option<AnyElement> {
    let time = message_footer(meta.date, meta.pending, meta.receipt);
    if meta.plain() {
        return time;
    }
    let tooltip = meta.tooltip.clone();
    let label = if meta.imported {
        Some("imported")
    } else if meta.edited {
        Some("edited")
    } else {
        None
    };
    Some(
        div()
            .id("message-footer")
            .flex()
            .items_center()
            .gap_2()
            .text_xs()
            .when_some(tooltip, |this, text| {
                this.tooltip(move |window, cx| {
                    gpui_kit::component::tooltip::Tooltip::new(text.clone()).build(window, cx)
                })
            })
            .when_some(meta.signature.clone(), |this, signature| {
                this.child(
                    div()
                        .opacity(0.7)
                        .max_w(px(200.))
                        .truncate()
                        .child(signature),
                )
            })
            .when(meta.pinned, |this| {
                this.child(
                    div()
                        .id("message-pinned")
                        .role(Role::Label)
                        .aria_label("pinned")
                        .opacity(0.7)
                        .child(Icon::new(gpui_kit::assets::IconName::Pin).size(px(12.))),
                )
            })
            .when_some(meta.views, |this, views| {
                this.child(
                    div()
                        .id("message-views")
                        .role(Role::Label)
                        .aria_label(format!(
                            "{} views",
                            super::statistics::format_view_count(views)
                        ))
                        .flex()
                        .items_center()
                        .gap_0p5()
                        .opacity(0.7)
                        .child(Icon::new(gpui_kit::assets::IconName::Eye).size(px(12.)))
                        .child(super::statistics::format_view_count(views)),
                )
            })
            .when_some(label, |this, label| {
                this.child(div().opacity(0.7).child(label))
            })
            .children(time)
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
    if let Some(path) = photo {
        return Avatar::new()
            .name(name)
            .with_size(size)
            // Decoded at the drawn size, not the file's (`image_budget`).
            .src(super::image_budget::sized_image(
                super::image_budget::SizedSource::Path(std::sync::Arc::from(path)),
                size,
            ))
            .into_any_element();
    }
    initials_circle(name, size)
}

/// Initials fallback avatar. The kit's own fallback sizes its text box
/// (not its font) for custom sizes, which leaves the initials small and
/// off-center; this draws the same identity colors (12 OkLCH hues keyed
/// by the initials, as gpui-kit does) with centered initials at ~40% of
/// the avatar size.
fn initials_circle(name: &str, size: Pixels) -> AnyElement {
    let initials = avatar_initials(name);
    let hue = (gpui_kit::hash(&SharedString::from(initials.clone())) % 12) as f32 * 30.;
    let (background, foreground) = if crate::ui::chat_theme::is_dark_palette() {
        (
            gpui_kit::component::oklch(0.30, 0.05, hue),
            gpui_kit::component::oklch(0.82, 0.11, hue),
        )
    } else {
        (
            gpui_kit::component::oklch(0.93, 0.05, hue),
            gpui_kit::component::oklch(0.48, 0.14, hue),
        )
    };
    div()
        .size(size)
        .flex_none()
        .rounded_full()
        .flex()
        .items_center()
        .justify_center()
        .bg(background)
        .text_color(foreground)
        .text_size(size * 0.4)
        .line_height(size * 0.4)
        .font_semibold()
        .child(initials)
        .into_any_element()
}

/// Up to two initials: first letters of the first two words, or the first
/// two characters of a single word (gpui-kit's rule), uppercased.
fn avatar_initials(name: &str) -> String {
    let words: Vec<&str> = name.split_whitespace().collect();
    let initials: String = match words.as_slice() {
        [] => String::new(),
        [single] => single.chars().take(1).collect(),
        [first, second, ..] => first
            .chars()
            .take(1)
            .chain(second.chars().take(1))
            .collect(),
    };
    initials.to_uppercase()
}

/// kit Phase 4: builds the shared kit-shell chrome (sender header /
/// avatar slot / footer) from row inputs.
pub(super) fn message_chrome(
    sender: Option<super::history_row::SenderLabel>,
    receipt: OutboxReceipt,
    sender_avatar: Option<(String, Option<PathBuf>)>,
    date: i32,
    pending: bool,
) -> MessageChrome {
    MessageChrome {
        sender_color: sender
            .as_ref()
            .and_then(|label| label.accent)
            .map(super::chat_theme::peer_name_color),
        sender: sender.map(|label| SharedString::from(label.name)),
        // An empty name is a spacer: a row inside a sender run keeps the
        // avatar column so bubbles stay aligned.
        avatar: sender_avatar.map(|(name, photo)| {
            if name.is_empty() {
                div().size(px(32.)).flex_none().into_any_element()
            } else {
                kit_avatar_element(&name, photo.as_deref(), px(32.))
            }
        }),
        footer: message_footer(date, pending, receipt),
        footer_inline: false,
        footer_overlay: false,
        footer_rebuild: None,
        media_led: false,
        actions: None,
        actions_span: px(34.),
        bottom_bar: None,
        media_width: None,
    }
}

/// Monospace family for `code` / `pre` entity runs (Phase 4.1). macOS has
/// no generic `monospace` alias, so name its system face; elsewhere the
/// generic family resolves through the platform font stack.
#[cfg(target_os = "macos")]
pub(super) const MONO_FONT: &str = "Menlo";
#[cfg(target_os = "windows")]
pub(super) const MONO_FONT: &str = "Consolas";
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub(super) const MONO_FONT: &str = "monospace";

/// Custom emoji ids in `entities` → downloaded sticker image paths, via the
/// EmojiPanel's `getCustomEmojiStickers` cache (thumbnail first, else static
/// WEBP — `StickerItem::display_file_id`). Ids without a resolved, downloaded
/// sticker are absent; the renderer falls back to the span text.
pub(super) fn custom_emoji_paths(
    entities: &[TextEntity],
    stickers: &[StickerItem],
    files: &HashMap<i32, ParsedFile>,
    media_roots: &[PathBuf],
) -> HashMap<i64, ImageSource> {
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
            out.insert(custom_emoji_id, ImageSource::from(path));
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
    emoji_paths: &HashMap<i64, ImageSource>,
    // Decoded animated custom emoji, drawn by the conversation's
    // animation layer (`anim_layer`); they take precedence over stills.
    layered: &HashMap<i64, LayeredClip>,
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
        // The text keeps its room but is invisible; specks in the text's
        // color drift over it (tdesktop).
        let layer = super::anim_layer::current();
        let specks = canvas(
            |_, _, _| {},
            move |bounds, _, window, _| {
                let color = window.text_style().color;
                super::spoiler_fx::layer_text_specks(
                    layer.as_ref(),
                    bounds,
                    bounds.origin,
                    color,
                    window,
                );
            },
        )
        .absolute()
        .inset_0()
        .size_full();
        return div()
            .id(run_id)
            .relative()
            .role(Role::Button)
            .aria_label("Reveal spoiler")
            .cursor_pointer()
            .on_click(cx.listener(move |this, _, _, cx| {
                this.spoiler_revealed.insert(key);
                super::spoiler_fx::mark_revealed(key);
                cx.notify();
            }))
            .child(
                div()
                    .text_color(gpui_kit::transparent_black())
                    .child(run.text.clone()),
            )
            .child(specks)
            .into_any_element();
    }
    let edge = font * 1.25;
    let emoji_image = run.custom_emoji_id.and_then(|id| {
        if let Some(clip) = layered.get(&id) {
            // Small emoji look the same at 30 fps.
            return Some(
                super::anim_layer::frames(clip.clone(), 30)
                    .flex_none()
                    .size(edge)
                    .into_any_element(),
            );
        }
        let fallback_text = run.text.clone();
        emoji_paths.get(&id).map(|source| {
            img(source.clone())
                .id(format!("{run_id}-emoji"))
                .w(edge)
                .h(edge)
                .aspect_square()
                .object_fit(ObjectFit::Contain)
                .with_fallback(move || div().child(fallback_text.clone()).into_any_element())
                .into_any_element()
        })
    });
    if let Some(image) = emoji_image {
        // A custom emoji inside a link keeps the link (role + click); the
        // image itself carries no text styling.
        if let Some(link) = run.link.clone() {
            return div()
                .id(run_id)
                .role(Role::Link)
                .aria_label(run.text.clone())
                .cursor_pointer()
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.queue_link(link.clone(), msg_key, cx);
                }))
                .child(image)
                .into_any_element();
        }
        // A tap shows which pack the emoji comes from.
        if let Some(emoji_id) = run.custom_emoji_id {
            return div()
                .id(format!("{run_id}-tap"))
                .role(Role::Button)
                .aria_label("Custom emoji")
                .cursor_pointer()
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.tap_custom_emoji(emoji_id, cx);
                }))
                .child(image)
                .into_any_element();
        }
        return image.into_any_element();
    }
    let mut el = div().id(run_id).when(!run.text.trim().is_empty(), |el| {
        el.role(if run.link.is_some() {
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
        return pre_block(run, &run_id_for_pre(msg_key, is_caption, index), font, cx);
    } else if style.code {
        el = el.bg(fill_muted()).rounded_sm().px_1();
    }
    if let Some(link) = run.link.clone() {
        el = el
            .text_color(accent_info())
            .underline()
            .cursor_pointer()
            .on_click(cx.listener(move |this, _, _, cx| {
                this.queue_link(link.clone(), msg_key, cx);
            }));
    }
    el.child(run.text.clone()).into_any_element()
}

fn run_id_for_pre(msg_key: (i64, u64), is_caption: bool, index: usize) -> String {
    format!(
        "msg-pre-{}-{}-{}-{index}",
        msg_key.0, msg_key.1, is_caption as u64
    )
}

/// A `pre` / `preCode` block like Telegram Desktop's: the language (when
/// there is one) above the code, and a copy button in the corner that shows
/// while the pointer is over the block.
fn pre_block(run: &TextRun, id: &str, font: Pixels, cx: &mut Context<QuillApp>) -> AnyElement {
    let group: SharedString = format!("{id}-group").into();
    let code = run.text.trim_end_matches('\n').to_string();
    let copy_text = code.clone();
    let language = run
        .style
        .language
        .clone()
        .filter(|language| !language.trim().is_empty());
    div()
        .id(id.to_string())
        .group(group.clone())
        .relative()
        .w_full()
        .bg(bg_code())
        .rounded_md()
        .px_2()
        .py_1()
        .my_1()
        .font_family(MONO_FONT)
        .when_some(language, |this, language| {
            this.child(
                div()
                    .text_size(font * 0.8)
                    .text_color(text_muted())
                    .child(language),
            )
        })
        .child(div().pr_6().child(code))
        .child(
            div()
                .absolute()
                .top_1()
                .right_1()
                .invisible()
                .group_hover(group, |style| style.visible())
                .child(
                    Button::new(format!("{id}-copy"))
                        .icon(IconName::Copy)
                        .xsmall()
                        .ghost()
                        .tooltip("Copy")
                        .accessibility_label("Copy code")
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.copy_entity_text(copy_text.clone(), cx);
                        })),
                ),
        )
        .into_any_element()
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
    emoji_paths: &HashMap<i64, ImageSource>,
    layered: &HashMap<i64, LayeredClip>,
    // Settings → Appearance: message font size.
    font: Pixels,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let quote_text: String = group.iter().map(|run| run.text.as_str()).collect();
    // Keep quote expansion separate from spoiler visibility at the same run.
    let key = (msg_key.0, msg_key.1, (start as u64) | (1 << 63), is_caption);
    let expanded = revealed.contains(&key);
    let collapsible = quote_collapses(&quote_text);
    let shown: Vec<TextRun> = if collapsible && !expanded {
        // Truncate to the first QUOTE_COLLAPSE_LINES lines, keeping each
        // run's own styling on the visible portion.
        let mut remaining = collapsed_quote_len(&quote_text);
        let mut shown = Vec::new();
        for run in group {
            if remaining == 0 {
                break;
            }
            let shown_len = remaining.min(run.text.len());
            remaining -= shown_len;
            let mut shown_run = run.clone();
            shown_run.text.truncate(shown_len);
            shown.push(shown_run);
        }
        shown
    } else {
        group.to_vec()
    };
    let runs_row = inline_paragraph(
        &shown,
        start,
        msg_key,
        is_caption,
        revealed,
        emoji_paths,
        layered,
        font,
        None,
        cx,
    );
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
    emoji_paths: &HashMap<i64, ImageSource>,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    rich_text_reserving(
        text,
        entities,
        msg_key,
        is_caption,
        revealed,
        font,
        emoji_paths,
        &HashMap::new(),
        None,
        cx,
    )
}

/// `rich_text_line`, optionally ending the last paragraph with `reserve`
/// of invisible trailing space. The bubble paints its time/receipt footer
/// over that space: the footer shares the last line when it fits and
/// moves to its own line when it doesn't, without measuring text.
pub(super) fn rich_text_reserving(
    text: &str,
    entities: &[TextEntity],
    msg_key: (i64, u64),
    is_caption: bool,
    revealed: &std::collections::HashSet<(i64, u64, u64, bool)>,
    font: Pixels,
    emoji_paths: &HashMap<i64, ImageSource>,
    // Animated custom emoji for the animation layer (see `paint_text_run`).
    layered: &HashMap<i64, LayeredClip>,
    reserve: Option<Pixels>,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let runs = styled_runs(text, entities);
    let mut column = div()
        .id(("msg-rich-text", msg_key.1 * 2 + is_caption as u64))
        .text_size(font)
        .flex()
        .flex_col()
        .min_w_0();
    // Paragraph = maximal stretch of inline runs; quotes and `pre` blocks
    // break the flow and render as their own blocks.
    let mut index = 0;
    while index < runs.len() {
        let start = index;
        if runs[index].style.quote {
            while index < runs.len() && runs[index].style.quote {
                index += 1;
            }
            column = column.child(quote_block(
                &runs[start..index],
                start,
                msg_key,
                is_caption,
                revealed,
                emoji_paths,
                layered,
                font,
                cx,
            ));
            continue;
        }
        if runs[index].style.pre {
            column = column.child(paint_text_run(
                &runs[index],
                index,
                msg_key,
                is_caption,
                revealed,
                emoji_paths,
                layered,
                font,
                cx,
            ));
            index += 1;
            continue;
        }
        while index < runs.len() && !runs[index].style.quote && !runs[index].style.pre {
            index += 1;
        }
        let last = index == runs.len();
        column = column.child(inline_paragraph(
            &runs[start..index],
            start,
            msg_key,
            is_caption,
            revealed,
            emoji_paths,
            layered,
            font,
            reserve.filter(|_| last),
            cx,
        ));
    }
    if let Some(reserve) = reserve
        && runs
            .last()
            .is_none_or(|run| run.style.quote || run.style.pre)
    {
        // Text ending in a block: the footer gets a line of its own.
        column = column.child(div().h(font * 1.2).w(reserve));
    }
    column.into_any_element()
}

/// Clickable span inside an inline paragraph.
#[derive(Clone)]
enum InlineAction {
    Link(quill::text::LinkTarget),
    /// Inline `code`: a click copies it (Telegram Desktop).
    CopyCode(String),
    RevealSpoiler((i64, u64, u64, bool)),
}

/// One flowing paragraph of inline runs as a single `StyledText`, so
/// mixed formatting wraps like prose (a bold word mid-sentence stays on
/// its line). A resolved custom emoji is one invisible em-wide glyph in
/// that text, with its image painted over the glyph after layout
/// (`InlineEmoji`), so it wraps and aligns like any other character.
fn inline_paragraph(
    runs: &[TextRun],
    first_index: usize,
    msg_key: (i64, u64),
    is_caption: bool,
    revealed: &std::collections::HashSet<(i64, u64, u64, bool)>,
    emoji_paths: &HashMap<i64, ImageSource>,
    layered: &HashMap<i64, LayeredClip>,
    font: Pixels,
    reserve: Option<Pixels>,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let mut text = String::new();
    let mut highlights: Vec<(std::ops::Range<usize>, HighlightStyle)> = Vec::new();
    let mut mono: Vec<(std::ops::Range<usize>, SharedString)> = Vec::new();
    let mut click_ranges = Vec::new();
    let mut actions = Vec::new();
    // Per click range: whether it is a link (underlined on hover).
    let mut link_flags: Vec<bool> = Vec::new();
    let mut spoilers: Vec<(std::ops::Range<usize>, f32)> = Vec::new();
    let mut inline_emoji: Vec<super::selectable_text::InlineEmoji> = Vec::new();
    for (offset, run) in runs.iter().enumerate() {
        if run.text.is_empty() {
            continue;
        }
        let style = &run.style;
        let key = (
            msg_key.0,
            msg_key.1,
            (first_index + offset) as u64,
            is_caption,
        );
        let hidden = style.spoiler && !revealed.contains(&key);
        // A resolved custom emoji (animated first) becomes a placeholder
        // glyph; unresolved ones and hidden spoilers keep their text.
        let visual = run.custom_emoji_id.filter(|_| !hidden).and_then(|id| {
            layered
                .get(&id)
                .map(|clip| super::selectable_text::InlineEmojiVisual::Clip(clip.clone()))
                .or_else(|| {
                    emoji_paths.get(&id).map(|source| {
                        super::selectable_text::InlineEmojiVisual::Image(source.clone())
                    })
                })
        });
        let range = if visual.is_some() {
            let at = text.len();
            text.push(super::selectable_text::EMOJI_PLACEHOLDER);
            at..text.len()
        } else {
            let at = text.len();
            text.push_str(&run.text);
            at..text.len()
        };
        if let Some(visual) = visual {
            // The picture carries no text styling; a link around it keeps
            // its click target.
            if let Some(link) = &run.link {
                click_ranges.push(range.clone());
                actions.push(InlineAction::Link(link.clone()));
                link_flags.push(true);
            }
            inline_emoji.push(super::selectable_text::InlineEmoji {
                range,
                visual,
                fallback: run.text.clone(),
            });
            continue;
        }
        let mut highlight = HighlightStyle::default();
        if style.bold {
            highlight.font_weight = Some(FontWeight::BOLD);
        }
        if style.italic {
            highlight.font_style = Some(FontStyle::Italic);
        }
        if style.underline {
            highlight.underline = Some(UnderlineStyle {
                thickness: px(1.),
                ..Default::default()
            });
        }
        if style.strikethrough {
            highlight.strikethrough = Some(StrikethroughStyle {
                thickness: px(1.),
                ..Default::default()
            });
        }
        if style.code {
            highlight.background_color = Some(fill_muted().into());
            mono.push((range.clone(), MONO_FONT.into()));
        }
        // tdesktop hides spoiler text under drifting specks of its color;
        // a revealed run's specks fade out over the text.
        if hidden {
            spoilers.push((range.clone(), 1.));
        } else if style.spoiler
            && let Some(fade) = super::spoiler_fx::reveal_fade(key)
        {
            spoilers.push((range.clone(), fade));
        }
        if hidden {
            // GPUI blends a highlight color over the text's, so a clear
            // color changes nothing: fade the glyphs out instead.
            highlight.color = None;
            highlight.fade_out = Some(1.);
            highlight.background_color = None;
            click_ranges.push(range.clone());
            actions.push(InlineAction::RevealSpoiler(key));
            link_flags.push(false);
        } else if let Some(link) = &run.link {
            highlight.color = Some(accent_info().into());
            click_ranges.push(range.clone());
            actions.push(InlineAction::Link(link.clone()));
            link_flags.push(true);
        } else if style.code && !style.pre {
            click_ranges.push(range.clone());
            actions.push(InlineAction::CopyCode(run.text.clone()));
            link_flags.push(false);
        }
        if highlight != HighlightStyle::default() {
            highlights.push((range, highlight));
        }
    }
    let label = text.clone();
    // Right-to-left paragraphs align right in the bubble (Telegram Desktop).
    let rtl = quill::text::is_rtl_text(&text);
    if let Some(reserve) = reserve {
        // Em spaces track the font size, so `reserve / font` of them span
        // the footer's width at any text size.
        let count = (reserve / font).ceil().max(1.) as usize;
        let range = text.len()..text.len() + count * '\u{2003}'.len_utf8();
        text.extend(std::iter::repeat_n('\u{2003}', count));
        highlights.push((
            range,
            HighlightStyle {
                color: Some(gpui_kit::transparent_black()),
                ..Default::default()
            },
        ));
    }
    let full: SharedString = text.clone().into();
    let bidi_source = (highlights.clone(), mono.clone());
    let styled = StyledText::new(text)
        .with_highlights(highlights)
        .with_font_family_overrides(mono);
    let owner = cx.entity().downgrade();
    let actions = Rc::new(actions);
    let press_actions = actions.clone();
    let press_owner = owner.clone();
    let hover_actions = actions.clone();
    let hover_owner = owner.clone();
    let paragraph = super::selectable_text::SelectableRichText::new(
        format!("msg-par-{}-{}-{first_index}", msg_key.1, is_caption as u8),
        full,
        styled,
    )
    .bidi(bidi_source.0, bidi_source.1)
    .selection_color(accent().opacity(0.35).into())
    .message(msg_key)
    // Messages read top to bottom by id; paragraphs within one in order.
    .document_order(msg_key.1.saturating_mul(1024) + first_index as u64 * 2 + is_caption as u64)
    .on_click(click_ranges, move |ix, _, cx| {
        let Some(action) = actions.get(ix).cloned() else {
            return;
        };
        let _ = owner.update(cx, |this, cx| match action {
            InlineAction::Link(link) => this.queue_link(link, msg_key, cx),
            InlineAction::CopyCode(text) => this.copy_entity_text(text, cx),
            InlineAction::RevealSpoiler(key) => {
                this.spoiler_revealed.insert(key);
                super::spoiler_fx::mark_revealed(key);
                cx.notify();
            }
        });
    })
    .spoilers(spoilers)
    .inline_emoji(inline_emoji)
    .link_underline(link_flags, accent_info().into())
    .on_secondary_press(move |ix, position, _, cx| {
        if let Some(InlineAction::Link(link)) = press_actions.get(ix) {
            let link = link.clone();
            let _ = press_owner.update(cx, |this, _| this.note_right_clicked_link(position, link));
        }
    })
    .on_range_hover(move |hover, _, cx| {
        let tooltip = hover.and_then(|(ix, position)| match hover_actions.get(ix) {
            Some(InlineAction::Link(link)) => link.tooltip().map(|text| (position, text)),
            _ => None,
        });
        let _ = hover_owner.update(cx, |this, cx| this.set_link_tooltip(tooltip, cx));
    });
    div()
        .id(format!(
            "msg-par-wrap-{}-{}-{first_index}",
            msg_key.1, is_caption as u8
        ))
        .role(Role::Label)
        .aria_label(label)
        .min_w_0()
        .when(rtl, |this| this.w_full().text_right())
        .child(paragraph)
        .into_any_element()
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
    // Animated frames of this message's custom emoji, when decoded; they
    // replace the still images.
    animated_emoji: &HashMap<i64, super::sticker_playback::AnimatedVisual>,
    revealed: &std::collections::HashSet<(i64, u64, u64, bool)>,
    // Settings → Appearance: message font size.
    font: Pixels,
    big_emoji: bool,
    // Trailing space for the bubble's inline time footer (see
    // `rich_text_reserving`); only when the text is the bubble's last part.
    reserve: Option<Pixels>,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let row_id = msg_key.1;
    let font = if big_emoji && text.link_preview.is_none() {
        match quill::emoji_catalog::big_emoji_count_with_entities(&text.text, &text.entities) {
            Some(1) => font.max(px(40.)),
            Some(2) => font.max(px(36.)),
            Some(3) => font.max(px(32.)),
            _ => font,
        }
    } else {
        font
    };
    let above = text
        .link_preview
        .as_ref()
        .is_some_and(|preview| preview.show_above_text);
    let card_below = !above
        && text
            .link_preview
            .as_ref()
            .is_some_and(|preview| preview.has_card());
    let mut images = custom_emoji_paths(&text.entities, custom_emoji, files, media_roots);
    let mut layered = HashMap::new();
    for (id, visual) in animated_emoji {
        match visual {
            super::sticker_playback::AnimatedVisual::Image(frame) => {
                images.insert(*id, ImageSource::from(frame.clone()));
            }
            super::sticker_playback::AnimatedVisual::Layered(clip) => {
                layered.insert(*id, clip.clone());
            }
        }
    }
    let line = rich_text_reserving(
        &text.text,
        &text.entities,
        msg_key,
        false,
        revealed,
        font,
        &images,
        &layered,
        reserve.filter(|_| !card_below),
        cx,
    );
    let card = text.link_preview.as_ref().and_then(|preview| {
        preview
            .has_card()
            .then(|| link_preview_card(row_id, preview, files, downloading, media_roots, font, cx))
    });
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
        .role(Role::Link)
        .aria_label("Open link preview")
        .tab_index(0)
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
        return img(super::image_budget::sized_media(
            &path,
            (w, h),
            photo
                .largest_size()
                .or_else(|| photo.thumb_size())
                .map(|size| (size.width, size.height)),
            super::image_budget::Fit::Cover,
        ))
        .id(("link-preview-img", row_id))
        .w(w)
        .h(h)
        .aspect_ratio(w / h)
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

/// Full local timestamp (`28 September 2026, 21:42`).
pub(super) fn format_unix_date_time(unix: i64) -> String {
    quill::local_time::full_stamp(&quill::local_time::civil_local(unix))
}

#[allow(dead_code)]
pub(super) fn reply_quote_strip(
    row_id: MessageId,
    target_id: MessageId,
    preview: String,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    div()
        .id(("reply-quote", row_id.0 as u64))
        .role(Role::Button)
        .aria_label(format!("Go to replied message: {preview}"))
        .tab_index(0)
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
        .child(div().text_xs().truncate().text_color(text_primary()).child(
            super::bidi_line::one_line_plain(super::search_ui::one_line_preview(&preview)),
        ))
        .into_any_element()
}
