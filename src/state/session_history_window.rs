//! The open chat's loaded history window: one contiguous run of messages,
//! opened at the first unread message when there is one, paged in both
//! directions, and replaced (never patched) when a jump lands outside it.
use super::*;

/// The edge of the history window a page request extends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WindowEdge {
    /// The page that starts the window: the latest messages, or the run
    /// around a message.
    First,
    /// The page below the oldest loaded message, asked from it.
    Older(i64),
    /// The page from the newest loaded message on, asked from it.
    Newer(i64),
}

/// Every request that fills the main history window.
const WINDOW_PURPOSES: [RequestPurpose; 3] = [
    RequestPurpose::GetHistory,
    RequestPurpose::GetHistoryAround,
    RequestPurpose::GetHistoryNewer,
];

impl ChatSummary {
    /// The last read incoming message, when the chat has unread messages
    /// after it: where an opened chat should start reading.
    pub fn unread_anchor(&self) -> Option<MessageId> {
        let last_read = self.last_read_inbox_message_id;
        (self.unread_count > 0
            && last_read.0 > 0
            && self
                .last_message
                .as_ref()
                .is_none_or(|last| last.id.0 > last_read.0))
        .then_some(last_read)
    }
}

/// The composer's `@` member suggestions for one chat and query.
#[derive(Debug, Clone, PartialEq)]
pub struct MentionSearch {
    pub chat_id: ChatId,
    pub query: String,
    /// Matching members (users only, excluding the current user), in
    /// TDLib's order.
    pub user_ids: Vec<i64>,
    /// The in-flight `searchChatMembers`; answers for any other request
    /// (an older query) are dropped.
    pub request: Option<RequestId>,
}

impl Session {
    /// Apply a `searchChatMembers` answer to the current `@` search.
    pub(crate) fn apply_mention_members(
        &mut self,
        request: RequestId,
        members: &[crate::telegram::envelope::ParsedChatMember],
    ) {
        let me = self.my_user_id;
        let Some(search) = self
            .messages
            .mention_search
            .as_mut()
            .filter(|s| s.request == Some(request))
        else {
            return;
        };
        search.request = None;
        search.user_ids = members
            .iter()
            .filter_map(|member| match member.member_id {
                MessageSender::User { user_id } if Some(user_id) != me => Some(user_id),
                _ => None,
            })
            .collect();
    }

    /// The chat's first history page failed or timed out and the window is
    /// still empty: the UI offers "Couldn't load messages · Retry".
    pub fn history_load_failed(&self, chat_id: ChatId) -> bool {
        self.histories
            .get(&chat_id.0)
            .is_some_and(|h| h.load_failed && h.messages.is_empty())
    }

    /// The open history has nothing to show yet because its first page is
    /// still on the way (no entry yet, or an empty window with a page in
    /// flight) — the UI shows a skeleton rather than "No messages".
    pub fn history_loading(&self, chat_id: ChatId) -> bool {
        match self.histories.get(&chat_id.0) {
            None => true,
            Some(history) => {
                history.messages.is_empty()
                    && !history.load_failed
                    && self.history_page_in_flight(chat_id, WindowEdge::First)
            }
        }
    }

    /// What Up in an empty composer edits (tdesktop `HistoryWidget`
    /// `keyPressEvent`): when the last message of the chat is still being
    /// sent, a media message opens its caption (the new caption goes out
    /// with the upload) and anything else leaves Up to scroll; otherwise
    /// the newest own message that can be edited in the composer.
    /// `None` when the loaded window doesn't reach the latest messages: its
    /// newest own message might not be the last one.
    pub fn last_editable_message(&self, chat_id: ChatId) -> Option<crate::composer::ComposerEdit> {
        if self.history_has_newer(chat_id) {
            return None;
        }
        let history = self.histories.get(&chat_id.0)?;
        if let Some(last) = history.messages.values().next_back()
            && last.pending
            && !last.failed
        {
            return crate::composer::ComposerEdit::from_pending_media(
                chat_id,
                last.id,
                &last.content,
            )
            .filter(|_| last.is_outgoing);
        }
        history
            .messages
            .values()
            .rev()
            // `HistoryItem::canBeEdited`: forwarded and via-bot messages
            // are skipped, the scan goes on to older ones.
            .filter(|message| {
                message.is_outgoing
                    && !message.pending
                    && !message.failed
                    && message.forward_info.is_none()
                    && message.extras.via_bot_user_id == 0
            })
            .find_map(|message| {
                crate::composer::ComposerEdit::from_own_content(
                    chat_id,
                    message.id,
                    message.is_outgoing,
                    message.pending,
                    &message.content,
                )
                .map(|edit| edit.in_album(message.media_album_id != 0))
            })
    }

    /// Replace `chat_id`'s loaded window with an empty one. In-flight pages
    /// for the old window are marked stale and dropped when they answer.
    pub fn reset_history_window(&mut self, chat_id: ChatId) {
        for id in self.requests.ids_for_chat(&WINDOW_PURPOSES, chat_id) {
            self.stale_history_requests.insert(id.0);
        }
        self.histories.entry(chat_id.0).or_default().reset_window();
    }

    /// R5: drop sweepable requests that never got an answer (see
    /// [`RequestRegistry::sweep_stale`]) and settle the state they guarded:
    /// an empty history window switches from the skeleton to the retry
    /// row, a newer-page request stops auto-paging. Skipped while the
    /// connection is not up — TDLib legitimately queues requests until the
    /// network is back. Returns whether anything was dropped.
    pub fn sweep_stale_requests(
        &mut self,
        now: std::time::Instant,
        ttl: std::time::Duration,
        keep: impl Fn(RequestId) -> bool,
    ) -> bool {
        if !matches!(
            self.connection,
            ConnectionState::Ready | ConnectionState::Updating
        ) {
            return false;
        }
        let dropped = self.requests.sweep_stale(now, ttl, keep);
        let any = !dropped.is_empty();
        for pending in dropped {
            // The mute toggle only guards a local flag: dropping it is
            // the whole settlement, its errors would be user-facing.
            if pending.purpose.is_sweepable() {
                self.settle_lost_request(pending);
            }
        }
        any
    }

    /// Whether `pending` was issued for a window that has since been
    /// replaced (consumes the stale mark).
    pub(crate) fn take_stale_history_request(&mut self, pending: &PendingRequest) -> bool {
        self.stale_history_requests.remove(&pending.id.0)
    }

    /// The newest message `chat_id` is known to have, from its summary
    /// (0 when unknown).
    fn chat_latest_id(&self, chat_id: ChatId) -> i64 {
        self.chats
            .get(&chat_id.0)
            .and_then(|chat| chat.last_message.as_ref())
            .map_or(0, |last| last.id.0)
    }

    /// The loaded window of `chat_id` stops short of the chat's latest
    /// message ([`HistoryState::stops_short_of`]).
    pub fn history_has_newer(&self, chat_id: ChatId) -> bool {
        // A session without a server (the screenshot demos) seeds the whole
        // history: there is nothing newer to load.
        if self.history_pruning.is_offline_copy() {
            return false;
        }
        let latest = self.chat_latest_id(chat_id);
        self.histories
            .get(&chat_id.0)
            .is_some_and(|history| history.stops_short_of(latest))
    }

    /// Which edge of the window a history page request extends.
    fn window_edge(pending: &PendingRequest) -> WindowEdge {
        let from = pending.around_message_id.map_or(0, |id| id.0);
        match pending.purpose {
            RequestPurpose::GetHistory if from != 0 => WindowEdge::Older(from),
            RequestPurpose::GetHistoryNewer => WindowEdge::Newer(from),
            _ => WindowEdge::First,
        }
    }

    /// Whether a page answering `pending` still extends the window as it
    /// is now. A first page fills an empty window; an older page continues
    /// below the message it was asked from, which must still be the oldest;
    /// a newer page starts with the message it was asked from, which must
    /// still be loaded. Anything else was asked for a window that has since
    /// been trimmed, released or replaced: storing it would leave a gap.
    fn page_attaches(&self, chat_id: ChatId, edge: WindowEdge) -> bool {
        let history = self.histories.get(&chat_id.0);
        match edge {
            WindowEdge::First => true,
            WindowEdge::Older(from) => {
                history.and_then(HistoryState::oldest_id) == Some(MessageId(from))
            }
            WindowEdge::Newer(from) => history.is_some_and(|h| h.contains(MessageId(from))),
        }
    }

    /// A page request for `edge` of `chat_id`'s window is on the way and
    /// will still extend the window when it answers. Requests for a window
    /// that was replaced, or that no longer attach, do not count: they
    /// must not hold back the page the window needs now.
    pub(crate) fn history_page_in_flight(&self, chat_id: ChatId, edge: WindowEdge) -> bool {
        self.requests
            .ids_for_chat(&WINDOW_PURPOSES, chat_id)
            .into_iter()
            .filter(|id| !self.stale_history_requests.contains(&id.0))
            .filter_map(|id| self.requests.get(id))
            .map(Self::window_edge)
            .any(|pending_edge| {
                std::mem::discriminant(&pending_edge) == std::mem::discriminant(&edge)
                    && self.page_attaches(chat_id, pending_edge)
            })
    }

    /// Whether a window page answer must be dropped: the window it was
    /// asked for was replaced, the chat is no longer shown, or the page no
    /// longer attaches. Records why.
    pub(crate) fn history_page_detached(&mut self, pending: &PendingRequest, seq: u64) -> bool {
        let note = if self.take_stale_history_request(pending) {
            "stale-history-window"
        } else if pending.chat_id.is_none_or(|chat| !self.chat_is_shown(chat)) {
            "stale-chat-history"
        } else if pending
            .chat_id
            .is_some_and(|chat| !self.page_attaches(chat, Self::window_edge(pending)))
        {
            "detached-history-page"
        } else {
            return false;
        };
        if pending.purpose == RequestPurpose::GetHistoryAround {
            self.drop_saved_open(pending.chat_id);
        }
        self.diagnostics.record(Diagnostic {
            category: "reducer",
            type_name: Some("messages".into()),
            extra: Some(pending.id.0),
            seq: Some(seq),
            note,
        });
        true
    }

    /// Store a page that starts the window (the latest messages, or the
    /// run around a message). A window that is not empty holds live
    /// messages that arrived meanwhile, or the tail kept from an earlier
    /// visit: the page joins them when the two overlap, and replaces them
    /// otherwise (they are not adjacent to it).
    pub(crate) fn store_first_page(&mut self, chat_id: ChatId, messages: &[ParsedMessage]) {
        let (Some(low), Some(high)) = (
            messages.iter().map(|m| m.id.0).min(),
            messages.iter().map(|m| m.id.0).max(),
        ) else {
            return;
        };
        let history = self.history_entry(chat_id);
        let overlaps = match (history.oldest_id(), history.newest_id()) {
            (Some(oldest), Some(newest)) => low <= newest.0 && oldest.0 <= high,
            _ => true,
        };
        if !overlaps {
            let doomed: Vec<MessageId> = history
                .messages
                .values()
                .filter(|message| !message.pending && !message.failed)
                .map(|message| message.id)
                .collect();
            for id in doomed {
                history.remove(id, false);
            }
            history.loaded_complete = false;
            history.window_epoch = history.window_epoch.wrapping_add(1);
        }
        for message in messages {
            self.upsert_message(message.clone(), false);
        }
    }

    /// Prepare the window when the driver opens `chat_id` (after
    /// `open_chat` recorded the unread anchor): with unread messages it must
    /// cover the read boundary (otherwise it is replaced and loads around
    /// it); a window left behind by an earlier jump is replaced so the chat
    /// opens at its latest messages.
    pub fn prepare_history_window(&mut self, chat_id: ChatId) {
        let has_newer = self.history_has_newer(chat_id);
        let history = self.histories.entry(chat_id.0).or_default();
        history.newer_failed = false;
        let keep = !has_newer
            && match history.unread_anchor {
                // A tail window that reaches back to the boundary shows the
                // unread run as is.
                Some(anchor) => {
                    history.loaded_complete
                        || history
                            .oldest_id()
                            .is_some_and(|oldest| oldest.0 <= anchor.0)
                }
                None => true,
            };
        if !keep {
            self.reset_history_window(chat_id);
        }
    }

    /// The opened history could not bring the "Unread messages" bar to the
    /// top of the view: the unread run fits on the last screen. tdesktop
    /// then opens at the bottom without a bar
    /// (`HistoryWidget::countAutomaticScrollTop`). Only the bar goes; reads
    /// are held as before (`opened_with_unread`). The open thread of
    /// `chat_id`, if any, owns the bar.
    pub fn hide_unread_bar(&mut self, chat_id: ChatId) {
        if let Some(thread) = self
            .threads
            .thread
            .as_mut()
            .filter(|thread| thread.chat_id == chat_id && thread.thread_id != 0)
        {
            thread.unread_anchor = None;
        } else if let Some(history) = self.histories.get_mut(&chat_id.0) {
            history.unread_anchor = None;
        }
    }

    /// Bring back the bar above the first unread message of the open
    /// history for the jump-down button (tdesktop `CornerButtons::downClick`
    /// shows the history at `ShowAtUnreadMsgId`), and re-anchor the view at
    /// it. Returns `None` when nothing is unread. `Some(true)`: the window
    /// holds the first unread message; `Some(false)`: it was replaced and a
    /// window around the read boundary has to load.
    pub fn show_first_unread(&mut self, chat_id: ChatId) -> Option<bool> {
        let anchor = self
            .chats
            .get(&chat_id.0)
            .and_then(ChatSummary::unread_anchor)?;
        let history = self.histories.entry(chat_id.0).or_default();
        history.unread_anchor = Some(anchor);
        let covered = history.unread_bar_message().is_some()
            && (history.loaded_complete
                || history
                    .oldest_id()
                    .is_some_and(|oldest| oldest.0 <= anchor.0));
        if covered {
            history.window_epoch = history.window_epoch.wrapping_add(1);
        } else {
            self.reset_history_window(chat_id);
        }
        Some(covered)
    }

    /// Prepare the window for a chat opened at `open` (after `open_chat`).
    /// A saved position keeps a window that holds its message; otherwise
    /// the window is replaced and the load around the message becomes the
    /// open (`HistoryState::saved_open`). Anything else is
    /// [`Self::prepare_history_window`]. Returns the message the first page
    /// must load around.
    pub fn prepare_history_window_at(
        &mut self,
        chat_id: ChatId,
        open: HistoryOpen,
    ) -> Option<MessageId> {
        let HistoryOpen::Saved(anchor) = open else {
            self.prepare_history_window(chat_id);
            return None;
        };
        if self
            .histories
            .get(&chat_id.0)
            .is_some_and(|history| history.contains(anchor))
        {
            return None;
        }
        self.reset_history_window(chat_id);
        self.histories.entry(chat_id.0).or_default().saved_open = Some(anchor);
        Some(anchor)
    }

    /// The load around a saved position answered or failed, so the first
    /// rows can anchor. Without its message (deleted, or the load failed)
    /// the window is emptied and the default one loads instead. Returns
    /// whether `anchor` was that load.
    pub(crate) fn settle_saved_open(&mut self, chat_id: ChatId, anchor: MessageId) -> bool {
        let Some(history) = self
            .histories
            .get_mut(&chat_id.0)
            .filter(|history| history.saved_open == Some(anchor))
        else {
            return false;
        };
        history.saved_open = None;
        if !history.contains(anchor) {
            history.reset_window();
            history.saved_open_missed = true;
        }
        true
    }

    /// A saved-position load was dropped unanswered (stale view, chat no
    /// longer shown): nothing is left waiting for it.
    pub(crate) fn drop_saved_open(&mut self, chat_id: Option<ChatId>) {
        if let Some(history) = chat_id.and_then(|id| self.histories.get_mut(&id.0)) {
            history.saved_open = None;
        }
    }

    /// The open chat missed its saved position and needs its default
    /// window loaded (taken once).
    pub fn take_saved_open_fallback(&mut self) -> Option<ChatId> {
        let chat_id = self.open_chat?;
        let history = self.histories.get_mut(&chat_id.0)?;
        std::mem::take(&mut history.saved_open_missed).then_some(chat_id)
    }

    /// The window of `chat_id` is still loading around its saved position:
    /// the first anchoring waits for it.
    pub fn saved_open_pending(&self, chat_id: ChatId) -> bool {
        self.histories
            .get(&chat_id.0)
            .is_some_and(|history| history.saved_open.is_some())
    }

    /// Apply a `GetHistoryNewer` page: everything at or after the request's
    /// `from_message_id` joins the window. A page with nothing newer than
    /// the boundary means the window now reaches the latest message.
    pub(crate) fn apply_history_newer(
        &mut self,
        pending: &PendingRequest,
        messages: Vec<ParsedMessage>,
        seq: u64,
    ) {
        if self.history_page_detached(pending, seq) {
            return;
        }
        let Some(chat_id) = pending.chat_id else {
            return;
        };
        let from = pending.around_message_id.map_or(0, |id| id.0);
        let any_newer = messages.iter().any(|m| m.id.0 > from);
        for message in messages {
            self.upsert_message(message, false);
        }
        if !any_newer {
            let latest = self.chat_latest_id(chat_id);
            if let Some(history) = self.histories.get_mut(&chat_id.0) {
                history.newer_exhausted = Some((latest.max(history.latest_seen), from));
            }
        }
        // R6: reading forward through a long window — drop the oldest end.
        self.trim_history_window(chat_id, WindowEnd::Oldest);
    }

    /// A newer-page request failed: stop auto-paging this window.
    pub(crate) fn fail_history_newer(&mut self, pending: &PendingRequest) {
        if self.take_stale_history_request(pending) {
            return;
        }
        if let Some(chat_id) = pending.chat_id
            && let Some(history) = self.histories.get_mut(&chat_id.0)
        {
            history.newer_failed = true;
        }
    }

    /// `updateNewMessage` while the window stops short of the latest
    /// message: an incoming message stays out of the window (it is not
    /// adjacent to it); an outgoing one means the user just sent from the
    /// middle of the history, so the window jumps to the latest run, which
    /// starts with this message. Returns whether the message still needs
    /// the normal upsert.
    pub(crate) fn route_new_message_into_window(&mut self, message: &ParsedMessage) -> bool {
        let chat_id = message.chat_id;
        // A send while the window waits for a page around an older message
        // would sit right after that page across a gap: open at the latest.
        let around_in_flight = message.is_outgoing
            && self
                .requests
                .ids_for_chat(&[RequestPurpose::GetHistoryAround], chat_id)
                .into_iter()
                .any(|id| !self.stale_history_requests.contains(&id.0));
        if !around_in_flight && !self.history_has_newer(chat_id) {
            return true;
        }
        // Incoming messages are always server messages; an outgoing one
        // may still carry a temporary id.
        if let Some(history) = self.histories.get_mut(&chat_id.0)
            && !message.is_outgoing
        {
            history.latest_seen = history.latest_seen.max(message.id.0);
        }
        if message.is_outgoing {
            self.reset_history_window(chat_id);
            if let Some(history) = self.histories.get_mut(&chat_id.0) {
                history.end_unread_open();
            }
            return true;
        }
        false
    }
}
