//! Stages of a window's history that were loose fields of `HistoryUi`: which
//! window the scroller is anchored for, what the paint probe measured, and
//! the rows drawn with stale chrome. One value each
//! (docs/contributing/state-machines.md).

use gpui_kit::Pixels;
use quill::bubble_layout::HistoryWidth;
use quill::chat_wide::ChatLayout;
use quill::ids::MessageId;
use quill::state::ScrollKey;

/// The window of messages the scroller was last synced for, and whether its
/// opening position was placed yet. A new chat or a replaced window starts
/// `Anchoring` and the first render with rows places it (`Anchored`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum WindowAnchor {
    /// Nothing synced yet.
    #[default]
    Unsynced,
    /// Synced, waiting for rows to place the saved position, the jump
    /// target, the "Unread messages" bar or the bottom.
    Anchoring {
        key: ScrollKey,
        epoch: u64,
        landed: Option<(u64, MessageId)>,
    },
    Anchored {
        key: ScrollKey,
        epoch: u64,
        /// The last jump the scroller landed on, as `(jump serial,
        /// message)`: it lands once per jump, so a second jump to the same
        /// message lands again, and a highlight that stays set is not
        /// scrolled to each frame. Message ids are chat-local, so a new
        /// chat starts with none.
        landed: Option<(u64, MessageId)>,
    },
}

impl WindowAnchor {
    pub(super) fn key(&self) -> Option<ScrollKey> {
        match *self {
            Self::Unsynced => None,
            Self::Anchoring { key, .. } | Self::Anchored { key, .. } => Some(key),
        }
    }

    pub(super) fn epoch(&self) -> u64 {
        match *self {
            Self::Unsynced => 0,
            Self::Anchoring { epoch, .. } | Self::Anchored { epoch, .. } => epoch,
        }
    }

    pub(super) fn is_anchoring(&self) -> bool {
        matches!(self, Self::Anchoring { .. })
    }

    /// The same window as `key` and `epoch`, already placed.
    pub(super) fn is_placed_for(&self, key: ScrollKey, epoch: u64) -> bool {
        matches!(*self, Self::Anchored { key: k, epoch: e, .. } if k == key && e == epoch)
    }

    /// Syncs to `key` and `epoch`: a different chat or window starts
    /// anchoring again, keeping the landed jump only within one chat.
    /// Returns true when it did.
    pub(super) fn sync(&mut self, key: ScrollKey, epoch: u64) -> bool {
        if self.key() == Some(key) && self.epoch() == epoch {
            return false;
        }
        let landed = if self.key() == Some(key) {
            self.landed()
        } else {
            None
        };
        *self = Self::Anchoring { key, epoch, landed };
        true
    }

    /// The rows are in and the opening position is placed.
    pub(super) fn placed(&mut self) {
        if let Self::Anchoring { key, epoch, landed } = *self {
            *self = Self::Anchored { key, epoch, landed };
        }
    }

    pub(super) fn landed(&self) -> Option<(u64, MessageId)> {
        match *self {
            Self::Unsynced => None,
            Self::Anchoring { landed, .. } | Self::Anchored { landed, .. } => landed,
        }
    }

    pub(super) fn set_landed(&mut self, jump: Option<(u64, MessageId)>) {
        match self {
            Self::Unsynced => {}
            Self::Anchoring { landed, .. } | Self::Anchored { landed, .. } => *landed = jump,
        }
    }
}

/// What the history knows of its own width: the pane it is laid out in
/// before it has painted, then what the paint probe measured. One value, so a
/// measurement always replaces the seed whole.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum HistoryMeasure {
    /// Not painted yet: bubbles are laid out for the pane it will fill
    /// (`seed_history_width`).
    Seeded(HistoryWidth),
    Measured {
        /// The width as bubble layout reads it: a narrow history narrows the
        /// bubbles (`bubble_width::BubbleRoom`).
        width: HistoryWidth,
        /// The width class at the last paint (tdesktop `ChatLayout`).
        layout: ChatLayout,
        /// The width at paint, before `HistoryWidth` tops it off: rows still
        /// move in a wide window that resizes.
        pane: Pixels,
    },
}

impl Default for HistoryMeasure {
    fn default() -> Self {
        Self::Seeded(HistoryWidth::default())
    }
}

impl HistoryMeasure {
    pub(crate) fn width(&self) -> HistoryWidth {
        match *self {
            Self::Seeded(width) | Self::Measured { width, .. } => width,
        }
    }

    /// Normal until the history has painted.
    pub(crate) fn layout(&self) -> ChatLayout {
        match *self {
            Self::Seeded(_) => ChatLayout::default(),
            Self::Measured { layout, .. } => layout,
        }
    }

    pub(crate) fn pane(&self) -> Pixels {
        match *self {
            Self::Seeded(_) => Pixels::default(),
            Self::Measured { pane, .. } => pane,
        }
    }

    pub(crate) fn is_measured(&self) -> bool {
        matches!(self, Self::Measured { .. })
    }
}

/// Rows drawn with stale chrome, and whether the notify that renders them
/// again is on its way. The list draws rows while the window draws, when a
/// notify is not heard, so it is sent after the frame, once for all rows.
#[derive(Debug, Default)]
pub(super) enum ChromeDue {
    #[default]
    Idle,
    /// Rows wait for the next render; no notify is on its way (it landed, or
    /// the render that skips the kept-rows branch left them undrained).
    Due(Vec<usize>),
    /// A deferred notify is on its way for these rows.
    Notifying(Vec<usize>),
}

impl ChromeDue {
    /// Notes a stale row. Returns true when the caller must schedule the
    /// notify; false when the row is already noted or a notify is coming.
    pub(super) fn note(&mut self, ix: usize) -> bool {
        match self {
            Self::Due(rows) | Self::Notifying(rows) if rows.contains(&ix) => false,
            Self::Notifying(rows) => {
                rows.push(ix);
                false
            }
            Self::Idle => {
                *self = Self::Notifying(vec![ix]);
                true
            }
            Self::Due(rows) => {
                let mut rows = std::mem::take(rows);
                rows.push(ix);
                *self = Self::Notifying(rows);
                true
            }
        }
    }

    /// The rows are being rendered (or the list rebuilt): hands them over.
    pub(super) fn take(&mut self) -> Vec<usize> {
        match self {
            Self::Idle => Vec::new(),
            Self::Due(rows) => {
                let rows = std::mem::take(rows);
                *self = Self::Idle;
                rows
            }
            Self::Notifying(rows) => std::mem::take(rows),
        }
    }

    /// The notify went out (or its window is gone): none is on its way.
    pub(super) fn notified(&mut self) {
        if let Self::Notifying(rows) = self {
            let rows = std::mem::take(rows);
            *self = if rows.is_empty() {
                Self::Idle
            } else {
                Self::Due(rows)
            };
        }
    }

    #[cfg(test)]
    pub(super) fn is_notifying(&self) -> bool {
        matches!(self, Self::Notifying(_))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: ScrollKey = (7, None, 0);

    #[test]
    fn a_window_anchors_once_and_a_replaced_one_anchors_again() {
        let mut anchor = WindowAnchor::default();
        assert!(anchor.sync(KEY, 1), "the first sync anchors");
        assert!(anchor.is_anchoring());
        assert!(!anchor.is_placed_for(KEY, 1));
        anchor.placed();
        assert!(anchor.is_placed_for(KEY, 1));
        assert!(!anchor.sync(KEY, 1), "the same window is left alone");
        assert!(anchor.sync(KEY, 2), "a replaced window anchors again");
        assert!(anchor.is_anchoring());
    }

    #[test]
    fn a_landed_jump_belongs_to_its_chat() {
        let mut anchor = WindowAnchor::default();
        anchor.sync(KEY, 1);
        anchor.placed();
        anchor.set_landed(Some((3, MessageId(9))));
        // A replaced window of the same chat keeps it: it lands once per jump.
        anchor.sync(KEY, 2);
        assert_eq!(anchor.landed(), Some((3, MessageId(9))));
        // Message ids are chat-local: another chat starts with none.
        anchor.sync((8, None, 0), 2);
        assert_eq!(anchor.landed(), None);
    }

    #[test]
    fn a_measured_history_replaces_its_seed_whole() {
        let seeded = HistoryMeasure::default();
        assert!(!seeded.is_measured());
        assert_eq!(seeded.layout(), ChatLayout::default());
        let measured = HistoryMeasure::Measured {
            width: HistoryWidth::new(400.),
            layout: ChatLayout::Wide,
            pane: Pixels::default(),
        };
        assert!(measured.is_measured());
        assert_eq!(measured.layout(), ChatLayout::Wide);
        assert_eq!(measured.width(), HistoryWidth::new(400.));
    }

    #[test]
    fn a_chrome_notify_is_asked_for_once_and_ends_with_the_render() {
        let mut due = ChromeDue::default();
        assert!(due.note(3), "the first stale row schedules the notify");
        assert!(!due.note(3), "the same row is not noted twice");
        assert!(!due.note(5), "a notify is already on its way");
        assert!(due.is_notifying());
        // The notify lands: the rows wait for the render, no notify is out.
        due.notified();
        assert!(!due.is_notifying());
        assert!(due.note(6), "a row noted after it schedules the next one");
        due.notified();
        assert_eq!(due.take(), vec![3, 5, 6]);
        assert!(!due.is_notifying());
        assert!(due.take().is_empty());
    }

    #[test]
    fn a_rebuild_that_empties_the_rows_does_not_strand_the_notify() {
        let mut due = ChromeDue::default();
        due.note(1);
        // The rows were rebuilt before the notify landed.
        assert_eq!(due.take(), vec![1]);
        assert!(due.is_notifying(), "the notify is still on its way");
        due.notified();
        assert!(!due.is_notifying());
        assert!(due.note(2), "the next stale row asks for a notify again");
    }
}
