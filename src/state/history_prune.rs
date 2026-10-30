//! Bounded memory for the message histories of chats nobody is looking at.
//!
//! `Session::histories` and `ThreadsState::topic_histories` used to keep
//! every chat's loaded window for the whole session, trimmed only per chat.
//! Telegram Desktop keeps rows for the chats on screen and lets the rest go
//! (`Data::Histories`, `History::clear`, `HistoryBlock` unloading); a chat
//! opened again is shown from what is cached and then loads what is
//! missing. This module does the same from state transitions, never from
//! a timer:
//!
//! - a chat (or forum topic) that stops being shown keeps only its newest
//!   [`CLOSED_TAIL_KEEP`] messages, enough to show at once on reopen;
//! - such tails are kept for the [`CLOSED_CHATS_MAX`] most recently used
//!   chats ([`CLOSED_TOPICS_MAX`] topics); the least recently used one is
//!   dropped whole;
//! - a chat that was left or deleted is dropped as soon as nothing shows it;
//! - live messages landing in a chat nobody shows keep that tail bounded
//!   ([`CLOSED_TAIL_CAP`]) and count as use of the chat;
//! - what is kept beside a chat's rows goes once nothing of the chat is
//!   kept: the replied-to messages fetched for its rows, their
//!   translations, its pinned list and sponsored messages (fetched again
//!   whenever the chat opens; the ads the user reported stay hidden), the
//!   index of its poll rows, and what its info panel loaded. A kept tail
//!   keeps them all, so rows paged back in show their reply strips and
//!   translations at once.
//!
//! A history is never pruned while any view shows it (the main window, a
//! chat window, a topic or comment thread of it) or while the UI holds it
//! ([`Session::set_history_holds`]: the source of a pending forward, the
//! chat a reply bar points at, the chat of the playing track, the chat an
//! open management box reads the member lists of), and an entry
//! with an unsent or failed local message is never dropped whole.
//!
//! A chat or topic that stops being shown is not trimmed in that same
//! event: it joins a release queue that [`Session::set_history_holds`]
//! settles once the UI's claims are in, so a claim made and a chat switched
//! in one event cannot trim the source. A release that finds the history
//! held stays queued until the hold lifts. Every history entry, however it
//! is created ([`Session::history_entry`]), joins the LRU unless a view,
//! the queue or a hold covers it, so the bound covers all of them.
use super::*;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

/// Newest messages a chat or topic keeps once it is no longer shown.
pub const CLOSED_TAIL_KEEP: usize = 60;
/// A closed chat's tail may grow to this many live messages before it is
/// trimmed back to [`CLOSED_TAIL_KEEP`].
pub const CLOSED_TAIL_CAP: usize = 100;
/// Closed chats that keep a tail; one more drops the least recently used.
pub const CLOSED_CHATS_MAX: usize = 16;
/// Closed forum topics that keep a tail.
pub const CLOSED_TOPICS_MAX: usize = 16;
/// Dropped chats whose deleted-message ids are remembered.
pub const DROPPED_MEMOS_MAX: usize = 64;
/// Deleted-message ids remembered per dropped chat (the newest ones).
pub const DROPPED_TOMBSTONES_MAX: usize = 256;

/// What a dropped history entry still knows that cannot be reloaded:
/// which messages were deleted and the newest server id ever seen.
#[derive(Debug, Default, Clone)]
struct DroppedHistory {
    tombstones: BTreeSet<i64>,
    latest_seen: i64,
}

/// Which closed histories are kept, most recently used first.
#[derive(Debug, Default)]
pub struct HistoryPruning {
    closed_chats: VecDeque<i64>,
    closed_topics: VecDeque<(i64, i32)>,
    holds: BTreeSet<ChatId>,
    /// Chats and topics that stopped being shown, in the order they closed,
    /// waiting for the UI's claims before they are trimmed.
    release_chats: VecDeque<i64>,
    release_topics: VecDeque<(i64, i32)>,
    dropped: BTreeMap<i64, DroppedHistory>,
    dropped_order: VecDeque<i64>,
    /// The histories are the only copy of the rows ([`Session::keep_all_histories`]).
    keep_all: bool,
}

impl HistoryPruning {
    /// The histories are the only copy of the rows: no server can load
    /// more ([`Session::keep_all_histories`]).
    pub fn is_offline_copy(&self) -> bool {
        self.keep_all
    }

    /// Closed chats with a kept tail, most recently used first.
    pub fn closed_chats(&self) -> impl Iterator<Item = ChatId> + '_ {
        self.closed_chats.iter().copied().map(ChatId)
    }

    /// Closed topics with a kept tail, most recently used first.
    pub fn closed_topics(&self) -> impl Iterator<Item = (ChatId, i32)> + '_ {
        self.closed_topics
            .iter()
            .map(|&(chat, topic)| (ChatId(chat), topic))
    }

    /// Chats the UI holds.
    pub fn held_chats(&self) -> impl Iterator<Item = ChatId> + '_ {
        self.holds.iter().copied()
    }

    /// Whether `chat_id` closed and its trim is still waiting for the UI's
    /// claims.
    pub fn release_pending(&self, chat_id: ChatId) -> bool {
        self.release_chats.contains(&chat_id.0)
    }

    /// Whether `message_id` was deleted in `chat_id` before its history
    /// entry was dropped.
    pub fn dropped_tombstone(&self, chat_id: ChatId, message_id: MessageId) -> bool {
        self.dropped
            .get(&chat_id.0)
            .is_some_and(|memo| memo.tombstones.contains(&message_id.0))
    }

    fn remember_dropped(&mut self, chat_id: i64, history: &HistoryState) {
        if history.tombstones.is_empty() && history.latest_seen <= 0 {
            return;
        }
        let memo = self.dropped.entry(chat_id).or_default();
        memo.tombstones.extend(history.tombstones.iter().copied());
        while memo.tombstones.len() > DROPPED_TOMBSTONES_MAX {
            memo.tombstones.pop_first();
        }
        memo.latest_seen = memo.latest_seen.max(history.latest_seen);
        self.dropped_order.retain(|id| *id != chat_id);
        self.dropped_order.push_back(chat_id);
        while self.dropped_order.len() > DROPPED_MEMOS_MAX {
            if let Some(oldest) = self.dropped_order.pop_front() {
                self.dropped.remove(&oldest);
            }
        }
    }

    fn take_dropped(&mut self, chat_id: i64) -> Option<DroppedHistory> {
        self.dropped_order.retain(|id| *id != chat_id);
        self.dropped.remove(&chat_id)
    }

    fn queue_chat(&mut self, chat_id: i64) {
        self.release_chats.retain(|id| *id != chat_id);
        self.release_chats.push_back(chat_id);
    }

    fn queue_topic(&mut self, key: (i64, i32)) {
        self.release_topics.retain(|k| *k != key);
        self.release_topics.push_back(key);
    }

    fn touch_chat(&mut self, chat_id: i64) {
        self.closed_chats.retain(|id| *id != chat_id);
        self.closed_chats.push_front(chat_id);
    }

    fn touch_topic(&mut self, key: (i64, i32)) {
        self.closed_topics.retain(|k| *k != key);
        self.closed_topics.push_front(key);
    }
}

/// The ids to drop to leave `keep` messages: the oldest ones, never a
/// pending or failed local send.
fn tail_excess(messages: &HistoryMessages, keep: usize) -> Vec<i64> {
    let excess = messages.len().saturating_sub(keep);
    messages
        .iter()
        .filter(|(_, message)| !message.pending && !message.failed)
        .map(|(id, _)| *id)
        .take(excess)
        .collect()
}

impl HistoryState {
    /// Keep the newest `keep` messages. The window no longer reaches the
    /// chat's start.
    pub(crate) fn trim_to_tail(&mut self, keep: usize) -> usize {
        let doomed = tail_excess(&self.messages, keep);
        self.drop_unread_bar_for(&doomed);
        for id in &doomed {
            self.messages.remove(id);
            self.visible.remove(id);
            self.viewed.remove(id);
        }
        if !doomed.is_empty() {
            self.loaded_complete = false;
        }
        doomed.len()
    }

    /// Whether dropping this entry would lose a local send in flight.
    fn has_unsent(&self) -> bool {
        self.messages
            .values()
            .any(|message| message.pending || message.failed)
    }
}

impl TopicHistory {
    /// [`HistoryState::trim_to_tail`] for a topic: the paging cursor moves
    /// to the oldest kept message so older pages continue from there.
    pub(crate) fn trim_to_tail(&mut self, keep: usize) -> usize {
        let doomed = tail_excess(&self.messages, keep);
        for id in &doomed {
            self.messages.remove(id);
        }
        if !doomed.is_empty() {
            self.loaded_complete = false;
            if let Some(oldest) = self.messages.keys().next() {
                self.next_from_message_id = MessageId(*oldest);
            }
        }
        doomed.len()
    }
}

impl Session {
    /// This session has no server to load rows back from: the screenshot
    /// demos seed every message in memory and answer no `getChatHistory`.
    /// A trimmed tail or a dropped entry would be lost for good, and a
    /// saved scroll position could never load around its row again, so
    /// nothing is trimmed on close or dropped by the LRU. Live messages in
    /// a chat nobody shows keep the page cap, as before pruning.
    pub fn keep_all_histories(&mut self) {
        self.history_pruning.keep_all = true;
        self.history_pruning.release_chats.clear();
        self.history_pruning.release_topics.clear();
        self.history_pruning.closed_chats.clear();
        self.history_pruning.closed_topics.clear();
    }

    /// The UI's claim on histories that are read while their chat is not
    /// shown: the source chat of a pending forward, the chat a reply bar
    /// points at, the chat of the playing track, the chat whose member
    /// lists an open management box reads. Replaces the previous
    /// claim, then settles the release queue: held chats are never pruned,
    /// and a chat whose hold just lifted is released.
    pub fn set_history_holds(&mut self, chats: impl IntoIterator<Item = ChatId>) {
        let holds: BTreeSet<ChatId> = chats.into_iter().collect();
        if holds != self.history_pruning.holds {
            let lifted: Vec<ChatId> = self
                .history_pruning
                .holds
                .difference(&holds)
                .copied()
                .collect();
            self.history_pruning.holds = holds;
            for chat_id in lifted {
                self.release_chat_history(chat_id);
            }
        }
        self.settle_history_releases();
    }

    /// A view (main window, chat window, topic or comment thread) shows
    /// `chat_id`.
    fn chat_shown_by_view(&self, chat_id: ChatId) -> bool {
        self.open_chat == Some(chat_id)
            || self.window_chats.contains(&chat_id)
            || self.window_chats.shows_chat(chat_id)
            || self
                .threads
                .thread
                .as_ref()
                .is_some_and(|thread| thread.chat_id == chat_id)
    }

    /// Whether some view shows `chat_id` or the UI holds it: its history
    /// must stay whole.
    pub fn chat_history_in_use(&self, chat_id: ChatId) -> bool {
        self.chat_shown_by_view(chat_id) || self.history_pruning.holds.contains(&chat_id)
    }

    fn topic_shown_by_view(&self, chat_id: ChatId, topic: i32) -> bool {
        (self.open_chat == Some(chat_id) && self.open_topic == Some(topic))
            || self.window_chats.shows_topic(chat_id, topic)
    }

    fn topic_history_in_use(&self, chat_id: ChatId, topic: i32) -> bool {
        self.history_pruning.holds.contains(&chat_id) || self.topic_shown_by_view(chat_id, topic)
    }

    /// The chat was left and is in no list: its history has no use.
    fn chat_is_gone(&self, chat_id: ChatId) -> bool {
        self.chats.get(&chat_id.0).is_some_and(|chat| {
            chat.my_member_status == Some(ChannelMemberStatus::Left)
                && !chat.in_main_list
                && !chat.in_archive
        })
    }

    /// A chat is shown again: it is no longer a closed tail.
    pub(crate) fn reclaim_chat_history(&mut self, chat_id: ChatId) {
        self.history_pruning
            .closed_chats
            .retain(|id| *id != chat_id.0);
    }

    /// A topic is shown again: it is no longer a closed tail.
    pub(crate) fn reclaim_topic_history(&mut self, chat_id: ChatId, topic: i32) {
        self.history_pruning
            .closed_topics
            .retain(|key| *key != (chat_id.0, topic));
    }

    /// `chat_id` stopped being shown by one view (the main window moved on
    /// or a chat window closed). It joins the release queue; once the UI's
    /// claims are in ([`Self::set_history_holds`]) and no view or hold uses
    /// it, its history and its topics shrink to a tail, or go whole for a
    /// chat that is gone.
    pub(crate) fn release_chat_history(&mut self, chat_id: ChatId) {
        if self.history_pruning.keep_all || self.chat_shown_by_view(chat_id) {
            return;
        }
        self.history_pruning.queue_chat(chat_id.0);
    }

    /// `topic` of `chat_id` stopped being shown; settled like
    /// [`Self::release_chat_history`].
    pub(crate) fn release_topic_history(&mut self, chat_id: ChatId, topic: i32) {
        if self.history_pruning.keep_all || self.topic_shown_by_view(chat_id, topic) {
            return;
        }
        self.history_pruning.queue_topic((chat_id.0, topic));
    }

    /// Trim what the release queue holds, in the order the views closed.
    /// An entry the UI still holds stays queued until its hold lifts.
    pub(crate) fn settle_history_releases(&mut self) {
        let chats = std::mem::take(&mut self.history_pruning.release_chats);
        for id in chats {
            let chat_id = ChatId(id);
            if self.chat_shown_by_view(chat_id) {
                continue;
            }
            if self.history_pruning.holds.contains(&chat_id) {
                self.history_pruning.release_chats.push_back(id);
                continue;
            }
            self.release_chat_history_now(chat_id);
        }
        let topics = std::mem::take(&mut self.history_pruning.release_topics);
        for (chat, topic) in topics {
            let chat_id = ChatId(chat);
            if self.topic_shown_by_view(chat_id, topic) {
                continue;
            }
            if self.history_pruning.holds.contains(&chat_id) {
                self.history_pruning.release_topics.push_back((chat, topic));
                continue;
            }
            self.release_topic_history_now(chat_id, topic);
        }
    }

    fn release_chat_history_now(&mut self, chat_id: ChatId) {
        if self.chat_history_in_use(chat_id) {
            return;
        }
        if self.chat_is_gone(chat_id) {
            self.forget_chat_history(chat_id);
            return;
        }
        if let Some(history) = self.histories.get_mut(&chat_id.0) {
            history.trim_to_tail(CLOSED_TAIL_KEEP);
            self.history_pruning.touch_chat(chat_id.0);
        }
        // Sorted, so the LRU order (and with it the eviction victim) does
        // not depend on the hash map's iteration order.
        let mut topics: Vec<i32> = self
            .threads
            .topic_histories
            .keys()
            .filter(|(chat, _)| *chat == chat_id.0)
            .map(|(_, topic)| *topic)
            .collect();
        topics.sort_unstable();
        for topic in topics {
            self.release_topic_history_now(chat_id, topic);
        }
        self.enforce_closed_bounds(None);
    }

    fn release_topic_history_now(&mut self, chat_id: ChatId, topic: i32) {
        if self.topic_history_in_use(chat_id, topic) {
            return;
        }
        if let Some(history) = self.threads.topic_histories.get_mut(&(chat_id.0, topic)) {
            history.trim_to_tail(CLOSED_TAIL_KEEP);
            self.history_pruning.touch_topic((chat_id.0, topic));
        }
        self.enforce_closed_bounds(None);
    }

    /// The history entry of `chat_id`, created when missing. An entry made
    /// for a chat no view, queue or hold covers joins the closed-chat LRU,
    /// so the number of kept histories stays bounded however they come to
    /// exist (a send answer, a delete, a read marker, a late page).
    pub(crate) fn history_entry(&mut self, chat_id: ChatId) -> &mut HistoryState {
        if !self.histories.contains_key(&chat_id.0) {
            let mut fresh = HistoryState::default();
            if let Some(memo) = self.history_pruning.take_dropped(chat_id.0) {
                fresh.tombstones.extend(memo.tombstones);
                fresh.latest_seen = memo.latest_seen;
            }
            self.histories.insert(chat_id.0, fresh);
            if !self.history_pruning.keep_all
                && !self.chat_history_in_use(chat_id)
                && !self.history_pruning.release_pending(chat_id)
            {
                self.history_pruning.touch_chat(chat_id.0);
                self.enforce_closed_bounds(Some(chat_id.0));
            }
        }
        self.histories.entry(chat_id.0).or_default()
    }

    /// [`Self::history_entry`] for a forum topic: a page that lands for a
    /// topic that was closed meanwhile recreates the entry inside the topic
    /// LRU, so [`CLOSED_TOPICS_MAX`] bounds it too.
    pub(crate) fn topic_history_entry(&mut self, chat_id: ChatId, topic: i32) -> &mut TopicHistory {
        let key = (chat_id.0, topic);
        let missing = !self.threads.topic_histories.contains_key(&key);
        if missing {
            self.threads.topic_histories.entry(key).or_default();
            if !self.history_pruning.keep_all
                && !self.topic_history_in_use(chat_id, topic)
                && !self.history_pruning.release_topics.contains(&key)
            {
                self.history_pruning.touch_topic(key);
                self.enforce_closed_bounds(None);
            }
        }
        self.threads.topic_histories.entry(key).or_default()
    }

    /// Drop a chat's history entry whole, keeping the deleted-message ids
    /// and the newest id seen in a small bounded side table.
    fn drop_history_entry(&mut self, chat_id: i64) {
        if let Some(history) = self.histories.remove(&chat_id) {
            self.history_pruning.remember_dropped(chat_id, &history);
        }
        self.history_pruning
            .closed_chats
            .retain(|id| *id != chat_id);
        self.forget_side_tables_once_unloaded(ChatId(chat_id));
    }

    /// Once nothing of `chat_id` is kept (no history entry, no topic),
    /// forget what was fetched beside its rows: the replied-to messages
    /// (`reply_targets`), the pinned list, the sponsored messages and the
    /// index of its poll rows, and what its info panel loaded (member
    /// lists, media counts). Opening the chat or its panel fetches them
    /// again. While a tail is kept they stay, so rows paged back in show
    /// their reply strips at once.
    fn forget_side_tables_once_unloaded(&mut self, chat_id: ChatId) {
        let kept = self.histories.contains_key(&chat_id.0)
            || self
                .threads
                .topic_histories
                .keys()
                .any(|(chat, _)| *chat == chat_id.0);
        if kept {
            return;
        }
        self.messages.pinned_messages.remove(&chat_id.0);
        self.messages
            .reply_targets
            .retain(|&(chat, _), _| chat != chat_id.0);
        let translate = &mut self.messages.translate;
        translate
            .messages
            .retain(|(chat, _, _), _| *chat != chat_id.0);
        // An answer still on its way would bring one back.
        translate.jobs.retain(|_, job| {
            !matches!(job.target, TranslateTarget::Message { chat_id: chat, .. } if chat == chat_id.0)
        });
        self.forget_sponsored_messages(chat_id);
        self.messages.poll_messages.retain(|_, rows| {
            rows.retain(|&(chat, _)| chat != chat_id.0);
            !rows.is_empty()
        });
        self.forget_info_panel_data(chat_id);
    }

    /// What the info panel of `chat_id` loaded: its media counts and member
    /// lists, unless the info panel shows the chat right now. The boxes
    /// that read member lists hold their chat (`ui::history_holds`), and a
    /// gallery shows its view's open chat, so neither reaches here. A list
    /// that goes marks the chat in `member_list_stale`, as a membership
    /// change does, so any reader asks again.
    fn forget_info_panel_data(&mut self, chat_id: ChatId) {
        let panel_shows = match self.users_state.open_info_panel {
            Some(InfoPanelTarget::Supergroup(id)) => self.chat_supergroup(chat_id) == Some(id),
            Some(InfoPanelTarget::BasicGroup(id)) => self.chats.get(&chat_id.0).is_some_and(
                |chat| matches!(chat.kind, ChatKind::BasicGroup { basic_group_id } if basic_group_id == id),
            ),
            _ => false,
        };
        if panel_shows {
            return;
        }
        // A count answer only fills the entry its fetch opened, so one
        // still on its way cannot bring this back.
        self.media.chat_media_counts.remove(&chat_id.0);
        // A first page still on its way would; its answer is dropped.
        let first_pages: Vec<RequestPurpose> = [
            MemberListFilter::Recent,
            MemberListFilter::Search,
            MemberListFilter::Administrators,
            MemberListFilter::Restricted,
            MemberListFilter::Banned,
        ]
        .map(|filter| RequestPurpose::Groups(GroupsPurpose::GetSupergroupMembers { filter }))
        .into_iter()
        .chain([RequestPurpose::GetBasicGroupFullInfo])
        .collect();
        for extra in self.requests.ids_for_chat(&first_pages, chat_id) {
            self.requests.take(extra);
        }
        let groups = &mut self.groups;
        let members = groups.supergroup_members.len();
        groups
            .supergroup_members
            .retain(|(chat, _), _| *chat != chat_id.0);
        let dropped = groups.basic_group_members.remove(&chat_id.0).is_some()
            || groups.supergroup_members.len() != members;
        if dropped && !groups.member_list_stale.contains(&chat_id.0) {
            groups.member_list_stale.push(chat_id.0);
        }
    }

    /// A chat that was left or deleted: its histories go when no view
    /// shows it (a local send still in flight keeps its entry).
    pub(crate) fn forget_chat_history(&mut self, chat_id: ChatId) {
        if self.chat_history_in_use(chat_id) {
            return;
        }
        if self
            .histories
            .get(&chat_id.0)
            .is_some_and(|history| !history.has_unsent())
        {
            self.drop_history_entry(chat_id.0);
        }
        self.threads
            .topic_histories
            .retain(|(chat, _), _| *chat != chat_id.0);
        self.history_pruning
            .closed_topics
            .retain(|(chat, _)| *chat != chat_id.0);
        self.forget_side_tables_once_unloaded(chat_id);
    }

    /// A live message landed in a chat no view shows: keep its tail
    /// bounded and mark the chat as recently used.
    pub(crate) fn note_closed_chat_update(&mut self, chat_id: ChatId) {
        if self.history_pruning.keep_all {
            self.trim_history_window(chat_id, WindowEnd::Oldest);
            return;
        }
        if let Some(history) = self.histories.get_mut(&chat_id.0) {
            history.trim_window_end(WindowEnd::Oldest, CLOSED_TAIL_CAP, CLOSED_TAIL_KEEP);
            self.history_pruning.touch_chat(chat_id.0);
            self.enforce_closed_bounds(None);
        }
    }

    /// Drop the least recently used closed histories beyond the bounds.
    fn enforce_closed_bounds(&mut self, keep: Option<i64>) {
        while self.history_pruning.closed_chats.len() > CLOSED_CHATS_MAX {
            let victim = self
                .history_pruning
                .closed_chats
                .iter()
                .rposition(|id| Some(*id) != keep && self.chat_history_evictable(ChatId(*id)));
            let Some(id) = victim.and_then(|at| self.history_pruning.closed_chats.remove(at))
            else {
                break;
            };
            self.drop_history_entry(id);
        }
        while self.history_pruning.closed_topics.len() > CLOSED_TOPICS_MAX {
            let victim = self
                .history_pruning
                .closed_topics
                .iter()
                .rposition(|&(chat, topic)| !self.topic_history_in_use(ChatId(chat), topic));
            let Some(key) = victim.and_then(|at| self.history_pruning.closed_topics.remove(at))
            else {
                break;
            };
            self.threads.topic_histories.remove(&key);
            self.forget_side_tables_once_unloaded(ChatId(key.0));
        }
    }

    fn chat_history_evictable(&self, chat_id: ChatId) -> bool {
        !self.chat_history_in_use(chat_id)
            && self
                .histories
                .get(&chat_id.0)
                .is_none_or(|history| !history.has_unsent())
    }
}
