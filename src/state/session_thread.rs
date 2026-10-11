//! Channel comments and group reply threads: the reducer side.
use super::*;
use crate::telegram::envelope::{MessageReplyInfo, ParsedMessageThreadInfo};

impl Session {
    /// Start resolving a thread (`getMessageThread`) for `message_id` of
    /// `chat_id`. Replaces any open thread.
    pub fn begin_thread(&mut self, chat_id: ChatId, message_id: MessageId) {
        let mut view = ThreadView::resolving(chat_id, message_id);
        // A thread opened from inside a forum topic keeps that topic.
        if self.open_chat == Some(chat_id) && self.chat_has_topics(chat_id) {
            view.forum_topic_id = self.open_topic;
        }
        self.threads.thread = Some(view);
        self.view_generation.bump();
    }

    /// Leave the thread view. Returns the thread so the driver can restore
    /// the origin chat.
    pub fn close_thread(&mut self) -> Option<ThreadView> {
        self.view_generation.bump();
        self.threads.thread.take()
    }

    /// The thread whose replies are shown in `chat_id` (the open chat).
    pub fn thread_for_chat(&self, chat_id: ChatId) -> Option<&ThreadView> {
        self.threads
            .thread
            .as_ref()
            .filter(|thread| thread.chat_id == chat_id && thread.thread_id != 0)
    }

    /// A thread is open but cannot show messages: still resolving, moving
    /// to its chat, or failed to load.
    pub fn thread_unavailable(&self) -> bool {
        self.threads.thread.as_ref().is_some_and(|thread| {
            matches!(thread.status, ThreadStatus::Failed(_))
                || self
                    .open_chat
                    .and_then(|chat| self.thread_for_chat(chat))
                    .is_none()
        })
    }

    /// `message_thread_id` a send to `chat_id` is addressed to: `Some` while
    /// the open thread lives in that chat.
    pub fn thread_send_target(&self, chat_id: ChatId) -> Option<(i64, MessageId)> {
        let thread = self.thread_for_chat(chat_id)?;
        Some((thread.thread_id, MessageId(thread.thread_id)))
    }

    /// The forum topic the open thread in `chat_id` belongs to, if any.
    pub fn thread_forum_topic(&self, chat_id: ChatId) -> Option<i32> {
        self.thread_for_chat(chat_id)?.forum_topic_id
    }

    /// `getMessageThread` answered: fill the thread and queue the chat switch.
    pub(crate) fn apply_message_thread_info(
        &mut self,
        info: ParsedMessageThreadInfo,
        pending: Option<&PendingRequest>,
    ) {
        let Some(RequestPurpose::GetMessageThread { message_id }) = pending.map(|p| p.purpose)
        else {
            return;
        };
        let origin_chat = pending.and_then(|p| p.chat_id);
        for message in &info.messages {
            self.remember_files(&message.files);
        }
        let open_chat = self.open_chat;
        let Some(thread) = self.threads.thread.as_mut() else {
            return;
        };
        if Some(thread.origin_chat_id) != origin_chat || thread.origin_message_id.0 != message_id {
            return;
        }
        thread.chat_id = info.chat_id;
        thread.thread_id = info.message_thread_id;
        thread.unread_count = info.unread_message_count.max(0);
        if let Some(reply) = &info.reply_info {
            thread.reply_count = reply.reply_count;
            thread.last_read_inbox_message_id = reply.last_read_inbox_message_id;
            thread.last_message_id = reply.last_message_id;
        }
        // The divider sits after the last read reply; with nothing read yet
        // it sits after the root, so the first reply is the first unread
        // (tdesktop `readTillId = max(afterId, rootId)`).
        thread.unread_anchor = (thread.unread_count > 0).then_some(MessageId(
            thread.last_read_inbox_message_id.max(thread.thread_id),
        ));
        thread.status = ThreadStatus::LoadingHistory;
        thread.needs_chat_switch = open_chat != Some(info.chat_id);
        let rows: Vec<HistoryMessage> = info
            .messages
            .into_iter()
            .map(|message| history_message(message, false))
            .collect();
        thread.root_ids = rows.iter().map(|row| row.id.0).collect();
        for row in rows {
            thread.history.upsert(row);
        }
    }

    /// One `getMessageThreadHistory` page: the first page (newest, or
    /// around the read position), an older page, or a newer page
    /// (`ThreadsPurpose::GetMessageThreadHistoryNewer`). Pages of a window
    /// that was replaced since are dropped.
    pub(crate) fn apply_thread_history(
        &mut self,
        messages: Vec<ParsedMessage>,
        pending: Option<&PendingRequest>,
    ) {
        let Some(pending) = pending else {
            return;
        };
        let (message_id, newer) = match pending.purpose {
            RequestPurpose::GetMessageThreadHistory { message_id } => (message_id, false),
            RequestPurpose::Threads(ThreadsPurpose::GetMessageThreadHistoryNewer {
                message_id,
            }) => (message_id, true),
            _ => return,
        };
        let origin_chat = pending.chat_id;
        for message in &messages {
            self.remember_files(&message.files);
        }
        let Some(thread) = self.threads.thread.as_mut() else {
            return;
        };
        if Some(thread.origin_chat_id) != origin_chat
            || thread.origin_message_id.0 != message_id
            || thread.stale_pages.remove(&pending.id.0)
        {
            return;
        }
        let empty = messages.is_empty();
        let mut added = 0usize;
        let mut oldest_reply: Option<i64> = None;
        let mut reached_root = false;
        for message in messages {
            let id = message.id.0;
            if thread.root_ids.contains(&id) || id == thread.thread_id {
                reached_root = true;
            } else {
                oldest_reply = Some(oldest_reply.map_or(id, |oldest| oldest.min(id)));
            }
            let row = history_message(message, false);
            if thread.history.messages.insert(id, row).is_none() {
                added += 1;
            }
        }
        if newer {
            // The newer side: an empty page says the window reached the
            // newest reply whatever the server reported before.
            if added == 0 {
                thread.has_newer = false;
            } else {
                thread.refresh_has_newer();
            }
        } else {
            if empty || added == 0 || reached_root || oldest_reply.is_none() {
                thread.history.loaded_complete = true;
            }
            if let Some(oldest) = oldest_reply {
                thread.history.next_from_message_id = MessageId(oldest);
            }
            // A first page around the read position stops short of the
            // newest reply; older pages leave the newest row as it is.
            thread.refresh_has_newer();
        }
        thread.history.total_count = thread.history.messages.len() as i32;
        if thread.status == ThreadStatus::LoadingHistory {
            thread.status = ThreadStatus::Ready;
        }
    }

    /// A thread request failed.
    pub(crate) fn fail_thread(&mut self, pending: Option<&PendingRequest>, line: String) {
        let Some(pending) = pending else {
            return;
        };
        let (message_id, newer) = match pending.purpose {
            RequestPurpose::GetMessageThread { message_id }
            | RequestPurpose::GetMessageThreadHistory { message_id } => (message_id, false),
            RequestPurpose::Threads(ThreadsPurpose::GetMessageThreadHistoryNewer {
                message_id,
            }) => (message_id, true),
            _ => return,
        };
        let origin_chat = pending.chat_id;
        let Some(thread) = self.threads.thread.as_mut() else {
            return;
        };
        if Some(thread.origin_chat_id) != origin_chat
            || thread.origin_message_id.0 != message_id
            || thread.stale_pages.remove(&pending.id.0)
        {
            return;
        }
        if newer {
            thread.newer_failed = true;
        } else if thread.status != ThreadStatus::Ready {
            thread.status = ThreadStatus::Failed(line);
        }
    }

    /// Replace the open thread's window (jump to the latest replies): the
    /// pages still in flight are marked stale. Returns whether a thread
    /// was open.
    pub(crate) fn reset_thread_window(&mut self) -> bool {
        let Some(thread) = self.threads.thread.as_ref() else {
            return false;
        };
        let message_id = thread.origin_message_id.0;
        let purposes = [
            RequestPurpose::GetMessageThreadHistory { message_id },
            RequestPurpose::Threads(ThreadsPurpose::GetMessageThreadHistoryNewer { message_id }),
        ];
        let stale = self.requests.ids_for_chat(&purposes, thread.origin_chat_id);
        let thread = self.threads.thread.as_mut().expect("checked above");
        thread.stale_pages.extend(stale.into_iter().map(|id| id.0));
        thread.reset_window();
        true
    }

    /// Keep the thread's counters in sync with the root's
    /// `interaction_info.reply_info` updates: the reply count, the newest
    /// reply and the server's read position.
    pub(crate) fn sync_thread_reply_info(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        reply: Option<&MessageReplyInfo>,
    ) {
        let Some(thread) = self.threads.thread.as_mut() else {
            return;
        };
        let is_origin = chat_id == thread.origin_chat_id && message_id == thread.origin_message_id;
        let is_root = chat_id == thread.chat_id && thread.root_ids.contains(&message_id.0);
        if !(is_origin || is_root) {
            return;
        }
        if let Some(reply) = reply {
            thread.reply_count = reply.reply_count;
            thread.last_message_id = thread.last_message_id.max(reply.last_message_id);
            thread.read_till(reply.last_read_inbox_message_id);
        }
    }

    /// The UI shows thread rows up to `ids`' newest: read them locally
    /// (`viewMessages` follows through the chat's own report) and let the
    /// origin post's comments bar lose its unread dot at once, as tdesktop's
    /// `setCommentsInboxReadTill` does.
    pub(crate) fn thread_read_till(&mut self, chat_id: ChatId, ids: &[MessageId]) {
        let Some(thread) = self
            .threads
            .thread
            .as_mut()
            .filter(|thread| thread.chat_id == chat_id && thread.thread_id != 0)
        else {
            return;
        };
        let Some(newest) = ids
            .iter()
            .map(|id| id.0)
            .filter(|id| *id > 0 && thread.history.messages.contains_key(id))
            .max()
        else {
            return;
        };
        if !thread.read_till(newest) {
            return;
        }
        let (origin_chat, origin_message, till) = (
            thread.origin_chat_id,
            thread.origin_message_id,
            thread.last_read_inbox_message_id,
        );
        self.edit_loaded_message(origin_chat, origin_message, |message| {
            if let Some(reply) = message
                .interaction_info
                .as_mut()
                .and_then(|info| info.reply_info.as_mut())
            {
                reply.last_read_inbox_message_id = reply.last_read_inbox_message_id.max(till);
            }
        });
    }

    /// A thread message arrived (update or own send): add it to the open
    /// thread's rows, or hold it outside a window that stops short of the
    /// newest replies.
    pub(crate) fn thread_upsert(&mut self, row: HistoryMessage) {
        if let Some(thread) = self.threads.thread.as_mut() {
            thread.note_live(row);
        }
    }

    /// Rows removed by `updateDeleteMessages`.
    pub(crate) fn thread_remove(&mut self, chat_id: ChatId, ids: &[MessageId]) {
        if let Some(thread) = self.threads.thread.as_mut()
            && thread.chat_id == chat_id
        {
            for id in ids {
                thread.history.messages.remove(&id.0);
            }
        }
    }

    /// Whether `message` belongs to the open thread.
    pub(crate) fn thread_accepts(&self, message: &ParsedMessage) -> bool {
        self.threads
            .thread
            .as_ref()
            .is_some_and(|thread| thread.accepts(message))
    }

    /// Reset the read bookkeeping of `chat_id` so thread rows are viewed as
    /// thread history (`messageSourceMessageThreadHistory`) and nothing but
    /// reported rows is read.
    pub(crate) fn prepare_thread_marks(&mut self, chat_id: ChatId) {
        let history = self.histories.entry(chat_id.0).or_default();
        history.viewed.clear();
        history.viewing.clear();
        history.visible.clear();
        history.visible_reported = true;
        history.unread_anchor = None;
    }
}
