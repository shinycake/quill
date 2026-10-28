//! Dice messages (`messageDice`), drawn as Telegram Desktop does
//! (`history_view_dice.cpp`, `history_view_slot_machine.cpp`): no bubble and
//! no caption, one 112 px square (`Sticker::EmojiSize`) holding the roll
//! while the value is unknown, then the throw, which plays once and rests on
//! the rolled value. A slot machine stacks its five stickers in that square.

use super::*;
use crate::ui::sticker_player::StickerView;
use quill::lang::{keys, tr_with};
use quill::telegram::envelope::{DiceContent, StickerContent};

/// `layers` are the decoded animations of `DiceContent::animation`'s
/// stickers, bottom first. Every layer fills the square, so nothing a layer
/// draws (or fails to draw) can leave it; with no layer to show, the dice
/// glyph stands in.
pub(in crate::ui) fn dice_row(
    row_id: u64,
    dice: &DiceContent,
    files: &quill::state::FileMap,
    media_roots: &[PathBuf],
    layers: Vec<Option<StickerView>>,
) -> AnyElement {
    let side = px(quill::bubble_layout::DICE_SIZE as f32);
    let stickers = dice
        .animation()
        .map_or(&[][..], |(_, stickers)| stickers.layers());
    let label = if dice.is_slot_machine() {
        tr_with(
            keys::Q_MISC_SLOT_MACHINE,
            &[("result_line", &dice.result_line())],
        )
    } else {
        dice.result_line()
    };
    let mut layers = layers.into_iter();
    let drawn: Vec<AnyElement> = (0u64..)
        .zip(stickers)
        .filter_map(|(index, sticker)| {
            let id = row_id.wrapping_mul(8).wrapping_add(index);
            dice_layer(
                id,
                sticker,
                files,
                media_roots,
                layers.next().flatten(),
                side,
            )
        })
        .collect();
    let square = div()
        .id(("dice", row_id))
        .role(Role::Image)
        .aria_label(label)
        .relative()
        .mt_2()
        .flex_none()
        .size(side)
        .overflow_hidden();
    if drawn.is_empty() {
        return square
            .flex()
            .items_center()
            .justify_center()
            .child(div().text_size(px(64.)).child(dice.face().to_string()))
            .into_any_element();
    }
    square
        .children(
            drawn
                .into_iter()
                .map(|layer| div().absolute().inset_0().size(side).child(layer)),
        )
        .into_any_element()
}

/// One sticker of a dice at `side`: its animation, else its still, else
/// nothing.
fn dice_layer(
    id: u64,
    sticker: &StickerContent,
    files: &quill::state::FileMap,
    media_roots: &[PathBuf],
    animated: Option<StickerView>,
    side: Pixels,
) -> Option<AnyElement> {
    // Drawn by the conversation's animation layer (`anim_layer`).
    if let Some(view) = animated {
        return Some(
            crate::ui::anim_layer::frames(view)
                .flex_none()
                .size(side)
                .into_any_element(),
        );
    }
    let path = sticker
        .display_file_id()
        .and_then(|file| files.get(&file.0))
        .and_then(|file| file.usable_path())
        .and_then(|path| sandboxed_display_path(path, media_roots))?;
    Some(
        img(crate::ui::image_budget::sized_media(
            &path,
            (side, side),
            Some((sticker.width, sticker.height)),
            crate::ui::image_budget::Fit::Contain,
        ))
        .id(("dice-still", id))
        .size(side)
        .object_fit(ObjectFit::Contain)
        .with_fallback(|| div().into_any_element())
        .into_any_element(),
    )
}
