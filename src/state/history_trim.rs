//! R6: cap on the open chat's loaded history window.
//!
//! Paging far back through a huge chat only ever grew the window. Like
//! Telegram Desktop (which keeps a bounded run of messages around the
//! viewport and re-fetches on demand), the window drops its *far* end once
//! it passes [`HISTORY_WINDOW_CAP`] messages:
//!
//! - an older page landed (the viewport is at the oldest end): the newest
//!   messages go, so the window stops short of the latest and scrolling back
//!   down pages them in again through the normal newer-page path;
//! - a newer page landed (the viewport is at the newest end): the oldest
//!   messages go and `loaded_complete` clears, so scrolling up pages them
//!   in again.
use super::*;

/// Messages kept in one chat's window before the far end is trimmed.
pub const HISTORY_WINDOW_CAP: usize = 1500;
/// A trim shrinks the window to this size (hysteresis: it is not
/// re-trimmed on every following page).
pub const HISTORY_WINDOW_TRIM_TO: usize = 1200;
const ROWS_KEPT_ABOVE_READER: usize = 200;

/// Which end of the window a trim drops.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowEnd {
    Oldest,
    Newest,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct LiveTrimSpan {
    oldest: usize,
    newest: usize,
}

fn is_local(message: &HistoryMessage) -> bool {
    message.pending || message.failed
}

/// Up to `count` server ids from the `end` of `messages`, local sends
/// skipped.
fn end_excess(messages: &HistoryMessages, end: WindowEnd, count: usize) -> Vec<i64> {
    let server = |(id, message): (&i64, &HistoryMessage)| (!is_local(message)).then_some(*id);
    match end {
        WindowEnd::Oldest => messages.iter().filter_map(server).take(count).collect(),
        WindowEnd::Newest => messages
            .iter()
            .rev()
            .filter_map(server)
            .take(count)
            .collect(),
    }
}

fn rows_above(messages: &HistoryMessages, reader: ReaderPosition) -> Option<usize> {
    match reader {
        ReaderPosition::Tail => None,
        ReaderPosition::Away { top } => Some(messages.range(..top.0).count()),
    }
}

fn live_trim_span(len: usize, rows_above_reader: Option<usize>) -> LiveTrimSpan {
    if len <= HISTORY_WINDOW_CAP {
        return LiveTrimSpan::default();
    }
    let excess = len - HISTORY_WINDOW_TRIM_TO;
    let oldest = rows_above_reader.map_or(excess, |above| {
        above.saturating_sub(ROWS_KEPT_ABOVE_READER).min(excess)
    });
    LiveTrimSpan {
        oldest,
        newest: excess - oldest,
    }
}

impl HistoryState {
    /// Drop the `end` of the window when it holds more than `cap`
    /// messages, shrinking it to `keep`. Returns how many were removed.
    ///
    /// Pending and failed local sends stay and the server rows around them
    /// go: a temporary id is not a server position, so it never pages back
    /// in, and a failed send can sit in the window for as long as it is open.
    pub(crate) fn trim_window_end(&mut self, end: WindowEnd, cap: usize, keep: usize) -> usize {
        let len = self.messages.len();
        if len <= cap || keep >= len {
            return 0;
        }
        let doomed = end_excess(&self.messages, end, len - keep);
        match end {
            WindowEnd::Newest => {
                // Remember where the chat's tail is even if no incoming
                // row is left in the window to say so.
                let newest = doomed.iter().copied().max().unwrap_or(0);
                self.latest_seen = self.latest_seen.max(newest);
                self.newer_failed = false;
            }
            WindowEnd::Oldest => {
                self.loaded_complete = false;
            }
        }
        self.drop_unread_bar_for(&doomed);
        for id in &doomed {
            self.messages.remove(id);
            self.visible.remove(id);
            self.viewed.remove(id);
        }
        doomed.len()
    }

    fn trim_live(&mut self) -> usize {
        let len = self.messages.len();
        let span = live_trim_span(len, rows_above(&self.messages, self.reader));
        let dropped = self.trim_window_end(WindowEnd::Oldest, 0, len - span.oldest);
        dropped + self.trim_window_end(WindowEnd::Newest, 0, len - dropped - span.newest)
    }
}

impl TopicHistory {
    fn trim_live(&mut self) -> usize {
        let len = self.messages.len();
        let span = live_trim_span(len, rows_above(&self.messages, self.reader));
        self.trim_to_tail(len - span.oldest)
    }
}

impl ThreadView {
    fn trim_replies(&mut self, end: WindowEnd, count: usize) -> usize {
        let roots = &self.root_ids;
        let replies = self
            .history
            .messages
            .keys()
            .copied()
            .filter(|id| *id > 0 && !roots.contains(id));
        let messages = &self.history.messages;
        let server = |id: &i64| messages.get(id).is_some_and(|message| !is_local(message));
        let doomed: Vec<i64> = match end {
            WindowEnd::Oldest => replies.filter(server).take(count).collect(),
            WindowEnd::Newest => replies.rev().filter(server).take(count).collect(),
        };
        for id in &doomed {
            self.history.messages.remove(id);
        }
        if doomed.is_empty() {
            return 0;
        }
        match end {
            WindowEnd::Oldest => {
                self.history.loaded_complete = false;
                let oldest = self
                    .history
                    .messages
                    .keys()
                    .copied()
                    .find(|id| *id > 0 && !self.root_ids.contains(id));
                if let Some(oldest) = oldest {
                    self.history.next_from_message_id = MessageId(oldest);
                }
            }
            WindowEnd::Newest => {
                let newest = doomed.iter().copied().max().unwrap_or(0);
                self.last_message_id = self.last_message_id.max(newest);
                self.newer_failed = false;
                self.refresh_has_newer();
            }
        }
        doomed.len()
    }

    pub(crate) fn trim_after_page(&mut self, far_end: WindowEnd) -> usize {
        match self.history.messages.len().checked_sub(HISTORY_WINDOW_CAP) {
            Some(1..) => {
                let excess = self.history.messages.len() - HISTORY_WINDOW_TRIM_TO;
                self.trim_replies(far_end, excess)
            }
            _ => 0,
        }
    }

    pub(crate) fn trim_live(&mut self) -> Vec<WindowEnd> {
        let history = &self.history;
        let span = live_trim_span(
            history.messages.len(),
            rows_above(&history.messages, history.reader),
        );
        let mut moved = Vec::new();
        for (end, count) in [
            (WindowEnd::Oldest, span.oldest),
            (WindowEnd::Newest, span.newest),
        ] {
            if self.trim_replies(end, count) > 0 {
                moved.push(end);
            }
        }
        moved
    }
}

impl Session {
    /// Apply the window cap to `chat_id`'s main history after a page
    /// landed at the opposite `end`'s far side (see the module docs).
    pub(crate) fn trim_history_window(&mut self, chat_id: ChatId, end: WindowEnd) -> usize {
        let dropped = self.histories.get_mut(&chat_id.0).map_or(0, |history| {
            history.trim_window_end(end, HISTORY_WINDOW_CAP, HISTORY_WINDOW_TRIM_TO)
        });
        if dropped > 0 {
            self.prune_reply_targets(chat_id);
        }
        dropped
    }

    pub(crate) fn trim_live_windows(&mut self, chat_id: ChatId, topic_id: Option<i32>) {
        if self.chat_is_shown(chat_id)
            && let Some(history) = self.histories.get_mut(&chat_id.0)
            && history.trim_live() > 0
        {
            self.prune_reply_targets(chat_id);
        }
        if let Some(topic) =
            topic_id.and_then(|topic| self.threads.topic_histories.get_mut(&(chat_id.0, topic)))
            && topic.trim_live() > 0
        {
            self.prune_reply_targets(chat_id);
        }
    }

    pub fn note_history_reader(
        &mut self,
        (chat, topic, thread): ScrollKey,
        position: ReaderPosition,
    ) {
        let reader = if thread != 0 {
            self.threads
                .thread
                .as_mut()
                .filter(|view| view.chat_id.0 == chat && view.thread_id == thread)
                .map(|view| &mut view.history.reader)
        } else if let Some(topic) = topic {
            self.threads
                .topic_histories
                .get_mut(&(chat, topic))
                .map(|history| &mut history.reader)
        } else {
            self.histories
                .get_mut(&chat)
                .map(|history| &mut history.reader)
        };
        if let Some(reader) = reader {
            *reader = position;
        }
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
    let front_added = new
        .iter()
        .take_while(|(_, last)| *last < kept.first().map_or(i64::MAX, |k| k.0))
        .count();
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
        HISTORY_WINDOW_CAP, HISTORY_WINDOW_TRIM_TO, LiveTrimSpan, ROWS_KEPT_ABOVE_READER,
        RowWindowShift, WindowEnd, live_trim_span, row_window_shift,
    };
    use crate::ids::{ChatId, MessageId};
    use crate::state::{HistoryMessage, HistoryState};
    use crate::telegram::envelope::MessageContent;
    use std::collections::HashSet;

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
        assert!(!state.stops_short_of(0));
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
        assert!(state.stops_short_of(0));
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
        assert!(!state.stops_short_of(0));
    }

    #[test]
    fn a_trim_keeps_a_pending_send_and_drops_the_server_rows_around_it() {
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
            401,
            "the server rows around it go"
        );
        assert_eq!(state.newest_id(), Some(MessageId(1_199)));
        assert!(state.contains(MessageId(1_700)));
    }

    #[test]
    fn trimming_keeps_tombstones_and_forgets_what_was_viewed() {
        let mut state = window(1..=1600);
        state.remove(MessageId(5), true);
        state.viewed.extend([100, 1_500]);
        state.trim_window_end(
            WindowEnd::Newest,
            HISTORY_WINDOW_CAP,
            HISTORY_WINDOW_TRIM_TO,
        );
        assert!(state.is_tombstone(MessageId(5)));
        assert_eq!(state.viewed, HashSet::from([100]));
    }

    #[test]
    fn a_live_trim_spares_the_rows_around_the_reader() {
        let cap = HISTORY_WINDOW_CAP;
        let excess = cap + 1 - HISTORY_WINDOW_TRIM_TO;
        let span = |oldest, newest| LiveTrimSpan { oldest, newest };
        assert_eq!(live_trim_span(cap, None), span(0, 0), "within the cap");
        assert_eq!(
            live_trim_span(cap + 1, None),
            span(excess, 0),
            "at the tail"
        );
        assert_eq!(
            live_trim_span(cap + 1, Some(0)),
            span(0, excess),
            "at the top"
        );
        assert_eq!(
            live_trim_span(cap + 1, Some(ROWS_KEPT_ABOVE_READER + 10)),
            span(10, excess - 10),
            "rows past the margin go, then the newest"
        );
        assert_eq!(
            live_trim_span(cap + 1, Some(cap - 3)),
            span(excess, 0),
            "near the bottom, away from the tail"
        );
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
        assert!(state.stops_short_of(0));
        state.reset_window();
        assert!(state.messages.is_empty());
        assert!(!state.stops_short_of(0));
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
