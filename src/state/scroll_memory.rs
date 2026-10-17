//! Where each chat was left when it is not read to the tail.
//!
//! Telegram Desktop keeps `History::scrollTopItem` / `scrollTopOffset` for
//! every history: they are counted on each scroll, forgotten while the view
//! sits at the bottom (`forgetScrollState`) and when an explicit target (a
//! jump to a message) opens the chat, and read back when the chat is shown
//! again at its default position. This is the same rule over message ids
//! and pixel offsets, kept in memory for the running account.
use super::*;
use std::collections::{HashMap, VecDeque};

/// Positions kept before the least recently saved one goes.
pub const SCROLL_MEMORY_CAP: usize = 300;

/// A history view: chat, forum topic, reply thread (`0` for none).
pub type ScrollKey = (i64, Option<i32>, i64);

/// The top visible row of a history: the message it starts with and how
/// far its top edge is above the viewport top.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SavedScroll {
    pub anchor: MessageId,
    pub offset: f32,
}

#[derive(Debug, Default)]
pub struct ScrollMemory {
    saved: HashMap<ScrollKey, SavedScroll>,
    order: VecDeque<ScrollKey>,
}

impl ScrollMemory {
    /// Remember `scroll` for `key`; `None` is the view at the tail (or
    /// with nothing to anchor to) and forgets it.
    pub fn record(&mut self, key: ScrollKey, scroll: Option<SavedScroll>) {
        let Some(scroll) = scroll else {
            self.forget(key);
            return;
        };
        if self.saved.get(&key) == Some(&scroll) {
            return;
        }
        if self.saved.insert(key, scroll).is_some() {
            if self.order.back() == Some(&key) {
                return;
            }
            self.order.retain(|queued| *queued != key);
        }
        self.order.push_back(key);
        while self.order.len() > SCROLL_MEMORY_CAP {
            if let Some(oldest) = self.order.pop_front() {
                self.saved.remove(&oldest);
            }
        }
    }

    pub fn forget(&mut self, key: ScrollKey) {
        if self.saved.remove(&key).is_some() {
            self.order.retain(|queued| *queued != key);
        }
    }

    pub fn get(&self, key: ScrollKey) -> Option<SavedScroll> {
        self.saved.get(&key).copied()
    }

    pub fn clear(&mut self) {
        self.saved.clear();
        self.order.clear();
    }

    pub fn len(&self) -> usize {
        self.saved.len()
    }

    pub fn is_empty(&self) -> bool {
        self.saved.is_empty()
    }
}

/// A remembered position waiting for its message to load. Any move the
/// reader makes ends the wait, so the position never pulls them back later.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RestoreWait {
    pending: bool,
    seen: u64,
}

impl RestoreWait {
    /// Start waiting; `interactions` is the scroller's reader-scroll count now.
    pub fn arm(&mut self, interactions: u64) {
        self.pending = true;
        self.seen = interactions;
    }

    pub fn is_pending(&self) -> bool {
        self.pending
    }

    /// The wait is over: restored, replaced by a jump, or abandoned.
    pub fn finish(&mut self) {
        self.pending = false;
    }

    /// End the wait when the reader scrolled (wheel, trackpad, keyboard, a
    /// send or jump to latest counted by the scroller) or holds the scrollbar.
    pub fn observe(&mut self, interactions: u64, scrollbar_dragging: bool) {
        if self.pending && (interactions != self.seen || scrollbar_dragging) {
            self.pending = false;
        }
    }
}

/// What opening a history does with its remembered position.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ScrollOpen {
    /// Start at the remembered row.
    Restore(SavedScroll),
    /// A jump target is set: it wins and the old position is dropped.
    Jump,
    /// Nothing remembered (or its message is not in the window).
    Default,
}

/// Decide how a history starts. `row_of` finds the row holding a message in
/// the loaded window.
pub fn scroll_open(
    saved: Option<SavedScroll>,
    explicit_jump: bool,
    row_of: impl Fn(MessageId) -> Option<usize>,
) -> (ScrollOpen, Option<usize>) {
    if explicit_jump {
        return (ScrollOpen::Jump, None);
    }
    match saved.and_then(|scroll| row_of(scroll.anchor).map(|row| (scroll, row))) {
        Some((scroll, row)) => (ScrollOpen::Restore(scroll), Some(row)),
        None => (ScrollOpen::Default, None),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        RestoreWait, SCROLL_MEMORY_CAP, SavedScroll, ScrollMemory, ScrollOpen, scroll_open,
    };
    use crate::ids::MessageId;

    fn at(id: i64, offset: f32) -> Option<SavedScroll> {
        Some(SavedScroll {
            anchor: MessageId(id),
            offset,
        })
    }

    #[test]
    fn leaving_mid_history_is_remembered_and_the_tail_is_forgotten() {
        let mut memory = ScrollMemory::default();
        memory.record((1, None, 0), at(40, 12.));
        assert_eq!(memory.get((1, None, 0)), at(40, 12.));
        assert_eq!(memory.get((2, None, 0)), None);
        memory.record((1, None, 0), None);
        assert_eq!(memory.get((1, None, 0)), None);
        assert!(memory.is_empty());
    }

    #[test]
    fn topics_and_threads_are_separate_views() {
        let mut memory = ScrollMemory::default();
        memory.record((1, None, 0), at(1, 0.));
        memory.record((1, Some(5), 0), at(2, 0.));
        memory.record((1, None, 9), at(3, 0.));
        assert_eq!(memory.len(), 3);
        assert_eq!(memory.get((1, Some(5), 0)), at(2, 0.));
    }

    #[test]
    fn the_oldest_saved_position_goes_past_the_cap() {
        let mut memory = ScrollMemory::default();
        for chat in 0..SCROLL_MEMORY_CAP as i64 + 5 {
            memory.record((chat, None, 0), at(chat, 0.));
        }
        assert_eq!(memory.len(), SCROLL_MEMORY_CAP);
        assert_eq!(memory.get((0, None, 0)), None);
        assert_eq!(memory.get((4, None, 0)), None);
        assert!(memory.get((5, None, 0)).is_some());
    }

    #[test]
    fn saving_again_refreshes_instead_of_duplicating() {
        let mut memory = ScrollMemory::default();
        memory.record((1, None, 0), at(1, 0.));
        memory.record((2, None, 0), at(2, 0.));
        memory.record((1, None, 0), at(3, 4.));
        for chat in 10..10 + SCROLL_MEMORY_CAP as i64 - 1 {
            memory.record((chat, None, 0), at(chat, 0.));
        }
        assert_eq!(memory.get((2, None, 0)), None);
        assert_eq!(memory.get((1, None, 0)), at(3, 4.));
        memory.clear();
        assert!(memory.is_empty());
    }

    #[test]
    fn opening_restores_when_the_anchor_is_loaded() {
        let (open, row) = scroll_open(at(40, 12.), false, |id| (id == MessageId(40)).then_some(7));
        assert!(matches!(
            open,
            ScrollOpen::Restore(SavedScroll {
                anchor: MessageId(40),
                ..
            })
        ));
        assert_eq!(row, Some(7));
    }

    #[test]
    fn an_explicit_jump_beats_the_remembered_position() {
        let (open, row) = scroll_open(at(40, 12.), true, |_| Some(7));
        assert_eq!((open, row), (ScrollOpen::Jump, None));
    }

    #[test]
    fn an_anchor_outside_the_window_falls_back() {
        let (open, row) = scroll_open(at(40, 12.), false, |_| None);
        assert_eq!((open, row), (ScrollOpen::Default, None));
        let (open, _) = scroll_open(None, false, |_| Some(1));
        assert_eq!(open, ScrollOpen::Default);
    }

    #[test]
    fn a_reader_move_ends_the_wait_for_a_late_page() {
        let mut wait = RestoreWait::default();
        wait.arm(4);
        wait.observe(4, false);
        assert!(wait.is_pending());
        wait.observe(5, false);
        assert!(!wait.is_pending());
    }

    #[test]
    fn holding_the_scrollbar_ends_the_wait() {
        let mut wait = RestoreWait::default();
        wait.arm(0);
        wait.observe(0, true);
        assert!(!wait.is_pending());
        // Once ended it stays ended, and recording resumes.
        wait.observe(0, false);
        assert!(!wait.is_pending());
    }

    #[test]
    fn leaving_the_late_restore_unarmed_never_pends() {
        let mut wait = RestoreWait::default();
        wait.observe(9, true);
        assert!(!wait.is_pending());
    }

    #[test]
    fn sending_or_jumping_to_latest_forgets_the_position() {
        let mut memory = ScrollMemory::default();
        let mut wait = RestoreWait::default();
        memory.record((1, None, 0), at(40, 12.));
        wait.arm(0);
        // The reader sends: the scroller counts it, the entry goes.
        memory.forget((1, None, 0));
        wait.observe(1, false);
        assert_eq!(memory.get((1, None, 0)), None);
        assert!(!wait.is_pending());
    }
}
