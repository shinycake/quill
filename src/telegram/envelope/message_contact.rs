use super::*;
use serde_json::Value;

/// `contact` (TDLib 1.8.67, `schema/td_api.tl:640`). `vcard` (raw vCard
/// data, up to 2048 bytes) is kept for future address-book use but not
/// rendered in this slice; `user_id` is 0 when unknown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContactContent {
    pub phone_number: String,
    pub first_name: String,
    pub last_name: String,
    pub vcard: String,
    pub user_id: i64,
}

impl ContactContent {
    /// `first_name` + `last_name`, trimmed; empty when both are empty.
    pub fn display_name(&self) -> String {
        let name = format!("{} {}", self.first_name.trim(), self.last_name.trim());
        name.trim().to_string()
    }
}

/// `messageDice` (TDLib 1.8.67, `schema/td_api.tl:5231`). `emoji` is the
/// dice glyph sent (🎲, 🎯, 🏀, ⚽, 🎰, 🎳) and `value` is the rolled
/// number (its range depends on the emoji, e.g. 1–6 for 🎲, 1–64 for 🎰).
/// Dropped fields (documented): `initial_state` / `final_state`
/// (`DiceStickers` animated stickers — the roll animation is out of scope
/// in this slice) and `success_animation_frame_number` (the frame where a
/// "success" animation starts; only used by the animated rendering).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiceContent {
    pub emoji: String,
    pub value: i32,
}

impl DiceContent {
    /// The glyph to render. An empty `emoji` (TDLib should always send
    /// one, but malformed payloads happen) falls back to the plain die
    /// rather than rendering nothing.
    pub fn face(&self) -> &str {
        if self.emoji.is_empty() {
            "🎲"
        } else {
            &self.emoji
        }
    }

    /// Short line for chat-list previews and the composer reply target,
    /// e.g. `🎲 4`.
    pub fn label(&self) -> String {
        format!("{} {}", self.face(), self.value)
    }
}

/// `messageContact` (TDLib 1.8.67, `schema/td_api.tl:5220`). `user_id` is
/// int53 (0 when unknown); the vCard is kept verbatim for future use.
pub(crate) fn parse_message_contact(value: &Value) -> (MessageContent, Vec<ParsedFile>) {
    let contact = value.get("contact");
    let phone_number = contact
        .and_then(|v| v.get("phone_number"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let first_name = contact
        .and_then(|v| v.get("first_name"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let last_name = contact
        .and_then(|v| v.get("last_name"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    if contact.is_none_or(Value::is_null)
        || (phone_number.is_empty() && first_name.is_empty() && last_name.is_empty())
    {
        return (
            MessageContent::Unsupported {
                type_name: "messageContact".into(),
            },
            Vec::new(),
        );
    }
    (
        MessageContent::Contact(ContactContent {
            phone_number,
            first_name,
            last_name,
            vcard: contact
                .and_then(|v| v.get("vcard"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            user_id: contact
                .and_then(|v| int53(v.get("user_id")).ok())
                .unwrap_or(0),
        }),
        Vec::new(),
    )
}

/// `messageDice` (TDLib 1.8.67, `schema/td_api.tl:5231`). `initial_state` /
/// `final_state` (`DiceStickers`) and `success_animation_frame_number` are
/// dropped (see `DiceContent`); `emoji` falls back to 🎲 when empty.
///
/// **Safe rule:** `value` is the rolled number and is required — a missing,
/// non-integer, or out-of-`i32`-range `value` can't be displayed honestly,
/// so the whole message becomes `Unsupported` (`messageDice`) instead of
/// inventing (or wrapping) a number.
pub(crate) fn parse_message_dice(value: &Value) -> (MessageContent, Vec<ParsedFile>) {
    let emoji = value
        .get("emoji")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let unsupported = (
        MessageContent::Unsupported {
            type_name: "messageDice".into(),
        },
        Vec::new(),
    );
    let Some(number) = value.get("value").and_then(Value::as_i64) else {
        return unsupported;
    };
    let Ok(value) = i32::try_from(number) else {
        return unsupported;
    };
    (
        MessageContent::Dice(DiceContent { emoji, value }),
        Vec::new(),
    )
}
