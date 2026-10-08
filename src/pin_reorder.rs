//! Pinned-chat drag reorder, ported from tdesktop
//! (`dialogs/dialogs_inner_widget.cpp`: `updateReorderPinned` 2851-2937,
//! `pinnedShiftAnimationCallback` 2939-2989, `finishReorderOnRelease`
//! 2800-2815).
//!
//! The dragged row follows the pointer. Whenever the pointer crosses a
//! neighbour's threshold the two swap slots live; the displaced row starts
//! offset by the dragged row's height and slides home over 200 ms with a
//! sine ease (`st::stickersRowDuration`, `anim::sineInOut`). On release the
//! dragged row slides into its slot the same way and the final order is
//! saved (`setPinnedChats`). Pure timing/geometry; the UI passes `now`.
//!
//! tdesktop does not animate anything else in the chat list: a chat moving
//! up because of a new message jumps (no `_rowsAnimation` exists), so Quill
//! does not invent one.

use std::collections::HashMap;
use std::f32::consts::PI;
use std::time::{Duration, Instant};

/// `kStartReorderThreshold`: vertical px before a press starts a reorder.
pub const START_THRESHOLD: f32 = 30.0;

/// `st::stickersRowDuration`.
pub const SLIDE: Duration = Duration::from_millis(200);

#[derive(Debug, Clone, Copy)]
struct Slide {
    /// Offset (px) at `at`; eases to 0.
    from: f32,
    at: Instant,
}

impl Slide {
    fn value(&self, now: Instant) -> f32 {
        let t = (now.saturating_duration_since(self.at).as_secs_f32() / SLIDE.as_secs_f32())
            .clamp(0.0, 1.0);
        self.from * (1.0 - sine_in_out(t))
    }

    fn done(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.at) >= SLIDE
    }
}

/// `anim::sineInOut`.
fn sine_in_out(t: f32) -> f32 {
    -((PI * t).cos() - 1.0) / 2.0
}

#[derive(Debug, Clone)]
pub struct PinReorder {
    ids: Vec<i64>,
    heights: HashMap<i64, f32>,
    dragging: Option<i64>,
    start_y: f32,
    cur_y: f32,
    slides: HashMap<i64, Slide>,
}

impl PinReorder {
    /// Start dragging `dragging` from pointer position `y`. Needs at least
    /// two pinned chats (`updateReorderIndexGetCount() < 2` cancels).
    pub fn begin(ids: Vec<i64>, heights: HashMap<i64, f32>, dragging: i64, y: f32) -> Option<Self> {
        if ids.len() < 2 || !ids.contains(&dragging) {
            return None;
        }
        Some(Self {
            ids,
            heights,
            dragging: Some(dragging),
            start_y: y,
            cur_y: y,
            slides: HashMap::new(),
        })
    }

    pub fn ids(&self) -> &[i64] {
        &self.ids
    }

    pub fn dragging(&self) -> Option<i64> {
        self.dragging
    }

    fn height(&self, id: i64) -> f32 {
        self.heights.get(&id).copied().unwrap_or(64.0)
    }

    /// Pointer moved to `y`: swap past any neighbour whose threshold the
    /// drag crossed (`updateReorderPinned`).
    pub fn drag_to(&mut self, y: f32, now: Instant) {
        let Some(dragging) = self.dragging else {
            return;
        };
        self.cur_y = y;
        let Some(mut index) = self.ids.iter().position(|id| *id == dragging) else {
            return;
        };
        let drag_height = self.height(dragging);
        let cross = |height: f32| (drag_height / 2.0).max(height - drag_height / 2.0);
        let mut shift_height = 0.0;
        if self.start_y > y && index > 0 {
            let mut delta = self.start_y - y;
            while index > 0 {
                let neighbour = self.ids[index - 1];
                let height = self.height(neighbour);
                if delta < cross(height) {
                    break;
                }
                delta -= height;
                self.ids.swap(index, index - 1);
                let from = self.offset(neighbour, now) - drag_height;
                self.slides.insert(neighbour, Slide { from, at: now });
                shift_height -= height;
                index -= 1;
            }
        } else if self.start_y < y && index + 1 < self.ids.len() {
            let mut delta = y - self.start_y;
            while index + 1 < self.ids.len() {
                let neighbour = self.ids[index + 1];
                let height = self.height(neighbour);
                if delta < cross(height) {
                    break;
                }
                delta -= height;
                self.ids.swap(index, index + 1);
                let from = self.offset(neighbour, now) + drag_height;
                self.slides.insert(neighbour, Slide { from, at: now });
                shift_height += height;
                index += 1;
            }
        }
        self.start_y += shift_height;
    }

    /// Pointer released: the dragged row slides into its slot. Returns the
    /// final order to save.
    pub fn release(&mut self, now: Instant) -> Vec<i64> {
        if let Some(dragging) = self.dragging.take() {
            let from = self.cur_y - self.start_y;
            self.slides.insert(dragging, Slide { from, at: now });
        }
        self.ids.clone()
    }

    /// Vertical offset (px) to draw `id` at right now.
    pub fn offset(&self, id: i64, now: Instant) -> f32 {
        if self.dragging == Some(id) {
            return self.cur_y - self.start_y;
        }
        self.slides.get(&id).map_or(0.0, |slide| slide.value(now))
    }

    /// Still dragging, or a slide is running (the list needs frames).
    pub fn animating(&self, now: Instant) -> bool {
        self.dragging.is_some() || self.slides.values().any(|slide| !slide.done(now))
    }

    /// Released and every slide finished: the state can be dropped.
    pub fn settled(&self, now: Instant) -> bool {
        !self.animating(now)
    }

    /// Position of `id` in the live order, for sorting the visible rows.
    pub fn position(&self, id: i64) -> Option<usize> {
        self.ids.iter().position(|x| *x == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(ids: &[i64]) -> (Vec<i64>, HashMap<i64, f32>) {
        (ids.to_vec(), ids.iter().map(|id| (*id, 60.0)).collect())
    }

    #[test]
    fn needs_two_pinned_chats() {
        let (ids, heights) = rows(&[1]);
        assert!(PinReorder::begin(ids, heights, 1, 0.0).is_none());
    }

    #[test]
    fn small_drag_keeps_order_and_follows_pointer() {
        let (ids, heights) = rows(&[1, 2, 3]);
        let now = Instant::now();
        let mut drag = PinReorder::begin(ids, heights, 2, 100.0).unwrap();
        drag.drag_to(110.0, now);
        assert_eq!(drag.ids(), [1, 2, 3]);
        assert_eq!(drag.offset(2, now), 10.0);
        assert_eq!(drag.offset(3, now), 0.0);
    }

    #[test]
    fn crossing_the_threshold_swaps_down_and_slides_the_neighbour() {
        let (ids, heights) = rows(&[1, 2, 3]);
        let t0 = Instant::now();
        let mut drag = PinReorder::begin(ids, heights, 1, 0.0).unwrap();
        // Threshold: max(30, 60 - 30) = 30.
        drag.drag_to(29.0, t0);
        assert_eq!(drag.ids(), [1, 2, 3]);
        drag.drag_to(31.0, t0);
        assert_eq!(drag.ids(), [2, 1, 3]);
        // The displaced row starts one row height below its new slot... no:
        // it was above, so it starts 60px up and eases home.
        assert_eq!(drag.offset(2, t0), 60.0);
        // The dragged row keeps its on-screen place: 31 - (0 + 60) = -29.
        assert!((drag.offset(1, t0) - (-29.0)).abs() < 1e-3);
        let later = t0 + Duration::from_millis(100);
        let mid = drag.offset(2, later);
        assert!((mid - 30.0).abs() < 1e-3, "sine ease is half-way at 100ms");
        let done = t0 + SLIDE;
        assert_eq!(drag.offset(2, done), 0.0);
        assert!(drag.animating(done), "still dragging");
    }

    #[test]
    fn crossing_up_swaps_with_the_row_above() {
        let (ids, heights) = rows(&[1, 2, 3]);
        let t0 = Instant::now();
        let mut drag = PinReorder::begin(ids, heights, 3, 200.0).unwrap();
        drag.drag_to(160.0, t0);
        assert_eq!(drag.ids(), [1, 3, 2]);
        assert_eq!(drag.offset(2, t0), -60.0);
    }

    #[test]
    fn one_long_drag_crosses_several_rows() {
        let (ids, heights) = rows(&[1, 2, 3, 4]);
        let t0 = Instant::now();
        let mut drag = PinReorder::begin(ids, heights, 1, 0.0).unwrap();
        drag.drag_to(150.0, t0);
        assert_eq!(drag.ids(), [2, 3, 4, 1]);
    }

    #[test]
    fn release_slides_home_then_settles() {
        let (ids, heights) = rows(&[1, 2]);
        let t0 = Instant::now();
        let mut drag = PinReorder::begin(ids, heights, 1, 0.0).unwrap();
        drag.drag_to(20.0, t0);
        let order = drag.release(t0);
        assert_eq!(order, [1, 2]);
        assert_eq!(drag.offset(1, t0), 20.0);
        assert!(drag.animating(t0));
        let end = t0 + SLIDE;
        assert_eq!(drag.offset(1, end), 0.0);
        assert!(drag.settled(end));
    }
}
