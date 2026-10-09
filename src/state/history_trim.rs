//! R6: cap on the open chat's loaded history window.
//!
//! Paging far back through a huge chat only ever grew the window. Like
//! Telegram Desktop (which keeps a bounded run of messages around the
//! viewport and re-fetches on demand), the window drops its *far* end once
//! it passes [`HISTORY_WINDOW_CAP`] messages:
//!
//! - an older page landed (the viewport is at the oldest end): the newest
//!   messages go and the window is marked `has_newer`, so scrolling back
//!   down pages them in again through the normal newer-page path;
//! - a newer page landed (the viewport is at the newest end): the oldest
//!   messages go and `loaded_complete` clears, so scrolling up pages them
//!   in again.
//!
//! The trim is page-driven only. Live `updateNewMessage` appends never
//! trim: a user reading the oldest loaded rows while new messages arrive
//! must not have them vanish. A trim never changes the end the viewport is
//! on, so scroll anchoring needs no reset and `window_epoch` is
//! deliberately not bumped (that would re-anchor to the bottom / unread
//! divider and yank the reader); every path that really replaces the window
//! (`reset_window`) still bumps it.
use super::*;

/// Messages kept in one chat's window before the far end is trimmed.
pub const HISTORY_WINDOW_CAP: usize = 1500;
/// A trim shrinks the window to this size (hysteresis: it is not
/// re-trimmed on every following page).
pub const HISTORY_WINDOW_TRIM_TO: usize = 1200;

/// Which end of the window a trim drops.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowEnd {
    Oldest,
    Newest,
}

impl HistoryState {
    /// Drop the `end` of the window when it holds more than `cap`
    /// messages, shrinking it to `keep`. Returns how many were removed.
    ///
    /// Nothing is trimmed when the doomed run holds a pending or failed
    /// local send (its temporary id is not a server position; it must
    /// stay visible until `updateMessageSendSucceeded`); the trim happens
    /// on a later page instead.
    pub(crate) fn trim_window_end(&mut self, end: WindowEnd, cap: usize, keep: usize) -> usize {
        let len = self.messages.len();
        if len <= cap || keep >= len {
            return 0;
        }
        let excess = len - keep;
        let doomed: Vec<i64> = match end {
            WindowEnd::Oldest => self.messages.keys().take(excess).copied().collect(),
            WindowEnd::Newest => self.messages.keys().rev().take(excess).copied().collect(),
        };
        if doomed.iter().any(|id| {
            self.messages
                .get(id)
                .is_some_and(|message| message.pending || message.failed)
        }) {
            return 0;
        }
        match end {
            WindowEnd::Newest => {
                // Remember where the chat's tail is even if no incoming
                // row is left in the window to say so.
                let newest = doomed.iter().copied().max().unwrap_or(0);
                self.latest_seen = self.latest_seen.max(newest);
                self.has_newer = true;
                self.newer_failed = false;
            }
            WindowEnd::Oldest => {
                self.loaded_complete = false;
            }
        }
        for id in &doomed {
            self.messages.remove(id);
            self.visible.remove(id);
        }
        doomed.len()
    }
}

impl Session {
    /// Apply the window cap to `chat_id`'s main history after a page
    /// landed at the opposite `end`'s far side (see the module docs).
    pub(crate) fn trim_history_window(&mut self, chat_id: ChatId, end: WindowEnd) -> usize {
        self.histories.get_mut(&chat_id.0).map_or(0, |history| {
            history.trim_window_end(end, HISTORY_WINDOW_CAP, HISTORY_WINDOW_TRIM_TO)
        })
    }
}

/// How one history rows list became the next one, when the change is a
/// pure slide of the window (rows dropped and/or added at its ends).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RowWindowShift {
    /// Old rows gone from the front / back.
    pub front_removed: usize,
    pub tail_removed: usize,
    /// New rows added at the front / back.
    pub front_added: usize,
    pub tail_added: usize,
}

/// Compare the previous and next row lists, each row given as its
/// `(first_id, last_id)` message ids in ascending order. Returns the shift
/// when the surviving rows are one contiguous identical run in both
/// lists, `None` for anything else (reorders, in-place edits that change
/// ids, disjoint lists) so callers fall back to a full splice.
pub fn row_window_shift(old: &[(i64, i64)], new: &[(i64, i64)]) -> Option<RowWindowShift> {
    let (&(new_first, _), &(_, new_last)) = (new.first()?, new.last()?);
    let front_removed = old.iter().take_while(|(_, last)| *last < new_first).count();
    let tail_removed = old
        .iter()
        .rev()
        .take_while(|(first, _)| *first > new_last)
        .count();
    let kept_end = old.len().checked_sub(tail_removed)?;
    let kept = old.get(front_removed..kept_end).filter(|k| !k.is_empty())?;
    let front_added = new.iter().take_while(|(_, last)| *last < kept[0].0).count();
    let run = new.get(front_added..front_added + kept.len())?;
    if run != kept {
        return None;
    }
    Some(RowWindowShift {
        front_removed,
        tail_removed,
        front_added,
        tail_added: new.len() - front_added - kept.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::{
        HISTORY_WINDOW_CAP, HISTORY_WINDOW_TRIM_TO, RowWindowShift, WindowEnd, row_window_shift,
    };
    use crate::ids::{ChatId, MessageId};
    use crate::state::{HistoryMessage, HistoryState};
    use crate::telegram::envelope::MessageContent;

    fn message(id: i64) -> HistoryMessage {
        HistoryMessage {
            sender: None,
            id: MessageId(id),
            chat_id: ChatId(7),
            is_outgoing: false,
            date: 0,
            content: MessageContent::Unsupported {
                type_name: "test".into(),
            },
            pending: false,
            reply_to: None,
            forward_info: None,
            extras: Default::default(),
            interaction_info: None,
            is_pinned: false,
            media_album_id: 0,
            reply_markup: None,
            self_destruct: None,
            auto_delete: None,
            author_signature: None,
            failed: false,
            can_retry: false,
            ephemeral: None,
        }
    }

    fn window(ids: std::ops::RangeInclusive<i64>) -> HistoryState {
        let mut state = HistoryState::default();
        for id in ids {
            state.upsert(message(id));
        }
        state
    }

    #[test]
    fn small_windows_are_untouched() {
        let mut state = window(1..=100);
        assert_eq!(
            state.trim_window_end(
                WindowEnd::Newest,
                HISTORY_WINDOW_CAP,
                HISTORY_WINDOW_TRIM_TO
            ),
            0
        );
        assert_eq!(state.messages.len(), 100);
        assert!(!state.has_newer);
    }

    #[test]
    fn scrolling_back_trims_the_newest_end_and_marks_has_newer() {
        let mut state = window(1..=1600);
        state.latest_seen = 0;
        let removed = state.trim_window_end(
            WindowEnd::Newest,
            HISTORY_WINDOW_CAP,
            HISTORY_WINDOW_TRIM_TO,
        );
        assert_eq!(removed, 400);
        assert_eq!(state.messages.len(), HISTORY_WINDOW_TRIM_TO);
        // The oldest rows (where the reader is) are untouched.
        assert_eq!(state.oldest_id(), Some(MessageId(1)));
        assert_eq!(state.newest_id(), Some(MessageId(1200)));
        assert!(state.has_newer);
        assert!(!state.newer_failed);
        // The dropped tail is remembered so has_newer survives a refresh.
        assert_eq!(state.latest_seen, 1600);
        // The window is still one contiguous run.
        let ids: Vec<i64> = state.messages.keys().copied().collect();
        assert!(ids.windows(2).all(|pair| pair[1] == pair[0] + 1));
    }

    #[test]
    fn scrolling_forward_trims_the_oldest_end_and_clears_loaded_complete() {
        let mut state = window(1..=1600);
        state.loaded_complete = true;
        let removed = state.trim_window_end(
            WindowEnd::Oldest,
            HISTORY_WINDOW_CAP,
            HISTORY_WINDOW_TRIM_TO,
        );
        assert_eq!(removed, 400);
        assert_eq!(state.oldest_id(), Some(MessageId(401)));
        assert_eq!(state.newest_id(), Some(MessageId(1600)));
        assert!(!state.loaded_complete);
        assert!(!state.has_newer);
    }

    #[test]
    fn a_trim_never_drops_a_pending_or_failed_send() {
        let mut state = window(1..=1600);
        let mut sending = message(1_700);
        sending.pending = true;
        sending.is_outgoing = true;
        state.upsert(sending);
        assert_eq!(
            state.trim_window_end(
                WindowEnd::Newest,
                HISTORY_WINDOW_CAP,
                HISTORY_WINDOW_TRIM_TO
            ),
            0
        );
        assert!(!state.has_newer);
        assert!(state.contains(MessageId(1_700)));
        // The oldest end has no pending rows, so that trim goes ahead.
        assert!(
            state.trim_window_end(
                WindowEnd::Oldest,
                HISTORY_WINDOW_CAP,
                HISTORY_WINDOW_TRIM_TO
            ) > 0
        );
        assert!(state.contains(MessageId(1_700)));
    }

    #[test]
    fn trimming_keeps_tombstones_and_viewed_bookkeeping() {
        let mut state = window(1..=1600);
        state.remove(MessageId(5), true);
        state.viewed.insert(1_500);
        state.trim_window_end(
            WindowEnd::Newest,
            HISTORY_WINDOW_CAP,
            HISTORY_WINDOW_TRIM_TO,
        );
        assert!(state.is_tombstone(MessageId(5)));
        assert!(state.viewed.contains(&1_500));
    }

    #[test]
    fn trim_does_not_bump_the_window_epoch() {
        let mut state = window(1..=1600);
        let epoch = state.window_epoch;
        state.trim_window_end(
            WindowEnd::Newest,
            HISTORY_WINDOW_CAP,
            HISTORY_WINDOW_TRIM_TO,
        );
        assert_eq!(state.window_epoch, epoch);
        // A real window replacement still bumps it.
        state.reset_window();
        assert_eq!(state.window_epoch, epoch + 1);
    }

    #[test]
    fn jumping_back_to_latest_replaces_a_trimmed_window() {
        // A trimmed window is `has_newer`, so the existing jump-to-latest
        // path (reset_window) empties it and the latest page refetches.
        let mut state = window(1..=1600);
        state.trim_window_end(
            WindowEnd::Newest,
            HISTORY_WINDOW_CAP,
            HISTORY_WINDOW_TRIM_TO,
        );
        assert!(state.has_newer);
        state.reset_window();
        assert!(state.messages.is_empty());
        assert!(!state.has_newer);
        assert!(!state.loaded_complete);
    }

    fn rows(ids: std::ops::RangeInclusive<i64>) -> Vec<(i64, i64)> {
        ids.map(|id| (id, id)).collect()
    }

    #[test]
    fn row_shift_detects_tail_trim_after_an_older_page() {
        // 100..=199 loaded; older 50..=99 prepended; 180..=199 trimmed.
        let shift = row_window_shift(&rows(100..=199), &rows(50..=179)).expect("slide");
        assert_eq!(
            shift,
            RowWindowShift {
                front_removed: 0,
                tail_removed: 20,
                front_added: 50,
                tail_added: 0,
            }
        );
    }

    #[test]
    fn row_shift_detects_front_trim_after_a_newer_page() {
        let shift = row_window_shift(&rows(100..=199), &rows(120..=249)).expect("slide");
        assert_eq!(
            shift,
            RowWindowShift {
                front_removed: 20,
                tail_removed: 0,
                front_added: 0,
                tail_added: 50,
            }
        );
    }

    #[test]
    fn row_shift_handles_album_rows_by_id_range() {
        let old = vec![(1, 1), (2, 4), (5, 5), (6, 6)];
        let new = vec![(2, 4), (5, 5), (6, 6), (7, 7)];
        let shift = row_window_shift(&old, &new).expect("slide");
        assert_eq!(shift.front_removed, 1);
        assert_eq!(shift.tail_added, 1);
        assert_eq!(shift.tail_removed + shift.front_added, 0);
    }

    #[test]
    fn row_shift_rejects_reorders_and_disjoint_lists() {
        assert_eq!(row_window_shift(&rows(1..=5), &rows(10..=15)), None);
        // An interior row vanished (a delete): not a pure slide.
        let mut gap = rows(1..=10);
        gap.remove(4);
        assert_eq!(row_window_shift(&rows(1..=10), &gap), None);
        assert_eq!(row_window_shift(&[], &rows(1..=3)), None);
        assert_eq!(row_window_shift(&rows(1..=3), &[]), None);
    }
}
