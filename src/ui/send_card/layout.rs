//! How the send card lays out the tray: the photos and videos as the
//! messages they will be sent as (an album mosaic, or a tile each), the
//! files as rows under them. Pure, so it is tested without a window.
#![deny(clippy::indexing_slicing)]

use quill::album::{ALBUM_MIN_WIDTH, AlbumRect, layout_bounds, layout_media_group, scale_layout};
use quill::composer::{AttachmentKind, ComposerAttachment};

/// The widest a mosaic grows in the card.
pub(super) const MOSAIC_WIDTH: i32 = 400;
/// The gap between the tiles of a mosaic.
pub(super) const MOSAIC_GAP: i32 = 2;
/// The tallest a preview stands: one message gets the room, several
/// share a strip.
pub(super) const ONE_HEIGHT: i32 = 280;
pub(super) const STRIP_HEIGHT: i32 = 150;
/// A tile narrower than this has room for one control only.
pub(super) const FULL_CONTROLS_WIDTH: i32 = 140;

/// The tray split the way it is sent (`split_for_send`), by tray index:
/// the photo and video messages, then every file.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct TrayLayout {
    /// One entry per message; two or more indices make an album.
    pub(super) media: Vec<Vec<usize>>,
    pub(super) files: Vec<usize>,
}

impl TrayLayout {
    pub(super) fn of(tray: &[ComposerAttachment], grouped: bool) -> Self {
        let mut layout = Self::default();
        let mut start = 0;
        for group in ComposerAttachment::split_for_send(tray, grouped) {
            let indices: Vec<usize> = (start..start + group.len()).collect();
            start += group.len();
            let media = group
                .first()
                .is_some_and(|first| first.kind != AttachmentKind::Document);
            if media {
                layout.media.push(indices);
            } else {
                layout.files.extend(indices);
            }
        }
        layout
    }

    /// The height a media message may take.
    pub(super) fn max_height(&self) -> i32 {
        if self.media.len() > 1 {
            STRIP_HEIGHT
        } else {
            ONE_HEIGHT
        }
    }
}

/// The tiles of one message whose pictures are `sizes` (width, height; an
/// unknown size is 4:3), no taller than `max_height`.
pub(super) fn message_tiles(sizes: &[(i32, i32)], max_height: i32) -> Vec<AlbumRect> {
    match sizes {
        [] => Vec::new(),
        [(width, height)] => {
            // A lone picture keeps its shape within 1:2 and 2:1.
            let ratio = (f64::from((*width).max(1)) / f64::from((*height).max(1))).clamp(0.5, 2.0);
            vec![AlbumRect {
                x: 0,
                y: 0,
                width: (f64::from(max_height) * ratio).round() as i32,
                height: max_height,
            }]
        }
        _ => {
            let full = layout_media_group(sizes, MOSAIC_WIDTH, ALBUM_MIN_WIDTH, MOSAIC_GAP);
            let (width, height) = layout_bounds(&full);
            if height <= max_height {
                return full;
            }
            let fitted = i64::from(width) * i64::from(max_height) / i64::from(height.max(1));
            scale_layout(&full, i32::try_from(fitted).unwrap_or(width), MOSAIC_GAP)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn item(name: &str, kind: AttachmentKind) -> ComposerAttachment {
        ComposerAttachment {
            path: PathBuf::from(name),
            kind,
            file_name: name.to_string(),
            spoiler: false,
            extras: Default::default(),
        }
    }

    #[test]
    fn media_go_as_albums_and_files_as_rows() {
        let tray = vec![
            item("a.png", AttachmentKind::Photo),
            item("b.mp4", AttachmentKind::Video),
            item("c.pdf", AttachmentKind::Document),
            item("d.png", AttachmentKind::Photo),
        ];
        let grouped = TrayLayout::of(&tray, true);
        assert_eq!(grouped.media, vec![vec![0, 1], vec![3]]);
        assert_eq!(grouped.files, vec![2]);
        assert_eq!(grouped.max_height(), STRIP_HEIGHT);

        let apart = TrayLayout::of(&tray, false);
        assert_eq!(apart.media, vec![vec![0], vec![1], vec![3]]);
        assert_eq!(apart.files, vec![2]);
    }

    #[test]
    fn one_message_gets_the_room() {
        let tray = vec![
            item("a.png", AttachmentKind::Photo),
            item("b.png", AttachmentKind::Photo),
        ];
        assert_eq!(TrayLayout::of(&tray, true).max_height(), ONE_HEIGHT);
    }

    #[test]
    fn a_lone_picture_keeps_its_shape_within_bounds() {
        let wide = message_tiles(&[(1600, 900)], 200);
        assert_eq!(
            wide,
            vec![AlbumRect {
                x: 0,
                y: 0,
                width: 356,
                height: 200
            }]
        );
        let panorama = message_tiles(&[(5000, 500)], 200);
        assert_eq!(panorama.first().map(|tile| tile.width), Some(400));
        let tall = message_tiles(&[(500, 5000)], 200);
        assert_eq!(tall.first().map(|tile| tile.width), Some(100));
    }

    #[test]
    fn an_album_never_stands_taller_than_its_room() {
        let tiles = message_tiles(&[(600, 1200), (600, 1200), (600, 1200)], STRIP_HEIGHT);
        assert_eq!(tiles.len(), 3);
        let (width, height) = layout_bounds(&tiles);
        assert!(height <= STRIP_HEIGHT + 1, "{height}");
        assert!(width <= MOSAIC_WIDTH, "{width}");
    }
}
