//! The open chat's loaded history window: one contiguous run of messages,
//! opened at the first unread message when there is one, paged in both
//! directions, and replaced (never patched) when a jump lands outside it.
use super::*;

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
                    && !self
                        .requests
                        .ids_for_chat(&WINDOW_PURPOSES, chat_id)
                        .is_empty()
            }
        }
    }

    /// The newest own message of `chat_id` that can be edited in the
    /// composer (Up in an empty composer, like Telegram Desktop). `None`
    /// when the loaded window doesn't reach the latest messages — its
    /// newest own message might not be the last one.
    pub fn last_editable_message(&self, chat_id: ChatId) -> Option<crate::composer::ComposerEdit> {
        let history = self.histories.get(&chat_id.0)?;
        if history.has_newer {
            return None;
        }
        history
            .messages
            .values()
            .rev()
            .filter(|message| message.is_outgoing && !message.pending && !message.failed)
            .find_map(|message| {
                crate::composer::ComposerEdit::from_own_content(
                    chat_id,
                    message.id,
                    message.is_outgoing,
                    message.pending,
                    &message.content,
                )
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
        for pending in &dropped {
            match pending.purpose {
                RequestPurpose::GetHistory | RequestPurpose::GetHistoryAround => {
                    if !self.take_stale_history_request(pending)
                        && let Some(chat_id) = pending.chat_id
                    {
                        self.histories.entry(chat_id.0).or_default().load_failed = true;
                    }
                }
                RequestPurpose::GetHistoryNewer => self.fail_history_newer(pending),
                _ => {}
            }
        }
        !dropped.is_empty()
    }

    /// Whether `pending` was issued for a window that has since been
    /// replaced (consumes the stale mark).
    pub(crate) fn take_stale_history_request(&mut self, pending: &PendingRequest) -> bool {
        self.stale_history_requests.remove(&pending.id.0)
    }

    /// Prepare the window when the driver opens `chat_id` (after
    /// `open_chat` recorded the unread anchor): with unread messages it must
    /// cover the read boundary (otherwise it is replaced and loads around
    /// it); a window left behind by an earlier jump is replaced so the chat
    /// opens at its latest messages.
    pub fn prepare_history_window(&mut self, chat_id: ChatId) {
        let history = self.histories.entry(chat_id.0).or_default();
        let keep = !history.has_newer
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

    /// Recompute whether the window stops short of the chat's latest
    /// message after a page landed.
    pub(crate) fn refresh_history_has_newer(&mut self, chat_id: ChatId) {
        let summary_last = self
            .chats
            .get(&chat_id.0)
            .and_then(|chat| chat.last_message.as_ref())
            .map_or(0, |last| last.id.0);
        let Some(history) = self.histories.get_mut(&chat_id.0) else {
            return;
        };
        let latest = summary_last.max(history.latest_seen);
        history.has_newer = latest > 0
            && !history.contains(MessageId(latest))
            && history.newest_id().is_some_and(|newest| newest.0 < latest);
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
        if self.take_stale_history_request(pending)
            || pending.view_generation != Some(self.view_generation)
        {
            self.diagnostics.record(Diagnostic {
                category: "reducer",
                type_name: Some("messages".into()),
                extra: Some(pending.id.0),
                seq: Some(seq),
                note: "stale-history-newer",
            });
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
        if any_newer {
            self.refresh_history_has_newer(chat_id);
        } else if let Some(history) = self.histories.get_mut(&chat_id.0) {
            history.has_newer = false;
        }
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
        if !self
            .histories
            .get(&chat_id.0)
            .is_some_and(|history| history.has_newer)
        {
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
                history.unread_anchor = None;
            }
            return true;
        }
        false
    }
}
