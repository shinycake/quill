#![deny(clippy::indexing_slicing)]

use super::*;
use std::hash::{Hash, Hasher};

pub(in crate::ui) struct RowsSource {
    pub(in crate::ui) list: &'static str,
    pub(in crate::ui) chat: Option<ChatId>,
    pub(in crate::ui) scope: i64,
    pub(in crate::ui) messages_revision: u64,
    pub(in crate::ui) extra: u64,
    pub(in crate::ui) window_epoch: u64,
    pub(in crate::ui) unread_anchor: Option<MessageId>,
}

#[derive(Clone, Debug, PartialEq)]
pub(in crate::ui) struct HistoryRowsKey {
    list: &'static str,
    chat: Option<i64>,
    scope: i64,
    messages_revision: u64,
    extra: u64,
    window_epoch: u64,
    unread_anchor: Option<MessageId>,
    highlight: Option<MessageId>,
    ui: u64,
    translate: u64,
    today: i64,
}

fn hash_of(value: impl Hash) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

impl QuillApp {
    /// The key for the rows of `source` and, when the rows built last
    /// cannot be reused, the messages to build them from.
    pub(in crate::ui) fn history_rows_input(
        &self,
        source: RowsSource,
        highlight: Option<MessageId>,
        messages: impl FnOnce() -> Vec<HistoryMessage>,
    ) -> (HistoryRowsKey, Option<Vec<HistoryMessage>>) {
        let today = quill::local_time::civil_local(quill::local_time::now_unix()).day_number();
        let key = HistoryRowsKey {
            list: source.list,
            chat: source.chat.map(|chat| chat.0),
            scope: source.scope,
            messages_revision: source.messages_revision,
            extra: source.extra,
            window_epoch: source.window_epoch,
            unread_anchor: source.unread_anchor,
            highlight,
            ui: self.history_rows_ui_hash(),
            translate: self.translate_revision(),
            today,
        };
        let reuse = self.history.rows_key.as_ref() == Some(&key);
        (key, (!reuse).then(messages))
    }

    /// The open chat's own history, with the translations shown in place
    /// and the joined-channel notice at its end.
    pub(in crate::ui) fn main_rows_input(
        &self,
        chat: Option<ChatId>,
        topic: Option<i32>,
        history: Option<&quill::state::HistoryState>,
        highlight: Option<MessageId>,
    ) -> (HistoryRowsKey, Option<Vec<HistoryMessage>>) {
        let session = self.session();
        let joined = chat.and_then(|chat| {
            let strip = session?.similar_strip(chat)?;
            Some((strip.joined_at, strip.expanded))
        });
        let source = RowsSource {
            list: "session-history",
            chat,
            scope: topic.map_or(0, i64::from),
            messages_revision: history.map_or(0, |h| h.messages.revision()),
            extra: hash_of(joined),
            window_epoch: history.map_or(0, |h| h.window_epoch),
            unread_anchor: history.and_then(|h| h.unread_anchor),
        };
        self.history_rows_input(source, highlight, || {
            let mut messages: Vec<HistoryMessage> = history
                .map(|h| h.ordered().into_iter().cloned().collect())
                .unwrap_or_default();
            if let Some(chat_id) = chat {
                self.apply_chat_translation(chat_id, &mut messages);
                if let Some(notice) = session.and_then(|s| s.joined_notice_row(chat_id)) {
                    messages.push(notice);
                }
            }
            messages
        })
    }

    pub(in crate::ui) fn topic_rows_input(
        &self,
        chat: Option<ChatId>,
        topic: Option<i32>,
        history: Option<&quill::state::TopicHistory>,
        highlight: Option<MessageId>,
    ) -> (HistoryRowsKey, Option<Vec<HistoryMessage>>) {
        let source = RowsSource {
            list: "topic-history",
            chat,
            scope: topic.map_or(0, i64::from),
            messages_revision: history.map_or(0, |h| h.messages.revision()),
            extra: 0,
            window_epoch: 0,
            unread_anchor: None,
        };
        self.history_rows_input(source, highlight, || {
            history
                .map(|h| h.ordered().into_iter().cloned().collect())
                .unwrap_or_default()
        })
    }

    /// A comment or reply thread: its root post, the "Discussion started"
    /// divider and the replies (`ThreadView::rows`).
    pub(in crate::ui) fn thread_rows_input(
        &self,
        thread: &quill::state::ThreadView,
        highlight: Option<MessageId>,
    ) -> (HistoryRowsKey, Option<Vec<HistoryMessage>>) {
        let source = RowsSource {
            list: "thread-history",
            chat: Some(thread.chat_id),
            scope: thread.thread_id,
            messages_revision: thread.history.messages.revision(),
            extra: hash_of(&thread.root_ids),
            window_epoch: thread.window_epoch,
            unread_anchor: thread.unread_anchor,
        };
        self.history_rows_input(source, highlight, || thread.rows())
    }

    pub(in crate::ui) fn saved_rows_input(
        &self,
        highlight: Option<MessageId>,
    ) -> Option<(HistoryRowsKey, Option<Vec<HistoryMessage>>)> {
        let (scope, history) = self.saved_history()?;
        let source = RowsSource {
            list: "saved-history",
            chat: self.session().and_then(|s| s.open_chat),
            scope,
            messages_revision: history.messages.revision(),
            extra: 0,
            window_epoch: 0,
            unread_anchor: None,
        };
        Some(self.history_rows_input(source, highlight, || {
            history.ordered().into_iter().cloned().collect()
        }))
    }

    fn history_rows_ui_hash(&self) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.forward_draft()
            .map(|draft| (draft.from_chat_id.0, draft.message_ids))
            .hash(&mut hasher);
        self.stream_rows_hash().hash(&mut hasher);
        let mut fact_checks: Vec<(i64, i64)> = self
            .message_ui
            .fact_check_expanded
            .iter()
            .copied()
            .collect();
        fact_checks.sort_unstable();
        fact_checks.hash(&mut hasher);
        self.vanish_rows_hash().hash(&mut hasher);
        self.chat_wide().hash(&mut hasher);
        hasher.finish()
    }

    pub(super) fn chrome_follows_revision(&self) -> bool {
        self.live.is_some() || super::demo::demo_stress_size().is_some()
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "rows_cache_tests.rs"]
mod tests;
