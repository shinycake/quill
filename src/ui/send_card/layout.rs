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
/// The least a preview stands in a short window: under this the box
/// scrolls instead of shrinking it further.
pub(super) const PREVIEW_FLOOR: i32 = 120;
/// What the box keeps of its height outside the part that scrolls: the
/// title, a caption of one line, the buttons and the box's own insets.
const BOX_OUTER: i32 = 164;
/// One more line of caption.
const CAPTION_LINE: i32 = 20;
/// The most lines the caption grows to (the composer's `auto_grow`).
const CAPTION_LINES: usize = 8;
/// Inside the part that scrolls, around the media: the inset and border of
/// the previews, and the space and line under the options.
const PREVIEWS_INSET: i32 = 27;
/// An option row with the gap over it, and the gap over the first.
const OPTION_ROW: i32 = 32;
const OPTIONS_GAP: i32 = 4;
/// A file row with the gap over it, and the most the list of files takes
/// before it scrolls by itself (`send_card_files`).
const FILE_ROW: i32 = 54;
const FILES_MAX: i32 = 192;

/// What stands in the box besides the media previews.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct BoxRows {
    pub(super) options: usize,
    pub(super) files: usize,
    pub(super) caption_lines: usize,
}

fn count(rows: usize) -> i32 {
    i32::try_from(rows).unwrap_or(i32::MAX)
}

/// The height of the part of the box that scrolls (the previews and the
/// options), in a box that may stand `box_max` tall: the window less the
/// margin the dialog keeps at both edges.
pub(super) fn scroll_room(box_max: i32, caption_lines: usize) -> i32 {
    let more_lines = count(caption_lines.clamp(1, CAPTION_LINES)) - 1;
    (box_max - BOX_OUTER - more_lines * CAPTION_LINE).max(PREVIEW_FLOOR)
}

/// The height left for the media previews when everything else the box
/// shows stays in view: the previews give way first, so no option row is
/// scrolled out of a short window. It may be less than [`PREVIEW_FLOOR`]
/// (or negative); [`TrayLayout::max_height`] stops at the floor and the box
/// scrolls.
pub(super) fn media_room(box_max: i32, rows: BoxRows) -> i32 {
    let options = match count(rows.options) {
        0 => 0,
        options => options.saturating_mul(OPTION_ROW) + OPTIONS_GAP,
    };
    let files = match count(rows.files) {
        0 => 0,
        // Under the media, a gap away.
        files => (files.saturating_mul(FILE_ROW) - 4).min(FILES_MAX) + MESSAGE_GAP,
    };
    scroll_room(box_max, rows.caption_lines) - PREVIEWS_INSET - options - files
}
/// The narrowest an album's mosaic is drawn with the full height to stand
/// in. One too tall for its room at this width (many pictures) scrolls, so
/// its tiles stay big enough to see and to click. A shorter room narrows
/// it by as much ([`narrowest_album`]).
pub(super) const ALBUM_MIN_DRAWN: i32 = 220;

/// [`ALBUM_MIN_DRAWN`] for an album that may stand `max_height` tall: a
/// short window draws the same preview smaller, so an album that fits
/// whole in a tall window still does in a short one.
fn narrowest_album(max_height: i32) -> i32 {
    let height = i64::from(max_height.clamp(1, ONE_HEIGHT));
    let width = i64::from(ALBUM_MIN_DRAWN) * height / i64::from(ONE_HEIGHT);
    i32::try_from(width).unwrap_or(ALBUM_MIN_DRAWN)
}
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

    /// The height a media message may take when the previews have `room`
    /// in all ([`media_room`]): lone pictures sharing rows stay low, an
    /// album gets what its mosaic needs, and in a short window each gives
    /// way down to [`PREVIEW_FLOOR`].
    pub(super) fn max_height(&self, room: i32) -> i32 {
        let (rows, tallest) = if self.lone_pictures() {
            (self.media.len().div_ceil(2), STRIP_HEIGHT)
        } else {
            (self.media.len(), ONE_HEIGHT)
        };
        let rows = count(rows).max(1);
        let gaps = (rows - 1).saturating_mul(MESSAGE_GAP);
        (room.saturating_sub(gaps) / rows).clamp(PREVIEW_FLOOR, tallest)
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
/// [`narrowest_album`] for it: that one keeps the width and scrolls.
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
                .max(narrowest_album(max_height))
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

    /// More room than any preview takes.
    const ROOMY: i32 = 10_000;

    fn item(name: &str, kind: AttachmentKind) -> ComposerAttachment {
        ComposerAttachment {
            id: Default::default(),
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
        assert_eq!(
            grouped.max_height(ROOMY),
            ONE_HEIGHT,
            "an album is in the strip"
        );

        let apart = TrayLayout::of(&tray, false);
        assert_eq!(apart.media, vec![vec![0], vec![1], vec![3]]);
        assert_eq!(apart.files, vec![2]);
        assert_eq!(apart.max_height(ROOMY), STRIP_HEIGHT);
        assert_eq!(apart.max_width() * 2 + MESSAGE_GAP, MOSAIC_WIDTH);
    }

    #[test]
    fn one_message_gets_the_room() {
        let tray = vec![
            item("a.png", AttachmentKind::Photo),
            item("b.png", AttachmentKind::Photo),
        ];
        assert_eq!(TrayLayout::of(&tray, true).max_height(ROOMY), ONE_HEIGHT);
    }

    /// The album box of the captures: Send as, Remember choice, Quality,
    /// Caption and Self-destruct under three pictures.
    const ALBUM: BoxRows = BoxRows {
        options: 5,
        files: 0,
        caption_lines: 1,
    };

    #[test]
    fn the_previews_give_way_before_an_option_row_does() {
        let album = TrayLayout {
            media: vec![vec![0, 1, 2]],
            files: Vec::new(),
        };
        let height = |box_max: i32| album.max_height(media_room(box_max, ALBUM));
        // What the album box takes at full height; a window that tall or
        // taller changes nothing.
        let full = BOX_OUTER + PREVIEWS_INSET + 5 * OPTION_ROW + OPTIONS_GAP + ONE_HEIGHT;
        assert_eq!(height(full), ONE_HEIGHT);
        assert_eq!(height(full + 300), ONE_HEIGHT);
        // Every point the window loses comes off the previews, so the rest
        // still fits.
        assert_eq!(height(full - 1), ONE_HEIGHT - 1);
        assert_eq!(height(full - 80), ONE_HEIGHT - 80);
        // Down to the floor: from there the box scrolls.
        let floor = full - (ONE_HEIGHT - PREVIEW_FLOOR);
        assert_eq!(height(floor), PREVIEW_FLOOR);
        assert_eq!(height(floor - 60), PREVIEW_FLOOR);
        assert_eq!(height(0), PREVIEW_FLOOR);
    }

    #[test]
    fn rows_files_and_caption_lines_come_out_of_the_previews_room() {
        let bare = media_room(600, BoxRows::default());
        let one_line = BoxRows {
            caption_lines: 1,
            ..BoxRows::default()
        };
        assert_eq!(media_room(600, one_line), bare, "one line is the caption");
        assert_eq!(bare - media_room(600, ALBUM), 5 * OPTION_ROW + OPTIONS_GAP);
        let with_files = |files| BoxRows {
            files,
            ..BoxRows::default()
        };
        assert_eq!(
            bare - media_room(600, with_files(1)),
            FILE_ROW - 4 + MESSAGE_GAP
        );
        // The list of files scrolls by itself past four rows.
        assert_eq!(
            media_room(600, with_files(9)),
            media_room(600, with_files(40))
        );
        let lines = |caption_lines| BoxRows {
            caption_lines,
            ..BoxRows::default()
        };
        assert_eq!(bare - media_room(600, lines(3)), 2 * CAPTION_LINE);
        assert_eq!(media_room(600, lines(8)), media_room(600, lines(200)));
    }

    #[test]
    fn lone_pictures_share_the_room_by_rows() {
        let apart = |pictures: usize| TrayLayout {
            media: (0..pictures).map(|index| vec![index]).collect(),
            files: Vec::new(),
        };
        // Two to a row: three pictures are two rows with a gap between.
        assert_eq!(apart(3).max_height(1000), STRIP_HEIGHT);
        assert_eq!(apart(3).max_height(2 * 130 + MESSAGE_GAP), 130);
        assert_eq!(apart(2).max_height(130), 130);
        assert_eq!(apart(9).max_height(100), PREVIEW_FLOOR);
        // An album and a picture after it are a row each.
        let two = TrayLayout {
            media: vec![(0..10).collect(), vec![10]],
            files: Vec::new(),
        };
        assert_eq!(two.max_height(2 * 200 + MESSAGE_GAP), 200);
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
    fn a_shorter_room_draws_the_same_album_smaller() {
        let three = [(800, 600), (800, 600), (600, 800)];
        let (full_width, full_height) =
            layout_bounds(&message_tiles(&three, ONE_HEIGHT, MOSAIC_WIDTH));
        for room in [200, 157, PREVIEW_FLOOR] {
            let (width, height) = layout_bounds(&message_tiles(&three, room, MOSAIC_WIDTH));
            // Whole in its room, in the shape it had.
            assert!(height <= room + 1, "{height} in {room}");
            assert!(width < full_width, "{width} in {room}");
            let same_shape = i64::from(width) * i64::from(full_height)
                - i64::from(height) * i64::from(full_width);
            assert!(same_shape.abs() <= i64::from(full_width + full_height));
        }
        // The album that scrolled at full height scrolls here too, half as
        // wide in half the room.
        let many = message_tiles(&[(600, 1200); 7], ONE_HEIGHT / 2, MOSAIC_WIDTH);
        let (width, height) = layout_bounds(&many);
        assert_eq!(width, ALBUM_MIN_DRAWN / 2);
        assert!(height > ONE_HEIGHT / 2, "{height}");
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
