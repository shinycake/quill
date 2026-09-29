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
/// ponytail: arbitrary page size — small enough to keep the composer
/// suggestion row to a single scroll-free strip, big enough that
/// `searchStickers` rarely needs a second page. Neither Telegram Desktop
/// nor TGX publishes this number.
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
    use crate::diagnostics::{DiagnosticSink, MemorySink};
    use crate::ids::AccountKey;
    use crate::state::{RequestPurpose, Session};
    use crate::telegram::client::copy_and_parse;
    use crate::telegram::envelope::StickerSetInfo;
    use std::sync::Arc;
    use std::sync::atomic::AtomicU64;

    fn session() -> (Session, Arc<MemorySink>) {
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        (Session::new(AccountKey::primary(), dyn_sink), sink)
    }

    fn apply_json(session: &mut Session, seq: &AtomicU64, sink: &Arc<MemorySink>, json: &str) {
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let owned = copy_and_parse(json, seq, &dyn_sink).unwrap();
        session.apply(owned);
    }

    fn stickers_answer(extra: u64) -> String {
        format!(
            r#"{{"@type":"stickers","stickers":[{{"@type":"sticker","id":"9001","set_id":"77","width":512,"height":512,"emoji":"😀","format":{{"@type":"stickerFormatWebp"}},"full_type":{{"@type":"stickerFullTypeRegular","premium_animation":null}},"thumbnail":null,"sticker":null}}],"@extra":"{extra}"}}"#,
        )
    }

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

    /// Slice S12: the clear path (composer text with no trailing emoji)
    /// drops the pending suggest purpose and the cached slot, so a late
    /// answer for the taken request is ignored instead of landing in
    /// `suggestions` while `suggest_for` is `None`.
    #[test]
    fn s12_clear_path_drops_pending_suggest_so_late_answer_is_ignored() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        // Simulate an in-flight suggestion: purpose registered, slot set.
        let extra = session.request(RequestPurpose::SuggestStickers, None);
        session.stickers.suggest_for = Some("😀".into());
        apply_json(&mut session, &seq, &sink, &stickers_answer(extra.0));
        assert!(!session.stickers.suggestions.is_empty());

        // Clear path: drop the purpose, clear the slot.
        drop(
            session
                .requests
                .take_purpose(RequestPurpose::SuggestStickers),
        );
        session.clear_sticker_suggestions();
        assert!(
            !session
                .requests
                .has_purpose(RequestPurpose::SuggestStickers)
        );
        assert!(session.stickers.suggest_for.is_none());
        assert!(session.stickers.suggestions.is_empty());

        // A late answer for the taken request has no pending entry left,
        // so it dispatches as a stray and the slot stays empty.
        apply_json(&mut session, &seq, &sink, &stickers_answer(extra.0));
        assert!(session.stickers.suggestions.is_empty());
    }

    /// Slice S12: composer sticker suggestions are stored only under a
    /// matching `SuggestStickers` purpose (stray answers ignored), in
    /// their own slot (never the search UI's `found_stickers`), and
    /// filtered by the suggestion mode.
    #[test]
    fn s12_sticker_suggest_purpose_gated_dispatch() {
        let (mut with_purpose, sink) = session();
        let seq = AtomicU64::new(0);
        // Two installed sets; the suggest answers below reference set 77
        // (installed) and set 99 (not installed).
        with_purpose.stickers.sets = vec![
            StickerSetInfo {
                id: 77,
                title: "Demo".into(),
                name: "DemoStickers".into(),
                size: 2,
                is_installed: true,
                is_official: true,
            },
            StickerSetInfo {
                id: 78,
                title: "Other".into(),
                name: "OtherStickers".into(),
                size: 1,
                is_installed: true,
                is_official: false,
            },
        ];
        let answer = |extra: u64| {
            format!(
                r#"{{"@type":"stickers","stickers":[{{"@type":"sticker","id":"9001","set_id":"77","width":512,"height":512,"emoji":"😀","format":{{"@type":"stickerFormatWebp"}},"full_type":{{"@type":"stickerFullTypeRegular","premium_animation":null}},"thumbnail":null,"sticker":null}},{{"@type":"sticker","id":"9002","set_id":"99","width":512,"height":512,"emoji":"😀","format":{{"@type":"stickerFormatWebp"}},"full_type":{{"@type":"stickerFullTypeRegular","premium_animation":null}},"thumbnail":null,"sticker":null}}],"@extra":"{extra}"}}"#,
            )
        };

        // Default mode (InstalledAndRecommended): both stickers stored in
        // the suggestions slot; the search slot is untouched.
        let extra = with_purpose.request(RequestPurpose::SuggestStickers, None);
        apply_json(&mut with_purpose, &seq, &sink, &answer(extra.0));
        assert_eq!(with_purpose.stickers.suggestions.len(), 2);
        assert!(with_purpose.stickers.found_stickers.is_empty());

        // InstalledOnly: the set-99 sticker is filtered out.
        with_purpose.media_prefs.sticker_suggest_mode = StickerSuggestMode::InstalledOnly;
        let extra = with_purpose.request(RequestPurpose::SuggestStickers, None);
        apply_json(&mut with_purpose, &seq, &sink, &answer(extra.0));
        assert_eq!(with_purpose.stickers.suggestions.len(), 1);
        assert_eq!(with_purpose.stickers.suggestions[0].set_id, 77);

        // None: the answer is dropped and the slot cleared.
        with_purpose.media_prefs.sticker_suggest_mode = StickerSuggestMode::None;
        let extra = with_purpose.request(RequestPurpose::SuggestStickers, None);
        apply_json(&mut with_purpose, &seq, &sink, &answer(extra.0));
        assert!(with_purpose.stickers.suggestions.is_empty());

        // A stray `stickers` answer (no matching purpose) is ignored.
        let (mut without_purpose, sink2) = session();
        let seq2 = AtomicU64::new(0);
        apply_json(&mut without_purpose, &seq2, &sink2, &answer(0));
        assert!(without_purpose.stickers.suggestions.is_empty());
    }
}
