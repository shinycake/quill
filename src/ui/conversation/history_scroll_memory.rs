//! Where each history was left, and where an opened history starts
//! (`docs/decisions/sweep-scroll-restore.md`).

use super::landing::LandingGoal;
use super::*;
use quill::state::{HistoryOpen, ReaderPosition, SavedScroll, ScrollKey, ScrollOpen};

impl QuillApp {
    /// Where `chat_id` opens from the chat list, search or a link: at an
    /// explicit target, else at its first unread message or the bottom.
    /// Either way the position it was left at is dropped. Re-selecting the
    /// open chat keeps its window (`reselect_open_chat` moves the view).
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

    /// The open chat chosen again from the list (tdesktop
    /// `HistoryWidget::showHistory` for the peer it shows): to the first
    /// unread message while that is still below the view
    /// (`insideJumpToEndInsteadOfToUnread`), else to the latest.
    pub(in crate::ui) fn reselect_open_chat(&mut self, cx: &mut Context<Self>) {
        let Some(session) = self.session() else {
            return;
        };
        let Some(chat) = session.open_chat else {
            return;
        };
        // Not landed yet (tdesktop answers "to the unread" before
        // `_historyInited`): the open in progress goes there already.
        if session.history_loading(chat)
            || self.history.anchor_pending
            || self.history.key.is_none_or(|key| key.0 != chat.0)
        {
            return;
        }
        if self.first_unread_below_view(chat) {
            self.jump_down_messages(cx);
        } else {
            self.jump_to_latest_messages(cx);
        }
    }

    /// The open chat's first unread message starts below the view, or past
    /// the loaded window (tdesktop `insideJumpToEndInsteadOfToUnread`:
    /// once it is on screen or above, going "to the unread" goes to the
    /// end instead).
    pub(in crate::ui) fn first_unread_below_view(&self, chat: ChatId) -> bool {
        let Some(session) = self.session() else {
            return false;
        };
        let anchor = session
            .chats
            .get(&chat.0)
            .and_then(quill::state::ChatSummary::unread_anchor);
        let rows = &self.history.rows;
        anchor.is_some_and(|anchor| {
            let first_unread = rows.iter().position(|row| {
                row.messages()
                    .iter()
                    .any(|message| !message.is_outgoing && message.id.0 > anchor.0)
            });
            match first_unread {
                Some(ix) => self
                    .history
                    .scroll_view_probe
                    .get()
                    .is_some_and(|(_, last)| ix > last),
                // The window ends before it.
                None => rows
                    .iter()
                    .rev()
                    .find_map(HistoryRow::last_id)
                    .is_none_or(|id| id.0 <= anchor.0),
            }
        })
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
        if let Some(session) = self.session_mut() {
            let reader = saved.map_or(ReaderPosition::Tail, |scroll| ReaderPosition::Away {
                top: scroll.anchor,
            });
            session.note_history_reader(key, reader);
        }
    }

    /// The first rows of a new or replaced window are in: start at the
    /// saved position (left behind when a thread opened over this history),
    /// else the jump target in the middle of the view, else the "Unread
    /// messages" bar at the top, else the bottom. A jump target or a saved
    /// message that is not there drops the saved position.
    pub(super) fn anchor_opened_history(
        &mut self,
        key: ScrollKey,
        highlight_id: Option<MessageId>,
        cx: &mut Context<Self>,
    ) {
        let rows = &self.history.rows;
        let row_of = |id: MessageId| rows.iter().position(|row| row.contains(id));
        let highlighted = highlight_id.filter(|id| row_of(*id).is_some());
        if highlighted.is_some() {
            self.history.last_highlight = highlighted;
        }
        let open = quill::state::scroll_open(
            self.history.scroll_memory.get(key),
            highlight_id.is_some(),
            row_of,
        );
        if open.forgets() {
            self.history.scroll_memory.forget(key);
        }
        self.end_landing();
        if let ScrollOpen::Restore { row, offset } = open {
            self.history.scroller.update(cx, |state, cx| {
                state.scroll_to_item_offset(row, offset, cx);
            });
            return;
        }
        if let Some(target) = highlighted {
            self.start_landing(LandingGoal::Message(target), cx);
        } else if !self.start_landing(LandingGoal::Bar, cx) {
            self.history
                .scroller
                .update(cx, |state, cx| state.scroll_to_end(cx));
        }
    }
}
