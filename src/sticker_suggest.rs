//! Sticker suggestions by emoji in the composer (Slice S12).
//!
//! Telegram Desktop / TGX suggest stickers when the composer text ends with
//! an emoji. The suggestion mode is a client-side preference — TDLib 1.8.67
//! has no such setting or option (concept-level check: "suggest" in
//! `schema/td_api.tl` only hits staking/suggested-post constructors).
//! Suggestions themselves ride the existing `searchStickers` constructor
//! (`schema/td_api.tl:14648`, builder in `telegram/requests.rs`); this
//! module owns the mode enum, the page size, and the trailing-emoji
//! detection. Rendering the suggestion row is the UI slice's job.

use serde::{Deserialize, Serialize};

/// How many stickers one suggestion query asks TDLib for.
pub const SUGGEST_LIMIT: i32 = 12;

/// Sticker-suggestion mode (client-side; Telegram Desktop's
/// "Suggest stickers by emoji": "Installed + recommended" / "Only
/// installed" / "None").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StickerSuggestMode {
    /// Installed + recommended (Telegram Desktop default).
    #[default]
    InstalledAndRecommended,
    /// Only stickers from installed sets.
    InstalledOnly,
    /// No suggestions.
    None,
}

/// The emoji to suggest stickers for: the last whitespace-delimited token
/// of `text`, when every char in it is emoji-ish. The token (not a
/// segmented grapheme) goes straight to `searchStickers`' `emojis`
/// argument, so ZWJ sequences and variation selectors survive intact.
///
/// ponytail: hand-rolled emoji ranges, no new dependency. The ceiling is a
/// false negative on an emoji outside the ranges below (harmless — just no
/// suggestions); upgrade to a proper emoji-properties crate if misses
/// become a real problem.
pub fn suggest_emoji_for(text: &str) -> Option<&str> {
    let token = text.split_whitespace().next_back()?;
    if token.is_empty() {
        return None;
    }
    if token.chars().all(is_emoji_char) || is_keycap_sequence(token) {
        return Some(token);
    }
    None
}

/// Keycap sequences (`1️⃣` = `1` + VS16 + U+20E3): the base char is a
/// plain digit/`#`/`*`, so `is_emoji_char` alone rejects them. Checked
/// structurally — a bare trailing digit must NOT suggest (no wasted
/// `searchStickers` for "meet at 5").
fn is_keycap_sequence(token: &str) -> bool {
    let mut chars = token.chars();
    if !matches!(chars.next(), Some('0'..='9' | '#' | '*')) {
        return false;
    }
    matches!(
        chars.collect::<Vec<_>>().as_slice(),
        ['\u{FE0F}', '\u{20E3}'] | ['\u{20E3}']
    )
}

/// Conservative "is this char part of an emoji token" check: emoji blocks,
/// emoji-capable symbols, and the combining glue (ZWJ, variation
/// selectors, keycap combiner). False positives are harmless — TDLib just
/// returns no stickers.
fn is_emoji_char(c: char) -> bool {
    let c = c as u32;
    matches!(c,
        // The glue: ZWJ, variation selectors, keycap combiner.
        0x200D | 0xFE0E | 0xFE0F | 0x20E3 |
        // Single-char emoji-capable symbols.
        0x00A9 | 0x00AE | 0x203C | 0x2049 | 0x2122 | 0x2139 |
        0x231A..=0x231B | 0x2328 | 0x23CF |
        0x23E9..=0x23F3 | 0x23F8..=0x23FA | 0x24C2 |
        0x25AA..=0x25AB | 0x25B6 | 0x25C0 | 0x25FB..=0x25FE |
        0x2600..=0x27BF | 0x2934..=0x2935 |
        0x2B05..=0x2B07 | 0x2B1B..=0x2B1C | 0x2B50 | 0x2B55 |
        0x3030 | 0x303D | 0x3297 | 0x3299 |
        // Emoji arrows (emoji-capable).
        0x2194..=0x2199 | 0x21A9..=0x21AA |
        // The supplementary emoji blocks (emoticons, pictographs,
        // transport, supplemental, symbols-ext-A) incl. regional
        // indicators (flags) and skin-tone modifiers.
        0x1F000..=0x1FAFF)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trailing_emoji_detected() {
        assert_eq!(suggest_emoji_for("hello 😀"), Some("😀"));
        // Trailing whitespace is ignored (split_whitespace).
        assert_eq!(suggest_emoji_for("hello 😀  "), Some("😀"));
        // Multi-codepoint tokens survive intact (ZWJ sequence + VS16).
        assert_eq!(suggest_emoji_for("party 👨‍👩‍👧"), Some("👨‍👩‍👧"));
        assert_eq!(suggest_emoji_for("love ❤️"), Some("❤️"));
        assert_eq!(suggest_emoji_for("call me 1️⃣"), Some("1️⃣"));
        assert_eq!(suggest_emoji_for("keycap #️⃣"), Some("#️⃣"));
        // A bare trailing digit is NOT a keycap — no wasted request.
        assert_eq!(suggest_emoji_for("meet at 5"), None);
        // Non-emoji tail: no suggestions.
        assert_eq!(suggest_emoji_for("hello"), None);
        assert_eq!(suggest_emoji_for(""), None);
        assert_eq!(suggest_emoji_for("   "), None);
        assert_eq!(suggest_emoji_for("😀 hello"), None);
        assert_eq!(suggest_emoji_for("abc😀"), None);
    }

    #[test]
    fn mode_default_and_serde_roundtrip() {
        assert_eq!(
            StickerSuggestMode::default(),
            StickerSuggestMode::InstalledAndRecommended
        );
        for mode in [
            StickerSuggestMode::InstalledAndRecommended,
            StickerSuggestMode::InstalledOnly,
            StickerSuggestMode::None,
        ] {
            let json = serde_json::to_string(&mode).unwrap();
            assert_eq!(
                serde_json::from_str::<StickerSuggestMode>(&json).unwrap(),
                mode
            );
        }
        assert_eq!(
            serde_json::to_string(&StickerSuggestMode::InstalledOnly).unwrap(),
            "\"installed_only\""
        );
        // A prefs file written before the field existed still loads with
        // the default (this is what `#[serde(default)]` on
        // `MediaPrefs::sticker_suggest_mode` relies on).
        #[derive(Debug, Deserialize, PartialEq)]
        struct PrefsWithMode {
            #[serde(default)]
            sticker_suggest_mode: StickerSuggestMode,
        }
        assert_eq!(
            serde_json::from_str::<PrefsWithMode>("{}").unwrap(),
            PrefsWithMode {
                sticker_suggest_mode: StickerSuggestMode::InstalledAndRecommended
            }
        );
    }
}
