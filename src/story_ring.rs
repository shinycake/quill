//! Story rings on chat-list avatars (tdesktop `Dialogs::Row::PaintCornerBadgeFrame`,
//! `dialogs/dialogs_row.cpp:520-580`, and `Ui::PaintOutlineSegments`,
//! `ui/effects/outline_segments.cpp:11-60`).
//!
//! A peer with active stories gets a ring around its avatar split into one
//! arc per story: unread arcs first (counter-clockwise from the top, in the
//! accent gradient), then read ones (grey, thinner). The avatar itself is
//! scaled down so the ring sits outside it. This module is the pure
//! geometry; the UI paints it.

use crate::telegram::envelope::ChatActiveStoriesView;

/// `Ui::kOutlineSegmentsMax`.
pub const SEGMENTS_MAX: usize = 50;
/// `st::dialogsStoriesFull.lineTwice / 2` — unread arc width in px.
pub const UNREAD_WIDTH: f32 = 2.0;
/// `st::dialogsStoriesFull.lineReadTwice / 2` — read arc width in px.
pub const READ_WIDTH: f32 = 1.0;

/// How many stories a peer has and how many of them are unread.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StoryRing {
    pub count: usize,
    pub unread: usize,
}

impl StoryRing {
    /// `None` when the peer has no active stories (no ring).
    pub fn from_active(view: &ChatActiveStoriesView) -> Option<Self> {
        if view.stories.is_empty() {
            return None;
        }
        let unread = view
            .stories
            .iter()
            .filter(|story| story.story_id > view.max_read_story_id)
            .count();
        Some(Self {
            count: view.stories.len().min(SEGMENTS_MAX),
            unread: unread.min(SEGMENTS_MAX),
        })
    }

    pub fn has_unread(&self) -> bool {
        self.unread > 0
    }

    /// Diameter of the avatar inside a ring: tdesktop scales the userpic by
    /// `1 - 2 * skip / photoSize` with `skip = line * 3 / 2`.
    pub fn avatar_size(photo: f32) -> f32 {
        let skip = UNREAD_WIDTH * 3.0 / 2.0;
        photo * (1.0 - 2.0 * skip / photo)
    }
}

/// One arc of the ring: `start`/`sweep` in degrees, counter-clockwise from
/// 3 o'clock (the math convention; screen y points down).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RingArc {
    pub start: f32,
    pub sweep: f32,
    pub unread: bool,
}

/// `PaintOutlineSegments`: a single story is a full circle; several are
/// separated by 10 degree gaps (or less when they would not fit), starting
/// at the top half a gap counter-clockwise, unread arcs first.
pub fn arcs(ring: StoryRing) -> Vec<RingArc> {
    let count = ring.count.min(SEGMENTS_MAX);
    if count == 0 {
        return Vec::new();
    }
    if count == 1 {
        return vec![RingArc {
            start: 90.0,
            sweep: 360.0,
            unread: ring.unread > 0,
        }];
    }
    let full = 360.0_f32;
    let small = 10.0_f32;
    let separator = if full > 1.1 * small * count as f32 {
        small
    } else {
        full / (count as f32 * 1.1)
    };
    let length = (full - separator * count as f32) / count as f32;
    let mut start = 90.0 + separator / 2.0;
    (0..count)
        .map(|index| {
            let arc = RingArc {
                start,
                sweep: length,
                unread: index < ring.unread,
            };
            start += length + separator;
            arc
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use crate::telegram::envelope::{ChatActiveStoriesView, StoryInfoView, StoryListView};

    fn view(ids: &[i32], max_read: i32) -> ChatActiveStoriesView {
        ChatActiveStoriesView {
            chat_id: 1,
            list: Some(StoryListView::Main),
            order: 1,
            max_read_story_id: max_read,
            stories: ids
                .iter()
                .map(|&story_id| StoryInfoView {
                    story_id,
                    date: 0,
                    is_for_close_friends: false,
                    is_live: false,
                })
                .collect(),
        }
    }

    #[test]
    fn counts_unread_by_max_read_story_id() {
        let ring = super::StoryRing::from_active(&view(&[3, 4, 5], 4)).unwrap();
        assert_eq!((ring.count, ring.unread), (3, 1));
        assert!(ring.has_unread());
        let read = super::StoryRing::from_active(&view(&[3, 4], 4)).unwrap();
        assert!(!read.has_unread());
        assert!(super::StoryRing::from_active(&view(&[], 0)).is_none());
    }

    #[test]
    fn single_story_is_a_full_circle() {
        let arcs = super::arcs(super::StoryRing {
            count: 1,
            unread: 1,
        });
        assert_eq!(arcs.len(), 1);
        assert_eq!(arcs[0].sweep, 360.0);
        assert!(arcs[0].unread);
    }

    #[test]
    fn several_stories_split_with_gaps_unread_first() {
        let arcs = super::arcs(super::StoryRing {
            count: 3,
            unread: 2,
        });
        assert_eq!(arcs.len(), 3);
        assert_eq!(
            arcs.iter().map(|a| a.unread).collect::<Vec<_>>(),
            [true, true, false]
        );
        // 3 gaps of 10 degrees, three equal arcs of 110.
        assert!((arcs[0].sweep - 110.0).abs() < 1e-3);
        assert!((arcs[0].start - 95.0).abs() < 1e-3);
        assert!((arcs[1].start - 215.0).abs() < 1e-3);
    }

    #[test]
    fn many_stories_shrink_the_gap() {
        let arcs = super::arcs(super::StoryRing {
            count: 50,
            unread: 0,
        });
        let total: f32 = arcs.iter().map(|a| a.sweep).sum();
        // Gaps take ~91% of the circle, arcs stay positive.
        assert!(total > 0.0 && total < 60.0);
    }

    #[test]
    fn avatar_shrinks_inside_the_ring() {
        // 46px photo -> 40px avatar, 3px of ring room per side.
        assert!((super::StoryRing::avatar_size(46.0) - 40.0).abs() < 1e-3);
    }
}
