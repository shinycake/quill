use super::*;
use crate::ids::{ChatId, MessageId, UserId};
use serde_json::Value;

/// `messageSenderUser` / `messageSenderChat` (TDLib 1.8.67).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageSender {
    User { user_id: i64 },
    Chat { chat_id: i64 },
}

/// `message.forward_info.origin` (TDLib 1.8.67 `MessageOrigin`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MessageOrigin {
    User {
        user_id: UserId,
    },
    HiddenUser {
        sender_name: String,
    },
    Chat {
        chat_id: ChatId,
        author_signature: String,
    },
    Channel {
        chat_id: ChatId,
        message_id: MessageId,
        author_signature: String,
    },
}

/// Typed `messageForwardInfo`. `source` (Saved Messages / Replies) stays out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageForwardInfo {
    pub origin: MessageOrigin,
    pub date: i32,
}

/// Same-chat / known-chat reply metadata from `message.reply_to`.
/// Stories and unknown `MessageReplyTo` variants are dropped (out of scope).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageReplyTo {
    pub chat_id: ChatId,
    pub message_id: MessageId,
    pub quote_text: Option<String>,
    pub content_preview: Option<String>,
}

impl MessageReplyTo {
    pub fn is_same_chat(&self, open_chat: ChatId) -> bool {
        self.chat_id.0 == 0 || self.chat_id == open_chat
    }
}

/// M1: `messageSchedulingState` (TDLib 1.8.67, `schema/td_api.tl:5902` /
/// `:5905`); `message.scheduling_state` is null when not scheduled
/// (schema line 3124).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageSchedulingState {
    SendAtDate { send_date: i32 },
    SendWhenOnline,
}

pub(crate) fn parse_message_scheduling_state(
    value: Option<&Value>,
) -> Option<MessageSchedulingState> {
    let value = value?;
    if value.is_null() {
        return None;
    }
    match value.get("@type").and_then(Value::as_str) {
        Some("messageSchedulingStateSendAtDate") => Some(MessageSchedulingState::SendAtDate {
            send_date: value.get("send_date").and_then(Value::as_i64).unwrap_or(0) as i32,
        }),
        Some("messageSchedulingStateSendWhenOnline") => {
            Some(MessageSchedulingState::SendWhenOnline)
        }
        _ => None,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedMessage {
    pub sender: Option<MessageSender>,
    pub id: MessageId,
    pub chat_id: ChatId,
    /// Schema `message.date` (TDLib 1.8.67, line 3165): unix seconds,
    /// server time. Feeds the in-bubble timestamp; 0 when absent.
    pub date: i32,
    pub is_outgoing: bool,
    /// Schema `message.is_pinned` (TDLib 1.8.67).
    pub is_pinned: bool,
    /// Schema `message.topic_id` (TDLib 1.8.67, line 3165): the
    /// `forum_topic_id` when the topic is `messageTopicForum`, `None` for
    /// every other `MessageTopic` variant (threads, direct messages, saved
    /// messages) and when the field is absent. Parity slice 4 routes topic
    /// messages into the topic's history.
    pub topic_id: Option<i32>,
    /// Schema `message.media_album_id` (int64). `0` means the message is not in an album.
    pub media_album_id: i64,
    /// Phase D2: schema `message.author_signature` (TDLib 1.8.67, lines
    /// 3155/3165) — "For channel posts and anonymous group messages,
    /// optional author signature". `None` when absent or empty; renders as
    /// the small signature line under the post (suppressed under
    /// forwarded-message headers, which already attribute it).
    pub author_signature: Option<String>,
    /// M1: `message.scheduling_state` (TDLib 1.8.67, lines 3124 / 3165).
    /// `Some` only on scheduled sends; the UI's scheduled list reads the
    /// planned time from here.
    pub scheduling_state: Option<MessageSchedulingState>,
    /// M1 fix-up: `message.sending_state.can_retry` (TDLib 1.8.67, lines
    /// 3038 / 5896) — only `messageSendingStateFailed` carries it.
    /// `true` means the failed send may be retried via `resendMessages`;
    /// the reducer gates the retry affordance on this instead of offering
    /// it on every `updateMessageSendFailed`.
    pub can_retry: bool,
    pub content: MessageContent,
    /// M2: `message.ephemeral_content` (TDLib 1.8.67, `schema/td_api.tl`
    /// lines 3161/3165) — visible only to the current user; renders
    /// **instead of** the regular content. `None` when absent or null.
    pub ephemeral: Option<EphemeralMessageContent>,
    pub files: Vec<ParsedFile>,
    pub reply_to: Option<MessageReplyTo>,
    pub forward_info: Option<MessageForwardInfo>,
    pub interaction_info: Option<MessageInteractionInfo>,
    /// Schema `message.reply_markup` (TDLib 1.8.67). All `replyMarkup*`
    /// constructors (B1); inline keyboards render as the button grid,
    /// `ShowKeyboard` as the custom keyboard above the composer.
    pub reply_markup: Option<ReplyMarkup>,
    /// Phase B3: `message.self_destruct_type` / `message.self_destruct_in`
    /// (TDLib 1.8.67, `schema/td_api.tl:3146`–`:3147` / `:3165`).
    /// `messageSelfDestructTypeTimer` (line 5915) /
    /// `messageSelfDestructTypeImmediately` (line 5918); unknown future
    /// variants degrade to `None` (message renders without a timer badge).
    pub self_destruct: Option<MessageSelfDestruct>,
    /// Phase B4: `message.auto_delete_in` (TDLib 1.8.67,
    /// `schema/td_api.tl:3148` / `:3165`) — seconds left before the
    /// chat's `message_auto_delete_time` setting deletes this message;
    /// `None` when never. Renders as a countdown chip on the row; the
    /// row itself leaves via `updateDeleteMessages`.
    pub auto_delete: Option<MessageAutoDelete>,
}

/// Phase B3: the configured self-destruct mode of a message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelfDestructKind {
    /// `messageSelfDestructTypeTimer` — destroyed `secs` seconds after the
    /// content was opened (schema 1.8.67 line 5915).
    Timer { secs: i32 },
    /// `messageSelfDestructTypeImmediately` — destroyed once closed after a
    /// single viewing (schema 1.8.67 line 5918).
    Immediately,
}

/// Phase B3: parsed `message.self_destruct_type` + `message.self_destruct_in`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MessageSelfDestruct {
    pub kind: SelfDestructKind,
    /// Latest `message.self_destruct_in`, converted to whole milliseconds.
    /// `0` means the destruction isn't scheduled yet (the content hasn't
    /// been opened; schema 1.8.67 line 3147). TDLib emits
    /// `updateDeleteMessages` when the timer fires, so the row disappears
    /// through the normal delete path — no special handling needed.
    /// Whole milliseconds (not `f64` seconds) so the struct keeps the
    /// `Eq` derive that `ParsedMessage` / `HistoryMessage` require.
    pub expires_in_ms: i64,
    /// Local clock (ms) when `expires_in_ms` was received, for the local
    /// countdown decay (`slow_mode_delay_expires_in` pattern, Phase A1).
    pub fetched_at_ms: u64,
}

impl MessageSelfDestruct {
    /// Locally decayed whole seconds left, or `None` when destruction isn't
    /// scheduled yet. The value keeps decaying to 0 (the row stays until
    /// TDLib's `updateDeleteMessages` removes it).
    pub fn remaining_secs(&self, now_ms: u64) -> Option<u64> {
        if self.expires_in_ms <= 0 {
            return None;
        }
        let elapsed_ms = now_ms.saturating_sub(self.fetched_at_ms) as i64;
        let remaining_ms = self.expires_in_ms - elapsed_ms;
        Some(if remaining_ms > 0 {
            ((remaining_ms + 999) / 1000) as u64
        } else {
            0
        })
    }

    /// Timer badge for media rows: "⏱ view once" / "⏱ 60s" (not scheduled
    /// yet) / "⏱ 42s left" (scheduled).
    pub fn badge_label(&self, now_ms: u64) -> String {
        match self.kind {
            SelfDestructKind::Immediately => "⏱ view once".to_string(),
            SelfDestructKind::Timer { secs } => match self.remaining_secs(now_ms) {
                Some(left) => format!("⏱ {left}s left"),
                None => format!("⏱ {secs}s"),
            },
        }
    }
}

pub(crate) fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Parse `message.self_destruct_type` + `message.self_destruct_in` (schema
/// 1.8.67 lines 3146–3147 / 3165 / 5915 / 5918). `self_destruct_in` arrives
/// as a double (seconds, possibly fractional); non-finite or negative
/// values are treated as unscheduled.
pub(crate) fn parse_self_destruct(
    type_value: Option<&Value>,
    in_value: Option<&Value>,
) -> Option<MessageSelfDestruct> {
    let type_value = type_value?;
    if type_value.is_null() {
        return None;
    }
    let kind = match type_value.get("@type").and_then(Value::as_str) {
        Some("messageSelfDestructTypeTimer") => SelfDestructKind::Timer {
            secs: type_value
                .get("self_destruct_time")
                .and_then(Value::as_i64)
                .map(|s| s.clamp(0, i32::MAX as i64) as i32)
                .unwrap_or(0),
        },
        Some("messageSelfDestructTypeImmediately") => SelfDestructKind::Immediately,
        _ => return None,
    };
    let expires_in_ms = in_value
        .and_then(Value::as_f64)
        .filter(|v| v.is_finite() && *v >= 0.0)
        .map(|v| (v * 1000.0).round() as i64)
        .unwrap_or(0);
    Some(MessageSelfDestruct {
        kind,
        expires_in_ms,
        fetched_at_ms: now_ms(),
    })
}

/// Phase B4: parsed `message.auto_delete_in` (schema 1.8.67, lines
/// 3148 / 3165) — "Time left before the message will be automatically
/// deleted by message_auto_delete_time setting of the chat, in seconds;
/// 0 if never". Arrives as a double (seconds, possibly fractional);
/// non-finite or negative values degrade to `None` (the row renders
/// without a countdown). TDLib removes the row via `updateDeleteMessages`
/// when it fires — no special deletion code. Whole milliseconds (not
/// `f64`) so the struct keeps the `Eq` derive that `ParsedMessage` /
/// `HistoryMessage` require.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MessageAutoDelete {
    pub expires_in_ms: i64,
    /// Local clock (ms) when `expires_in_ms` was received, for the local
    /// countdown decay (same pattern as `MessageSelfDestruct`, Phase B3).
    pub fetched_at_ms: u64,
}

impl MessageAutoDelete {
    /// Locally decayed whole seconds left. The value keeps decaying to 0
    /// (the row stays until TDLib's `updateDeleteMessages` removes it).
    pub fn remaining_secs(&self, now_ms: u64) -> u64 {
        let elapsed_ms = now_ms.saturating_sub(self.fetched_at_ms) as i64;
        let remaining_ms = self.expires_in_ms - elapsed_ms;
        if remaining_ms > 0 {
            ((remaining_ms + 999) / 1000) as u64
        } else {
            0
        }
    }

    /// Countdown chip label for message rows, e.g. "🗑 59m left".
    pub fn chip_label(&self, now_ms: u64) -> String {
        format!(
            "🗑 {} left",
            format_countdown_secs(self.remaining_secs(now_ms))
        )
    }
}

/// Compact duration for countdown labels: 45 → "45s", 90 → "2m",
/// 3600 → "1h", 90000 → "1d". Rounds up like the self-destruct badge.
pub fn format_countdown_secs(secs: u64) -> String {
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{}m", secs.div_ceil(60))
    } else if secs < 86400 {
        format!("{}h", secs.div_ceil(3600))
    } else {
        format!("{}d", secs.div_ceil(86400))
    }
}

/// Phase B4: compact label for a chat-level TTL setting in seconds —
/// "5s" / "30s" / "1m" / "1h" / "1d" / "7d" for the values the picker
/// offers, exact units for arbitrary values other clients may set
/// (90 → "2m" would be wrong; 90s → "90s"). Picks the largest unit that
/// divides the value evenly.
pub fn format_ttl_setting(secs: i32) -> String {
    if secs <= 0 {
        return "Off".to_string();
    }
    if secs % 86400 == 0 {
        format!("{}d", secs / 86400)
    } else if secs % 3600 == 0 {
        format!("{}h", secs / 3600)
    } else if secs % 60 == 0 {
        format!("{}m", secs / 60)
    } else {
        format!("{secs}s")
    }
}

/// Phase B4: service-row label for `messageChatSetMessageAutoDeleteTime`
/// (schema 1.8.67, line 5387). `secret` selects the official wording:
/// "Self-destruct timer" in secret chats, "Auto-delete timer" elsewhere.
pub fn chat_ttl_service_label(secs: i32, secret: bool) -> String {
    let noun = if secret {
        "Self-destruct timer"
    } else {
        "Auto-delete timer"
    };
    if secs > 0 {
        format!("{noun} set to {}", format_ttl_setting(secs))
    } else {
        format!("{noun} turned off")
    }
}

/// Parse `message.auto_delete_in` (schema 1.8.67, lines 3148 / 3165).
/// `None` when the field is absent, null, or 0 (never auto-deleted);
/// garbage degrades to `None`.
pub(crate) fn parse_auto_delete_in(value: Option<&Value>) -> Option<MessageAutoDelete> {
    let secs = value?.as_f64()?;
    if !secs.is_finite() || secs <= 0.0 {
        return None;
    }
    Some(MessageAutoDelete {
        expires_in_ms: (secs * 1000.0).round() as i64,
        fetched_at_ms: now_ms(),
    })
}

/// Slice G2: one parsed `welcomeMessage` (TDLib 1.8.67, line 6839:
/// `welcomeMessage id:int32 content:MessageContent = WelcomeMessage`).
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedWelcomeMessage {
    pub id: i32,
    pub content: MessageContent,
}

pub(crate) fn parse_welcome_message(value: &Value) -> Option<ParsedWelcomeMessage> {
    let id = value.get("id")?.as_i64()? as i32;
    let (content, _) = parse_content(value.get("content"));
    Some(ParsedWelcomeMessage { id, content })
}

pub(crate) fn parse_message_sender(value: Option<&Value>) -> Result<MessageSender, ParseError> {
    let value = value.ok_or(ParseError::MissingField)?;
    match value.get("@type").and_then(Value::as_str) {
        Some("messageSenderUser") => Ok(MessageSender::User {
            user_id: int53(value.get("user_id"))?,
        }),
        Some("messageSenderChat") => Ok(MessageSender::Chat {
            chat_id: int53(value.get("chat_id"))?,
        }),
        _ => Err(ParseError::MissingField),
    }
}

pub(crate) fn parse_message(value: &Value) -> Result<ParsedMessage, ParseError> {
    let (content, files) = parse_content(value.get("content"));
    Ok(ParsedMessage {
        sender: parse_message_sender(value.get("sender_id")).ok(),
        id: MessageId(int53(value.get("id"))?),
        chat_id: ChatId(int53(value.get("chat_id"))?),
        date: value.get("date").and_then(Value::as_i64).unwrap_or(0) as i32,
        is_outgoing: value
            .get("is_outgoing")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        is_pinned: value
            .get("is_pinned")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        media_album_id: int64(value.get("media_album_id")).unwrap_or(0),
        // Phase D2: `message.author_signature` (TDLib 1.8.67, lines
        // 3155/3165). Empty or absent → `None`.
        author_signature: value
            .get("author_signature")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(str::to_string),
        topic_id: parse_message_topic(value.get("topic_id")),
        content,
        ephemeral: parse_ephemeral_message_content(value.get("ephemeral_content")),
        files,
        reply_to: parse_reply_to(value.get("reply_to")),
        forward_info: parse_forward_info(value.get("forward_info")),
        interaction_info: parse_interaction_info(value.get("interaction_info")),
        reply_markup: parse_reply_markup(value.get("reply_markup")),
        self_destruct: parse_self_destruct(
            value.get("self_destruct_type"),
            value.get("self_destruct_in"),
        ),
        auto_delete: parse_auto_delete_in(value.get("auto_delete_in")),
        scheduling_state: parse_message_scheduling_state(value.get("scheduling_state")),
        // M1 fix-up: `can_retry` lives on `messageSendingStateFailed`
        // only (schema 1.8.67 line 5896); absent everywhere else.
        can_retry: value
            .get("sending_state")
            .filter(|s| s.get("@type").and_then(Value::as_str) == Some("messageSendingStateFailed"))
            .and_then(|s| s.get("can_retry"))
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
}

/// Parity slice 4: `message.topic_id` (TDLib 1.8.67, lines 3001–3010).
/// Returns the `forum_topic_id` for `messageTopicForum`; every other
/// variant (thread, direct messages, saved messages) and a missing/null
/// field map to `None` — Quill only models forum topics.
pub(crate) fn parse_message_topic(value: Option<&Value>) -> Option<i32> {
    let value = value?;
    if value.is_null() {
        return None;
    }
    match value.get("@type").and_then(Value::as_str) {
        Some("messageTopicForum") => value
            .get("forum_topic_id")
            .and_then(Value::as_i64)
            .map(|id| id as i32),
        _ => None,
    }
}

pub(crate) fn parse_forward_info(value: Option<&Value>) -> Option<MessageForwardInfo> {
    let value = value?;
    if value.is_null() {
        return None;
    }
    match value.get("@type").and_then(Value::as_str) {
        Some("messageForwardInfo") => {
            let origin = parse_message_origin(value.get("origin"))?;
            Some(MessageForwardInfo {
                origin,
                date: value.get("date").and_then(Value::as_i64).unwrap_or(0) as i32,
            })
        }
        _ => None,
    }
}

pub(crate) fn parse_message_origin(value: Option<&Value>) -> Option<MessageOrigin> {
    let value = value?;
    match value.get("@type").and_then(Value::as_str) {
        Some("messageOriginUser") => Some(MessageOrigin::User {
            user_id: UserId(int53_or_zero(value.get("sender_user_id"))),
        }),
        Some("messageOriginHiddenUser") => Some(MessageOrigin::HiddenUser {
            sender_name: value
                .get("sender_name")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
        }),
        Some("messageOriginChat") => Some(MessageOrigin::Chat {
            chat_id: ChatId(int53_or_zero(value.get("sender_chat_id"))),
            author_signature: value
                .get("author_signature")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
        }),
        Some("messageOriginChannel") => Some(MessageOrigin::Channel {
            chat_id: ChatId(int53_or_zero(value.get("chat_id"))),
            message_id: MessageId(int53_or_zero(value.get("message_id"))),
            author_signature: value
                .get("author_signature")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
        }),
        _ => None,
    }
}

pub(crate) fn parse_reply_to(value: Option<&Value>) -> Option<MessageReplyTo> {
    let value = value?;
    if value.is_null() {
        return None;
    }
    match value.get("@type").and_then(Value::as_str) {
        Some("messageReplyToMessage") => {
            let message_id = int53_or_zero(value.get("message_id"));
            if message_id == 0 {
                return None;
            }
            let quote_text = value.get("quote").and_then(|quote| {
                if quote.is_null() {
                    return None;
                }
                let text = parse_formatted_text(quote.get("text"));
                if text.is_empty() { None } else { Some(text) }
            });
            let content_preview = value.get("content").and_then(|content| {
                if content.is_null() {
                    return None;
                }
                let preview = parse_content(Some(content)).0.preview();
                if preview.is_empty() {
                    None
                } else {
                    Some(preview)
                }
            });
            Some(MessageReplyTo {
                chat_id: ChatId(int53_or_zero(value.get("chat_id"))),
                message_id: MessageId(message_id),
                quote_text,
                content_preview,
            })
        }
        _ => None,
    }
}
