use crate::composer::{
    ComposerEntity, ComposerScheduling, FormatKind, PreviewMediaSize, SendOptions, find_urls,
    parse_format_markup,
};
use crate::ids::{ChatId, MessageId, RequestId};
use serde_json::{Value, json};

/// Slice G1: `inputTextQuote` JSON (TDLib 1.8.67, `schema/td_api.tl:3056`):
/// `inputTextQuote text:formattedText position:int32 = InputTextQuote;`
/// `position` is the offset of the quoted text in the original message in
/// UTF-16 code units.
pub fn input_text_quote_json(text: &str, position: i32) -> Value {
    json!({
        "@type": "inputTextQuote",
        "text": {
            "@type": "formattedText",
            "text": text,
            "entities": []
        },
        "position": position,
    })
}

/// Slice G1: reply target for the send builders — a message id plus an
/// optional validated partial quote (`inputTextQuote`, schema 1.8.67
/// line 3056). Replaces bare `Option<MessageId>` wherever a send can
/// carry a quote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SendReply {
    pub message_id: MessageId,
    pub quote: Option<(String, i32)>,
}

impl SendReply {
    pub fn plain(message_id: MessageId) -> Self {
        Self {
            message_id,
            quote: None,
        }
    }
}

/// Slice G1: `reply_to` JSON for a send builder — delegates to
/// `input_message_reply_to_with_quote` so whole-message replies keep
/// the exact shape the old `input_message_reply_to` produced.
pub(crate) fn send_reply_value(reply_to: Option<&SendReply>) -> Value {
    input_message_reply_to_with_quote(
        reply_to.map(|reply| reply.message_id),
        reply_to.and_then(|reply| {
            reply
                .quote
                .as_ref()
                .map(|(text, position)| (text.as_str(), *position))
        }),
    )
}

/// Slice G1: same-chat reply with an optional quote (TDLib 1.8.67,
/// `schema/td_api.tl:3086`):
/// `inputMessageReplyToMessage message_id:int53 quote:inputTextQuote
/// checklist_task_id:int32 poll_option_id:string = InputMessageReplyTo;`
/// `quote` is null for a whole-message reply.
pub fn input_message_reply_to_with_quote(
    message_id: Option<MessageId>,
    quote: Option<(&str, i32)>,
) -> Value {
    match message_id {
        None => Value::Null,
        Some(id) => json!({
            "@type": "inputMessageReplyToMessage",
            "message_id": id.0,
            "quote": quote.map(|(text, position)| input_text_quote_json(text, position)).unwrap_or(Value::Null),
            "checklist_task_id": 0,
            "poll_option_id": ""
        }),
    }
}

/// `viewMessages` (TDLib 1.8.67). `source` is `messageSourceChatHistory`.
/// `force_read` marks the ids read even if `openChat` has not completed.
pub fn view_messages(
    extra: RequestId,
    chat_id: ChatId,
    message_ids: &[MessageId],
    source: &str,
    force_read: bool,
) -> String {
    json!({
        "@type": "viewMessages",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "message_ids": message_ids.iter().map(|id| id.0).collect::<Vec<_>>(),
        "source": { "@type": source },
        "force_read": force_read,
    })
    .to_string()
}

/// M1: `textEntity` JSON for a parsed composer entity (TDLib 1.8.67,
/// `schema/td_api.tl:5743`–`:5773` — `textEntityTypeBold`,
/// `textEntityTypeItalic`, `textEntityTypeUnderline`,
/// `textEntityTypeStrikethrough`, `textEntityTypeSpoiler`,
/// `textEntityTypeCode`, `textEntityTypePre` / `textEntityTypePreCode
/// language`, `textEntityTypeBlockQuote`, `textEntityTypeTextUrl url`).
/// Offsets/lengths are UTF-16 code units, per the schema.
pub fn format_entity_json(entity: &ComposerEntity) -> Value {
    let entity_type = match entity.kind {
        FormatKind::Bold => json!({"@type": "textEntityTypeBold"}),
        FormatKind::Italic => json!({"@type": "textEntityTypeItalic"}),
        FormatKind::Underline => json!({"@type": "textEntityTypeUnderline"}),
        FormatKind::Strikethrough => json!({"@type": "textEntityTypeStrikethrough"}),
        FormatKind::Code => json!({"@type": "textEntityTypeCode"}),
        FormatKind::Pre if entity.language.is_empty() => {
            json!({"@type": "textEntityTypePre"})
        }
        FormatKind::Pre => json!({
            "@type": "textEntityTypePreCode",
            "language": entity.language,
        }),
        FormatKind::Spoiler => json!({"@type": "textEntityTypeSpoiler"}),
        FormatKind::BlockQuote => json!({"@type": "textEntityTypeBlockQuote"}),
        FormatKind::TextUrl => json!({
            "@type": "textEntityTypeTextUrl",
            "url": entity.url,
        }),
        FormatKind::CustomEmoji => json!({
            "@type": "textEntityTypeCustomEmoji",
            // int64 ids travel as strings.
            "custom_emoji_id": entity
                .url
                .strip_prefix("tg://emoji?id=")
                .unwrap_or_default(),
        }),
    };
    json!({
        "@type": "textEntity",
        "offset": entity.offset,
        "length": entity.length,
        "type": entity_type,
    })
}

/// M1: `messageSendOptions` JSON (TDLib 1.8.67, `schema/td_api.tl:5934`).
/// `scheduling_state` is `messageSchedulingStateSendAtDate` (`:5902`) or
/// `messageSchedulingStateSendWhenOnline` (`:5905`); null otherwise.
pub fn message_send_options(options: &SendOptions) -> Value {
    let scheduling_state = match options.scheduling {
        ComposerScheduling::None => Value::Null,
        ComposerScheduling::SendAtDate(send_date) => json!({
            "@type": "messageSchedulingStateSendAtDate",
            "send_date": send_date as i32,
            "repeat_period": 0,
        }),
        ComposerScheduling::SendWhenOnline => {
            json!({"@type": "messageSchedulingStateSendWhenOnline"})
        }
    };
    json!({
        "@type": "messageSendOptions",
        "suggested_post_info": Value::Null,
        "disable_notification": options.disable_notification,
        "from_background": false,
        "protect_content": false,
        "allow_paid_broadcast": false,
        "paid_message_star_count": 0,
        "update_order_of_installed_sticker_sets": options.update_order_of_installed_sticker_sets,
        "scheduling_state": scheduling_state,
        "effect_id": 0,
        "sending_id": 0,
        "only_preview": false,
    })
}

/// `sendMessage` for the pinned 1.8.67 schema: typed `topic_id`, not `message_thread_id`.
/// Parity slice 4: posting into a forum topic passes
/// `topic_id = messageTopicForum{forum_topic_id}` (schema 1.8.67, lines
/// 12200 and 3004); `None` sends null (no topic).
pub(crate) fn message_topic_value(topic_id: Option<i32>) -> Value {
    match topic_id {
        Some(forum_topic_id) => json!({
            "@type": "messageTopicForum",
            "forum_topic_id": forum_topic_id,
        }),
        None => Value::Null,
    }
}

pub fn send_text(
    extra: RequestId,
    chat_id: ChatId,
    topic_id: Option<i32>,
    text: &str,
    reply_to: Option<SendReply>,
    options: &SendOptions,
) -> String {
    // M1: composer markup (`**bold**` etc.) becomes `textEntities` here,
    // so formatting genuinely reaches the wire on every text send.
    // M1 fix-up: secret chats strip `textEntityTypeBlockQuote` (schema:
    // unsupported there) — see `secret_chat_entities`.
    let (clean_text, entities) = parse_format_markup(text);
    let entities = secret_chat_entities(entities, options.is_secret);
    let entities_json: Vec<Value> = entities.iter().map(format_entity_json).collect();
    // Phase S1: secret chats never get link previews (TGX default-off —
    // previews are generated on Telegram servers, which can't see E2E
    // content). `is_disabled: true` makes the default-off explicit on the
    // wire instead of relying on TDLib to skip it.
    let link_preview_options = if options.link_preview_disabled {
        json!({
            "@type": "linkPreviewOptions",
            "is_disabled": true,
            "url": "",
            "force_small_media": false,
            "force_large_media": false,
            "show_above_text": false,
        })
    } else if options.link_preview_above_text
        || !matches!(options.link_preview_media, PreviewMediaSize::Auto)
    {
        // MED4b: full `linkPreviewOptions` (schema:2237). The force flags
        // are ignored unless the URL is explicitly specified, so the
        // detected first URL rides along (TGX sets `options.url` when
        // forcing — `MessagesController.takeOutputLinkPreviewOptions`).
        let first_url = find_urls(text).into_iter().next().unwrap_or_default();
        json!({
            "@type": "linkPreviewOptions",
            "is_disabled": false,
            "url": first_url,
            "force_small_media": matches!(options.link_preview_media, PreviewMediaSize::ForceSmall),
            "force_large_media": matches!(options.link_preview_media, PreviewMediaSize::ForceLarge),
            "show_above_text": options.link_preview_above_text,
        })
    } else {
        Value::Null
    };
    json!({
        "@type": "sendMessage",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "topic_id": message_topic_value(topic_id),
        "reply_to": send_reply_value(reply_to.as_ref()),
        "options": message_send_options(options),
        "reply_markup": Value::Null,
        "input_message_content": {
            "@type": "inputMessageText",
            "text": {
                "@type": "formattedText",
                "text": clean_text,
                "entities": entities_json
            },
            "link_preview_options": link_preview_options,
            "clear_draft": true
        }
    })
    .to_string()
}

/// M2: `sendMessage` + `inputMessageRichMessage` (TDLib 1.8.67, line 6084).
/// `rich` is the `inputRichMessage` object built by
/// `quill::rich::input_rich_message`. Reply / scheduling / silent options
/// ride the same `sendMessage` envelope as text sends.
pub fn send_rich_message(
    extra: RequestId,
    chat_id: ChatId,
    topic_id: Option<i32>,
    rich: &Value,
    reply_to: Option<SendReply>,
    options: &SendOptions,
) -> String {
    json!({
        "@type": "sendMessage",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "topic_id": message_topic_value(topic_id),
        "reply_to": send_reply_value(reply_to.as_ref()),
        "options": message_send_options(options),
        "reply_markup": Value::Null,
        "input_message_content": {
            "@type": "inputMessageRichMessage",
            "message": rich,
            "clear_draft": true
        }
    })
    .to_string()
}

/// MED4: `getWebPageInstantView` (TDLib 1.8.67, `schema/td_api.tl:14794`).
/// `only_local: false` — a network fetch is exactly what opening IV is
/// for. TDLib answers `webPageInstantView` or a 404 error when the page
/// has no Instant View (the caller falls back to the browser).
pub fn get_web_page_instant_view(extra: RequestId, url: &str) -> String {
    json!({
        "@type": "getWebPageInstantView",
        "@extra": extra.as_extra(),
        "url": url,
        "only_local": false,
    })
    .to_string()
}

/// MED4b: `getLinkPreview` (TDLib 1.8.67, `schema/td_api.tl:14792`) —
/// "Returns a link preview by the text of a message. Do not call this
/// function too often. Returns a 404 error if the text has no link
/// preview". TGX (`LinkPreview.loadLinkPreview`) passes the URL as the
/// text with null options; Quill does the same and debounces at the UI.
pub fn get_link_preview(extra: RequestId, url: &str) -> String {
    json!({
        "@type": "getLinkPreview",
        "@extra": extra.as_extra(),
        "text": {
            "@type": "formattedText",
            "text": url,
            "entities": []
        },
        "link_preview_options": Value::Null,
    })
    .to_string()
}

/// M2: `getFullRichMessage` (TDLib 1.8.67, line 11554) — fetch the full
/// blocks of a partially received `richMessage` (`is_full == false`).
pub fn get_full_rich_message(extra: RequestId, chat_id: ChatId, message_id: MessageId) -> String {
    json!({
        "@type": "getFullRichMessage",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "message_id": message_id.0,
    })
    .to_string()
}

/// Slice msg-richtext-ai-tools: `fixTextWithAi` (TDLib 1.8.67,
/// `schema/td_api.tl:12172`):
/// `fixTextWithAi text:formattedText = FixedText;`
/// The composer draft is plain text, so it rides as a `formattedText`
/// with no entities.
pub fn fix_text_with_ai(extra: RequestId, text: &str) -> String {
    json!({
        "@type": "fixTextWithAi",
        "@extra": extra.as_extra(),
        "text": { "@type": "formattedText", "text": text, "entities": [] },
    })
    .to_string()
}

/// Slice msg-richtext-ai-tools: `composeTextWithAi` (TDLib 1.8.67,
/// `schema/td_api.tl:12154`):
/// `composeTextWithAi text:formattedText translate_to_language_code:string
/// style_name:string add_emojis:Bool = FormattedText;`
pub fn compose_text_with_ai(
    extra: RequestId,
    text: &str,
    translate_to_language_code: &str,
    style_name: &str,
    add_emojis: bool,
) -> String {
    json!({
        "@type": "composeTextWithAi",
        "@extra": extra.as_extra(),
        "text": { "@type": "formattedText", "text": text, "entities": [] },
        "translate_to_language_code": translate_to_language_code,
        "style_name": style_name,
        "add_emojis": add_emojis,
    })
    .to_string()
}

/// Slice msg-richtext-ai-tools: `composeRichMessageWithAi` (TDLib 1.8.67,
/// `schema/td_api.tl:12162`):
/// `composeRichMessageWithAi message:inputRichMessage
/// translate_to_language_code:string style_name:string custom_prompt:string
/// add_emojis:Bool = RichMessage;`
/// `message` is the `inputRichMessage` object built by
/// `quill::rich::input_rich_message`.
pub fn compose_rich_message_with_ai(
    extra: RequestId,
    message: &Value,
    translate_to_language_code: &str,
    style_name: &str,
    custom_prompt: &str,
    add_emojis: bool,
) -> String {
    json!({
        "@type": "composeRichMessageWithAi",
        "@extra": extra.as_extra(),
        "message": message,
        "translate_to_language_code": translate_to_language_code,
        "style_name": style_name,
        "custom_prompt": custom_prompt,
        "add_emojis": add_emojis,
    })
    .to_string()
}

/// Slice msg-richtext-ai-tools: `createRichMessageWithAi` (TDLib 1.8.67,
/// `schema/td_api.tl:12168`):
/// `createRichMessageWithAi prompt:string language_code:string
/// add_emojis:Bool = RichMessage;`
pub fn create_rich_message_with_ai(
    extra: RequestId,
    prompt: &str,
    language_code: &str,
    add_emojis: bool,
) -> String {
    json!({
        "@type": "createRichMessageWithAi",
        "@extra": extra.as_extra(),
        "prompt": prompt,
        "language_code": language_code,
        "add_emojis": add_emojis,
    })
    .to_string()
}

/// Slice msg-richtext-ai-tools: `fixRichMessageWithAi` (TDLib 1.8.67,
/// `schema/td_api.tl:12176`):
/// `fixRichMessageWithAi message:inputRichMessage = RichMessage;`
/// `message` is the `inputRichMessage` object built by
/// `quill::rich::input_rich_message`.
pub fn fix_rich_message_with_ai(extra: RequestId, message: &Value) -> String {
    json!({
        "@type": "fixRichMessageWithAi",
        "@extra": extra.as_extra(),
        "message": message,
    })
    .to_string()
}

pub(crate) fn formatted_caption(caption: &str, strip_blockquote: bool) -> Value {
    // M1 fix-up: captions get the same markup→entities treatment as
    // message text (the toolbar is always visible above the composer,
    // including with attachments pending, so `**bold**` in a caption
    // must not go out literal).
    let (clean_text, entities) = parse_format_markup(caption);
    let entities = secret_chat_entities(entities, strip_blockquote);
    let entities_json: Vec<Value> = entities.iter().map(format_entity_json).collect();
    json!({
        "@type": "formattedText",
        "text": clean_text,
        "entities": entities_json
    })
}

/// M1 fix-up: `textEntityTypeBlockQuote` is not supported in secret chats
/// (schema 1.8.67) — strip it driver-side instead of letting TDLib drop
/// it (S1's layer-based gating philosophy).
fn secret_chat_entities(
    entities: Vec<ComposerEntity>,
    strip_blockquote: bool,
) -> Vec<ComposerEntity> {
    if strip_blockquote {
        entities
            .into_iter()
            .filter(|e| e.kind != FormatKind::BlockQuote)
            .collect()
    } else {
        entities
    }
}

/// Phase B3: self-destruct choice for `inputMessagePhoto` /
/// `inputMessageVideo` (TDLib 1.8.67, `schema/td_api.tl:6117` /
/// `:6128` — "private chats only"). TDLib validates the choice at runtime
/// (`MessageSelfDestructType::get_message_self_destruct_type`): the timer
/// must be 1–60 seconds (`MAX_PRIVATE_MESSAGE_TTL = 60`), and any non-empty
/// choice in a non-`DialogType::User` chat fails with 400 "Messages can
/// self-destruct only in private chats" — secret chats included. The driver
/// therefore strips the choice for every non-`chatTypePrivate` chat
/// (defense in depth; the composer picker is gated the same way).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelfDestructSend {
    /// `messageSelfDestructTypeTimer` (schema 1.8.67 line 5915).
    Timer(i32),
    /// `messageSelfDestructTypeImmediately` (schema 1.8.67 line 5918) —
    /// view once, destroyed after being closed.
    Immediately,
}

/// `MessageSelfDestructType` JSON for an `inputMessage*` `self_destruct_type`
/// field. `None` is JSON null (schema: "pass null if none").
pub(crate) fn self_destruct_type_value(choice: Option<SelfDestructSend>) -> Value {
    match choice {
        None => Value::Null,
        Some(SelfDestructSend::Timer(secs)) => json!({
            "@type": "messageSelfDestructTypeTimer",
            "self_destruct_time": secs
        }),
        Some(SelfDestructSend::Immediately) => json!({
            "@type": "messageSelfDestructTypeImmediately"
        }),
    }
}

/// `editMessageText` (TDLib 1.8.67). `reply_markup` null — bots only.
/// `input_message_content` must be `inputMessageText` (or `inputMessageRichMessage`).
pub fn edit_message_text(
    extra: RequestId,
    chat_id: ChatId,
    message_id: MessageId,
    text: &str,
    strip_blockquote: bool,
) -> String {
    // M1: edits carry the same markup→entities conversion as sends.
    // M1 fix-up: secret chats strip `textEntityTypeBlockQuote`
    // (unsupported there), same as sends.
    let (clean_text, entities) = parse_format_markup(text);
    let entities = secret_chat_entities(entities, strip_blockquote);
    let entities_json: Vec<Value> = entities.iter().map(format_entity_json).collect();
    json!({
        "@type": "editMessageText",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "message_id": message_id.0,
        "reply_markup": Value::Null,
        "input_message_content": {
            "@type": "inputMessageText",
            "text": {
                "@type": "formattedText",
                "text": clean_text,
                "entities": entities_json
            },
            "link_preview_options": Value::Null,
            "clear_draft": false
        }
    })
    .to_string()
}

/// `editMessageCaption` (TDLib 1.8.67). Caption-only media edit.
/// `show_caption_above_media` is false unless the original already inverted it.
/// M1 fix-up: the caption gets the same markup→entities treatment as
/// sends (blocker: `**bold**` went out literal).
pub fn edit_message_caption(
    extra: RequestId,
    chat_id: ChatId,
    message_id: MessageId,
    caption: &str,
    show_caption_above_media: bool,
    strip_blockquote: bool,
) -> String {
    json!({
        "@type": "editMessageCaption",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "message_id": message_id.0,
        "reply_markup": Value::Null,
        "caption": formatted_caption(caption, strip_blockquote),
        "show_caption_above_media": show_caption_above_media
    })
    .to_string()
}

/// `deleteMessages` (TDLib 1.8.67). `revoke` true = delete for all members
/// (tdesktop `DeleteMessagesBox` / Unigram `DeleteMessagesPopup` default for
/// own outgoing that `can_be_deleted_for_all_users`). Always true in
/// supergroups, channels, and secret chats per schema.
pub fn delete_messages(
    extra: RequestId,
    chat_id: ChatId,
    message_ids: &[MessageId],
    revoke: bool,
) -> String {
    json!({
        "@type": "deleteMessages",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "message_ids": message_ids.iter().map(|id| id.0).collect::<Vec<_>>(),
        "revoke": revoke
    })
    .to_string()
}

/// `forwardMessages` (TDLib 1.8.67, `schema/td_api.tl:12237` —
/// `forwardMessages chat_id topic_id from_chat_id message_ids options
/// send_copy remove_caption`). `send_copy` true drops the "Forwarded from"
/// attribution (TGX "Hide sender name"); `remove_caption` strips captions
/// on the copies (ignored unless `send_copy` is true). `topic_id` /
/// `options` null. Ids must already be strictly increasing (≤ 100).
pub fn forward_messages(
    extra: RequestId,
    chat_id: ChatId,
    from_chat_id: ChatId,
    message_ids: &[MessageId],
    send_copy: bool,
    remove_caption: bool,
) -> String {
    json!({
        "@type": "forwardMessages",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "topic_id": Value::Null,
        "from_chat_id": from_chat_id.0,
        "message_ids": message_ids.iter().map(|id| id.0).collect::<Vec<_>>(),
        "options": Value::Null,
        "send_copy": send_copy,
        "remove_caption": remove_caption && send_copy
    })
    .to_string()
}

/// M1: `unpinAllChatMessages` (TDLib 1.8.67, `schema/td_api.tl:13565` —
/// `unpinAllChatMessages chat_id:int53 = Ok;`).
pub fn unpin_all_chat_messages(extra: RequestId, chat_id: ChatId) -> String {
    json!({
        "@type": "unpinAllChatMessages",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
    })
    .to_string()
}

/// `readAllChatMentions` / `readAllChatReactions` (`schema/td_api.tl:13302,
/// 13305` — `… chat_id:int53 = Ok;`): the corner buttons' "Mark all as read".
pub fn read_all_chat_markers(extra: RequestId, chat_id: ChatId, reactions: bool) -> String {
    json!({
        "@type": if reactions { "readAllChatReactions" } else { "readAllChatMentions" },
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
    })
    .to_string()
}

/// M1: `getMessageLink` (TDLib 1.8.67, `schema/td_api.tl:12064` —
/// `getMessageLink chat_id message_id media_timestamp checklist_task_id
/// poll_option_id for_album in_message_thread = MessageLink`). Plain
/// message link: no timestamp / album / thread.
pub fn get_message_link(extra: RequestId, chat_id: ChatId, message_id: MessageId) -> String {
    json!({
        "@type": "getMessageLink",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "message_id": message_id.0,
        "media_timestamp": 0,
        "checklist_task_id": 0,
        "poll_option_id": "",
        "for_album": false,
        "in_message_thread": false,
    })
    .to_string()
}

/// M1 fix-up: `getMessageProperties` (TDLib 1.8.67,
/// `schema/td_api.tl:11557`). "Share link" sends this first so the
/// driver can gate `getMessageLink` on `messageProperties.can_get_link`
/// (schema line 12056) instead of letting it silently 400.
pub fn get_message_properties(extra: RequestId, chat_id: ChatId, message_id: MessageId) -> String {
    json!({
        "@type": "getMessageProperties",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "message_id": message_id.0,
    })
    .to_string()
}

/// M1: `resendMessages` (TDLib 1.8.67, `schema/td_api.tl:12251` —
/// `resendMessages chat_id message_ids quote paid_message_star_count =
/// Messages;`). Re-sends messages that failed to send (`message.can_retry`,
/// schema line 3038); `quote` null keeps the original reply context.
pub fn resend_messages(extra: RequestId, chat_id: ChatId, message_ids: &[MessageId]) -> String {
    json!({
        "@type": "resendMessages",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "message_ids": message_ids.iter().map(|id| id.0).collect::<Vec<_>>(),
        "quote": Value::Null,
        "paid_message_star_count": 0,
    })
    .to_string()
}

/// M1: `getChatScheduledMessages` (TDLib 1.8.67, `schema/td_api.tl:12000` —
/// `getChatScheduledMessages chat_id:int53 = Messages;`).
pub fn get_chat_scheduled_messages(extra: RequestId, chat_id: ChatId) -> String {
    json!({
        "@type": "getChatScheduledMessages",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
    })
    .to_string()
}

/// `reactionTypeEmoji` (TDLib 1.8.67). Paid reactions stay out —
///
/// `setStoryReaction` can't set them (schema comment, `td_api.tl:13809`).
/// `getMessageAvailableReactions chat_id message_id row_size` — the
/// reactions a message's picker may offer (`availableReactions`).
pub fn get_message_available_reactions(
    extra: RequestId,
    chat_id: ChatId,
    message_id: MessageId,
    row_size: i32,
) -> String {
    json!({
        "@type": "getMessageAvailableReactions",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "message_id": message_id.0,
        "row_size": row_size,
    })
    .to_string()
}

/// `addMessageReaction` / `removeMessageReaction` with an already-built
/// `ReactionType` value (emoji or custom emoji).
pub fn set_message_reaction(
    extra: RequestId,
    chat_id: ChatId,
    message_id: MessageId,
    reaction_type: Value,
    add: bool,
) -> String {
    if add {
        json!({
            "@type": "addMessageReaction",
            "@extra": extra.as_extra(),
            "chat_id": chat_id.0,
            "message_id": message_id.0,
            "reaction_type": reaction_type,
            "is_big": false,
            "update_recent_reactions": true,
        })
    } else {
        json!({
            "@type": "removeMessageReaction",
            "@extra": extra.as_extra(),
            "chat_id": chat_id.0,
            "message_id": message_id.0,
            "reaction_type": reaction_type,
        })
    }
    .to_string()
}

pub fn reaction_type_emoji(emoji: &str) -> Value {
    json!({
        "@type": "reactionTypeEmoji",
        "emoji": emoji
    })
}

/// Phase 9.2+: `reactionTypeCustomEmoji` (TDLib 1.8.67,
/// `schema/td_api.tl:2918`) — int64 ids serialize as JSON strings, like
/// every other int53/int64 field in these builders.
pub fn reaction_type_custom_emoji(custom_emoji_id: i64) -> Value {
    json!({
        "@type": "reactionTypeCustomEmoji",
        "custom_emoji_id": custom_emoji_id.to_string()
    })
}

/// `addMessageReaction` (TDLib 1.8.67). Chip / picker add: `is_big` false
/// (tdesktop InlineList click, not the big-animation double-click).
/// `update_recent_reactions` true matches the official picker.
pub fn add_message_reaction(
    extra: RequestId,
    chat_id: ChatId,
    message_id: MessageId,
    emoji: &str,
    is_big: bool,
    update_recent_reactions: bool,
) -> String {
    json!({
        "@type": "addMessageReaction",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "message_id": message_id.0,
        "reaction_type": reaction_type_emoji(emoji),
        "is_big": is_big,
        "update_recent_reactions": update_recent_reactions
    })
    .to_string()
}

/// `removeMessageReaction` (TDLib 1.8.67). A chosen reaction can always be
/// removed (schema). Official chip click on `is_chosen` sends this.
pub fn remove_message_reaction(
    extra: RequestId,
    chat_id: ChatId,
    message_id: MessageId,
    emoji: &str,
) -> String {
    json!({
        "@type": "removeMessageReaction",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "message_id": message_id.0,
        "reaction_type": reaction_type_emoji(emoji)
    })
    .to_string()
}

/// `getCallbackQueryAnswer` (TDLib 1.8.67, `schema/td_api.tl:13138`).
/// Pressing an `inlineKeyboardButtonTypeCallback` button: sends the callback
/// query to the bot; TDLib returns `callbackQueryAnswer`. (Not
/// `answerCallbackQuery` — that one is bots-only per its schema doc
/// comment.) `payload` is `callbackQueryPayloadData` (line 7737); schema
/// `bytes` is base64 in the JSON interface.
pub fn get_callback_query_answer(
    extra: RequestId,
    chat_id: ChatId,
    message_id: MessageId,
    data: &[u8],
) -> String {
    use base64::Engine;
    get_callback_query_answer_payload(
        extra,
        chat_id,
        message_id,
        json!({
            "@type": "callbackQueryPayloadData",
            "data": base64::engine::general_purpose::STANDARD.encode(data),
        }),
    )
}

/// B1: `getCallbackQueryAnswer` for an
/// `inlineKeyboardButtonTypeCallbackWithPassword` button press. `payload`
/// is `callbackQueryPayloadDataWithPassword` (schema 1.8.67, line 7740).
pub fn get_callback_query_answer_with_password(
    extra: RequestId,
    chat_id: ChatId,
    message_id: MessageId,
    password: &str,
    data: &[u8],
) -> String {
    use base64::Engine;
    get_callback_query_answer_payload(
        extra,
        chat_id,
        message_id,
        json!({
            "@type": "callbackQueryPayloadDataWithPassword",
            "password": password,
            "data": base64::engine::general_purpose::STANDARD.encode(data),
        }),
    )
}

/// B1: `getCallbackQueryAnswer` for an `inlineKeyboardButtonTypeCallbackGame`
/// button press. `payload` is `callbackQueryPayloadGame` (schema 1.8.67,
/// line 7743); `game_short_name` comes from the message's `messageGame`
/// content (schema:5234 / game class schema:673). A `sendGame` constructor
/// does not exist in this schema — the game launches through this callback
/// query (TGX `TGInlineKeyboard` does exactly this).
pub fn get_callback_query_answer_game(
    extra: RequestId,
    chat_id: ChatId,
    message_id: MessageId,
    game_short_name: &str,
) -> String {
    get_callback_query_answer_payload(
        extra,
        chat_id,
        message_id,
        json!({
            "@type": "callbackQueryPayloadGame",
            "game_short_name": game_short_name,
        }),
    )
}

fn get_callback_query_answer_payload(
    extra: RequestId,
    chat_id: ChatId,
    message_id: MessageId,
    payload: Value,
) -> String {
    json!({
        "@type": "getCallbackQueryAnswer",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "message_id": message_id.0,
        "payload": payload,
    })
    .to_string()
}

/// Slice bots-games: `getGameHighScores` (TDLib 1.8.67,
/// `schema/td_api.tl:13174`) — high scores for the game in `message_id`,
/// with the table range around `user_id`. Response is `gameHighScores`.
pub fn get_game_high_scores(
    extra: RequestId,
    chat_id: ChatId,
    message_id: MessageId,
    user_id: i64,
) -> String {
    json!({
        "@type": "getGameHighScores",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "message_id": message_id.0,
        "user_id": user_id,
    })
    .to_string()
}

/// Slice bots-games: `sendMessage` + `inputMessageGame` (TDLib 1.8.67,
/// `schema/td_api.tl:6156`) — send the bot's game to the chat. Not
/// supported for channels or secret chats (the driver pre-checks).
/// Rides `RequestPurpose::SendMessage` so the optimistic row flows
/// through the normal send path.
pub fn send_game(
    extra: RequestId,
    chat_id: ChatId,
    topic_id: Option<i32>,
    bot_user_id: i64,
    game_short_name: &str,
) -> String {
    json!({
        "@type": "sendMessage",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "topic_id": message_topic_value(topic_id),
        "reply_to": send_reply_value(None),
        "options": message_send_options(&SendOptions::default()),
        "reply_markup": Value::Null,
        "input_message_content": {
            "@type": "inputMessageGame",
            "bot_user_id": bot_user_id,
            "game_short_name": game_short_name,
        }
    })
    .to_string()
}

/// B1: `getLoginUrlInfo` (TDLib 1.8.67, `schema/td_api.tl:12985`) — resolve
/// an `inlineKeyboardButtonTypeLoginUrl` button (`id`, schema:3780) to the
/// authorized URL. Response is `loginUrlInfo*`.
pub fn get_login_url_info(
    extra: RequestId,
    chat_id: ChatId,
    message_id: MessageId,
    button_id: i64,
) -> String {
    json!({
        "@type": "getLoginUrlInfo",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "message_id": message_id.0,
        "button_id": button_id,
    })
    .to_string()
}

/// B1: `getLoginUrl` (TDLib 1.8.67, `schema/td_api.tl:12993`) — the
/// authorized URL after the user consented to a
/// `loginUrlInfoRequestConfirmation`. Response is `httpUrl`.
pub fn get_login_url(
    extra: RequestId,
    chat_id: ChatId,
    message_id: MessageId,
    button_id: i64,
    allow_write_access: bool,
) -> String {
    json!({
        "@type": "getLoginUrl",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "message_id": message_id.0,
        "button_id": button_id,
        "allow_write_access": allow_write_access,
    })
    .to_string()
}

/// B1: `deleteChatReplyMarkup` (TDLib 1.8.67, `schema/td_api.tl:13183`).
/// Must be called after a one-time custom keyboard has been used.
pub fn delete_chat_reply_markup(
    extra: RequestId,
    chat_id: ChatId,
    message_id: MessageId,
) -> String {
    json!({
        "@type": "deleteChatReplyMarkup",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "message_id": message_id.0,
    })
    .to_string()
}

/// `openMessageContent` — user started listening to a voice note (1.8.67).
pub fn open_message_content(extra: RequestId, chat_id: ChatId, message_id: MessageId) -> String {
    json!({
        "@type": "openMessageContent",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "message_id": message_id.0
    })
    .to_string()
}

/// MED2: `recognizeSpeech` (TDLib 1.8.67, `schema/td_api.tl:12181`).
/// Recognizes speech in a voice note or video note message. Returns `Ok`;
/// the result arrives later as `updateMessageContent` carrying the new
/// `speech_recognition_result` (`speechRecognitionResultPending` →
/// `speechRecognitionResultText` / `speechRecognitionResultError`).
pub fn recognize_speech(extra: RequestId, chat_id: ChatId, message_id: MessageId) -> String {
    json!({
        "@type": "recognizeSpeech",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "message_id": message_id.0
    })
    .to_string()
}
