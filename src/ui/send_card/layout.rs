//! How the send box lays out the tray: the photos and videos as the
//! messages they will be sent as (an album mosaic, or a tile each) and the
//! files as rows under them. Pure, so it is tested without a window.
#![deny(clippy::indexing_slicing)]

use quill::album::{ALBUM_MIN_WIDTH, AlbumRect, layout_bounds, layout_media_group, scale_layout};
use quill::composer::{AttachmentKind, ComposerAttachment};

/// The widest a mosaic grows: the room inside the box.
pub(super) const MOSAIC_WIDTH: i32 = 400;
/// The gap between the tiles of a mosaic.
pub(super) const MOSAIC_GAP: i32 = 2;
/// The tallest a preview stands: one message gets the room, several lone
/// pictures stand two to a row.
pub(super) const ONE_HEIGHT: i32 = 240;
pub(super) const STRIP_HEIGHT: i32 = 150;
/// The gap between the messages of a tray.
pub(super) const MESSAGE_GAP: i32 = 8;
/// The narrowest an album's mosaic is drawn. One too tall for its room at
/// this width (many pictures) scrolls, so its tiles stay big enough to see
/// and to click.
pub(super) const ALBUM_MIN_DRAWN: i32 = 220;
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

    /// Every message is one picture and there are several: they share
    /// rows, two to a row, instead of each taking the box's width.
    fn lone_pictures(&self) -> bool {
        self.media.len() > 1 && self.media.iter().all(|message| message.len() == 1)
    }

    /// The height a media message may take: lone pictures sharing rows
    /// stay low, an album gets the room its mosaic needs.
    pub(super) fn max_height(&self) -> i32 {
        if self.lone_pictures() {
            STRIP_HEIGHT
        } else {
            ONE_HEIGHT
        }
    }

    /// The width a media message may take: half the box for lone pictures
    /// sharing rows, so two fit side by side and the rest wrap under them.
    pub(super) fn max_width(&self) -> i32 {
        if self.lone_pictures() {
            (MOSAIC_WIDTH - MESSAGE_GAP) / 2
        } else {
            MOSAIC_WIDTH
        }
    }
}

/// The tiles of one message whose pictures are `sizes` (width, height; an
/// unknown size is 4:3), no wider than `max_width` and no taller than
/// `max_height`, except an album that would be narrower than
/// [`ALBUM_MIN_DRAWN`] for it: that one keeps the width and scrolls.
pub(super) fn message_tiles(
    sizes: &[(i32, i32)],
    max_height: i32,
    max_width: i32,
) -> Vec<AlbumRect> {
    let max_width = max_width.clamp(1, MOSAIC_WIDTH);
    match sizes {
        [] => Vec::new(),
        [(width, height)] => {
            // A lone picture keeps its shape within 1:2 and 2:1.
            let ratio = (f64::from((*width).max(1)) / f64::from((*height).max(1))).clamp(0.5, 2.0);
            let width = ((f64::from(max_height) * ratio).round() as i32).min(max_width);
            vec![AlbumRect {
                x: 0,
                y: 0,
                width,
                height: ((f64::from(width) / ratio).round() as i32).min(max_height),
            }]
        }
        _ => {
            let full = layout_media_group(sizes, MOSAIC_WIDTH, ALBUM_MIN_WIDTH, MOSAIC_GAP);
            let (width, height) = layout_bounds(&full);
            let by_height = i64::from(width) * i64::from(max_height) / i64::from(height.max(1));
            let fitted = i32::try_from(by_height)
                .unwrap_or(width)
                .max(ALBUM_MIN_DRAWN)
                .min(max_width);
            if fitted >= width {
                return full;
            }
            scale_layout(&full, fitted, MOSAIC_GAP)
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
        assert_eq!(grouped.max_height(), ONE_HEIGHT, "an album is in the strip");

        let apart = TrayLayout::of(&tray, false);
        assert_eq!(apart.media, vec![vec![0], vec![1], vec![3]]);
        assert_eq!(apart.files, vec![2]);
        assert_eq!(apart.max_height(), STRIP_HEIGHT);
        assert_eq!(apart.max_width() * 2 + MESSAGE_GAP, MOSAIC_WIDTH);
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
        let wide = message_tiles(&[(1600, 900)], 200, MOSAIC_WIDTH);
        assert_eq!(
            wide,
            vec![AlbumRect {
                x: 0,
                y: 0,
                width: 356,
                height: 200
            }]
        );
        let panorama = message_tiles(&[(5000, 500)], 200, MOSAIC_WIDTH);
        assert_eq!(panorama.first().map(|tile| tile.width), Some(400));
        let tall = message_tiles(&[(500, 5000)], 200, MOSAIC_WIDTH);
        assert_eq!(tall.first().map(|tile| tile.width), Some(100));
    }

    #[test]
    fn an_album_fits_its_room_until_its_tiles_would_get_too_small() {
        let tiles = message_tiles(
            &[(800, 600), (800, 600), (600, 800)],
            ONE_HEIGHT,
            MOSAIC_WIDTH,
        );
        let (width, height) = layout_bounds(&tiles);
        assert!(height <= ONE_HEIGHT + 1, "{height}");
        assert!((ALBUM_MIN_DRAWN..=MOSAIC_WIDTH).contains(&width), "{width}");

        // Seven portraits would be a sliver at this height: the mosaic
        // keeps its width and is taller than the room (it scrolls).
        let many = message_tiles(&[(600, 1200); 7], ONE_HEIGHT, MOSAIC_WIDTH);
        assert_eq!(many.len(), 7);
        let (width, height) = layout_bounds(&many);
        assert_eq!(width, ALBUM_MIN_DRAWN);
        assert!(height > ONE_HEIGHT, "{height}");
    }

    #[test]
    fn previews_narrow_to_the_room_they_have() {
        let lone = message_tiles(&[(1600, 900)], 240, 213);
        assert_eq!(
            lone,
            vec![AlbumRect {
                x: 0,
                y: 0,
                width: 213,
                height: 120
            }]
        );
        let album = message_tiles(&[(800, 600), (800, 600), (600, 800)], ONE_HEIGHT, 210);
        let (width, height) = layout_bounds(&album);
        assert!(width <= 210, "{width}");
        assert!(height <= ONE_HEIGHT, "{height}");
    }
}
