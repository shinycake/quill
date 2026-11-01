//! Album grouping, and photo/video album geometry from tdesktop
//! `Ui::LayoutMediaGroup` (`ui/grouped_layout.cpp`). File and audio
//! albums stack their rows in one bubble and need no geometry.
//!
//! Schema (TDLib 1.8.67): `message.media_album_id` is int64, `0` if none.
//! `sendMessageAlbum` takes 2–10 `InputMessageContent` values.
#![deny(clippy::indexing_slicing)]

use crate::telegram::envelope::MessageContent;

/// Schema: at most 10 messages in one album.
pub const ALBUM_MAX_ITEMS: usize = 10;

/// tdesktop `st::historyGroupWidthMax` (`maxMediaSize`).
pub const ALBUM_MAX_WIDTH: i32 = crate::bubble_layout::GROUP_WIDTH_MAX;
/// tdesktop `st::historyGroupWidthMin` (`minPhotoSize`).
pub const ALBUM_MIN_WIDTH: i32 = crate::bubble_layout::GROUP_WIDTH_MIN;
/// tdesktop `st::historyGroupSkip`.
pub const ALBUM_SPACING: i32 = crate::bubble_layout::GROUP_SKIP;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AlbumRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

/// One history row: a lone message, or 2+ consecutive messages of one
/// [`AlbumKind`] that share a non-zero `media_album_id` and the same
/// outgoing flag.
#[derive(Debug)]
pub enum HistoryGroup<'a, T> {
    Single(&'a T),
    Album { album_id: i64, messages: Vec<&'a T> },
}

/// What one album holds (tdesktop `GroupedMedia`): a photo/video mosaic,
/// or files or audio files stacked one under another.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlbumKind {
    Media,
    Files,
    Music,
}

/// The album `content` can be part of; `None` is always its own row.
pub fn album_kind(content: &MessageContent) -> Option<AlbumKind> {
    match content {
        MessageContent::Photo(_) | MessageContent::Video(_) => Some(AlbumKind::Media),
        MessageContent::Document(_) => Some(AlbumKind::Files),
        MessageContent::Audio(_) => Some(AlbumKind::Music),
        _ => None,
    }
}

/// Pixel size used by the mosaic. Missing dimensions become 1×1 so a ratio
/// stays finite (tdesktop divides width by height).
pub fn album_pixel_size(content: &MessageContent) -> (i32, i32) {
    let (w, h) = match content {
        MessageContent::Photo(photo) => photo
            .largest_size()
            .or_else(|| photo.thumb_size())
            .map(|size| (size.width, size.height))
            .unwrap_or((1, 1)),
        MessageContent::Video(video) => (video.width, video.height),
        _ => (1, 1),
    };
    (w.max(1), h.max(1))
}

/// Group consecutive album items of one kind. A lone id, content of no
/// [`AlbumKind`], a kind change or a sender change stays on the
/// single-message path.
pub fn group_media_albums<'a, T, K: PartialEq>(
    messages: &'a [T],
    album_id: impl Fn(&T) -> i64,
    is_outgoing: impl Fn(&T) -> bool,
    kind: impl Fn(&T) -> Option<K>,
) -> Vec<HistoryGroup<'a, T>> {
    let mut groups = Vec::new();
    let mut index = 0;
    while let Some(first) = messages.get(index) {
        let id = album_id(first);
        let outgoing = is_outgoing(first);
        let first_kind = kind(first);
        if id != 0 && first_kind.is_some() {
            let mut end = index + 1;
            while let Some(next) = messages.get(end)
                && album_id(next) == id
                && is_outgoing(next) == outgoing
                && kind(next) == first_kind
            {
                end += 1;
            }
            if end - index >= 2 {
                groups.push(HistoryGroup::Album {
                    album_id: id,
                    messages: messages
                        .get(index..end)
                        .unwrap_or_default()
                        .iter()
                        .collect(),
                });
                index = end;
                continue;
            }
        }
        groups.push(HistoryGroup::Single(first));
        index += 1;
    }
    groups
}

/// Port of `Ui::LayoutMediaGroup`. `Round` matches Qt `qRound` (half away from zero).
pub fn layout_media_group(
    sizes: &[(i32, i32)],
    max_width: i32,
    min_width: i32,
    spacing: i32,
) -> Vec<AlbumRect> {
    if sizes.is_empty() || max_width <= 0 {
        return Vec::new();
    }
    let ratios: Vec<f64> = sizes
        .iter()
        .map(|(w, h)| f64::from((*w).max(1)) / f64::from((*h).max(1)))
        .collect();
    let count = ratios.len();
    // `ranges::accumulate(_ratios, 1.) / count` starts the sum at 1.
    let average = (ratios.iter().sum::<f64>() + 1.0) / count as f64;
    let max_height = max_width;
    let max_size_ratio = f64::from(max_width) / f64::from(max_height);
    let proportions: String = ratios
        .iter()
        .map(|ratio| {
            if *ratio > 1.2 {
                'w'
            } else if *ratio < 0.8 {
                'n'
            } else {
                'q'
            }
        })
        .collect();

    if count == 1 {
        let width = max_width;
        let (size_w, size_h) = sizes.first().copied().unwrap_or((1, 1));
        // In i64: a tall item's height times the width overflows i32.
        let height = i64::from(size_h.max(1)) * i64::from(width) / i64::from(size_w.max(1));
        let height = i32::try_from(height).unwrap_or(i32::MAX);
        return vec![rect(0, 0, width, height.max(1))];
    }
    if count >= 5 || ratios.iter().any(|ratio| *ratio > 2.0) {
        return layout_complex(&ratios, average, max_width, min_width, spacing);
    }
    // `count` is 2, 3 or 4 from here (1 and 5+ returned above).
    match *ratios.as_slice() {
        [r0, r1] => {
            let pair = [r0, r1];
            if proportions == "ww" && average > 1.4 * max_size_ratio && (r1 - r0) < 0.2 {
                return layout_two_top_bottom(pair, max_width, max_height, spacing);
            }
            if proportions == "ww" || proportions == "qq" {
                return layout_two_left_right_equal(pair, max_width, max_height, spacing);
            }
            layout_two_left_right(pair, max_width, max_height, min_width, spacing)
        }
        [r0, r1, r2] => {
            let trio = [r0, r1, r2];
            if proportions.as_bytes().first() == Some(&b'n') {
                return layout_three_left(trio, max_width, max_height, min_width, spacing);
            }
            layout_three_top(trio, max_width, max_height, spacing)
        }
        [r0, r1, r2, r3] => {
            let quad = [r0, r1, r2, r3];
            if proportions.as_bytes().first() == Some(&b'w') {
                layout_four_top(quad, max_width, max_height, min_width, spacing)
            } else {
                layout_four_left(quad, max_width, max_height, min_width, spacing)
            }
        }
        _ => Vec::new(),
    }
}

fn rect(x: i32, y: i32, width: i32, height: i32) -> AlbumRect {
    AlbumRect {
        x,
        y,
        width: width.max(1),
        height: height.max(1),
    }
}

fn round_to_i32(value: f64) -> i32 {
    value.round() as i32
}

fn layout_two_top_bottom(
    [r0, r1]: [f64; 2],
    max_width: i32,
    max_height: i32,
    spacing: i32,
) -> Vec<AlbumRect> {
    let width = max_width;
    let height = round_to_i32(
        (width as f64 / r0)
            .min(width as f64 / r1)
            .min((max_height - spacing) as f64 / 2.0),
    );
    vec![
        rect(0, 0, width, height),
        rect(0, height + spacing, width, height),
    ]
}

fn layout_two_left_right_equal(
    [r0, r1]: [f64; 2],
    max_width: i32,
    max_height: i32,
    spacing: i32,
) -> Vec<AlbumRect> {
    let width = (max_width - spacing) / 2;
    let height = round_to_i32(
        (width as f64 / r0)
            .min(width as f64 / r1)
            .min(max_height as f64),
    );
    vec![
        rect(0, 0, width, height),
        rect(width + spacing, 0, width, height),
    ]
}

fn layout_two_left_right(
    [r0, r1]: [f64; 2],
    max_width: i32,
    max_height: i32,
    min_width: i32,
    spacing: i32,
) -> Vec<AlbumRect> {
    let minimal_width = round_to_i32(min_width as f64 * 1.5);
    let span = max_width - spacing;
    let second = round_to_i32((0.4 * span as f64).max(span as f64 / r0 / (1.0 / r0 + 1.0 / r1)))
        .min(max_width - spacing - minimal_width);
    let first = max_width - second - spacing;
    let height = max_height.min(round_to_i32((first as f64 / r0).min(second as f64 / r1)));
    vec![
        rect(0, 0, first, height),
        rect(first + spacing, 0, second, height),
    ]
}

fn layout_three_left(
    [r0, r1, r2]: [f64; 3],
    max_width: i32,
    max_height: i32,
    min_width: i32,
    spacing: i32,
) -> Vec<AlbumRect> {
    let first_height = max_height;
    let third_height = round_to_i32(
        ((max_height - spacing) as f64 / 2.0).min(r1 * (max_width - spacing) as f64 / (r2 + r1)),
    );
    let second_height = first_height - third_height - spacing;
    let right_width = min_width.max(round_to_i32(
        ((max_width - spacing) as f64 / 2.0)
            .min(third_height as f64 * r2)
            .min(second_height as f64 * r1),
    ));
    let left_width = round_to_i32(first_height as f64 * r0).min(max_width - spacing - right_width);
    vec![
        rect(0, 0, left_width, first_height),
        rect(left_width + spacing, 0, right_width, second_height),
        rect(
            left_width + spacing,
            second_height + spacing,
            right_width,
            third_height,
        ),
    ]
}

fn layout_three_top(
    [r0, r1, r2]: [f64; 3],
    max_width: i32,
    max_height: i32,
    spacing: i32,
) -> Vec<AlbumRect> {
    let first_width = max_width;
    let first_height =
        round_to_i32((first_width as f64 / r0).min((max_height - spacing) as f64 * 0.66));
    let second_width = (max_width - spacing) / 2;
    let second_height = (max_height - first_height - spacing).min(round_to_i32(
        (second_width as f64 / r1).min(second_width as f64 / r2),
    ));
    let third_width = first_width - second_width - spacing;
    vec![
        rect(0, 0, first_width, first_height),
        rect(0, first_height + spacing, second_width, second_height),
        rect(
            second_width + spacing,
            first_height + spacing,
            third_width,
            second_height,
        ),
    ]
}

fn layout_four_top(
    [r0, r1, r2, r3]: [f64; 4],
    max_width: i32,
    max_height: i32,
    min_width: i32,
    spacing: i32,
) -> Vec<AlbumRect> {
    let h0 = round_to_i32((max_width as f64 / r0).min((max_height - spacing) as f64 * 0.66));
    let h = round_to_i32((max_width - 2 * spacing) as f64 / (r1 + r2 + r3));
    let w0 = min_width.max(round_to_i32(
        ((max_width - 2 * spacing) as f64 * 0.4).min(h as f64 * r1),
    ));
    let w2 = round_to_i32(
        (min_width as f64)
            .max((max_width - 2 * spacing) as f64 * 0.33)
            .max(h as f64 * r3),
    );
    let w1 = max_width - w0 - w2 - 2 * spacing;
    let h1 = (max_height - h0 - spacing).min(h);
    vec![
        rect(0, 0, max_width, h0),
        rect(0, h0 + spacing, w0, h1),
        rect(w0 + spacing, h0 + spacing, w1, h1),
        rect(w0 + spacing + w1 + spacing, h0 + spacing, w2, h1),
    ]
}

fn layout_four_left(
    [r0, r1, r2, r3]: [f64; 4],
    max_width: i32,
    max_height: i32,
    min_width: i32,
    spacing: i32,
) -> Vec<AlbumRect> {
    let h = max_height;
    let w0 = round_to_i32((h as f64 * r0).min((max_width - spacing) as f64 * 0.6));
    let w = round_to_i32((max_height - 2 * spacing) as f64 / (1.0 / r1 + 1.0 / r2 + 1.0 / r3));
    let h0 = round_to_i32(w as f64 / r1);
    let h1 = round_to_i32(w as f64 / r2);
    let h2 = h - h0 - h1 - 2 * spacing;
    let w1 = min_width.max((max_width - w0 - spacing).min(w));
    vec![
        rect(0, 0, w0, h),
        rect(w0 + spacing, 0, w1, h0),
        rect(w0 + spacing, h0 + spacing, w1, h1),
        rect(w0 + spacing, h0 + h1 + 2 * spacing, w1, h2),
    ]
}

fn layout_complex(
    ratios: &[f64],
    average_ratio: f64,
    max_width: i32,
    min_width: i32,
    spacing: i32,
) -> Vec<AlbumRect> {
    let ratios: Vec<f64> = ratios
        .iter()
        .map(|ratio| {
            if average_ratio > 1.1 {
                ratio.clamp(1.0, 2.75)
            } else {
                ratio.clamp(0.6667, 1.0)
            }
        })
        .collect();
    let count = ratios.len();
    let max_height = max_width * 4 / 3;
    let multi_height = |offset: usize, n: usize| -> f64 {
        // The offsets and counts partition `ratios`, so the range exists.
        let sum: f64 = ratios
            .get(offset..offset + n)
            .map_or(1.0, |part| part.iter().sum());
        (max_width - (n as i32 - 1) * spacing) as f64 / sum
    };
    let mut attempts: Vec<(Vec<usize>, Vec<f64>)> = Vec::new();
    let mut push = |counts: Vec<usize>| {
        let mut heights = Vec::new();
        let mut offset = 0;
        for n in &counts {
            heights.push(multi_height(offset, *n));
            offset += *n;
        }
        attempts.push((counts, heights));
    };
    for first in 1..count {
        let second = count - first;
        if first <= 3 && second <= 3 {
            push(vec![first, second]);
        }
    }
    for first in 1..count.saturating_sub(1) {
        for second in 1..(count - first) {
            let third = count - first - second;
            let second_cap = if average_ratio < 0.85 { 4 } else { 3 };
            if first <= 3 && second <= second_cap && (1..=3).contains(&third) {
                push(vec![first, second, third]);
            }
        }
    }
    for first in 1..count.saturating_sub(1) {
        for second in 1..(count - first) {
            for third in 1..(count - first - second) {
                let fourth = count - first - second - third;
                if first <= 3 && second <= 3 && third <= 3 && (1..=3).contains(&fourth) {
                    push(vec![first, second, third, fourth]);
                }
            }
        }
    }
    let mut best: Option<usize> = None;
    let mut best_diff = 0.0;
    for (index, (counts, heights)) in attempts.iter().enumerate() {
        let line_count = counts.len();
        let total_height: f64 =
            heights.iter().sum::<f64>() + spacing as f64 * (line_count - 1) as f64;
        let min_line = heights.iter().copied().fold(f64::MAX, f64::min);
        let bad1 = if min_line < min_width as f64 {
            1.5
        } else {
            1.0
        };
        let bad2 = if counts
            .windows(2)
            .any(|pair| matches!(pair, [a, b] if a > b))
        {
            1.5
        } else {
            1.0
        };
        let diff = (total_height - max_height as f64).abs() * bad1 * bad2;
        if best.is_none() || diff < best_diff {
            best = Some(index);
            best_diff = diff;
        }
    }
    let Some(best) = best else {
        return Vec::new();
    };
    let Some((counts, heights)) = attempts.get(best) else {
        return Vec::new();
    };
    let mut result = Vec::with_capacity(count);
    let mut index = 0;
    let mut y = 0.0;
    for (row, col_count) in counts.iter().enumerate() {
        let Some(&line_height) = heights.get(row) else {
            return Vec::new();
        };
        let height = round_to_i32(line_height);
        let mut x = 0;
        for col in 0..*col_count {
            let Some(&ratio) = ratios.get(index) else {
                return Vec::new();
            };
            let width = if col + 1 == *col_count {
                max_width - x
            } else {
                round_to_i32(ratio * line_height)
            };
            result.push(rect(x, round_to_i32(y), width, height));
            x += width + spacing;
            index += 1;
        }
        y += height as f64 + spacing as f64;
    }
    result
}

pub fn layout_bounds(parts: &[AlbumRect]) -> (i32, i32) {
    let mut width = 1;
    let mut height = 1;
    for part in parts {
        width = width.max(part.x + part.width);
        height = height.max(part.y + part.height);
    }
    (width, height)
}

/// `GroupedMedia::countCurrentSize`: a mosaic laid out at its full width,
/// scaled down to `width` with its gaps (never up), so a narrow bubble
/// keeps the same arrangement. Each tile's right and bottom edges are
/// scaled together with the gap after them, as Telegram Desktop does, so
/// the gaps stay even.
pub fn scale_layout(parts: &[AlbumRect], width: i32, spacing: i32) -> Vec<AlbumRect> {
    let (full_w, full_h) = layout_bounds(parts);
    if width >= full_w || width <= 0 {
        return parts.to_vec();
    }
    let factor = f64::from(width) / f64::from(full_w);
    let scale = |value: i32| (f64::from(value) * factor).round() as i32;
    let gap = scale(spacing);
    parts
        .iter()
        .map(|part| {
            let right_skip = part.x + part.width < full_w;
            let bottom_skip = part.y + part.height < full_h;
            let right = part.x + part.width + if right_skip { spacing } else { 0 };
            let bottom = part.y + part.height + if bottom_skip { spacing } else { 0 };
            let (left, top) = (scale(part.x), scale(part.y));
            AlbumRect {
                x: left,
                y: top,
                width: (scale(right) - left - if right_skip { gap } else { 0 }).max(1),
                height: (scale(bottom) - top - if bottom_skip { gap } else { 0 }).max(1),
            }
        })
        .collect()
}

/// MED1: the message ids of one album, oldest first, from an unordered
/// history slice. Drives album pin/unpin (`pinChatMessage` per message —
/// TGX `MessagePinAlbum` pins each member; TDLib has no album-level pin).
pub fn album_message_ids(
    messages: &[crate::state::HistoryMessage],
    album_id: i64,
) -> Vec<crate::ids::MessageId> {
    if album_id == 0 {
        return Vec::new();
    }
    let mut ids: Vec<crate::ids::MessageId> = messages
        .iter()
        .filter(|m| m.media_album_id == album_id)
        .map(|m| m.id)
        .collect();
    ids.sort_by_key(|id| id.0);
    ids
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_squares_sit_side_by_side() {
        let parts = layout_media_group(&[(200, 200), (200, 200)], 320, 120, 2);
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0].x, 0);
        assert_eq!(parts[1].y, 0);
        assert!(parts[1].x > parts[0].width / 2);
        assert_eq!(parts[0].height, parts[1].height);
    }

    #[test]
    fn wide_pair_stacks_when_ratios_match() {
        let parts = layout_media_group(&[(800, 200), (820, 210)], 320, 120, 2);
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0].x, 0);
        assert_eq!(parts[1].x, 0);
        assert!(parts[1].y > parts[0].height);
    }

    #[test]
    fn three_with_wide_first_is_top_and_two() {
        let parts = layout_media_group(&[(400, 220), (200, 200), (200, 220)], 320, 120, 2);
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[0].width, 320);
        assert!(parts[1].y > 0);
        assert!(parts[2].x > parts[1].x);
        assert_eq!(parts[1].y, parts[2].y);
    }

    #[test]
    fn lone_album_id_stays_single() {
        let rows = [(0_i64, true), (7, true), (7, true)];
        let groups = group_media_albums(&rows, |row| row.0, |row| row.1, |_| Some(()));
        assert!(matches!(groups[0], HistoryGroup::Single(_)));
        assert!(matches!(groups[1], HistoryGroup::Album { album_id: 7, .. }));
    }

    #[test]
    fn outgoing_change_splits_the_album() {
        let rows = [(9_i64, false), (9, true)];
        let groups = group_media_albums(&rows, |row| row.0, |row| row.1, |_| Some(()));
        assert_eq!(groups.len(), 2);
    }

    #[test]
    fn files_group_apart_from_photos_and_audio() {
        use AlbumKind::{Files, Media, Music};
        let rows = [
            Some(Files),
            Some(Files),
            Some(Media),
            Some(Music),
            Some(Music),
            None,
        ];
        let groups = group_media_albums(&rows, |_| 7, |_| false, |row| *row);
        let sizes: Vec<usize> = groups
            .iter()
            .map(|group| match group {
                HistoryGroup::Album { messages, .. } => messages.len(),
                HistoryGroup::Single(_) => 1,
            })
            .collect();
        assert_eq!(sizes, [2, 1, 2, 1]);
    }

    #[test]
    fn album_message_ids_collects_sorted_oldest_first() {
        use crate::ids::{ChatId, MessageId};
        use crate::state::HistoryMessage;
        use crate::telegram::envelope::TextContent;
        let message = |id: i64, album: i64| HistoryMessage {
            sender: None,
            id: MessageId(id),
            chat_id: ChatId(1),
            is_outgoing: false,
            date: 0,
            content: crate::telegram::envelope::MessageContent::Text(TextContent::plain("x")),
            pending: false,
            reply_to: None,
            forward_info: None,
            extras: Default::default(),
            interaction_info: None,
            is_pinned: false,
            media_album_id: album,
            reply_markup: None,
            self_destruct: None,
            auto_delete: None,
            author_signature: None,
            failed: false,
            can_retry: false,
            ephemeral: None,
        };
        let history = vec![
            message(30, 7),
            message(10, 7),
            message(20, 8),
            message(40, 0),
            message(25, 7),
        ];
        assert_eq!(
            album_message_ids(&history, 7),
            vec![MessageId(10), MessageId(25), MessageId(30)]
        );
        assert_eq!(album_message_ids(&history, 8), vec![MessageId(20)]);
        assert!(album_message_ids(&history, 0).is_empty());
        assert!(album_message_ids(&history, 99).is_empty());
    }

    #[test]
    fn a_narrow_bubble_scales_the_mosaic_and_its_gaps() {
        let full = layout_media_group(
            &[(200, 200), (200, 200), (300, 200)],
            ALBUM_MAX_WIDTH,
            ALBUM_MIN_WIDTH,
            ALBUM_SPACING,
        );
        let (full_w, full_h) = layout_bounds(&full);
        // Wide enough: unchanged.
        assert_eq!(scale_layout(&full, full_w, ALBUM_SPACING), full);
        assert_eq!(scale_layout(&full, 1000, ALBUM_SPACING), full);
        // Half as wide: same arrangement, edges and gaps halved.
        let half = scale_layout(&full, full_w / 2, ALBUM_SPACING);
        assert_eq!(half.len(), full.len());
        let (w, h) = layout_bounds(&half);
        assert!((w - full_w / 2).abs() <= 1, "{w}");
        assert!((h - full_h / 2).abs() <= 1, "{h}");
        for (small, big) in half.iter().zip(&full) {
            assert_eq!(small.x == 0, big.x == 0);
            assert_eq!(small.y == 0, big.y == 0);
        }
        // Neighbours keep a gap of half the spacing (2 px of 4).
        let pairs = half.iter().flat_map(|a| half.iter().map(move |b| (a, b)));
        for (a, b) in pairs {
            if b.x > a.x + a.width - 1 && b.y < a.y + a.height && a.y < b.y + b.height {
                assert!(b.x - (a.x + a.width) >= 1, "{a:?} {b:?}");
            }
        }
    }
}
