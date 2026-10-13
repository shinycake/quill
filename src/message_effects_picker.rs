//! The send menu's effect picker (tdesktop `SendMenu::AttachSendMenuEffect`,
//! `HistoryView::Reactions::Selector` in `EmojiListMode::MessageEffects`
//! and the `EffectPreview` card in `menu/menu_send.cpp`): which effects it
//! lists, how the strip above the menu is laid out, how search narrows the
//! expanded list and where the "Send with Effect" card goes.
//!
//! Pure logic only; `src/ui/send_effects/` draws it.

use crate::message_effects::{MessageEffect, MessageEffects};
use crate::telegram::envelope::StickerItem;
use std::collections::HashSet;

/// `st::reactStripHeight`: the strip's row.
pub const STRIP_HEIGHT: f32 = 40.;
/// `st::reactStripSize`: one cell of the strip and of the expanded emoji
/// list (`desiredSize`).
pub const STRIP_SIZE: f32 = 32.;
/// `st::reactStripImage`: the picture inside a cell.
pub const STRIP_IMAGE: f32 = 26.;
/// `st::reactStripSkip`: the strip's side padding.
pub const STRIP_SKIP: f32 = 7.;
/// `st::reactStripMinWidth`.
const STRIP_MIN_WIDTH: f32 = 60.;
/// `kMinNonTransparentColumns`: the strip shows at least this many cells
/// when the window allows.
const MIN_COLUMNS: usize = 7;
/// `st::stickerEffectWidthMin`: the smallest cell of the sticker effects.
pub const STICKER_CELL_MIN: f32 = 48.;
/// `st::emojiPanMinHeight`: the expanded list never asks for more height
/// than this.
pub const PANEL_MIN_HEIGHT: f32 = 278.;
/// The search field's row in the expanded list (`searchMargin` 10 + 6 and
/// the field).
pub const SEARCH_ROW: f32 = 48.;
/// `st::emojiPanHeader`: the "Effects from stickers" title row.
pub const SECTION_HEADER: f32 = 32.;
/// `HistoryView::Sticker::MessageEffectSize()`: twice the 112 px big emoji.
pub const PREVIEW_SIDE: f32 = 224.;
/// `st::previewMarkRead.height`: the "Send with Effect" button.
pub const PREVIEW_BUTTON: f32 = 39.;
/// `kToggleDuration`: the card fades in and out over this.
pub const PREVIEW_TOGGLE_MS: u64 = 400;

/// What the strip above the send menu shows (`Selector::countWidth`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StripLayout {
    /// Cells in the row, the expand button included.
    pub columns: usize,
    /// Effects shown before the expand button.
    pub shown: usize,
    /// The last cell expands the list (more effects than cells).
    pub expand: bool,
    /// The strip's width.
    pub width: f32,
}

/// Lay out `count` reaction effects in a strip at least `desired_width`
/// wide (the menu's) and at most `max_width` (the window's room).
pub fn strip_layout(count: usize, desired_width: f32, max_width: f32) -> StripLayout {
    let skip = ((STRIP_MIN_WIDTH - count as f32 * STRIP_SIZE) / 2.).max(STRIP_SKIP);
    let desired_columns =
        (((desired_width - 2. * skip) / STRIP_SIZE).ceil().max(0.) as usize).max(MIN_COLUMNS);
    let possible = (((max_width - 2. * skip) / STRIP_SIZE).floor().max(1.) as usize)
        .min(desired_columns)
        .max(1);
    let columns = possible.min(count);
    let expand = columns < count;
    StripLayout {
        columns,
        shown: if expand {
            columns.saturating_sub(1)
        } else {
            count
        },
        expand,
        width: (2. * skip + columns as f32 * STRIP_SIZE).max(desired_width),
    }
}

/// Columns of the expanded list's emoji effects in a panel `width` wide.
pub fn emoji_columns(width: f32) -> usize {
    (((width - 2. * STRIP_SKIP) / STRIP_SIZE).floor().max(1.)) as usize
}

/// Columns and cell side of the sticker effects in a panel `width` wide
/// (`StickersListWidget::countDesiredHeight` with `stickerEffectWidthMin`).
pub fn sticker_grid(width: f32) -> (usize, f32) {
    let available = (width - 2. * STRIP_SKIP).max(STICKER_CELL_MIN);
    let columns = ((available / STICKER_CELL_MIN).floor().max(1.)) as usize;
    (columns, available / columns as f32)
}

/// The height the expanded list asks for (`Selector::minimalHeight`): its
/// rows, never more than `emojiPanMinHeight`, plus the strip's top skip and
/// the bottom padding.
pub fn panel_min_height(width: f32, emoji: usize, stickers: usize) -> f32 {
    let emoji_rows = emoji.div_ceil(emoji_columns(width));
    let mut inner = emoji_rows as f32 * STRIP_SIZE;
    if stickers > 0 {
        let (columns, cell) = sticker_grid(width);
        inner += SECTION_HEADER + stickers.div_ceil(columns) as f32 * cell;
    }
    inner += SEARCH_ROW;
    let skip_y = (STRIP_HEIGHT - STRIP_SIZE) / 2.;
    skip_y + inner.min(PANEL_MIN_HEIGHT) + STRIP_SKIP + STRIP_SKIP
}

/// Where the expanded list goes: over the strip and the menu (`column`,
/// as x, y, width, height), grown to `min_height` downwards when the window
/// (`window_height`) has room below, else upwards (tdesktop grows the menu
/// on the side away from where it opened).
pub fn panel_rect(
    column: (f32, f32, f32, f32),
    min_height: f32,
    window_height: f32,
) -> (f32, f32, f32, f32) {
    let (x, y, w, h) = column;
    if h >= min_height {
        return column;
    }
    let grow = min_height - h;
    let top = if y + min_height <= window_height {
        y
    } else {
        (y - grow).max(0.)
    };
    (x, top, w, min_height)
}

/// The card's top-left corner (`EffectPreview::setupGeometry`): centred on
/// the chosen effect, kept inside the menu (`parent`, as x, y, width,
/// height); a menu shorter than the card centres it on the menu instead.
pub fn preview_origin(
    center: (f32, f32),
    card: (f32, f32),
    parent: (f32, f32, f32, f32),
) -> (f32, f32) {
    let (px, py, pw, ph) = parent;
    let (cw, ch) = card;
    let left = (center.0 - px - cw / 2.).min(pw - cw).max(0.);
    let top_min = ((ph - ch) / 2.).min(0.);
    let top = (center.1 - py - ch / 2.).min(ph - ch).max(top_min);
    (px + left, py + top)
}

/// Whether the send menu offers effects (tdesktop `sendButtonDefaultDetails`
/// and `AttachSelectorToMenu`): a chat with a user (`_peer->isUser()`),
/// something to send, and at least one reaction effect to show.
pub fn picker_offered(private_chat: bool, sendable: bool, reaction_effects: usize) -> bool {
    private_chat && sendable && reaction_effects > 0
}

/// `EffectPreview::canSend`: a Premium effect needs Premium.
pub fn can_send(effect: &MessageEffect, premium: bool) -> bool {
    !effect.is_premium || premium
}

/// The cell is drawn locked in the expanded list (`EmojiListWidget::
/// drawRecent` in `MessageEffects` mode, the sticker list's Premium mark).
pub fn locked(effect: &MessageEffect, premium: bool) -> bool {
    effect.is_premium && !premium
}

/// The sticker a picker cell draws: a reaction effect's select animation,
/// a sticker effect's sticker (tdesktop `Reaction::selectAnimation`).
pub fn cell_sticker(effect: &MessageEffect) -> Option<&StickerItem> {
    match &effect.kind {
        crate::message_effects::EffectKind::Emoji {
            select_animation, ..
        } => select_animation.as_ref(),
        crate::message_effects::EffectKind::Sticker { sticker } => sticker.as_ref(),
    }
    .filter(|sticker| sticker.file_id.0 != 0)
}

/// An emoji without its variation selectors, to compare the effects' emoji
/// with search results.
pub fn emoji_key(emoji: &str) -> String {
    emoji.chars().filter(|c| *c != '\u{fe0f}').collect()
}

/// Of `effects`, those whose emoji is among `found` (tdesktop
/// `appendPremiumSearchResults` for the reaction effects,
/// `filterEffectsByEmoji` for the sticker ones), in their order.
pub fn search<'a>(
    effects: &[&'a MessageEffect],
    found: &HashSet<String>,
) -> Vec<&'a MessageEffect> {
    effects
        .iter()
        .copied()
        .filter(|effect| found.contains(&emoji_key(&effect.emoji)))
        .collect()
}

/// The two lists the picker shows: reaction effects, then sticker effects.
pub struct PickerLists<'a> {
    pub reactions: Vec<&'a MessageEffect>,
    pub stickers: Vec<&'a MessageEffect>,
}

/// The picker's lists for an account (`LookupPossibleEffects`), narrowed by
/// `found` (the emoji a search matched) when searching.
pub fn picker_lists<'a>(
    effects: &'a MessageEffects,
    premium_possible: bool,
    found: Option<&HashSet<String>>,
) -> PickerLists<'a> {
    let (reactions, stickers) = effects.for_picker(premium_possible);
    match found {
        Some(found) => PickerLists {
            reactions: search(&reactions, found),
            stickers: search(&stickers, found),
        },
        None => PickerLists {
            reactions,
            stickers,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message_effects::{EffectKind, MessageEffect};

    fn effect(id: i64, emoji: &str, premium: bool, sticker: bool) -> MessageEffect {
        MessageEffect {
            id,
            static_icon: None,
            emoji: emoji.to_string(),
            is_premium: premium,
            kind: if sticker {
                EffectKind::Sticker { sticker: None }
            } else {
                EffectKind::Emoji {
                    select_animation: None,
                    effect_animation: None,
                }
            },
        }
    }

    #[test]
    fn a_narrow_menu_still_gets_seven_cells_and_an_expand_button() {
        let strip = strip_layout(20, 180., 1000.);
        assert_eq!(strip.columns, 7);
        assert!(strip.expand);
        assert_eq!(strip.shown, 6);
        assert_eq!(strip.width, 2. * STRIP_SKIP + 7. * STRIP_SIZE);
    }

    #[test]
    fn a_wide_menu_fits_more_and_a_short_list_needs_no_expand() {
        let strip = strip_layout(20, 300., 1000.);
        assert_eq!(strip.columns, 9);
        assert_eq!(strip.width, 2. * STRIP_SKIP + 9. * STRIP_SIZE);
        let all = strip_layout(5, 200., 1000.);
        assert_eq!((all.columns, all.shown, all.expand), (5, 5, false));
        // The window limits the strip.
        assert_eq!(strip_layout(20, 180., 150.).columns, 4);
    }

    #[test]
    fn the_expanded_list_asks_for_at_most_the_panels_minimum() {
        let small = panel_min_height(270., 3, 0);
        assert_eq!(small, 4. + 32. + SEARCH_ROW + 14.);
        assert_eq!(panel_min_height(270., 60, 30), 4. + PANEL_MIN_HEIGHT + 14.);
        assert_eq!(emoji_columns(270.), 8);
        let (columns, cell) = sticker_grid(270.);
        assert_eq!(columns, 5);
        assert!(cell >= STICKER_CELL_MIN);
    }

    #[test]
    fn the_panel_grows_down_when_there_is_room_else_up() {
        assert_eq!(
            panel_rect((10., 100., 250., 150.), 300., 900.),
            (10., 100., 250., 300.)
        );
        assert_eq!(
            panel_rect((10., 500., 250., 150.), 300., 700.),
            (10., 350., 250., 300.)
        );
        assert_eq!(
            panel_rect((10., 500., 250., 400.), 300., 700.),
            (10., 500., 250., 400.)
        );
    }

    #[test]
    fn the_card_centres_on_the_effect_and_stays_in_the_menu() {
        let card = (PREVIEW_SIDE, PREVIEW_SIDE + PREVIEW_BUTTON);
        // Room all around: centred.
        let parent = (0., 0., 600., 600.);
        assert_eq!(preview_origin((300., 300.), card, parent), (188., 168.5));
        // Near the right and bottom edges: pushed in.
        assert_eq!(
            preview_origin((590., 590.), card, parent),
            (600. - 224., 600. - 263.)
        );
        // A menu shorter than the card: centred on it, sticking out evenly.
        let short = (100., 100., 300., 163.);
        assert_eq!(preview_origin((150., 120.), card, short), (100., 50.));
    }

    #[test]
    fn effects_are_offered_in_private_chats_with_something_to_send() {
        assert!(picker_offered(true, true, 3));
        assert!(!picker_offered(false, true, 3));
        assert!(!picker_offered(true, false, 3));
        assert!(!picker_offered(true, true, 0));
    }

    #[test]
    fn premium_effects_need_premium_and_draw_locked() {
        let free = effect(1, "👍", false, false);
        let paid = effect(2, "🎉", true, false);
        assert!(can_send(&free, false));
        assert!(!can_send(&paid, false));
        assert!(can_send(&paid, true));
        assert!(locked(&paid, false));
        assert!(!locked(&paid, true));
        assert!(!locked(&free, false));
    }

    #[test]
    fn search_keeps_the_effects_whose_emoji_matched() {
        let heart = effect(1, "❤\u{fe0f}", false, false);
        let fire = effect(2, "🔥", false, false);
        let list = vec![&heart, &fire];
        let found: HashSet<String> = ["❤".to_string()].into_iter().collect();
        let hits = search(&list, &found);
        assert_eq!(hits.iter().map(|e| e.id).collect::<Vec<_>>(), vec![1]);
        assert!(search(&list, &HashSet::new()).is_empty());
    }

    #[test]
    fn picker_lists_filter_both_sections() {
        use crate::message_effects::parse_message_effect;
        use serde_json::json;
        let mut effects = MessageEffects::default();
        effects.set_available(vec![1, 2], vec![3]);
        for (id, emoji, sticker) in [("1", "🔥", false), ("2", "👍", false), ("3", "🔥", true)]
        {
            let kind = if sticker {
                json!({"@type": "messageEffectTypePremiumSticker", "sticker": null})
            } else {
                json!({"@type": "messageEffectTypeEmojiReaction",
                    "select_animation": null, "effect_animation": null})
            };
            let value = json!({"@type": "messageEffect", "id": id, "emoji": emoji,
                "is_premium": sticker, "static_icon": null, "type": kind});
            effects.accept(parse_message_effect(&value).unwrap().0);
        }
        let all = picker_lists(&effects, true, None);
        assert_eq!((all.reactions.len(), all.stickers.len()), (2, 1));
        let found: HashSet<String> = ["🔥".to_string()].into_iter().collect();
        let fire = picker_lists(&effects, true, Some(&found));
        assert_eq!(
            fire.reactions.iter().map(|e| e.id).collect::<Vec<_>>(),
            vec![1]
        );
        assert_eq!(
            fire.stickers.iter().map(|e| e.id).collect::<Vec<_>>(),
            vec![3]
        );
    }
}
