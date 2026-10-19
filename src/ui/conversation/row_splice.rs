//! Messages that came or went (a new message, a deletion, a pending send
//! that got its server id) rebuild only the rows around them: the rows
//! that held or neighbour the changed messages are built again from what
//! the rows above hand down, and the rest of the list is kept.
#![deny(clippy::indexing_slicing)]

use super::row_build::{RowBuild, RowSeed, identity};
use super::*;
use crate::ui::history_row::{HistoryRow, RowChrome};

/// Rows past which a change builds the whole list again instead.
const SPLICE_ROWS_MAX: usize = 64;

/// Old rows `start..end` to build again from `messages` (row order: the
/// history's messages oldest first, then the rows built after them).
pub(in crate::ui) struct RowSplice {
    start: usize,
    end: usize,
    messages: Vec<HistoryMessage>,
}

/// The rows built for a `RowSplice`, ready to go in.
pub(super) struct SplicedRows {
    start: usize,
    /// End of the old rows they replace: past the splice's own end when a
    /// row below changed with it (its day separator or unread divider).
    end: usize,
    rows: Vec<HistoryRow>,
    /// Kept rows near the viewport with their chrome derived again, at
    /// their index after the splice.
    stale: Vec<(usize, Option<RowChrome>)>,
    cloned: u64,
}

impl QuillApp {
    /// The rows to build again for the messages of `list` stored, changed
    /// or removed after `since`. `tail`: rows the list builds after the
    /// messages (the joined notice). `None` when the whole list is built
    /// again instead: too many changes, changes far apart, or a streaming
    /// bot reply below the rows.
    pub(super) fn row_splice(
        &self,
        list: &quill::state::HistoryMessages,
        instance: u64,
        since: u64,
        translated: Option<ChatId>,
        tail: usize,
    ) -> Option<RowSplice> {
        // A crumbling message is merged into the rows on a full build only.
        // In the app, `conversation` pins the dissolve clock once per frame,
        // so this reads the instant the key was made at. Tests that call
        // this directly pin it themselves (`pin_vanish_now`).
        if self.stream_rows_hash().is_some() || self.vanish_rows_hash().is_some() {
            return None;
        }
        let changes = list.changes_since(instance, since)?;
        if changes.changed.len() + changes.removed.len() > super::rows_cache::PATCH_MAX {
            return None;
        }
        let mut affected: Vec<i64> = changes
            .changed
            .iter()
            .chain(&changes.removed)
            .copied()
            .collect();
        affected.sort_unstable();
        affected.dedup();
        let (&low, &high) = (affected.first()?, affected.last()?);
        let rows = &self.history.rows;
        // The message rows are in id order; the tail rows follow them.
        let message_rows = rows.len().checked_sub(tail)?;
        let by_id = rows.get(..message_rows)?;
        let first_hit = by_id.partition_point(|row| row.last_id().is_some_and(|id| id.0 < low));
        let past_hit = by_id.partition_point(|row| row.first_id().is_some_and(|id| id.0 <= high));
        // One kept row on each side: the row above learns who is below it,
        // the row below who is above it.
        let start = first_hit.saturating_sub(1);
        let end = past_hit.saturating_add(1).min(rows.len());
        if end.saturating_sub(start) > SPLICE_ROWS_MAX {
            return None;
        }
        let mut messages = Vec::new();
        let mut tail_messages = Vec::new();
        for (ix, row) in (start..).zip(rows.get(start..end)?) {
            let kept = row
                .messages()
                .iter()
                .filter(|message| affected.binary_search(&message.id.0).is_err())
                .cloned();
            if ix < message_rows {
                messages.extend(kept);
            } else {
                tail_messages.extend(kept);
            }
        }
        let mut changed: Vec<HistoryMessage> = changes
            .changed
            .iter()
            .filter_map(|id| list.get(id).cloned())
            .collect();
        if let Some(chat_id) = translated {
            self.apply_chat_translation(chat_id, &mut changed);
        }
        messages.extend(changed);
        messages.sort_by_key(|message| message.id.0);
        messages.extend(tail_messages);
        Some(RowSplice {
            start,
            end,
            messages,
        })
    }

    /// Build the rows of `splice`, and each row below whose place changed
    /// with them, until the rows below take the same from above as
    /// before. Also derives the chrome of the kept rows near the viewport.
    pub(super) fn splice_rows(&self, splice: RowSplice, build: &RowBuild) -> SplicedRows {
        let old = &self.history.rows;
        let RowSplice {
            start,
            mut end,
            messages,
        } = splice;
        let mut cloned = messages.len() as u64;
        let below = |ix: usize| old.get(ix).and_then(identity);
        let (mut rows, mut seed) =
            self.build_rows(messages, RowSeed::before(old, start), below(end), build);
        while let Some(row) = old.get(end) {
            if seed.carries_like(&RowSeed::before(old, end)) {
                break;
            }
            let messages = row.messages().to_vec();
            cloned += messages.len() as u64;
            let (more, next) = self.build_rows(messages, seed, below(end + 1), build);
            rows.extend(more);
            seed = next;
            end += 1;
        }
        let new_len = rows.len();
        let moved = |ix: usize| {
            if ix < start {
                Some(ix)
            } else if ix >= end {
                Some(ix - end + start + new_len)
            } else {
                None
            }
        };
        let due: Vec<usize> = self
            .chrome_due_rows()
            .into_iter()
            .filter(|ix| moved(*ix).is_some())
            .collect();
        let stale = build
            .chrome
            .stale_rows(old, &due, &[], build.epoch)
            .into_iter()
            .filter_map(|(ix, chrome)| Some((moved(ix)?, chrome)))
            .collect();
        SplicedRows {
            start,
            end,
            rows,
            stale,
            cloned,
        }
    }

    /// Put spliced rows in, tell the scroller which rows came, went or
    /// changed, and bring the kept rows' chrome up to date.
    pub(super) fn commit_spliced_rows(
        &mut self,
        spliced: SplicedRows,
        mark: super::row_chrome::ChromeMark,
        started: std::time::Instant,
        cx: &mut Context<Self>,
    ) {
        let SplicedRows {
            start,
            end,
            rows,
            stale,
            cloned,
        } = spliced;
        if start > end || end > self.history.rows.len() {
            // Not the rows the splice was made for: build them whole.
            self.history.rows_key = None;
            cx.notify();
            return;
        }
        let new_len = rows.len();
        let replaced: Vec<HistoryRow> = self.history.rows.splice(start..end, rows).collect();
        self.sync_spliced_scroller(start, &replaced, new_len, cx);
        self.history.row_stats.messages_cloned += cloned;
        self.history.row_stats.note_spliced(new_len, started);
        self.apply_row_patches(Vec::new(), stale, mark, started, cx);
    }

    /// The scroller's side of a splice: rows `start..start + new_len`
    /// replaced `replaced`.
    fn sync_spliced_scroller(
        &mut self,
        start: usize,
        replaced: &[HistoryRow],
        new_len: usize,
        cx: &mut Context<Self>,
    ) {
        let count = self.history.rows.len();
        let old_len = replaced.len();
        let prev_count = (count + old_len).saturating_sub(new_len);
        let listed = self.history.scroller.read(cx).item_count();
        let follow = self.history.scroller.read(cx).is_following_tail();
        if listed != prev_count {
            // Out of step with the rows: take the new list whole.
            self.history.scroller.update(cx, |state, cx| {
                state.splice(0..listed, count, cx);
                if follow {
                    state.scroll_to_end(cx);
                }
            });
            return;
        }
        let fresh = self
            .history
            .rows
            .get(start..start + new_len)
            .unwrap_or_default();
        // Rows that kept their index but not their look.
        let changed: Vec<usize> = fresh
            .iter()
            .zip(replaced)
            .enumerate()
            .filter(|(_, (new, old))| !new.renders_like(old))
            .map(|(offset, _)| start + offset)
            .collect();
        let appended = start + old_len == prev_count
            && new_len > old_len
            && fresh
                .iter()
                .zip(replaced)
                .all(|(new, old)| new.first_id() == old.first_id());
        if new_len == old_len || appended {
            if appended {
                let outgoing = fresh
                    .get(old_len..)
                    .is_some_and(|added| added.iter().any(super::chat_palette::row_is_outgoing));
                self.append_tail_rows(prev_count, count, outgoing, cx);
            }
            if !changed.is_empty() {
                self.history.scroller.update(cx, |state, cx| {
                    for ix in changed {
                        let _ = state.remeasure_items(ix..ix + 1, cx);
                    }
                });
            }
            return;
        }
        // A row came or went inside the list: the splice keeps the scroll
        // anchor; stay at the tail only if the user was following it.
        self.history.scroller.update(cx, |state, cx| {
            state.splice(start..start + old_len, new_len, cx);
            if follow {
                state.scroll_to_end(cx);
            }
        });
    }
}
