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

/// `messageDice` (TDLib 1.8.69). `emoji` is the dice glyph sent (🎲, 🎯,
/// 🏀, ⚽, 🎰, 🎳) and `value` the rolled number (its range depends on the
/// emoji, e.g. 1–6 for 🎲, 1–64 for 🎰); 0 while the roll is not known yet.
/// `success_animation_frame_number` is dropped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiceContent {
    pub emoji: String,
    pub value: i32,
    /// `initial_state`: the roll that plays until the value is known.
    pub initial: Option<DiceStickers>,
    /// `final_state`: the throw that lands on `value`. `None` while the
    /// value is unknown and when TDLib has not sent the stickers yet.
    pub landing: Option<DiceStickers>,
}

/// `DiceStickers`: what a dice draws.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiceStickers {
    /// `diceStickersRegular`: one animated sticker.
    Regular(StickerContent),
    /// `diceStickersSlotMachine`: five stickers drawn one over the other,
    /// bottom first: background, left reel, center reel, right reel, lever.
    SlotMachine(Box<[StickerContent; 5]>),
}

impl DiceStickers {
    /// The stickers to draw, bottom first.
    pub fn layers(&self) -> &[StickerContent] {
        match self {
            Self::Regular(sticker) => std::slice::from_ref(sticker),
            Self::SlotMachine(layers) => layers.as_slice(),
        }
    }
}

/// Which of a dice's animations is on screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DicePhase {
    /// The value is not known yet: the roll loops.
    Rolling,
    /// The throw plays once and rests on the rolled value.
    Landing,
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

    /// What draws now (Telegram Desktop `Dice::draw`): the throw once its
    /// stickers are here, the roll until then; `None` before either is.
    pub fn animation(&self) -> Option<(DicePhase, &DiceStickers)> {
        match (&self.landing, &self.initial) {
            (Some(landing), _) => Some((DicePhase::Landing, landing)),
            (None, Some(initial)) => Some((DicePhase::Rolling, initial)),
            (None, None) => None,
        }
    }

    /// The roll in words, for screen readers: "Rolled 4" for dice and
    /// darts, the three reels for a slot machine ("Seven · Seven · Seven,
    /// jackpot!" when all three match on sevens); the glyph while rolling.
    pub fn result_line(&self) -> String {
        if self.value <= 0 {
            return self.face().to_string();
        }
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
    /// e.g. `🎲 4`; just the glyph while rolling.
    pub fn label(&self) -> String {
        if self.value <= 0 {
            return self.face().to_string();
        }
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
        .map(|v| json_name_str(v, "first_name"))
        .unwrap_or_default();
    let last_name = contact
        .map(|v| json_name_str(v, "last_name"))
        .unwrap_or_default();
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

/// `messageDice` (TDLib 1.8.69); `emoji` falls back to 🎲 when empty.
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
    let (initial_state, final_state) = (value.get("initial_state"), value.get("final_state"));
    let Some(number) = value.get("value").and_then(Value::as_i64) else {
        return unsupported;
    };
    let Ok(value) = i32::try_from(number) else {
        return unsupported;
    };
    let mut files = Vec::new();
    let initial = parse_dice_stickers(initial_state, &mut files);
    let landing = parse_dice_stickers(final_state, &mut files);
    (
        MessageContent::Dice(DiceContent {
            emoji,
            value,
            initial,
            landing,
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
        premium_animation: None,
        set_id: item.set_id,
    });
    (sticker, files)
}

/// A `DiceStickers` value; its files join `files` so the downloader sees
/// them. A slot machine missing any of its five stickers can't be drawn
/// and counts as absent.
fn parse_dice_stickers(state: Option<&Value>, files: &mut Vec<ParsedFile>) -> Option<DiceStickers> {
    let state = state?;
    let mut found = Vec::new();
    let mut sticker = |key: &str| {
        let (sticker, sticker_files) = dice_sticker(state.get(key));
        found.extend(sticker_files);
        sticker
    };
    let stickers = match state.get("@type").and_then(Value::as_str)? {
        "diceStickersRegular" => DiceStickers::Regular(sticker("sticker")?),
        "diceStickersSlotMachine" => DiceStickers::SlotMachine(Box::new([
            sticker("background")?,
            sticker("left_reel")?,
            sticker("center_reel")?,
            sticker("right_reel")?,
            sticker("lever")?,
        ])),
        _ => return None,
    };
    files.extend(found);
    Some(stickers)
}
