#![deny(clippy::indexing_slicing)]

use super::*;
use std::hash::{Hash, Hasher};

pub(in crate::ui) struct RowsSource<'a> {
    pub(in crate::ui) list: &'static str,
    pub(in crate::ui) chat: Option<ChatId>,
    pub(in crate::ui) scope: i64,
    /// The messages the rows show, when the list has them.
    pub(in crate::ui) messages: Option<&'a quill::state::HistoryMessages>,
    pub(in crate::ui) extra: u64,
    pub(in crate::ui) window_epoch: u64,
    pub(in crate::ui) unread_anchor: Option<MessageId>,
    /// The chat whose translations the rows show in place.
    pub(in crate::ui) translated: Option<ChatId>,
    /// Rows built after the messages (the joined notice). `None` when the
    /// rows are not the messages in id order, so a message coming or going
    /// builds them all again.
    pub(in crate::ui) tail: Option<usize>,
}

#[derive(Clone, Debug, PartialEq)]
pub(in crate::ui) struct HistoryRowsKey {
    list: &'static str,
    chat: Option<i64>,
    scope: i64,
    /// Which `HistoryMessages` the revisions belong to.
    messages_instance: u64,
    messages_revision: u64,
    ids_revision: u64,
    extra: u64,
    window_epoch: u64,
    unread_anchor: Option<MessageId>,
    highlight: Option<MessageId>,
    ui: u64,
    translate: u64,
    today: i64,
}

impl HistoryRowsKey {
    /// Same rows as `other` except for the messages: changed in place,
    /// added or removed.
    fn differs_only_in_messages(&self, other: &Self) -> bool {
        self.messages_revision != other.messages_revision
            && *self
                == Self {
                    messages_revision: self.messages_revision,
                    ids_revision: self.ids_revision,
                    ..other.clone()
                }
    }
}

/// What the history list does with its rows this render.
pub(in crate::ui) enum RowsInput {
    /// Nothing feeding the rows changed.
    Keep,
    /// These messages changed in place: `(row, new message)`, each row's
    /// place among its neighbours the same.
    Patch(Vec<(usize, HistoryMessage)>),
    /// Messages came or went: build again only the rows around them.
    Splice(super::row_splice::RowSplice),
    /// Build every row from these messages (oldest first).
    Build(Vec<HistoryMessage>),
}

/// Changed messages past which the rows are built again instead.
pub(super) const PATCH_MAX: usize = 32;

fn hash_of(value: impl Hash) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

/// Whether `new` keeps the place `old` had among its neighbours: the same
/// sender run, day and album grouping (`history_message_list`).
fn same_place(old: &HistoryMessage, new: &HistoryMessage) -> bool {
    old.sender == new.sender
        && old.is_outgoing == new.is_outgoing
        && old.date == new.date
        && old.media_album_id == new.media_album_id
        && quill::album::is_album_media(&old.content) == quill::album::is_album_media(&new.content)
}

impl QuillApp {
    /// The key for the rows of `source` and what to do with the rows built
    /// last: keep them, patch the messages changed in place, or build them
    /// again from `messages`.
    pub(in crate::ui) fn history_rows_input(
        &self,
        source: RowsSource,
        highlight: Option<MessageId>,
        messages: impl FnOnce() -> Vec<HistoryMessage>,
    ) -> (HistoryRowsKey, RowsInput) {
        let today = quill::local_time::civil_local(quill::local_time::now_unix()).day_number();
        let key = HistoryRowsKey {
            list: source.list,
            chat: source.chat.map(|chat| chat.0),
            scope: source.scope,
            messages_instance: source.messages.map_or(0, |m| m.instance()),
            messages_revision: source.messages.map_or(0, |m| m.revision()),
            ids_revision: source.messages.map_or(0, |m| m.ids_revision()),
            extra: source.extra,
            window_epoch: source.window_epoch,
            unread_anchor: source.unread_anchor,
            highlight,
            ui: self.history_rows_ui_hash(),
            translate: self.translate_revision(),
            today,
        };
        let input = match self.history.rows_key.as_ref() {
            Some(old) if *old == key => RowsInput::Keep,
            Some(old) if key.differs_only_in_messages(old) => self
                .changed_rows(&source, old, &key)
                .unwrap_or_else(|| RowsInput::Build(messages())),
            _ => RowsInput::Build(messages()),
        };
        (key, input)
    }

    /// Patch the rows when messages only changed in place and kept their
    /// place, else build again the rows around the changes; `None` when
    /// every row must be built.
    fn changed_rows(
        &self,
        source: &RowsSource,
        old: &HistoryRowsKey,
        new: &HistoryRowsKey,
    ) -> Option<RowsInput> {
        let list = source.messages?;
        if old.ids_revision == new.ids_revision
            && let Some(patch) =
                self.in_place_changes(list, old.messages_revision, source.translated)
        {
            return Some(RowsInput::Patch(patch));
        }
        self.row_splice(
            list,
            old.messages_instance,
            old.messages_revision,
            source.translated,
            source.tail?,
        )
        .map(RowsInput::Splice)
    }

    /// The messages of `list` changed after `since`, each with the row that
    /// shows it, when every one keeps its place; `None` when the rows must
    /// be built again.
    fn in_place_changes(
        &self,
        list: &quill::state::HistoryMessages,
        since: u64,
        translated: Option<ChatId>,
    ) -> Option<Vec<(usize, HistoryMessage)>> {
        let ids = list.changed_since(since);
        if ids.len() > PATCH_MAX {
            return None;
        }
        let mut changed: Vec<HistoryMessage> =
            ids.iter().filter_map(|id| list.get(id).cloned()).collect();
        if let Some(chat_id) = translated {
            self.apply_chat_translation(chat_id, &mut changed);
        }
        let rows = &self.history.rows;
        changed
            .into_iter()
            .map(|message| {
                let ix = rows.iter().position(|row| row.contains(message.id))?;
                let old = rows.get(ix)?.message(message.id)?;
                same_place(old, &message).then_some((ix, message))
            })
            .collect()
    }

    /// The open chat's own history, with the translations shown in place
    /// and the joined-channel notice at its end.
    pub(in crate::ui) fn main_rows_input(
        &self,
        chat: Option<ChatId>,
        topic: Option<i32>,
        history: Option<&quill::state::HistoryState>,
        highlight: Option<MessageId>,
    ) -> (HistoryRowsKey, RowsInput) {
        let session = self.session();
        let joined = chat.and_then(|chat| {
            let strip = session?.similar_strip(chat)?;
            Some((strip.joined_at, strip.expanded))
        });
        let source = RowsSource {
            list: "session-history",
            chat,
            scope: topic.map_or(0, i64::from),
            messages: history.map(|h| &h.messages),
            extra: hash_of(joined),
            window_epoch: history.map_or(0, |h| h.window_epoch),
            unread_anchor: history.and_then(|h| h.unread_anchor),
            translated: chat,
            tail: Some(usize::from(chat.is_some() && joined.is_some())),
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
    ) -> (HistoryRowsKey, RowsInput) {
        let source = RowsSource {
            list: "topic-history",
            chat,
            scope: topic.map_or(0, i64::from),
            messages: history.map(|h| &h.messages),
            extra: 0,
            window_epoch: history.map_or(0, |h| h.window_epoch),
            unread_anchor: None,
            translated: None,
            tail: Some(0),
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
    ) -> (HistoryRowsKey, RowsInput) {
        let source = RowsSource {
            list: "thread-history",
            chat: Some(thread.chat_id),
            scope: thread.thread_id,
            messages: Some(&thread.history.messages),
            extra: hash_of(&thread.root_ids),
            window_epoch: thread.history.window_epoch,
            unread_anchor: thread.unread_anchor,
            translated: None,
            // The root post and the "Discussion started" divider head the
            // replies.
            tail: None,
        };
        self.history_rows_input(source, highlight, || thread.rows())
    }

    pub(in crate::ui) fn saved_rows_input(
        &self,
        highlight: Option<MessageId>,
    ) -> Option<(HistoryRowsKey, RowsInput)> {
        let (scope, history) = self.saved_history()?;
        let source = RowsSource {
            list: "saved-history",
            chat: self.session().and_then(|s| s.open_chat),
            scope,
            messages: Some(&history.messages),
            extra: 0,
            window_epoch: 0,
            unread_anchor: None,
            translated: None,
            tail: Some(0),
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
        #[cfg(test)]
        if self.history.follow_revision_in_tests {
            return true;
        }
        self.live.is_some() || super::demo::demo_stress_size().is_some()
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "rows_cache_tests.rs"]
mod tests;
