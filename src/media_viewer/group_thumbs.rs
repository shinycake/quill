//! The thumbnail strip under an album in the viewer (tdesktop
//! `GroupThumbs`, media_view_group_thumbs.cpp). Pure layout: which items of
//! the current item's album show, where each thumb sits and how wide it is,
//! and the frames of the short slide when the current item changes.
//!
//! x is measured from the strip's centre. The current thumb is centred on
//! it at its picture's own width at the strip height (at most 160); the
//! others are 56 wide crops. Every thumb keeps 3 points of margin on each
//! side and the current one 12, so neighbours sit 6 apart and 15 from the
//! current one (`animateToLeft` / `animateToRight`).
#![deny(clippy::indexing_slicing)]

use super::MediaViewerItem;
use crate::ids::MessageId;

/// Thumb height (`mediaviewGroupHeight`).
pub const GROUP_THUMB_HEIGHT: f32 = 80.0;
/// A thumb other than the current one (`mediaviewGroupWidth`).
pub const GROUP_THUMB_WIDTH: f32 = 56.0;
/// The current thumb at most (`mediaviewGroupWidthMax`).
pub const GROUP_THUMB_WIDTH_MAX: f32 = 160.0;
/// Margin on each side of a thumb (`mediaviewGroupSkip`).
pub const GROUP_THUMB_SKIP: f32 = 3.0;
/// Margin on each side of the current thumb (`mediaviewGroupSkipCurrent`).
pub const GROUP_THUMB_SKIP_CURRENT: f32 = 12.0;
/// Room above the thumbs (`mediaviewGroupPadding` top).
pub const GROUP_STRIP_PADDING: f32 = 14.0;
/// The slide to a new current thumb (`kThumbDuration`).
pub const GROUP_THUMB_SLIDE_MS: u64 = 150;
/// Thumbs other than the current one are drawn dimmed (`setState(Alive)`).
const OTHER_OPACITY: f32 = 0.7;

/// The grouped message an item belongs to. The items of one group sit next
/// to each other in the viewer's list and show as a strip.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ViewerGroup {
    /// A `media_album_id` album: one message per item.
    Album(i64),
    /// A bought paid-media album: one message holding every item.
    PaidMedia(MessageId),
}

/// One thumb of a laid-out strip.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GroupThumb {
    /// Its place in the album (0 = the album's first item), which stays the
    /// same when older items are merged in front of the list.
    pub position: usize,
    /// Left edge, from the strip's centre.
    pub left: f32,
    pub width: f32,
    pub current: bool,
}

/// The strip for one current item: the album's thumbs that fit, left to
/// right.
#[derive(Clone, Debug, PartialEq)]
pub struct GroupStrip {
    pub group: ViewerGroup,
    /// Index in the viewer's list of the album's first item: the item of a
    /// thumb is `start + position`.
    pub start: usize,
    pub thumbs: Vec<GroupThumb>,
}

impl GroupStrip {
    pub fn current_position(&self) -> Option<usize> {
        self.thumbs
            .iter()
            .find(|thumb| thumb.current)
            .map(|thumb| thumb.position)
    }

    /// Leftmost left edge and rightmost right edge, from the centre.
    pub fn extent(&self) -> (f32, f32) {
        self.thumbs
            .iter()
            .fold((0.0_f32, 0.0_f32), |(min, max), thumb| {
                (min.min(thumb.left), max.max(thumb.left + thumb.width))
            })
    }
}

/// How many thumbs fit on each side of the current one in `available`
/// points, at least one (`ComputeThumbsLimit`, which keeps room for two
/// more on the sides for the slide).
fn thumbs_limit(available: f32) -> usize {
    let single = GROUP_THUMB_WIDTH + 2.0 * GROUP_THUMB_SKIP;
    let current = GROUP_THUMB_WIDTH_MAX + 2.0 * GROUP_THUMB_SKIP_CURRENT;
    let left = available - current - 2.0 * single;
    let fit = (left / (2.0 * single)).floor();
    if fit.is_finite() && fit > 1.0 {
        fit as usize
    } else {
        1
    }
}

/// The current thumb's width: the picture's own width at the strip height,
/// at most `GROUP_THUMB_WIDTH_MAX`; a picture of unknown size is as wide as
/// the others.
fn current_width(item: &MediaViewerItem) -> f32 {
    let natural = item
        .natural_size
        .filter(|(w, h)| *w > 0 && *h > 0)
        .map(|(w, h)| GROUP_THUMB_HEIGHT * w as f32 / h as f32)
        .filter(|width| width.is_finite());
    natural
        .unwrap_or(GROUP_THUMB_WIDTH)
        .clamp(1.0, GROUP_THUMB_WIDTH_MAX)
}

/// The strip for the item at `current`, laid out in `available` points:
/// the run of items around it in the same group, as many as fit on each
/// side. `None` for an item outside a group or alone in what is listed.
pub fn group_strip(
    items: &[MediaViewerItem],
    current: usize,
    available: f32,
) -> Option<GroupStrip> {
    let item = items.get(current)?;
    let group = item.group?;
    let in_group = |index: usize| items.get(index).is_some_and(|i| i.group == Some(group));
    let mut start = current;
    while start > 0 && in_group(start - 1) {
        start -= 1;
    }
    let mut end = current + 1;
    while in_group(end) {
        end += 1;
    }
    let limit = thumbs_limit(available);
    let from = start.max(current.saturating_sub(limit));
    let till = end.min(current.saturating_add(limit).saturating_add(1));
    if till.saturating_sub(from) < 2 {
        return None;
    }
    let width = current_width(item);
    let mut thumbs = Vec::with_capacity(till - from);
    let mut left = -width / 2.0;
    let mut skip_next = GROUP_THUMB_SKIP_CURRENT;
    for position in (from..current).rev().map(|index| index - start) {
        left -= GROUP_THUMB_WIDTH + GROUP_THUMB_SKIP + skip_next;
        skip_next = GROUP_THUMB_SKIP;
        thumbs.push(GroupThumb {
            position,
            left,
            width: GROUP_THUMB_WIDTH,
            current: false,
        });
    }
    thumbs.reverse();
    thumbs.push(GroupThumb {
        position: current - start,
        left: -width / 2.0,
        width,
        current: true,
    });
    let mut right = width / 2.0;
    let mut skip_prev = GROUP_THUMB_SKIP_CURRENT;
    for position in (current + 1..till).map(|index| index - start) {
        let left = right + GROUP_THUMB_SKIP + skip_prev;
        skip_prev = GROUP_THUMB_SKIP;
        right = left + GROUP_THUMB_WIDTH;
        thumbs.push(GroupThumb {
            position,
            left,
            width: GROUP_THUMB_WIDTH,
            current: false,
        });
    }
    Some(GroupStrip {
        group,
        start,
        thumbs,
    })
}

/// One thumb as drawn at a moment of the slide.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ThumbFrame {
    pub position: usize,
    pub left: f32,
    pub width: f32,
    pub opacity: f32,
    /// A thumb of the new strip other than the current one: a click opens
    /// its item. The current thumb and thumbs leaving the strip take none.
    pub opens: bool,
}

fn opacity_of(thumb: &GroupThumb) -> f32 {
    if thumb.current { 1.0 } else { OTHER_OPACITY }
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// How far the slide from `from` to `to` moves the thumb at `position`:
/// the move of the nearest thumb both strips show (0 with none). A thumb
/// joining or leaving the strip travels with its neighbour, as tdesktop's
/// `animateToLeft` / `animateToRight` start a new thumb next to it.
fn shift_near(from: &GroupStrip, to: &GroupStrip, position: usize) -> f32 {
    to.thumbs
        .iter()
        .filter_map(|thumb| {
            let was = from
                .thumbs
                .iter()
                .find(|was| was.position == thumb.position)?;
            Some((thumb.position.abs_diff(position), thumb.left - was.left))
        })
        .min_by_key(|(distance, _)| *distance)
        .map_or(0.0, |(_, shift)| shift)
}

/// The thumbs at `t` (0 to 1, linear like tdesktop) of the slide from the
/// strip `from` to `to`. A thumb in both moves and resizes; one only in
/// `to` slides in with its neighbour and fades in, one only in `from`
/// slides out with its neighbour and fades out. Without a `from` of the
/// same album, or at `t` = 1, it is `to` as laid out.
pub fn strip_frames(from: Option<&GroupStrip>, to: &GroupStrip, t: f32) -> Vec<ThumbFrame> {
    let t = if t.is_finite() {
        t.clamp(0.0, 1.0)
    } else {
        1.0
    };
    let from = from.filter(|from| from.group == to.group && t < 1.0);
    let mut frames: Vec<ThumbFrame> = to
        .thumbs
        .iter()
        .map(|thumb| {
            let target = opacity_of(thumb);
            let was = from.map(|from| {
                (
                    from,
                    from.thumbs
                        .iter()
                        .find(|was| was.position == thumb.position),
                )
            });
            let (left, width, opacity) = match was {
                Some((_, Some(was))) => (
                    lerp(was.left, thumb.left, t),
                    lerp(was.width, thumb.width, t),
                    lerp(opacity_of(was), target, t),
                ),
                Some((from, None)) => {
                    let shift = shift_near(from, to, thumb.position);
                    (thumb.left - shift * (1.0 - t), thumb.width, target * t)
                }
                None => (thumb.left, thumb.width, target),
            };
            ThumbFrame {
                position: thumb.position,
                left,
                width,
                opacity,
                opens: !thumb.current,
            }
        })
        .collect();
    if let Some(from) = from {
        let leaving = from
            .thumbs
            .iter()
            .filter(|was| !to.thumbs.iter().any(|thumb| thumb.position == was.position))
            .map(|was| ThumbFrame {
                position: was.position,
                left: was.left + shift_near(from, to, was.position) * t,
                width: was.width,
                opacity: opacity_of(was) * (1.0 - t),
                opens: false,
            });
        frames.extend(leaving);
    }
    frames
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::media_viewer::MediaViewerKind;
    use crate::media_viewer::test_support::item;

    const WIDE: f32 = 1200.0;

    fn album(count: usize, album_id: i64) -> Vec<MediaViewerItem> {
        (0..count)
            .map(|n| {
                let mut part = item(MediaViewerKind::Photo, 100 + n as i64);
                part.group = Some(ViewerGroup::Album(album_id));
                part.natural_size = Some((1100, 740));
                part
            })
            .collect()
    }

    fn positions(strip: &GroupStrip) -> Vec<usize> {
        strip.thumbs.iter().map(|thumb| thumb.position).collect()
    }

    #[test]
    fn a_single_item_or_an_ungrouped_one_has_no_strip() {
        assert_eq!(group_strip(&album(1, 9), 0, WIDE), None);
        let mut loose = album(3, 9);
        for part in &mut loose {
            part.group = None;
        }
        assert_eq!(group_strip(&loose, 1, WIDE), None);
        assert_eq!(group_strip(&[], 0, WIDE), None);
        assert_eq!(group_strip(&album(3, 9), 7, WIDE), None);
    }

    #[test]
    fn two_items_sit_15_points_from_the_centred_current_one() {
        let strip = group_strip(&album(2, 9), 0, WIDE).expect("a strip");
        assert_eq!(positions(&strip), vec![0, 1]);
        let [current, other] = strip.thumbs.as_slice() else {
            panic!("two thumbs");
        };
        let width = 80.0 * 1100.0 / 740.0;
        assert!(current.current && !other.current);
        assert!((current.width - width).abs() < 0.01);
        assert!((current.left + width / 2.0).abs() < 0.01, "centred");
        assert_eq!(other.width, GROUP_THUMB_WIDTH);
        assert!((other.left - (current.left + current.width) - 15.0).abs() < 0.01);
    }

    #[test]
    fn five_items_keep_6_points_between_the_others() {
        let strip = group_strip(&album(5, 9), 2, WIDE).expect("a strip");
        assert_eq!(positions(&strip), vec![0, 1, 2, 3, 4]);
        let gaps: Vec<f32> = strip
            .thumbs
            .windows(2)
            .map(|pair| pair[1].left - (pair[0].left + pair[0].width))
            .collect();
        for (gap, want) in gaps.iter().zip([6.0, 15.0, 15.0, 6.0]) {
            assert!((gap - want).abs() < 0.01, "gaps {gaps:?}");
        }
        let (min, max) = strip.extent();
        assert!((min + max).abs() < 0.01, "symmetric around the centre");
    }

    #[test]
    fn the_current_item_first_or_last_lines_the_rest_up_on_one_side() {
        let first = group_strip(&album(5, 9), 0, WIDE).expect("a strip");
        assert!(first.thumbs[0].current);
        assert!(first.thumbs.iter().skip(1).all(|thumb| thumb.left > 0.0));
        let last = group_strip(&album(5, 9), 4, WIDE).expect("a strip");
        assert!(last.thumbs[4].current);
        assert!(
            last.thumbs
                .iter()
                .take(4)
                .all(|thumb| thumb.left + thumb.width < 0.0)
        );
    }

    #[test]
    fn thirty_items_show_what_fits_around_the_current_one() {
        let items = album(30, 9);
        // (1200 - 184 - 124) / 124 = 7 on each side.
        let strip = group_strip(&items, 15, WIDE).expect("a strip");
        assert_eq!(positions(&strip), (8..=22).collect::<Vec<_>>());
        let start = group_strip(&items, 0, WIDE).expect("a strip");
        assert_eq!(positions(&start), (0..=7).collect::<Vec<_>>());
        let end = group_strip(&items, 29, WIDE).expect("a strip");
        assert_eq!(positions(&end), (22..=29).collect::<Vec<_>>());
        // However narrow, one on each side.
        let narrow = group_strip(&items, 15, 100.0).expect("a strip");
        assert_eq!(positions(&narrow), vec![14, 15, 16]);
    }

    #[test]
    fn the_current_width_follows_the_picture_up_to_the_cap() {
        let mut items = album(3, 9);
        items[1].natural_size = Some((4000, 500));
        let wide = group_strip(&items, 1, WIDE).expect("a strip");
        assert_eq!(wide.thumbs[1].width, GROUP_THUMB_WIDTH_MAX);
        items[1].natural_size = Some((300, 600));
        let tall = group_strip(&items, 1, WIDE).expect("a strip");
        assert_eq!(tall.thumbs[1].width, 40.0);
        items[1].natural_size = None;
        let unknown = group_strip(&items, 1, WIDE).expect("a strip");
        assert_eq!(unknown.thumbs[1].width, GROUP_THUMB_WIDTH);
    }

    #[test]
    fn only_the_current_items_album_shows_and_positions_count_from_its_start() {
        let mut items = album(2, 1);
        items.push(item(MediaViewerKind::Photo, 50));
        items.extend(album(3, 2));
        let strip = group_strip(&items, 4, WIDE).expect("a strip");
        assert_eq!(strip.group, ViewerGroup::Album(2));
        assert_eq!(strip.start, 3);
        assert_eq!(positions(&strip), vec![0, 1, 2]);
        assert_eq!(strip.current_position(), Some(1));
        assert_eq!(group_strip(&items, 2, WIDE), None, "the loose photo");
    }

    #[test]
    fn the_slide_moves_the_thumbs_from_the_old_layout_to_the_new() {
        let items = album(3, 9);
        let from = group_strip(&items, 0, WIDE).expect("a strip");
        let to = group_strip(&items, 2, WIDE).expect("a strip");
        let start = strip_frames(Some(&from), &to, 0.0);
        for (frame, was) in start.iter().zip(&from.thumbs) {
            assert_eq!((frame.left, frame.width), (was.left, was.width));
        }
        let half = strip_frames(Some(&from), &to, 0.5);
        let mid = |n: usize| (from.thumbs[n].left + to.thumbs[n].left) / 2.0;
        assert!((half[1].left - mid(1)).abs() < 0.01);
        let end = strip_frames(Some(&from), &to, 1.0);
        let settled = strip_frames(None, &to, 0.0);
        assert_eq!(end, settled);
        assert!(settled[2].opacity == 1.0 && !settled[2].opens);
        assert!(settled[0].opacity < 1.0 && settled[0].opens);
    }

    #[test]
    fn thumbs_entering_or_leaving_the_strip_slide_with_a_neighbour_and_fade() {
        let items = album(30, 9);
        let from = group_strip(&items, 10, WIDE).expect("a strip");
        let to = group_strip(&items, 11, WIDE).expect("a strip");
        let frames = strip_frames(Some(&from), &to, 0.5);
        let entering = frames.iter().find(|f| f.position == 18).expect("entering");
        assert!((entering.opacity - 0.35).abs() < 0.01);
        let leaving = frames.iter().find(|f| f.position == 3).expect("leaving");
        assert!((leaving.opacity - 0.35).abs() < 0.01 && !leaving.opens);
        // Both travel with their neighbour: half of its move at t = 0.5.
        let moved = |p: usize| {
            let was = from.thumbs.iter().find(|t| t.position == p).expect("was");
            let now = to.thumbs.iter().find(|t| t.position == p).expect("now");
            now.left - was.left
        };
        let landing = to.thumbs.iter().find(|t| t.position == 18).expect("in");
        assert!((entering.left - (landing.left - moved(17) / 2.0)).abs() < 0.01);
        let was = from.thumbs.iter().find(|t| t.position == 3).expect("out");
        assert!((leaving.left - (was.left + moved(4) / 2.0)).abs() < 0.01);
        let other_album = group_strip(&album(3, 5), 1, WIDE).expect("a strip");
        assert_eq!(
            strip_frames(Some(&other_album), &to, 0.0),
            strip_frames(None, &to, 0.0)
        );
    }

    #[test]
    fn a_step_slides_from_the_strip_of_the_item_it_left() {
        use crate::media_viewer::MediaViewer;
        let mut viewer = MediaViewer::open(album(3, 9), 0);
        assert_eq!(
            viewer.group_strip_before(WIDE),
            None,
            "opening does not slide"
        );
        let opened = viewer.steps();
        viewer.go_to(0);
        assert_eq!(viewer.steps(), opened, "the current thumb is not a step");
        viewer.go_to(2);
        assert_eq!(
            viewer.group_strip_before(WIDE),
            group_strip(&album(3, 9), 0, WIDE)
        );
        assert_ne!(viewer.steps(), opened);
        viewer.prev();
        let before = viewer.group_strip_before(WIDE).expect("a strip");
        assert_eq!(before.current_position(), Some(2));
        viewer.close();
        assert_eq!(viewer.group_strip_before(WIDE), None);
    }
}
