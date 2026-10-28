use super::{EmojiMedia, EmojiPlay, EmojiPlays, STICKER_SIDE, emoji_media};
use crate::ids::FileId;
use crate::sticker_playback::FitzType;
use crate::telegram::envelope::{AnimatedEmoji, StickerFormat, StickerItem, TextContent};
use crate::text::{TextEntity, TextEntityKind};

fn animated() -> AnimatedEmoji {
    AnimatedEmoji {
        sticker: StickerItem {
            custom_emoji_id: None,
            id: 1,
            set_id: 0,
            emoji: "👍".into(),
            width: 512,
            height: 512,
            format: StickerFormat::Tgs,
            file_id: FileId(3),
            thumb_file_id: None,
            thumb_width: 0,
            thumb_height: 0,
            requires_premium: false,
            premium_animation: None,
        },
        width: 512,
        height: 512,
        fitz: FitzType::None,
        sound: None,
        is_regular_emoji: true,
    }
}

/// `parts` joined, with a custom emoji entity over each `(text, Some(id))`.
fn custom(parts: &[(&str, Option<i64>)]) -> TextContent {
    let mut text = TextContent::plain("");
    for (part, id) in parts {
        let start = text.text.len();
        text.text.push_str(part);
        if let Some(custom_emoji_id) = id {
            text.entities.push(TextEntity {
                utf8_start: start,
                utf8_end: text.text.len(),
                kind: TextEntityKind::CustomEmoji {
                    custom_emoji_id: *custom_emoji_id,
                },
            });
        }
    }
    text
}

#[test]
fn a_lone_emoji_with_a_sticker_is_a_112pt_sticker() {
    let mut text = TextContent::plain("👍");
    text.animated = Some(animated());
    let media = emoji_media(&text, true).expect("big");
    assert_eq!(media, EmojiMedia::Sticker);
    assert_eq!(media.side(), STICKER_SIDE);
    assert_eq!(media.side(), 112);
    assert!(media.is_single(&text));
    // "Large emoji" off: a plain text bubble.
    assert_eq!(emoji_media(&text, false), None);
}

#[test]
fn emoji_without_a_sticker_keep_the_large_glyphs() {
    for sample in ["😀", "😀😀", "😀🎉👍"] {
        let media = emoji_media(&TextContent::plain(sample), true);
        assert_eq!(media, Some(EmojiMedia::Glyphs), "{sample}");
        assert_eq!(media.map(|m| m.side()), Some(36));
    }
    assert_eq!(emoji_media(&TextContent::plain("😀😀😀😀"), true), None);
    assert_eq!(emoji_media(&TextContent::plain("hi 😀"), true), None);
}

#[test]
fn custom_emoji_take_tdesktops_three_sizes() {
    let one = custom(&[("😀", Some(1))]);
    let two = custom(&[("😀", Some(1)), ("🎉", Some(2))]);
    let three = custom(&[("😀", Some(1)), ("🎉", Some(2)), ("👍", Some(3))]);
    let sides: Vec<i32> = [&one, &two, &three]
        .iter()
        .map(|text| emoji_media(text, true).expect("big").side())
        .collect();
    assert_eq!(sides, [112, 78, 58]);
    assert!(emoji_media(&one, true).expect("big").is_single(&one));
    assert!(!emoji_media(&two, true).expect("big").is_single(&two));
    // A lone custom emoji that came with its sticker is a custom emoji.
    let mut lone = one.clone();
    lone.animated = Some(animated());
    assert_eq!(
        emoji_media(&lone, true),
        Some(EmojiMedia::CustomEmoji {
            lines: vec![vec![1]]
        })
    );
}

#[test]
fn custom_emoji_lines_count_toward_the_size() {
    // Two lines of two: dimension 2, so 78 pt each.
    let grid = custom(&[
        ("😀", Some(1)),
        ("🎉", Some(2)),
        ("\n", None),
        ("👍", Some(3)),
        ("🔥", Some(4)),
    ]);
    let media = emoji_media(&grid, true).expect("big");
    assert_eq!(
        media,
        EmojiMedia::CustomEmoji {
            lines: vec![vec![1, 2], vec![3, 4]]
        }
    );
    assert_eq!(media.side(), 78);
    // Three lines of one: dimension 3.
    let column = custom(&[
        ("😀", Some(1)),
        ("\n", None),
        ("🎉", Some(2)),
        ("\n", None),
        ("👍", Some(3)),
    ]);
    assert_eq!(emoji_media(&column, true).map(|m| m.side()), Some(58));
    // Four on a line is past the table, and too many for large glyphs.
    let four = custom(&[
        ("😀", Some(1)),
        ("🎉", Some(2)),
        ("👍", Some(3)),
        ("🔥", Some(4)),
    ]);
    assert_eq!(emoji_media(&four, true), None);
    // A plain emoji or a space among them: the large glyphs.
    let mixed = custom(&[("😀", Some(1)), ("🎉", None)]);
    assert_eq!(emoji_media(&mixed, true), Some(EmojiMedia::Glyphs));
    let spaced = custom(&[("😀", Some(1)), (" ", None), ("🎉", Some(2))]);
    assert_eq!(emoji_media(&spaced, true), Some(EmojiMedia::Glyphs));
}

#[test]
fn a_lone_emoji_plays_once_then_rests() {
    let mut plays = EmojiPlays::default();
    let key = (1, 10);
    // Never shown: it plays.
    assert_eq!(plays.play(key, false), EmojiPlay::Play { generation: 0 });
    plays.started(key);
    assert!(plays.has_played(key));
    // While its head lives it keeps going (and rests on its first frame).
    assert_eq!(plays.play(key, true), EmojiPlay::Play { generation: 0 });
    // Its head forgotten (scrolled away long ago): it does not play again.
    assert_eq!(plays.play(key, false), EmojiPlay::Rest);
    // Another message still plays.
    assert_eq!(
        plays.play((1, 11), false),
        EmojiPlay::Play { generation: 0 }
    );
}

#[test]
fn a_tap_plays_it_again_on_a_new_head() {
    let mut plays = EmojiPlays::default();
    let key = (1, 10);
    plays.started(key);
    assert_eq!(plays.play(key, false), EmojiPlay::Rest);
    plays.replay(key);
    // The new head is not known yet; it still plays.
    assert_eq!(plays.play(key, false), EmojiPlay::Play { generation: 1 });
    plays.started(key);
    assert_eq!(plays.play(key, true), EmojiPlay::Play { generation: 1 });
    assert_eq!(plays.play(key, false), EmojiPlay::Rest);
    plays.replay(key);
    assert_eq!(plays.play(key, false), EmojiPlay::Play { generation: 2 });
}

#[test]
fn leaving_the_chat_forgets_what_played() {
    let mut plays = EmojiPlays::default();
    plays.started((1, 10));
    plays.replay((1, 10));
    plays.started((1, 10));
    plays.clear();
    assert!(!plays.has_played((1, 10)));
    assert_eq!(
        plays.play((1, 10), false),
        EmojiPlay::Play { generation: 0 }
    );
}
