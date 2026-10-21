//! The gifts panel's geometry in logical pixels (tdesktop
//! `PeerQrGiftsPanel::Inner`): one or two wide tiles, a row of three or a
//! scrolling grid of squares, the empty state, and how tall the panel
//! opens.

use super::layout::{PANEL_PADDING, panel_height};
use super::pattern::Rect;
use super::theme::{GIFT_COLUMNS, GiftsLayout};

/// `peerQrGiftSkip`.
pub const GIFT_SKIP: f64 = 9.;
/// `peerQrGiftRadius`.
pub const GIFT_RADIUS: f64 = 14.;
/// `kWideTileRatio`: a wide tile is this many squares tall.
pub const WIDE_TILE_RATIO: f64 = 1.28;
/// `kTileModelRatio`: the model's share of a tile's height.
pub const TILE_MODEL_RATIO: f64 = 0.82;
/// `peerQrGiftsEmptyPadding` (left, top, right, bottom).
pub const EMPTY_PADDING: (f64, f64, f64, f64) = (16., 8., 16., 12.);
/// `peerQrGiftsEmptySticker`.
pub const EMPTY_STICKER: f64 = 80.;
/// `peerQrGiftsEmptyTitleSkip`, `peerQrGiftsEmptyTextSkip`.
pub const EMPTY_TITLE_SKIP: f64 = 4.;
pub const EMPTY_TEXT_SKIP: f64 = 2.;
/// `peerQrGiftsEmptyTextWidth`.
pub const EMPTY_TEXT_WIDTH: f64 = 240.;
/// `peerQrGiftsEmptyButtonSkip` and the button's height.
pub const EMPTY_BUTTON_SKIP: f64 = 10.;
pub const EMPTY_BUTTON: f64 = 34.;
/// The title's line (15px semibold) and the about text's two lines at
/// 13px: tdesktop measures its labels, Quill lays them out in these.
pub const EMPTY_TITLE_LINE: f64 = 20.;
pub const EMPTY_TEXT_HEIGHT: f64 = 36.;

/// A grid square's side in a panel `width` wide (tdesktop `SquareSize`).
pub fn square_size(width: f64) -> f64 {
    let inner = width - 2. * PANEL_PADDING;
    let columns = GIFT_COLUMNS as f64;
    ((inner - (columns - 1.) * GIFT_SKIP) / columns).max(0.)
}

fn wide(layout: GiftsLayout) -> bool {
    matches!(layout, GiftsLayout::Wide(1 | 2))
}

/// Tile `index`'s rectangle inside the panel's content (tdesktop
/// `tileRect`).
pub fn tile_rect(layout: GiftsLayout, width: f64, index: usize) -> Rect {
    let size = square_size(width);
    if let GiftsLayout::Wide(count @ (1 | 2)) = layout {
        let inner = width - 2. * PANEL_PADDING;
        let tile = if count == 1 {
            inner
        } else {
            (inner - GIFT_SKIP) / 2.
        };
        return Rect::new(
            PANEL_PADDING + index as f64 * (tile + GIFT_SKIP),
            PANEL_PADDING,
            tile,
            size * WIDE_TILE_RATIO,
        );
    }
    let pitch = size + GIFT_SKIP;
    Rect::new(
        PANEL_PADDING + (index % GIFT_COLUMNS) as f64 * pitch,
        PANEL_PADDING + (index / GIFT_COLUMNS) as f64 * pitch,
        size,
        size,
    )
}

/// The empty state's height, with or without the Browse button.
pub fn empty_height(browse: bool) -> f64 {
    let (_, top, _, bottom) = EMPTY_PADDING;
    let button = if browse {
        EMPTY_BUTTON_SKIP + EMPTY_BUTTON
    } else {
        0.
    };
    top + EMPTY_STICKER
        + EMPTY_TITLE_SKIP
        + EMPTY_TITLE_LINE
        + EMPTY_TEXT_SKIP
        + EMPTY_TEXT_HEIGHT
        + button
        + bottom
}

/// The content's height (tdesktop `contentHeight`); 0 while loading.
pub fn content_height(layout: GiftsLayout, width: f64, count: usize, browse: bool) -> f64 {
    let size = square_size(width);
    let content = match layout {
        GiftsLayout::Loading => return 0.,
        GiftsLayout::Empty => return empty_height(browse),
        layout if wide(layout) => size * WIDE_TILE_RATIO,
        _ => {
            let rows = count.div_ceil(GIFT_COLUMNS) as f64;
            rows * size + (rows - 1.).max(0.) * GIFT_SKIP
        }
    };
    (2. * PANEL_PADDING + content).ceil()
}

/// How tall the gifts panel opens: as tall as the colors panel while it
/// loads or scrolls, else its content up to that (tdesktop `openHeight`).
pub fn gifts_panel_height(layout: GiftsLayout, width: f64, count: usize, browse: bool) -> f64 {
    let cap = panel_height(width);
    match layout {
        GiftsLayout::Loading | GiftsLayout::Grid => cap,
        _ => content_height(layout, width, count, browse).min(cap),
    }
}
