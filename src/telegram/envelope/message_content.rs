use super::*;
use crate::rich::{RichBlock, parse_rich_message};
use crate::text::{TextEntity, TextEntityKind, utf16_to_utf8_offset};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MessageContent {
    Text(TextContent),
    /// A chat event (member added, pinned message, gift, forum topic…):
    /// structured facts only; the wording is `crate::service_text`.
    Action(Box<ServiceAction>),
    Photo(PhotoContent),
    Document(DocumentContent),
    Sticker(StickerContent),
    Animation(AnimationContent),
    Video(VideoContent),
    VideoNote(VideoNoteContent),
    VoiceNote(VoiceNoteContent),
    Audio(AudioContent),
    /// Phase 4.2: `messagePoll` (TDLib 1.8.67, `schema/td_api.tl:5241`).
    Poll(PollContent),
    /// Phase 4.3: `messageLocation` (TDLib 1.8.67, `schema/td_api.tl:5214`)
    /// and `messageLiveLocation` (`schema/td_api.tl:5211`). The latter
    /// carries `LiveLocation` state; the former sets `live: None`.
    Location(LocationContent),
    /// Phase 4.3: `messageVenue` (TDLib 1.8.67, `schema/td_api.tl:5217`).
    Venue(VenueContent),
    /// Phase 4.3: `messageContact` (TDLib 1.8.67, `schema/td_api.tl:5220`).
    Contact(ContactContent),
    /// Phase 4.4: `messageDice` (TDLib 1.8.67, `schema/td_api.tl:5231`).
    Dice(DiceContent),
    /// Phase B4: `messageChatSetMessageAutoDeleteTime` (TDLib 1.8.67,
    /// `schema/td_api.tl:5387`) — the chat's auto-delete / self-destruct
    /// (secret chats) timer was changed; new value in seconds, 0 when
    /// disabled. Rendered as a centered service row (Quill has no generic
    /// service-message pipeline; this is the first one).
    ChatTtlChanged {
        secs: i32,
    },
    /// Phase C2f: `messageGroupCall` (TDLib 1.8.67,
    /// `schema/td_api.tl:5288`) — a group call not bound to a chat.
    /// Incoming, not active, not missed: an invitation the user can
    /// accept (`joinGroupCall`) or decline
    /// (`declineGroupCallInvitation`). `other_participant_ids` is not
    /// kept (the row only needs the invitation state).
    GroupCallInvitation {
        unique_id: i64,
        is_active: bool,
        was_missed: bool,
        is_video: bool,
    },
    /// Phase C2i: `messageCall` (TDLib 1.8.67,
    /// `schema/td_api.tl:5277`) — a 1:1 call entry in chat history.
    /// `messageCall unique_id:int64 is_video:Bool
    /// discard_reason:CallDiscardReason duration:int32 = MessageContent;`
    /// `unique_id` is not kept (the row needs only kind/reason/duration).
    Call {
        is_video: bool,
        discard_reason: CallDiscardReason,
        duration: i32,
    },
    /// Phase S1: `messageScreenshotTaken` (TDLib 1.8.67,
    /// `schema/td_api.tl:5375`) — a screenshot of a message in the chat
    /// has been taken. No fields; attribution comes from
    /// `message.is_outgoing` at render time.
    ScreenshotTaken,
    /// Slice C2k: `messageChatAddedToCommunity` (TDLib 1.8.67,
    /// `schema/td_api.tl:5360`) — the chat was added to a community.
    /// Only `community_id` is kept; the renderer resolves the community
    /// name post-Phase-9 (the envelope layer has no name lookup).
    ChatAddedToCommunity {
        community_id: i64,
    },
    /// Slice C2k: `messageChatRemovedFromCommunity` (TDLib 1.8.67,
    /// `schema/td_api.tl:5363`) — the chat was removed from a community.
    ChatRemovedFromCommunity,
    /// Slice G9: `messageChatJoinFromCommunity` (TDLib 1.8.67,
    /// `schema/td_api.tl:5354`) — a new member joined the chat from a
    /// community. Only `community_id` is kept; the renderer resolves the
    /// community name from the `updateCommunity` cache (the envelope
    /// layer has no name lookup).
    ChatJoinFromCommunity {
        community_id: i64,
    },
    /// M2: `messageRichMessage` (TDLib 1.8.67, `schema/td_api.tl:5143`) —
    /// an anniversary rich message; `blocks` are the parsed `pageBlock*`
    /// list (possibly partial when `is_full` is false — the renderer
    /// fetches the rest via `getFullRichMessage`, schema line 11554).
    RichMessage(RichMessageContent),
    /// Slice bots-games: `messageGame` (TDLib 1.8.67,
    /// `schema/td_api.tl:5234`; the `game` object at lines 665-673).
    /// The card renders the title, text/description, a thumbnail, and
    /// Play / Scores actions. `short_name` feeds
    /// `callbackQueryPayloadGame.game_short_name` (schema:7743) on Play.
    Game(GameContent),
    /// Slice P1: `messageInvoice` (TDLib 1.8.67, `schema/td_api.tl:5270`) —
    /// a bot's invoice card. Only the fields the card renders are kept.
    Invoice(InvoiceContent),
    /// Slice P1: `messagePaymentSuccessful` (TDLib 1.8.67,
    /// `schema/td_api.tl:5436`) — the receipt row after a successful
    /// payment.
    PaymentSuccessful(PaymentSuccessContent),
    /// Slice P1: `messagePaymentSuccessfulBot` (TDLib 1.8.67,
    /// `schema/td_api.tl:5449`) — the seller-side notice. Quill is a
    /// buyer client; kept minimal for a compact "payment received" row.
    PaymentReceived(PaymentReceivedContent),
    Unsupported {
        type_name: String,
    },
}

/// Slice bots-games: parsed `messageGame` (TDLib 1.8.67,
/// `schema/td_api.tl:5234` / `game` at lines 665-673).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameContent {
    /// `game.short_name` (schema:673) — the
    /// `callbackQueryPayloadGame.game_short_name` (schema:7743).
    pub short_name: String,
    pub title: String,
    /// `game.text` (`formattedText`, schema:670).
    pub text: TextContent,
    pub description: String,
    /// Thumbnail source: `game.photo` sizes, falling back to the
    /// animation's thumbnail when the photo carries no sizes. Empty
    /// sizes → the card renders a glyph placeholder.
    pub photo: PhotoContent,
}

/// M2: parsed `richMessage` (TDLib 1.8.67, `schema/td_api.tl:123`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RichMessageContent {
    pub blocks: Vec<RichBlock>,
    pub is_full: bool,
}

impl RichMessageContent {
    /// Plain-text form of the blocks for "Copy" — every text-ish block
    /// joined with newlines. Buttons/dividers contribute nothing.
    pub fn copy_text(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        for block in &self.blocks {
            match block {
                RichBlock::Paragraph { text, .. } | RichBlock::Heading { text, .. } => {
                    parts.push(text.clone())
                }
                RichBlock::List { items, .. } => {
                    parts.extend(items.iter().map(|item| item.text.clone()))
                }
                RichBlock::Collapsible { header, body, .. } => {
                    parts.push(header.clone());
                    parts.push(body.clone());
                }
                RichBlock::Document {
                    file_name, caption, ..
                } => {
                    parts.push(file_name.clone());
                    parts.push(caption.clone());
                }
                RichBlock::Photo { caption, .. } | RichBlock::Video { caption, .. } => {
                    parts.push(caption.clone());
                }
                RichBlock::Table { rows } => parts.extend(rows.iter().map(|row| row.join(" "))),
                RichBlock::ButtonRow { .. }
                | RichBlock::Divider
                | RichBlock::Empty
                | RichBlock::Unsupported { .. } => {}
            }
        }
        parts
            .into_iter()
            .filter(|text| !text.is_empty())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// M2: parsed `ephemeralMessageContent` (TDLib 1.8.67,
/// `schema/td_api.tl:3115`) — content visible only to the current user,
/// shown **instead of** the regular content (bot-built flows per the
/// 2026-08-25 anniversary post).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EphemeralMessageContent {
    pub content: Box<MessageContent>,
    pub reply_markup: Option<ReplyMarkup>,
}

/// `formattedText` plus optional `messageText.link_preview` (TDLib 1.8.67).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextContent {
    pub text: String,
    pub entities: Vec<TextEntity>,
    pub link_preview: Option<LinkPreview>,
}

/// Phase C2i: service-row label for a `messageCall`, following
/// Telegram X's `TD.getCallName` convention (short form): missed /
/// declined / canceled are distinguishable; answered calls show the
/// direction plus duration. `duration` is seconds (0 when unanswered).
pub fn call_entry_label(
    is_video: bool,
    reason: &CallDiscardReason,
    duration: i32,
    is_outgoing: bool,
) -> String {
    let kind = if is_video { "video call" } else { "call" };
    let base = match reason {
        CallDiscardReason::Missed => {
            if is_outgoing {
                "Cancelled"
            } else {
                "Missed"
            }
        }
        CallDiscardReason::Declined => {
            if is_outgoing {
                "Busy"
            } else {
                "Declined"
            }
        }
        _ => {
            if is_outgoing {
                "Outgoing"
            } else {
                "Incoming"
            }
        }
    };
    if duration > 0 {
        format!("{base} {kind} · {}", format_duration(duration))
    } else {
        format!("{base} {kind}")
    }
}

/// mm:ss (or h:mm:ss) for call durations.
pub(crate) fn format_duration(total_secs: i32) -> String {
    let total_secs = total_secs.max(0) as i64;
    let (h, m, s) = (total_secs / 3600, (total_secs / 60) % 60, total_secs % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

impl TextContent {
    pub fn plain(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            entities: Vec::new(),
            link_preview: None,
        }
    }
}

impl From<&str> for TextContent {
    fn from(text: &str) -> Self {
        Self::plain(text)
    }
}

impl From<String> for TextContent {
    fn from(text: String) -> Self {
        Self::plain(text)
    }
}

impl MessageContent {
    pub fn preview(&self) -> String {
        match self {
            MessageContent::Text(text) => text.text.chars().take(80).collect(),
            // M2: first text-ish block of the rich message.
            MessageContent::RichMessage(rich) => rich
                .blocks
                .iter()
                .filter_map(RichBlock::preview_text)
                .next()
                .map(|text| text.chars().take(80).collect())
                .unwrap_or_else(|| "Rich message".to_string()),
            MessageContent::Photo(photo) if photo.caption.is_empty() => "Photo".into(),
            MessageContent::Photo(photo) => photo.caption.chars().take(80).collect(),
            MessageContent::Document(doc) if !doc.caption.is_empty() => {
                doc.caption.chars().take(80).collect()
            }
            MessageContent::Document(doc) if !doc.file_name.is_empty() => {
                doc.file_name.chars().take(80).collect()
            }
            MessageContent::Document(_) => "Document".into(),
            MessageContent::Sticker(sticker) if !sticker.emoji.is_empty() => sticker.emoji.clone(),
            MessageContent::Sticker(_) => "Sticker".into(),
            MessageContent::Animation(animation) if !animation.caption.is_empty() => {
                animation.caption.chars().take(80).collect()
            }
            MessageContent::Animation(_) => "GIF".into(),
            MessageContent::Video(video) if !video.caption.is_empty() => {
                video.caption.chars().take(80).collect()
            }
            MessageContent::Video(_) => "Video".into(),
            MessageContent::VideoNote(_) => "Video note".into(),
            MessageContent::VoiceNote(note) if !note.caption.is_empty() => {
                note.caption.chars().take(80).collect()
            }
            MessageContent::VoiceNote(_) => "Voice message".into(),
            MessageContent::Audio(audio) if !audio.caption.is_empty() => {
                audio.caption.chars().take(80).collect()
            }
            MessageContent::Audio(audio) if !audio.title.is_empty() => {
                audio.title.chars().take(80).collect()
            }
            MessageContent::Audio(audio) if !audio.file_name.is_empty() => {
                audio.file_name.chars().take(80).collect()
            }
            MessageContent::Audio(_) => "Audio".into(),
            MessageContent::Poll(poll) => {
                let question = poll.poll.question.trim();
                if question.is_empty() {
                    "Poll".into()
                } else {
                    question.chars().take(80).collect()
                }
            }
            MessageContent::Location(location) => {
                if location.live.is_some() {
                    "📍 Live location".into()
                } else {
                    "📍 Location".into()
                }
            }
            MessageContent::Venue(venue) => {
                let title = venue.title.trim();
                if title.is_empty() {
                    "📍 Venue".into()
                } else {
                    format!("📍 {}", title.chars().take(76).collect::<String>())
                }
            }
            MessageContent::Contact(contact) => {
                let name = contact.display_name();
                if name.is_empty() {
                    "👤 Contact".into()
                } else {
                    format!("👤 {}", name.chars().take(76).collect::<String>())
                }
            }
            MessageContent::Dice(dice) => dice.label(),
            // Phase B4: neutral noun — `preview()` has no chat-kind
            // context; the row renderer (which knows the chat) uses
            // "Self-destruct"/"Auto-delete" via `chat_ttl_service_label`.
            MessageContent::ChatTtlChanged { secs } => {
                if *secs > 0 {
                    format!("Timer set to {}", format_ttl_setting(*secs))
                } else {
                    "Timer turned off".to_string()
                }
            }
            // Phase C2f: chat-list last-message preview for a
            // `messageGroupCall` invitation.
            MessageContent::GroupCallInvitation { is_video, .. } => {
                if *is_video {
                    "📹 Video chat invitation".to_string()
                } else {
                    "📞 Voice chat invitation".to_string()
                }
            }
            // Phase C2i: chat-list last-message preview for a `messageCall`.
            // Direction is unknown without the message wrapper, so the
            // neutral "Call" is used; the full label renders in history.
            MessageContent::Call {
                is_video,
                discard_reason,
                duration,
            } => {
                let icon = if *is_video { "📹" } else { "📞" };
                format!(
                    "{icon} {}",
                    call_entry_label(*is_video, discard_reason, *duration, false)
                )
            }
            MessageContent::Action(action) => crate::service_text::action_preview(action),
            MessageContent::Unsupported { .. } => "Unsupported message".into(),
            // Slice bots-games: chat-list preview for a game card.
            MessageContent::Game(game) => {
                let title = game.title.trim();
                if title.is_empty() {
                    "🎮 Game".to_string()
                } else {
                    format!("🎮 {}", title.chars().take(76).collect::<String>())
                }
            }
            // Phase S1: chat-list preview for `messageScreenshotTaken`
            // (TGX ChatContentScreenshot).
            MessageContent::ScreenshotTaken => "Took a screenshot".to_string(),
            // Slice C2k: community service messages (TGX
            // ActionChatAddedToCommunityUnknown / ActionChatRemovedFromCommunity;
            // no name lookup in the envelope layer, so no "%1$s" form).
            MessageContent::ChatAddedToCommunity { .. } => {
                "This chat was added to a community".to_string()
            }
            MessageContent::ChatRemovedFromCommunity => {
                "This chat was removed from a community".to_string()
            }
            // Slice G9: chat-list preview for `messageChatJoinFromCommunity`
            // (TGX ChatContentGroupJoinCommunity; no name lookup in the
            // envelope layer, so no sender/community-name form).
            MessageContent::ChatJoinFromCommunity { .. } => {
                "Joined the group from the community".to_string()
            }
            // Slice P1: chat-list previews for payments (TGX shows the
            // invoice title / "Payment successful").
            MessageContent::Invoice(invoice) => {
                let title = invoice.title.trim();
                if title.is_empty() {
                    "🧾 Invoice".to_string()
                } else {
                    format!("🧾 {}", title.chars().take(76).collect::<String>())
                }
            }
            MessageContent::PaymentSuccessful(success) => {
                let name = success.invoice_name.trim();
                if name.is_empty() {
                    "✅ Payment successful".to_string()
                } else {
                    format!(
                        "✅ Payment successful: {}",
                        name.chars().take(60).collect::<String>()
                    )
                }
            }
            MessageContent::PaymentReceived(_) => "💸 Payment received".to_string(),
        }
    }

    /// `updateMessageContentOpened` sets `messageVoiceNote.is_listened` and
    /// `messageVideoNote.is_viewed`.
    pub fn mark_content_opened(&mut self) {
        match self {
            MessageContent::VoiceNote(note) => note.is_listened = true,
            MessageContent::VideoNote(note) => note.is_viewed = true,
            _ => {}
        }
    }
}

pub(crate) fn parse_content(value: Option<&Value>) -> (MessageContent, Vec<ParsedFile>) {
    let Some(value) = value else {
        return (
            MessageContent::Unsupported {
                type_name: "missing".into(),
            },
            Vec::new(),
        );
    };
    match value.get("@type").and_then(Value::as_str) {
        Some("messageText") => parse_message_text(value),
        Some("messageAnimatedEmoji") => {
            // TDLib sends standalone emoji as its own content constructor.
            // Keep the text when animation metadata isn't available locally.
            let text = serde_json::json!({"text": {"text": value.get("emoji").and_then(Value::as_str).unwrap_or(""), "entities": []}});
            parse_message_text(&text)
        }
        Some("messagePhoto") => parse_message_photo(value),
        Some("messageDocument") => parse_message_document(value),
        Some("messageSticker") => parse_message_sticker(value),
        Some("messageAnimation") => parse_message_animation(value),
        Some("messageVideo") => parse_message_video(value),
        Some("messageVideoNote") => parse_message_video_note(value),
        Some("messageVoiceNote") => parse_message_voice_note(value),
        Some("messageAudio") => parse_message_audio(value),
        Some("messagePoll") => parse_message_poll(value),
        Some("messageLocation") => parse_message_location(value),
        Some("messageLiveLocation") => parse_message_live_location(value),
        Some("messageVenue") => parse_message_venue(value),
        Some("messageContact") => parse_message_contact(value),
        Some("messageDice") => parse_message_dice(value),
        // Phase S1: `messageScreenshotTaken` (schema 1.8.67, line 5375) —
        // no fields; the row renderer attributes it via
        // `message.is_outgoing` ("You took a screenshot" /
        // "{name} took a screenshot", TGX YouTookAScreenshot /
        // XTookAScreenshot).
        Some("messageScreenshotTaken") => (MessageContent::ScreenshotTaken, Vec::new()),
        // Slice C2k: `messageChatAddedToCommunity` (schema 1.8.67, line
        // 5360) — keep only `community_id`; name resolution is the
        // post-Phase-9 renderer's job.
        Some("messageChatAddedToCommunity") => (
            MessageContent::ChatAddedToCommunity {
                community_id: value
                    .get("community_id")
                    .and_then(Value::as_i64)
                    .unwrap_or(0),
            },
            Vec::new(),
        ),
        // Slice C2k: `messageChatRemovedFromCommunity` (schema 1.8.67,
        // line 5363) — no fields.
        Some("messageChatRemovedFromCommunity") => {
            (MessageContent::ChatRemovedFromCommunity, Vec::new())
        }
        // Slice G9: `messageChatJoinFromCommunity` (schema 1.8.67, line
        // 5354) — keep only `community_id`; name resolution is the
        // renderer's job (session `communities` cache).
        Some("messageChatJoinFromCommunity") => (
            MessageContent::ChatJoinFromCommunity {
                community_id: value
                    .get("community_id")
                    .and_then(Value::as_i64)
                    .unwrap_or(0),
            },
            Vec::new(),
        ),
        // M2: `messageRichMessage` (schema 1.8.67, line 5143).
        Some("messageRichMessage") => parse_message_rich_message(value),
        // Slice bots-games: `messageGame` (schema 1.8.67, line 5234).
        Some("messageGame") => parse_message_game(value),
        // Slice P1: `messageInvoice` (schema 1.8.67, line 5270).
        Some("messageInvoice") => parse_message_invoice(value),
        // Slice P1: `messagePaymentSuccessful` (schema 1.8.67, line 5436).
        Some("messagePaymentSuccessful") => parse_message_payment_successful(value),
        // Slice P1: `messagePaymentSuccessfulBot` (schema 1.8.67, line
        // 5449) — seller-side notice, minimal parse.
        Some("messagePaymentSuccessfulBot") => parse_message_payment_received(value),
        // Phase B4: `messageChatSetMessageAutoDeleteTime` (schema 1.8.67,
        // line 5387) — the chat's auto-delete / self-destruct timer was
        // changed. `from_user_id` is not kept (the row is a neutral
        // service notice, not attributed in the UI).
        Some("messageChatSetMessageAutoDeleteTime") => (
            MessageContent::ChatTtlChanged {
                secs: value
                    .get("message_auto_delete_time")
                    .and_then(Value::as_i64)
                    .unwrap_or(0) as i32,
            },
            Vec::new(),
        ),
        // Phase C2f: `messageGroupCall` (schema 1.8.67, line 5288).
        Some("messageGroupCall") => (
            MessageContent::GroupCallInvitation {
                unique_id: int64(value.get("unique_id")).unwrap_or(0),
                is_active: value
                    .get("is_active")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                was_missed: value
                    .get("was_missed")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                is_video: value
                    .get("is_video")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            },
            Vec::new(),
        ),
        // Phase C2i: `messageCall` (schema 1.8.67, line 5277).
        Some("messageCall") => (
            MessageContent::Call {
                is_video: value
                    .get("is_video")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                discard_reason: CallDiscardReason::from_value(value.get("discard_reason")),
                duration: value.get("duration").and_then(Value::as_i64).unwrap_or(0) as i32,
            },
            Vec::new(),
        ),
        Some(other) => {
            let mut files = Vec::new();
            match ServiceAction::from_td(other, value, &mut files) {
                Some(action) => (MessageContent::Action(Box::new(action)), files),
                None => (
                    MessageContent::Unsupported {
                        type_name: other.to_string(),
                    },
                    Vec::new(),
                ),
            }
        }
        None => (
            MessageContent::Unsupported {
                type_name: "unknown".into(),
            },
            Vec::new(),
        ),
    }
}

pub(crate) fn parse_formatted_text(value: Option<&Value>) -> String {
    value
        .and_then(|text| text.get("text"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

pub(crate) fn parse_message_text(value: &Value) -> (MessageContent, Vec<ParsedFile>) {
    let mut content = parse_text_content(value.get("text"));
    let (preview, files) = parse_link_preview(value.get("link_preview"));
    content.link_preview = preview.filter(LinkPreview::has_card);
    (MessageContent::Text(content), files)
}

/// Slice bots-games: `messageGame` → `MessageContent::Game` (schema
/// 1.8.67, lines 5234 / 665-673). Thumbnail: `game.photo` sizes, falling
/// back to the animation's thumbnail when the photo carries no sizes;
/// both absent → empty sizes and the card renders a glyph. The parsed
/// files ride the files vec so `remember_files` registers them (same as
/// `parse_message_photo`).
pub(crate) fn parse_message_game(value: &Value) -> (MessageContent, Vec<ParsedFile>) {
    let game = value.get("game");
    let field = |name: &str| {
        game.and_then(|g| g.get(name))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string()
    };
    let (mut sizes, mut files) = game
        .and_then(|g| g.get("photo"))
        .map(parse_photo_sizes)
        .unwrap_or_default();
    if sizes.is_empty() {
        let (anim_sizes, anim_files) = game
            .and_then(|g| g.get("animation"))
            .and_then(|a| a.get("thumbnail"))
            .and_then(|t| t.get("photo"))
            .map(parse_photo_sizes)
            .unwrap_or_default();
        sizes = anim_sizes;
        files = anim_files;
    }
    (
        MessageContent::Game(GameContent {
            short_name: field("short_name"),
            title: field("title"),
            text: parse_text_content(game.and_then(|g| g.get("text"))),
            description: field("description"),
            photo: PhotoContent {
                has_stickers: false,
                caption: String::new(),
                caption_entities: Vec::new(),
                show_caption_above_media: false,
                sizes,
                is_secret: false,
                has_spoiler: false,
                minithumbnail: None,
            },
        }),
        files,
    )
}

/// M2: `messageRichMessage` → `MessageContent::RichMessage` (schema 1.8.67,
/// line 5143; `richMessage` line 123).
pub(crate) fn parse_message_rich_message(value: &Value) -> (MessageContent, Vec<ParsedFile>) {
    let (blocks, is_full) = parse_rich_message(value.get("message").unwrap_or(&Value::Null));
    (
        MessageContent::RichMessage(RichMessageContent { blocks, is_full }),
        Vec::new(),
    )
}

/// M2: `ephemeralMessageContent` (schema 1.8.67, line 3115). `None` when
/// the field is absent or null — the regular content renders then.
pub(crate) fn parse_ephemeral_message_content(
    value: Option<&Value>,
) -> Option<EphemeralMessageContent> {
    let value = value.filter(|v| !v.is_null())?;
    if value.get("@type").and_then(Value::as_str) != Some("ephemeralMessageContent") {
        return None;
    }
    let (content, _) = parse_content(value.get("content"));
    Some(EphemeralMessageContent {
        content: Box::new(content),
        reply_markup: parse_reply_markup(value.get("reply_markup")),
    })
}

/// M2: the content a row actually renders — `ephemeral_content` wins over
/// the regular content (schema 1.8.67, line 3161: "must be shown instead
/// of the regular content").
pub fn effective_content<'a>(
    content: &'a MessageContent,
    ephemeral: Option<&'a EphemeralMessageContent>,
) -> &'a MessageContent {
    ephemeral.map(|e| e.content.as_ref()).unwrap_or(content)
}

pub(crate) fn parse_text_content(value: Option<&Value>) -> TextContent {
    let text = parse_formatted_text(value);
    TextContent {
        text: text.clone(),
        entities: parse_text_entities(&text, value),
        link_preview: None,
    }
}

/// Parse a `formattedText` caption into (text, entities). Captions carry the
/// same entity list as message text (Phase 4.1).
pub(crate) fn parse_caption(value: Option<&Value>) -> (String, Vec<TextEntity>) {
    let text = parse_formatted_text(value);
    let entities = parse_text_entities(&text, value);
    (text, entities)
}

/// Keep the entity types Quill renders (Phase 4.1): links, the style
/// entities (`textEntityTypeBold` … `textEntityTypePreCode`), block quotes
/// (`textEntityTypeBlockQuote` / `textEntityTypeExpandableBlockQuote`), and
/// custom emoji (`textEntityTypeCustomEmoji`, rendered as sticker images),
/// and the interactive entities: mentions, hashtags, cashtags, bot
/// commands, emails, phone and bank-card numbers, media timestamps and
/// date-times. Unknown entity types are ignored.
pub(crate) fn parse_text_entities(text: &str, formatted: Option<&Value>) -> Vec<TextEntity> {
    let Some(entries) = formatted
        .and_then(|value| value.get("entities"))
        .and_then(Value::as_array)
    else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in entries {
        let offset = int53_or_zero(entry.get("offset"));
        let length = int53_or_zero(entry.get("length"));
        if length <= 0 || offset > i32::MAX as i64 || length > i32::MAX as i64 {
            continue;
        }
        let offset = offset as i32;
        let length = length as i32;
        let Ok(start) = utf16_to_utf8_offset(text, offset) else {
            continue;
        };
        let end_units = offset.saturating_add(length);
        let Ok(end) = utf16_to_utf8_offset(text, end_units) else {
            continue;
        };
        if start >= end || end > text.len() {
            continue;
        }
        let type_value = entry.get("type");
        let kind = match type_value
            .and_then(|t| t.get("@type"))
            .and_then(Value::as_str)
        {
            Some("textEntityTypeUrl") => TextEntityKind::Url,
            Some("textEntityTypeTextUrl") => TextEntityKind::TextUrl {
                url: type_value
                    .and_then(|t| t.get("url"))
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
            },
            Some("textEntityTypeMention") => TextEntityKind::Mention,
            Some("textEntityTypeMentionName") => {
                match int64(type_value.and_then(|t| t.get("user_id"))) {
                    Some(user_id) if user_id > 0 => TextEntityKind::MentionName { user_id },
                    _ => continue,
                }
            }
            Some("textEntityTypeHashtag") => TextEntityKind::Hashtag,
            Some("textEntityTypeCashtag") => TextEntityKind::Cashtag,
            Some("textEntityTypeBotCommand") => TextEntityKind::BotCommand,
            Some("textEntityTypeEmailAddress") => TextEntityKind::EmailAddress,
            Some("textEntityTypePhoneNumber") => TextEntityKind::PhoneNumber,
            Some("textEntityTypeBankCardNumber") => TextEntityKind::BankCardNumber,
            Some("textEntityTypeMediaTimestamp") => {
                match int64(type_value.and_then(|t| t.get("media_timestamp"))) {
                    Some(seconds) if (0..=i64::from(i32::MAX)).contains(&seconds) => {
                        TextEntityKind::MediaTimestamp {
                            seconds: seconds as i32,
                        }
                    }
                    _ => continue,
                }
            }
            Some("textEntityTypeDateTime") => {
                match int64(type_value.and_then(|t| t.get("unix_time"))) {
                    Some(unix_time) if (0..=i64::from(i32::MAX)).contains(&unix_time) => {
                        TextEntityKind::DateTime {
                            unix_time: unix_time as i32,
                        }
                    }
                    _ => continue,
                }
            }
            Some("textEntityTypeBold") => TextEntityKind::Bold,
            Some("textEntityTypeItalic") => TextEntityKind::Italic,
            Some("textEntityTypeUnderline") => TextEntityKind::Underline,
            Some("textEntityTypeStrikethrough") => TextEntityKind::Strikethrough,
            Some("textEntityTypeSpoiler") => TextEntityKind::Spoiler,
            Some("textEntityTypeCode") => TextEntityKind::Code,
            Some("textEntityTypePre") => TextEntityKind::Pre,
            Some("textEntityTypePreCode") => TextEntityKind::PreCode {
                language: type_value
                    .and_then(|t| t.get("language"))
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
            },
            Some("textEntityTypeBlockQuote") => TextEntityKind::BlockQuote,
            Some("textEntityTypeExpandableBlockQuote") => TextEntityKind::ExpandableBlockQuote,
            Some("textEntityTypeCustomEmoji") => {
                match int64(type_value.and_then(|t| t.get("custom_emoji_id"))) {
                    Some(id) if id > 0 => TextEntityKind::CustomEmoji {
                        custom_emoji_id: id,
                    },
                    // Unresolvable without an id — render the plain text.
                    _ => continue,
                }
            }
            _ => continue,
        };
        out.push(TextEntity {
            utf8_start: start,
            utf8_end: end,
            kind,
        });
    }
    out
}
