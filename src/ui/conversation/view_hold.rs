//! The rows on screen stay put while rows come or go around them (tdesktop
//! `HistoryWidget::updateHistoryGeometry` keeps the top item in place).
//! Before the rows change, the history notes the message at the view's top
//! and how far into its row the view is; after, it scrolls that message's
//! row back to the same offset, wherever the change moved it. A history
//! following its latest message keeps following, except over a page of
//! newer history: that page joins below what the reader sees. A message the
//! reader just sent brings the history to its end (tdesktop
//! `HistoryWidget::fastShowAtEnd`).

use super::*;

/// What the view keeps across a change of rows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct ViewHold {
    /// The row holding this message starts this many pixels above the
    /// view's top. `None` at the latest message: rows added below keep the
    /// view at the bottom.
    top: Option<(MessageId, f32)>,
    /// The newest message, when the rows reach the chat's latest: a message
    /// the reader sends joins after it. `None` for a window that stops short
    /// of the latest, where rows come only by pages.
    newest: Option<MessageId>,
}

impl QuillApp {
    /// Note what the view shows before the rows change.
    pub(super) fn capture_view_hold(&self, cx: &App) -> ViewHold {
        let rows = &self.history.rows;
        let state = self.history.scroller.read(cx);
        let top = state.logical_scroll_top();
        let reaches_latest = !self.history.had_newer;
        let following = state.is_following_tail() && reaches_latest;
        ViewHold {
            top: rows
                .get(top.item_ix)
                .and_then(HistoryRow::first_id)
                .filter(|_| !following)
                .map(|message| (message, f32::from(top.offset_in_item))),
            newest: reaches_latest
                .then(|| messages_newest_first(rows).next().map(|m| m.id))
                .flatten(),
        }
    }

    /// Put the view back where `hold` found it, now that the rows changed.
    pub(super) fn apply_view_hold(&mut self, hold: ViewHold, cx: &mut Context<Self>) {
        if hold.newest.is_some_and(|newest| self.sent_since(newest)) {
            self.end_landing();
            self.history
                .scroller
                .update(cx, |state, cx| state.scroll_to_end(cx));
            return;
        }
        let Some((message, offset)) = hold.top else {
            return;
        };
        let rows = &self.history.rows;
        let target = match rows.iter().position(|row| row.contains(message)) {
            Some(ix) => Some((ix, offset)),
            // The top message went: the next one takes its place.
            None => rows
                .iter()
                .position(|row| row.first_id().is_some_and(|id| id.0 > message.0))
                .map(|ix| (ix, 0.)),
        };
        self.history.scroller.update(cx, |state, cx| match target {
            Some((ix, offset)) => {
                state.scroll_to_item_offset(ix, offset, cx);
            }
            None => state.scroll_to_end(cx),
        });
    }

    /// A message after `newest` is one this app is still sending: the
    /// reader just sent it.
    fn sent_since(&self, newest: MessageId) -> bool {
        messages_newest_first(&self.history.rows)
            .take_while(|message| message.id.0 > newest.0)
            .any(|message| message.is_outgoing && message.pending() && !message.failed())
    }
}

/// The rows' messages, newest first, without the local ones below zero (a
/// bot reply still streaming sits after the newest message).
fn messages_newest_first(rows: &[HistoryRow]) -> impl Iterator<Item = &HistoryMessage> {
    rows.iter()
        .rev()
        .flat_map(|row| row.messages().iter().rev())
        .filter(|message| message.id.0 > 0)
}
