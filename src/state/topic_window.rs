//! The loaded window of a forum topic, and of the comment thread view,
//! which keeps its rows in a [`TopicHistory`] too: paged at both ends and
//! capped around the reader like the main history (`history_trim`).
//!
//! A topic window that dropped its newest end stops short of the topic's
//! latest message (`has_newer`): live messages wait outside it, newer
//! pages (`GetTopicHistoryNewer`) bring the end back as the reader scrolls
//! down, and jumping to the latest replaces the window.
use super::history_trim::{is_local, live_trim_span, rows_above};
use super::*;

impl TopicHistory {
    /// Newest server message in the window (local sends excluded: their
    /// temporary ids are not positions in the history).
    pub fn newest_loaded_id(&self) -> Option<MessageId> {
        self.messages
            .values()
            .rev()
            .find(|message| message.id.0 > 0 && !is_local(message))
            .map(|message| message.id)
    }

    /// Drop up to `count` server rows from `end`, sparing `spare` (a
    /// thread's pinned root) and local sends. The dropped end pages back
    /// in: an older page continues from the oldest kept row, and a dropped
    /// newest end leaves the window short of the latest.
    fn drop_end(&mut self, end: WindowEnd, count: usize, spare: &[i64]) -> usize {
        let droppable = |(id, message): (&i64, &HistoryMessage)| {
            (*id > 0 && !is_local(message) && !spare.contains(id)).then_some(*id)
        };
        let doomed: Vec<i64> = match end {
            WindowEnd::Oldest => self
                .messages
                .iter()
                .filter_map(droppable)
                .take(count)
                .collect(),
            WindowEnd::Newest => self
                .messages
                .iter()
                .rev()
                .filter_map(droppable)
                .take(count)
                .collect(),
        };
        for id in &doomed {
            self.messages.remove(id);
        }
        if doomed.is_empty() {
            return 0;
        }
        match end {
            WindowEnd::Oldest => {
                self.loaded_complete = false;
                let oldest = self
                    .messages
                    .keys()
                    .copied()
                    .find(|id| *id > 0 && !spare.contains(id));
                if let Some(oldest) = oldest {
                    self.next_from_message_id = MessageId(oldest);
                }
            }
            WindowEnd::Newest => {
                let newest = doomed.iter().copied().max().unwrap_or(0);
                self.latest_seen = self.latest_seen.max(newest);
                self.has_newer = true;
                self.newer_failed = false;
            }
        }
        doomed.len()
    }

    /// The cap after a page landed: past [`HISTORY_WINDOW_CAP`] rows the
    /// `far_end` (opposite the page) goes. Returns how many were dropped.
    pub(crate) fn trim_after_page(&mut self, far_end: WindowEnd, spare: &[i64]) -> usize {
        let len = self.messages.len();
        if len <= HISTORY_WINDOW_CAP {
            return 0;
        }
        self.drop_end(far_end, len - HISTORY_WINDOW_TRIM_TO, spare)
    }

    /// The cap after a live message: rows far from the reader go, older
    /// ones first. Returns the ends that moved.
    pub(crate) fn trim_live(&mut self, spare: &[i64]) -> Vec<WindowEnd> {
        let span = live_trim_span(self.messages.len(), rows_above(&self.messages, self.reader));
        [
            (WindowEnd::Oldest, span.oldest),
            (WindowEnd::Newest, span.newest),
        ]
        .into_iter()
        .filter(|(end, count)| self.drop_end(*end, *count, spare) > 0)
        .map(|(end, _)| end)
        .collect()
    }

    /// Replace the window with the topic's latest page: only local sends
    /// stay, and the next page request starts at the newest message.
    pub(crate) fn reset_window(&mut self) {
        self.messages.retain(|_, message| is_local(message));
        self.loaded_complete = false;
        self.next_from_message_id = MessageId(0);
        self.reader = ReaderPosition::Tail;
        self.has_newer = false;
        self.newer_failed = false;
        self.window_epoch = self.window_epoch.wrapping_add(1);
    }

    /// A message for this topic arrived outside a page (an update or an
    /// own send). While the window stops short of the latest, a message
    /// past its end waits outside it, and an own send replaces the window
    /// so the latest page shows it (tdesktop `showAtEnd`).
    pub(crate) fn note_live(&mut self, row: HistoryMessage) {
        if self.has_newer {
            if is_local(&row) {
                self.reset_window();
            } else if self
                .newest_loaded_id()
                .is_none_or(|newest| row.id.0 > newest.0)
            {
                self.latest_seen = self.latest_seen.max(row.id.0);
                return;
            }
        }
        self.upsert(row);
    }

    /// A newer page landed: `added` rows past the old newest end. An empty
    /// page says the window reached the latest whatever was seen before.
    fn settle_newer_page(&mut self, added: usize) {
        self.has_newer = added > 0
            && self
                .newest_loaded_id()
                .is_some_and(|newest| newest.0 < self.latest_seen);
    }
}

fn topic_page_purpose(end: WindowEnd) -> RequestPurpose {
    match end {
        WindowEnd::Oldest => RequestPurpose::GetTopicHistory,
        WindowEnd::Newest => RequestPurpose::Threads(ThreadsPurpose::GetTopicHistoryNewer),
    }
}

impl Session {
    /// One forum topic page (`searchChatMessages` with the topic): the
    /// first page, an older page, or a newer page. A page asked for before
    /// its end of the window moved is dropped.
    pub(crate) fn apply_topic_page(
        &mut self,
        messages: Vec<ParsedMessage>,
        total_count: i32,
        next_from_message_id: MessageId,
        pending: &PendingRequest,
    ) {
        let newer = match pending.purpose {
            RequestPurpose::GetTopicHistory => false,
            RequestPurpose::Threads(ThreadsPurpose::GetTopicHistoryNewer) => true,
            _ => return,
        };
        let (Some(chat_id), Some(topic)) = (pending.chat_id, pending.forum_topic_id) else {
            return;
        };
        for message in &messages {
            self.remember_files(&message.files);
        }
        let empty = messages.is_empty();
        let rows: Vec<HistoryMessage> = messages
            .into_iter()
            .map(|message| history_message(message, false))
            .collect();
        for row in &rows {
            self.index_poll(row);
        }
        if !newer {
            self.clear_topic_load_failed(chat_id, topic);
        }
        let entry = self.topic_history_entry(chat_id, topic);
        if entry.stale_pages.remove(&pending.id.0) {
            return;
        }
        let before = entry.newest_loaded_id().map_or(0, |id| id.0);
        let mut added_newer = 0;
        for row in rows {
            let id = row.id.0;
            if entry.messages.insert(id, row).is_none() && id > before {
                added_newer += 1;
            }
        }
        let far_end = if newer {
            entry.settle_newer_page(added_newer);
            WindowEnd::Oldest
        } else {
            if next_from_message_id.0 == 0 || empty {
                entry.loaded_complete = true;
            }
            entry.next_from_message_id = next_from_message_id;
            WindowEnd::Newest
        };
        let trimmed = entry.trim_after_page(far_end, &[]) > 0;
        entry.total_count = total_count.max(entry.messages.len() as i32);
        if trimmed {
            self.drop_topic_pages_past(chat_id, topic, &[far_end]);
            self.prune_reply_targets(chat_id);
        }
    }

    /// A topic page failed. A failed newer page stops auto-paging until a
    /// jump; a failed first page offers Retry.
    pub(crate) fn fail_topic_page(&mut self, pending: &PendingRequest) {
        let (Some(chat_id), Some(topic)) = (pending.chat_id, pending.forum_topic_id) else {
            return;
        };
        let history = self.threads.topic_histories.get_mut(&(chat_id.0, topic));
        if history
            .as_ref()
            .is_some_and(|history| history.stale_pages.contains(&pending.id.0))
        {
            if let Some(history) = history {
                history.stale_pages.remove(&pending.id.0);
            }
            return;
        }
        match pending.purpose {
            RequestPurpose::GetTopicHistory => self.mark_topic_load_failed(pending),
            _ => {
                if let Some(history) = history {
                    history.newer_failed = true;
                }
            }
        }
    }

    /// A page for `end` of the topic's current window is in flight (pages
    /// asked for before that end moved do not count).
    pub fn topic_page_in_flight(&self, chat_id: ChatId, topic: i32, end: WindowEnd) -> bool {
        let stale = self
            .threads
            .topic_histories
            .get(&(chat_id.0, topic))
            .map(|history| &history.stale_pages);
        self.topic_page_ids(chat_id, topic, &[end])
            .into_iter()
            .any(|id| stale.is_none_or(|stale| !stale.contains(&id)))
    }

    fn topic_page_ids(&self, chat_id: ChatId, topic: i32, ends: &[WindowEnd]) -> Vec<u64> {
        let purposes: Vec<RequestPurpose> =
            ends.iter().map(|end| topic_page_purpose(*end)).collect();
        self.requests
            .ids_for_chat(&purposes, chat_id)
            .into_iter()
            .filter(|id| self.requests.get(*id).and_then(|p| p.forum_topic_id) == Some(topic))
            .map(|id| id.0)
            .collect()
    }

    /// The cap after a live message landed in a loaded topic.
    pub(crate) fn trim_live_topic(&mut self, chat_id: ChatId, topic: i32) {
        let Some(history) = self.threads.topic_histories.get_mut(&(chat_id.0, topic)) else {
            return;
        };
        let moved = history.trim_live(&[]);
        if !moved.is_empty() {
            self.drop_topic_pages_past(chat_id, topic, &moved);
            self.prune_reply_targets(chat_id);
        }
    }

    /// Replace the topic's window with its latest page (jump to latest).
    /// Returns whether the topic is loaded.
    pub(crate) fn reset_topic_window(&mut self, chat_id: ChatId, topic: i32) -> bool {
        self.drop_topic_pages_past(chat_id, topic, &[WindowEnd::Oldest, WindowEnd::Newest]);
        match self.threads.topic_histories.get_mut(&(chat_id.0, topic)) {
            Some(history) => {
                history.reset_window();
                true
            }
            None => false,
        }
    }

    /// Mark the topic's page requests in flight for `ends` stale: they were
    /// asked for from an end that has moved since.
    fn drop_topic_pages_past(&mut self, chat_id: ChatId, topic: i32, ends: &[WindowEnd]) {
        let stale = self.topic_page_ids(chat_id, topic, ends);
        if let Some(history) = self.threads.topic_histories.get_mut(&(chat_id.0, topic)) {
            history.stale_pages.extend(stale);
        }
    }
}
