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
    /// `final_state` of a regular dice (`diceStickersRegular`): the
    /// animated sticker that throws the die and lands on `value`. `None`
    /// for slot machines (`diceStickersSlotMachine`, five stickers
    /// composed on a reel) and when TDLib has not sent the stickers yet;
    /// the row then shows the face glyph.
    pub final_sticker: Option<StickerContent>,
    /// `final_state` of a slot machine (`diceStickersSlotMachine`): the
    /// five stickers drawn one over the other, bottom to top: background,
    /// left reel, center reel, right reel, lever. Empty for other dice and
    /// until TDLib has sent them.
    pub slot_layers: Vec<StickerContent>,
}

/// What one reel of a slot machine landed on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotSymbol {
    Bar,
    Grapes,
    Lemon,
    Seven,
}

impl SlotSymbol {
    pub fn name(self) -> &'static str {
        match self {
            Self::Bar => "Bar",
            Self::Grapes => "Grapes",
            Self::Lemon => "Lemon",
            Self::Seven => "Seven",
        }
    }
}

/// The 🎰 emoji.
const SLOT_EMOJI: &str = "🎰";

/// Rolls of a slot machine run 1..=64: the value minus one holds one
/// symbol per reel in two bits, left reel lowest (Telegram Desktop
/// `ComputePartValue`).
const SLOT_VALUES: std::ops::RangeInclusive<i32> = 1..=64;

/// The three symbols a slot-machine `value` shows, left to right.
pub fn slot_symbols(value: i32) -> Option<[SlotSymbol; 3]> {
    if !SLOT_VALUES.contains(&value) {
        return None;
    }
    let symbol = |reel: i32| match ((value - 1) >> (reel * 2)) & 3 {
        0 => SlotSymbol::Bar,
        1 => SlotSymbol::Grapes,
        2 => SlotSymbol::Lemon,
        _ => SlotSymbol::Seven,
    };
    Some([symbol(0), symbol(1), symbol(2)])
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

    /// Whether this is a slot machine.
    pub fn is_slot_machine(&self) -> bool {
        self.face() == SLOT_EMOJI
    }

    /// What the roll reads as under the picture: "Rolled 4" for dice and
    /// darts, the three reels for a slot machine ("Seven · Seven · Seven,
    /// jackpot!" when all three match on sevens).
    pub fn result_line(&self) -> String {
        if self.is_slot_machine()
            && let Some(symbols) = slot_symbols(self.value)
        {
            let reels = symbols.map(SlotSymbol::name).join(" · ");
            if symbols == [SlotSymbol::Seven; 3] {
                return format!("{reels}, jackpot!");
            }
            return reels;
        }
        format!("Rolled {}", self.value)
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
    let state = value.get("final_state");
    let Some(number) = value.get("value").and_then(Value::as_i64) else {
        return unsupported;
    };
    let Ok(value) = i32::try_from(number) else {
        return unsupported;
    };
    let (final_sticker, mut files) = parse_dice_final_state(state);
    let (slot_layers, slot_files) = parse_slot_final_state(state);
    files.extend(slot_files);
    (
        MessageContent::Dice(DiceContent {
            emoji,
            value,
            final_sticker,
            slot_layers,
        }),
        files,
    )
}

/// A `sticker` value as the sticker a message carries.
fn dice_sticker(value: Option<&Value>) -> (Option<StickerContent>, Vec<ParsedFile>) {
    let (item, mut files) = parse_sticker_value(value);
    files.retain(|file| file.id.0 != 0);
    let sticker = item.map(|item| StickerContent {
        emoji: item.emoji,
        width: item.width,
        height: item.height,
        format: item.format,
        file_id: item.file_id,
        thumb_file_id: item.thumb_file_id,
        thumb_width: item.thumb_width,
        thumb_height: item.thumb_height,
        is_premium: false,
        requires_premium: false,
        set_id: item.set_id,
    });
    (sticker, files)
}

/// The `diceStickersRegular` sticker of a dice `final_state`, plus its
/// files (they join the message's file list so the downloader sees them).
fn parse_dice_final_state(state: Option<&Value>) -> (Option<StickerContent>, Vec<ParsedFile>) {
    let regular = state
        .filter(|state| state.get("@type").and_then(Value::as_str) == Some("diceStickersRegular"));
    dice_sticker(regular.and_then(|state| state.get("sticker")))
}

/// The five stickers of a `diceStickersSlotMachine` `final_state`, bottom
/// to top, plus their files. All five are needed to draw the machine; if
/// any is missing the row falls back to the emoji.
fn parse_slot_final_state(state: Option<&Value>) -> (Vec<StickerContent>, Vec<ParsedFile>) {
    let Some(state) = state.filter(|state| {
        state.get("@type").and_then(Value::as_str) == Some("diceStickersSlotMachine")
    }) else {
        return (Vec::new(), Vec::new());
    };
    let mut layers = Vec::new();
    let mut files = Vec::new();
    for key in [
        "background",
        "left_reel",
        "center_reel",
        "right_reel",
        "lever",
    ] {
        let (sticker, layer_files) = dice_sticker(state.get(key));
        let Some(sticker) = sticker else {
            return (Vec::new(), Vec::new());
        };
        layers.push(sticker);
        files.extend(layer_files);
    }
    (layers, files)
}
