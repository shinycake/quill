//! Where each history was left, and where an opened history starts
//! (`docs/decisions/sweep-scroll-restore.md`).

use super::*;
use quill::state::{HistoryOpen, SavedScroll, ScrollKey, ScrollOpen};

impl QuillApp {
    /// Where `chat_id` opens from the chat list, search or a link. An
    /// explicit target wins and drops the saved scroll position; a chat
    /// opened at its default position loads around the saved one, so its
    /// first rows already hold it. Re-selecting the open chat keeps its
    /// live view.
    pub(in crate::ui) fn chat_open_at(
        &mut self,
        chat_id: ChatId,
        explicit: Option<MessageId>,
    ) -> HistoryOpen {
        let reselect = self.session().and_then(|s| s.open_chat) == Some(chat_id);
        if reselect && explicit.is_none() {
            return HistoryOpen::Default;
        }
        self.history
            .scroll_memory
            .open((chat_id.0, None, 0), explicit)
    }

    /// The reader sent a message or jumped to the latest: the position left
    /// behind is not where the chat reopens.
    pub(in crate::ui) fn forget_scroll_on_return_to_latest(&mut self) {
        if let Some(key) = self.history.key {
            self.history.scroll_memory.forget(key);
        }
    }

    /// Keep the top row and its offset for the open history while the
    /// reader is away from the tail (tdesktop `countScrollState`); at the
    /// tail the position is forgotten (`forgetScrollState`). Nothing is
    /// recorded before the opened history has anchored.
    pub(in crate::ui) fn remember_scroll(&mut self, key: ScrollKey, cx: &App) {
        if self.history.key != Some(key)
            || self.history.anchor_pending
            || self.history.rows.is_empty()
        {
            return;
        }
        let saved = self
            .history
            .scroller
            .read(cx)
            .top_anchor()
            .and_then(|(ix, offset)| {
                let anchor = self.history.rows.get(ix)?.first_id()?;
                Some(SavedScroll { anchor, offset })
            });
        self.history.scroll_memory.record(key, saved);
    }

    /// The first rows of a new or replaced window are in: start at the
    /// saved position (its window was loaded around it), else the jump
    /// target, else a little above the "Unread messages" divider, else the
    /// bottom. A jump target or a saved message that is not there drops
    /// the saved position.
    pub(super) fn anchor_opened_history(
        &mut self,
        key: ScrollKey,
        highlight_id: Option<MessageId>,
        cx: &mut Context<Self>,
    ) {
        let rows = &self.history.rows;
        let row_of = |id: MessageId| rows.iter().position(|row| row.contains(id));
        let highlighted = highlight_id.and_then(row_of);
        if highlighted.is_some() {
            self.history.last_highlight = highlight_id;
        }
        let open = quill::state::scroll_open(
            self.history.scroll_memory.get(key),
            highlight_id.is_some(),
            row_of,
        );
        if open.forgets() {
            self.history.scroll_memory.forget(key);
        }
        let bar = rows.iter().position(HistoryRow::unread_divider);
        self.history.bar_anchor.set(None);
        match (open, highlighted, bar) {
            (ScrollOpen::Restore { row, offset }, _, _) => {
                self.history.scroller.update(cx, |state, cx| {
                    state.scroll_to_item_offset(row, offset, cx);
                });
            }
            (_, Some(ix), _) => {
                self.history.scroller.update(cx, |state, cx| {
                    state.scroll_to_item(ix, cx);
                });
            }
            (_, None, Some(ix)) => self.anchor_at_unread_bar(ix, cx),
            _ => self
                .history
                .scroller
                .update(cx, |state, cx| state.scroll_to_end(cx)),
        }
    }
}
