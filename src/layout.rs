//! Mixed-height history layout with a stable visible anchor.
//!
//! GPUI Kit's `MessageScroller` measures rows in the live app. This module is the
//! CI-testable model of prepend / resize / follow-tail behavior.

use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RowId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Anchor {
    pub row: RowId,
    pub offset_px: f32,
}

#[derive(Debug, Clone)]
pub struct HistoryLayout {
    pub ids: Vec<RowId>,
    heights: HashMap<RowId, f32>,
    pub follow_tail: bool,
    pub viewport_px: f32,
    /// Scroll offset from the top of the first row, in pixels.
    pub scroll_top: f32,
}

impl HistoryLayout {
    pub fn new(viewport_px: f32) -> Self {
        Self {
            ids: Vec::new(),
            heights: HashMap::new(),
            follow_tail: true,
            viewport_px,
            scroll_top: 0.0,
        }
    }

    pub fn height_of(&self, id: RowId) -> f32 {
        *self.heights.get(&id).unwrap_or(&0.0)
    }

    pub fn content_height(&self) -> f32 {
        self.ids.iter().map(|id| self.height_of(*id)).sum()
    }

    pub fn visible_anchor(&self) -> Option<Anchor> {
        let mut y = 0.0;
        for id in &self.ids {
            let h = self.height_of(*id);
            let next = y + h;
            if next > self.scroll_top + 0.5 {
                return Some(Anchor {
                    row: *id,
                    offset_px: self.scroll_top - y,
                });
            }
            y = next;
        }
        self.ids.last().map(|id| Anchor {
            row: *id,
            offset_px: 0.0,
        })
    }

    pub fn restore_anchor(&mut self, anchor: Anchor) {
        let mut y = 0.0;
        for id in &self.ids {
            if *id == anchor.row {
                self.scroll_top = (y + anchor.offset_px).max(0.0);
                self.clamp_scroll();
                return;
            }
            y += self.height_of(*id);
        }
    }

    fn clamp_scroll(&mut self) {
        let max = (self.content_height() - self.viewport_px).max(0.0);
        if self.scroll_top > max {
            self.scroll_top = max;
        }
        if self.scroll_top < 0.0 {
            self.scroll_top = 0.0;
        }
    }

    pub fn append(&mut self, id: RowId, height: f32) {
        self.ids.push(id);
        self.heights.insert(id, height);
        if self.follow_tail {
            self.scroll_to_end();
        }
    }

    pub fn prepend(&mut self, id: RowId, height: f32) {
        let anchor = self.visible_anchor();
        self.ids.insert(0, id);
        self.heights.insert(id, height);
        if let Some(anchor) = anchor {
            self.restore_anchor(anchor);
        }
    }

    pub fn resize(&mut self, id: RowId, height: f32) {
        let anchor = if self.follow_tail {
            None
        } else {
            self.visible_anchor()
        };
        self.heights.insert(id, height);
        if self.follow_tail {
            self.scroll_to_end();
        } else if let Some(anchor) = anchor {
            self.restore_anchor(anchor);
        }
    }

    pub fn scroll_to_end(&mut self) {
        self.follow_tail = true;
        self.scroll_top = (self.content_height() - self.viewport_px).max(0.0);
    }

    pub fn scroll_away_from_tail(&mut self, scroll_top: f32) {
        self.follow_tail = false;
        self.scroll_top = scroll_top;
        self.clamp_scroll();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(n: u64) -> RowId {
        RowId(n)
    }

    #[test]
    fn prepend_preserves_visible_row() {
        let mut layout = HistoryLayout::new(100.0);
        layout.append(id(1), 40.0);
        layout.append(id(2), 80.0);
        layout.append(id(3), 40.0);
        layout.scroll_away_from_tail(40.0);
        let before = layout.visible_anchor().unwrap();
        assert_eq!(before.row, id(2));
        layout.prepend(id(0), 60.0);
        let after = layout.visible_anchor().unwrap();
        assert_eq!(after.row, id(2));
        assert!((after.offset_px - before.offset_px).abs() < 0.01);
    }

    #[test]
    fn image_resize_does_not_jump_when_not_following() {
        let mut layout = HistoryLayout::new(100.0);
        layout.append(id(1), 40.0);
        layout.append(id(2), 40.0);
        layout.append(id(3), 200.0);
        layout.scroll_away_from_tail(40.0);
        let before = layout.visible_anchor().unwrap();
        layout.resize(id(3), 400.0);
        let after = layout.visible_anchor().unwrap();
        assert_eq!(after.row, before.row);
        assert!((after.offset_px - before.offset_px).abs() < 0.01);
    }

    #[test]
    fn following_tail_sticks_after_new_message() {
        let mut layout = HistoryLayout::new(80.0);
        layout.append(id(1), 40.0);
        layout.append(id(2), 40.0);
        layout.append(id(3), 40.0);
        assert!(layout.follow_tail);
        let top_before_end = layout.scroll_top;
        layout.append(id(4), 40.0);
        assert!(layout.scroll_top >= top_before_end);
        assert!((layout.scroll_top - (layout.content_height() - 80.0)).abs() < 0.01);
    }
}
