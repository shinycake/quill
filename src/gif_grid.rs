//! The GIF panel's grid: saved or found GIFs packed into justified rows
//! (tdesktop `GifsListWidget`, whose mosaic is the inline results' one),
//! and which of their files the panel needs.
#![deny(clippy::indexing_slicing)]

use crate::inline_grid::{Mosaic, MosaicItem, SKIP, layout_mosaic, media_item};
use crate::telegram::envelope::AnimationItem;

/// Gap between the grid and the panel's sides (tdesktop `gifsPadding`).
pub const GIF_GRID_PADDING: i32 = 8;

/// The panel's GIFs packed across `full_width` points. Every GIF gets a
/// tile: one without a size takes its thumbnail's shape, or a square.
pub fn layout_gifs(animations: &[AnimationItem], full_width: i32) -> Mosaic {
    let items: Vec<MosaicItem> = animations
        .iter()
        .enumerate()
        .map(|(index, animation)| {
            let (width, height) = gif_shape(animation);
            media_item(index, width, height)
        })
        .collect();
    let inner = full_width - 2 * GIF_GRID_PADDING;
    layout_mosaic(&items, inner, full_width, GIF_GRID_PADDING, SKIP)
}

fn gif_shape(animation: &AnimationItem) -> (i32, i32) {
    if animation.width > 0 && animation.height > 0 {
        (animation.width, animation.height)
    } else if animation.thumb_width > 0 && animation.thumb_height > 0 {
        (animation.thumb_width, animation.thumb_height)
    } else {
        (1, 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::FileId;
    use crate::inline_grid::MEDIA_HEIGHT;

    fn gif(width: i32, height: i32) -> AnimationItem {
        AnimationItem {
            duration: 1,
            width,
            height,
            file_name: "clip.mp4".into(),
            mime_type: "video/mp4".into(),
            file_id: FileId(1),
            thumb_file_id: None,
            thumb_width: 0,
            thumb_height: 0,
            minithumbnail: None,
            thumb_still: true,
        }
    }

    #[test]
    fn full_rows_fill_the_width_and_keep_their_order() {
        let gifs: Vec<_> = [(320, 180), (200, 200), (480, 270), (240, 320), (500, 280)]
            .into_iter()
            .map(|(w, h)| gif(w, h))
            .collect();
        let mosaic = layout_gifs(&gifs, 400);
        let first = mosaic.rows.first().expect("a row");
        let last = first.cells.last().expect("a cell");
        // A closed row ends at the right padding, give or take rounding.
        let right = last.x + last.width;
        assert!((400 - GIF_GRID_PADDING - right).abs() <= first.cells.len() as i32);
        assert_eq!(first.height, MEDIA_HEIGHT + SKIP);
        assert_eq!(mosaic.order(), vec![0, 1, 2, 3, 4]);
    }

    #[test]
    fn a_gif_without_a_size_still_gets_a_tile() {
        let mut thumbed = gif(0, 0);
        thumbed.thumb_width = 90;
        thumbed.thumb_height = 45;
        let mosaic = layout_gifs(&[gif(0, 0), thumbed], 400);
        assert_eq!(mosaic.order(), vec![0, 1]);
        let cells: Vec<i32> = mosaic
            .rows
            .iter()
            .flat_map(|row| row.cells.iter().map(|cell| cell.item.max_width))
            .collect();
        // A square, then the thumbnail's 2:1 at the row height.
        assert_eq!(cells, vec![MEDIA_HEIGHT, 2 * MEDIA_HEIGHT]);
    }
}
