use super::*;
use crate::ids::FileId;
use crate::telegram::envelope::StickerFormat;
use serde_json::Value;

fn sticker(file: i32, emoji: &str) -> StickerItem {
    StickerItem {
        custom_emoji_id: None,
        id: i64::from(file),
        set_id: 5,
        emoji: emoji.into(),
        width: 512,
        height: 512,
        format: StickerFormat::Tgs,
        file_id: FileId(file),
        thumb_file_id: None,
        thumb_width: 0,
        thumb_height: 0,
        requires_premium: false,
        premium_animation: None,
    }
}

fn list(emoticons: &[&str]) -> Vec<String> {
    emoticons.iter().map(|e| e.to_string()).collect()
}

/// A pack shaped like the server's: thumbs up with effects 1 and 2 (the
/// second also tagged 3), the red heart with effect 1 (written with its
/// variation selector), a waving hand in one skin tone only, and a sticker
/// with no index, which is no effect.
fn pack() -> EmojiAnimations {
    let stickers = [
        sticker(41, "👍"),
        sticker(42, "👍"),
        sticker(43, "❤"),
        sticker(44, "👋"),
        sticker(45, "🎉"),
    ];
    let emojis = [
        list(&["👍", "1\u{20E3}"]),
        list(&["👍", "2\u{FE0F}\u{20E3}", "3\u{20E3}"]),
        list(&["❤\u{FE0F}", "1\u{20E3}"]),
        list(&["👋", "\u{1F51F}"]),
        list(&["🎉"]),
    ];
    let mut pack = EmojiAnimations::default();
    pack.apply_set(&stickers, &emojis);
    pack
}

fn files(pack: &EmojiAnimations, emoji: &str) -> Vec<(u8, i32)> {
    pack.animations_for(emoji)
        .map(|list| list.iter().map(|(i, s)| (*i, s.file_id.0)).collect())
        .unwrap_or_default()
}

#[test]
fn index_emoticons_are_keycaps_one_to_nine_and_ten() {
    assert_eq!(index_from_emoticon("1\u{20E3}"), Some(0));
    assert_eq!(index_from_emoticon("1\u{FE0F}\u{20E3}"), Some(0));
    assert_eq!(index_from_emoticon("9\u{20E3}"), Some(8));
    assert_eq!(index_from_emoticon("\u{1F51F}"), Some(9));
    // tdesktop needs something after the digit, and has no zero.
    assert_eq!(index_from_emoticon("1"), None);
    assert_eq!(index_from_emoticon("0\u{20E3}"), None);
    assert_eq!(index_from_emoticon("👍"), None);
    assert_eq!(index_from_emoticon(""), None);
}

#[test]
fn the_pack_maps_each_emoji_to_its_effects_by_index() {
    let pack = pack();
    assert!(pack.is_ready());
    assert_eq!(files(&pack, "👍"), vec![(0, 41), (1, 42), (2, 42)]);
    assert_eq!(files(&pack, "❤"), vec![(0, 43)]);
    assert_eq!(files(&pack, "👋"), vec![(9, 44)]);
    // A sticker with no index is not an effect, and indexes are no emoji.
    assert!(pack.animations_for("🎉").is_none());
    assert!(pack.animations_for("1\u{20E3}").is_none());
}

#[test]
fn the_variation_selector_does_not_matter() {
    let pack = pack();
    assert_eq!(files(&pack, "❤\u{FE0F}"), files(&pack, "❤"));
    assert_eq!(files(&pack, " 👍 "), files(&pack, "👍"));
}

#[test]
fn a_skin_tone_falls_back_to_the_plain_emoji() {
    let pack = pack();
    assert_eq!(files(&pack, "👋🏽"), vec![(9, 44)]);
    assert_eq!(files(&pack, "👍🏿"), files(&pack, "👍"));
}

#[test]
fn other_hearts_play_the_red_heart() {
    let pack = pack();
    for heart in HEARTS {
        assert_eq!(files(&pack, heart), vec![(0, 43)], "{heart}");
    }
    // Not every heart: the broken one has none.
    assert!(pack.animations_for("💔").is_none());
}

#[test]
fn an_emoji_with_its_own_effects_keeps_them() {
    let stickers = [sticker(43, "❤"), sticker(46, "💙")];
    let emojis = [list(&["❤", "1\u{20E3}"]), list(&["💙", "1\u{20E3}"])];
    let mut pack = EmojiAnimations::default();
    pack.apply_set(&stickers, &emojis);
    assert_eq!(files(&pack, "💙"), vec![(0, 46)]);
}

#[test]
fn a_pick_is_one_of_the_emojis_effects() {
    let pack = pack();
    let mut seen = std::collections::BTreeSet::new();
    for seed in 0..64 {
        let picked = pack.pick("👍", seed).expect("an effect");
        assert!([41, 42].contains(&picked.file_id.0));
        seen.insert(picked.file_id.0);
    }
    assert_eq!(seen.len(), 2, "both effects come up");
    assert_eq!(pack.pick("👍", 9), pack.pick("👍", 9));
    assert!(pack.pick("🎉", 1).is_none());
}

#[test]
fn a_new_set_replaces_the_old_one() {
    let mut pack = pack();
    pack.apply_set(&[sticker(50, "😂")], &[list(&["😂", "1\u{20E3}"])]);
    assert!(pack.animations_for("👍").is_none());
    assert_eq!(files(&pack, "😂"), vec![(0, 50)]);
}

#[test]
fn a_sticker_without_a_file_is_skipped() {
    let mut pack = EmojiAnimations::default();
    pack.apply_set(&[sticker(0, "👍")], &[list(&["👍", "1\u{20E3}"])]);
    assert!(pack.is_ready());
    assert!(pack.animations_for("👍").is_none());
}

#[test]
fn the_pack_loads_once_then_every_two_hours() {
    let mut pack = EmojiAnimations::default();
    assert!(pack.load_due(5));
    pack.loading(5);
    assert!(!pack.load_due(6));
    pack.apply_set(&[], &[]);
    assert!(!pack.load_due(5 + REFRESH_MS - 1));
    assert!(pack.load_due(5 + REFRESH_MS));
    pack.loading(5 + REFRESH_MS);
    pack.failed();
    assert!(!pack.load_due(5 + REFRESH_MS + 1));
    assert!(pack.load_due(5 + 2 * REFRESH_MS));
}

#[test]
fn a_failed_load_drops_what_waited() {
    let mut pack = EmojiAnimations::default();
    pack.loading(1);
    pack.wait(Waiting {
        chat_id: ChatId(1),
        message_id: MessageId(2),
        emoji: "👍".into(),
    });
    pack.failed();
    assert!(pack.take_waiting().is_empty());
    assert!(!pack.is_ready());
}

#[test]
fn waiting_keeps_the_newest() {
    let mut pack = EmojiAnimations::default();
    for id in 0..20 {
        pack.wait(Waiting {
            chat_id: ChatId(1),
            message_id: MessageId(id),
            emoji: "👍".into(),
        });
    }
    let waiting = pack.take_waiting();
    assert_eq!(waiting.len(), MAX_WAITING);
    assert_eq!(waiting.first().map(|w| w.message_id), Some(MessageId(12)));
    assert!(pack.take_waiting().is_empty());
}

#[test]
fn a_message_is_watched_once() {
    let mut pack = EmojiAnimations::default();
    assert!(pack.mark_watched((1, 2)));
    assert!(!pack.mark_watched((1, 2)));
    assert!(pack.mark_watched((1, 3)));
}

#[test]
fn the_request_searches_the_pack_by_name() {
    let v: Value = serde_json::from_str(&search_emoji_animations(RequestId(8))).unwrap();
    assert_eq!(v["@type"], "searchStickerSet");
    assert_eq!(v["name"], "EmojiAnimations");
    assert_eq!(v["ignore_cache"], false);
    assert_eq!(v["@extra"], RequestId(8).as_extra());
}
