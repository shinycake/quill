//! Channel comments and group reply threads: the reducer side.
use super::*;
use crate::telegram::envelope::{MessageReplyInfo, ParsedMessageThreadInfo};

impl Session {
    /// Start resolving a thread (`getMessageThread`) for `message_id` of
    /// `chat_id`. Replaces any open thread.
    pub fn begin_thread(&mut self, chat_id: ChatId, message_id: MessageId) {
        self.thread = Some(ThreadView::resolving(chat_id, message_id));
        self.view_generation.bump();
    }

    /// Leave the thread view. Returns the thread so the driver can restore
    /// the origin chat.
    pub fn close_thread(&mut self) -> Option<ThreadView> {
        self.view_generation.bump();
        self.thread.take()
    }

    /// The thread whose replies are shown in `chat_id` (the open chat).
    pub fn thread_for_chat(&self, chat_id: ChatId) -> Option<&ThreadView> {
        self.thread
            .as_ref()
            .filter(|thread| thread.chat_id == chat_id && thread.thread_id != 0)
    }

    /// `message_thread_id` a send to `chat_id` is addressed to: `Some` while
    /// the open thread lives in that chat.
    pub fn thread_send_target(&self, chat_id: ChatId) -> Option<(i64, MessageId)> {
        let thread = self.thread_for_chat(chat_id)?;
        Some((thread.thread_id, MessageId(thread.thread_id)))
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
        let Some(thread) = self.thread.as_mut() else {
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
        }
        thread.unread_anchor = (thread.unread_count > 0
            && thread.last_read_inbox_message_id > 0)
            .then_some(MessageId(thread.last_read_inbox_message_id));
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

    /// One `getMessageThreadHistory` page.
    pub(crate) fn apply_thread_history(
        &mut self,
        messages: Vec<ParsedMessage>,
        pending: Option<&PendingRequest>,
    ) {
        let Some(RequestPurpose::GetMessageThreadHistory { message_id }) = pending.map(|p| p.purpose)
        else {
            return;
        };
        let origin_chat = pending.and_then(|p| p.chat_id);
        for message in &messages {
            self.remember_files(&message.files);
        }
        let Some(thread) = self.thread.as_mut() else {
            return;
        };
        if Some(thread.origin_chat_id) != origin_chat || thread.origin_message_id.0 != message_id {
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
        if empty || added == 0 || reached_root || oldest_reply.is_none() {
            thread.history.loaded_complete = true;
        }
        if let Some(oldest) = oldest_reply {
            thread.history.next_from_message_id = MessageId(oldest);
        }
        thread.history.total_count = thread.history.messages.len() as i32;
        if thread.status == ThreadStatus::LoadingHistory {
            thread.status = ThreadStatus::Ready;
        }
    }

    /// A thread request failed.
    pub(crate) fn fail_thread(&mut self, pending: Option<&PendingRequest>, line: String) {
        let Some(purpose) = pending.map(|p| p.purpose) else {
            return;
        };
        let (RequestPurpose::GetMessageThread { message_id }
        | RequestPurpose::GetMessageThreadHistory { message_id }) = purpose
        else {
            return;
        };
        let origin_chat = pending.and_then(|p| p.chat_id);
        if let Some(thread) = self.thread.as_mut()
            && Some(thread.origin_chat_id) == origin_chat
            && thread.origin_message_id.0 == message_id
            && thread.status != ThreadStatus::Ready
        {
            thread.status = ThreadStatus::Failed(line);
        }
    }

    /// Keep the thread's reply counter in sync with the root's
    /// `interaction_info.reply_info` updates.
    pub(crate) fn sync_thread_reply_info(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        reply: Option<&MessageReplyInfo>,
    ) {
        let Some(thread) = self.thread.as_mut() else {
            return;
        };
        let is_origin = chat_id == thread.origin_chat_id && message_id == thread.origin_message_id;
        let is_root = chat_id == thread.chat_id && thread.root_ids.contains(&message_id.0);
        if !(is_origin || is_root) {
            return;
        }
        if let Some(reply) = reply {
            thread.reply_count = reply.reply_count;
        }
    }

    /// A thread message arrived (update or own send): add it to the open
    /// thread's rows.
    pub(crate) fn thread_upsert(&mut self, row: HistoryMessage) {
        if let Some(thread) = self.thread.as_mut() {
            thread.history.upsert(row);
        }
    }

    /// Rows removed by `updateDeleteMessages`.
    pub(crate) fn thread_remove(&mut self, chat_id: ChatId, ids: &[MessageId]) {
        if let Some(thread) = self.thread.as_mut()
            && thread.chat_id == chat_id
        {
            for id in ids {
                thread.history.messages.remove(&id.0);
            }
        }
    }

    /// Whether `message` belongs to the open thread.
    pub(crate) fn thread_accepts(&self, message: &ParsedMessage) -> bool {
        self.thread
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
