use super::*;
use serde_json::Value;

/// Official Telegram default emoji reactions (tdesktop active-emoji first
/// row / Unigram default picker). Custom emoji and paid stay out of this slice.
pub const DEFAULT_EMOJI_REACTIONS: &[&str] = &[
    "👍", "👎", "❤", "🔥", "🥰", "👏", "😁", "🤔", "🤯", "😱", "🤬", "😢", "🎉", "🤩", "🤮", "💩",
];

/// `ReactionType` (TDLib 1.8.67). Phase 1 chips use emoji only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReactionType {
    Emoji { emoji: String },
    CustomEmoji { custom_emoji_id: i64 },
    Paid,
    Unknown,
}

impl ReactionType {
    pub fn emoji(emoji: impl Into<String>) -> Self {
        Self::Emoji {
            emoji: emoji.into(),
        }
    }

    pub fn emoji_text(&self) -> Option<&str> {
        match self {
            Self::Emoji { emoji } => Some(emoji.as_str()),
            _ => None,
        }
    }

    /// JSON object for `addMessageReaction` / `removeMessageReaction`.
    pub fn to_tdlib_json(&self) -> serde_json::Value {
        match self {
            Self::Emoji { emoji } => serde_json::json!({
                "@type": "reactionTypeEmoji",
                "emoji": emoji
            }),
            Self::CustomEmoji { custom_emoji_id } => serde_json::json!({
                "@type": "reactionTypeCustomEmoji",
                "custom_emoji_id": custom_emoji_id.to_string()
            }),
            Self::Paid => serde_json::json!({ "@type": "reactionTypePaid" }),
            Self::Unknown => serde_json::json!({ "@type": "reactionTypeEmoji", "emoji": "" }),
        }
    }
}

/// `messageReaction` (TDLib 1.8.67). `used_sender_id` stays out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageReaction {
    pub reaction_type: ReactionType,
    pub total_count: i32,
    pub is_chosen: bool,
    /// Who reacted most recently (`recent_sender_ids`): small reactions
    /// show their avatars instead of a count, as in Telegram Desktop.
    pub recent_senders: Vec<MessageSender>,
}

impl MessageReaction {
    /// Chip label: emoji + count (tdesktop InlineList / Unigram ReactionButton).
    pub fn chip_label(&self) -> Option<String> {
        let emoji = self.reaction_type.emoji_text()?;
        Some(format!("{emoji} {}", self.total_count))
    }
}

/// `messageReactions` (TDLib 1.8.67). Tags / paid reactors stay out of display.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MessageReactions {
    pub reactions: Vec<MessageReaction>,
    pub are_tags: bool,
}

impl MessageReactions {
    pub fn emoji_chips(&self) -> impl Iterator<Item = &MessageReaction> {
        self.reactions
            .iter()
            .filter(|reaction| reaction.reaction_type.emoji_text().is_some())
    }

    /// Chips under a message: emoji and custom-emoji reactions (paid
    /// reactions are a separate flow).
    pub fn display_chips(&self) -> impl Iterator<Item = &MessageReaction> {
        self.reactions.iter().filter(|reaction| {
            matches!(
                reaction.reaction_type,
                ReactionType::Emoji { .. } | ReactionType::CustomEmoji { .. }
            )
        })
    }

    pub fn chosen_emoji(&self, emoji: &str) -> bool {
        self.reactions.iter().any(|reaction| {
            reaction.is_chosen && reaction.reaction_type.emoji_text() == Some(emoji)
        })
    }

    /// Whether the current user chose the custom-emoji reaction `id`.
    pub fn chosen_custom_emoji(&self, id: i64) -> bool {
        self.reactions.iter().any(|reaction| {
            reaction.is_chosen
                && matches!(reaction.reaction_type, ReactionType::CustomEmoji { custom_emoji_id } if custom_emoji_id == id)
        })
    }
}

/// `messageInteractionInfo` (TDLib 1.8.67). `reply_info` stays out of this slice.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MessageInteractionInfo {
    pub view_count: i32,
    pub forward_count: i32,
    pub reactions: Option<MessageReactions>,
}

impl MessageInteractionInfo {
    pub fn emoji_chips(&self) -> Vec<&MessageReaction> {
        self.reactions
            .as_ref()
            .map(|reactions| reactions.emoji_chips().collect())
            .unwrap_or_default()
    }

    pub fn display_chips(&self) -> Vec<&MessageReaction> {
        self.reactions
            .as_ref()
            .map(|reactions| reactions.display_chips().collect())
            .unwrap_or_default()
    }

    pub fn chosen_emoji(&self, emoji: &str) -> bool {
        self.reactions
            .as_ref()
            .is_some_and(|reactions| reactions.chosen_emoji(emoji))
    }
}

/// Toggle the current user's chosen emoji (tdesktop chip click / Unigram
/// ReactionButton). Used to build the next `updateMessageInteractionInfo`.
pub fn toggle_chosen_emoji_reaction(
    current: Option<&MessageInteractionInfo>,
    emoji: &str,
) -> MessageInteractionInfo {
    let mut info = current.cloned().unwrap_or_default();
    let mut reactions = info.reactions.take().unwrap_or_default();
    if let Some(existing) = reactions
        .reactions
        .iter_mut()
        .find(|reaction| reaction.reaction_type.emoji_text() == Some(emoji))
    {
        if existing.is_chosen {
            existing.is_chosen = false;
            existing.total_count = (existing.total_count - 1).max(0);
        } else {
            existing.is_chosen = true;
            existing.total_count += 1;
        }
    } else {
        reactions.reactions.push(MessageReaction {
            reaction_type: ReactionType::emoji(emoji),
            total_count: 1,
            is_chosen: true,
            recent_senders: Vec::new(),
        });
    }
    reactions
        .reactions
        .retain(|reaction| reaction.total_count > 0);
    info.reactions = if reactions.reactions.is_empty() {
        None
    } else {
        Some(reactions)
    };
    info
}

pub(crate) fn parse_interaction_info(value: Option<&Value>) -> Option<MessageInteractionInfo> {
    let value = value?;
    if value.is_null() {
        return None;
    }
    match value.get("@type").and_then(Value::as_str) {
        None | Some("messageInteractionInfo") => Some(MessageInteractionInfo {
            view_count: value.get("view_count").and_then(Value::as_i64).unwrap_or(0) as i32,
            forward_count: value
                .get("forward_count")
                .and_then(Value::as_i64)
                .unwrap_or(0) as i32,
            reactions: parse_message_reactions(value.get("reactions")),
        }),
        _ => None,
    }
}

pub(crate) fn parse_message_reactions(value: Option<&Value>) -> Option<MessageReactions> {
    let value = value?;
    if value.is_null() {
        return None;
    }
    match value.get("@type").and_then(Value::as_str) {
        None | Some("messageReactions") => {
            let reactions = value
                .get("reactions")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(parse_message_reaction)
                .collect();
            Some(MessageReactions {
                reactions,
                are_tags: value
                    .get("are_tags")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            })
        }
        _ => None,
    }
}

pub(crate) fn parse_message_reaction(value: &Value) -> Option<MessageReaction> {
    let reaction_type = parse_reaction_type(value.get("type"))?;
    Some(MessageReaction {
        reaction_type,
        total_count: value
            .get("total_count")
            .and_then(Value::as_i64)
            .unwrap_or(0) as i32,
        is_chosen: value
            .get("is_chosen")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        recent_senders: value
            .get("recent_sender_ids")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|sender| super::message::parse_message_sender(Some(sender)).ok())
            .collect(),
    })
}

pub(crate) fn parse_reaction_type(value: Option<&Value>) -> Option<ReactionType> {
    let value = value?;
    if value.is_null() {
        return None;
    }
    match value.get("@type").and_then(Value::as_str) {
        Some("reactionTypeEmoji") => Some(ReactionType::Emoji {
            emoji: value
                .get("emoji")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
        }),
        Some("reactionTypeCustomEmoji") => Some(ReactionType::CustomEmoji {
            custom_emoji_id: int64(value.get("custom_emoji_id")).unwrap_or(0),
        }),
        Some("reactionTypePaid") => Some(ReactionType::Paid),
        Some(_) => Some(ReactionType::Unknown),
        None => None,
    }
}
