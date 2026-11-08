//! Every view on screen, whichever is in the session's place.
//!
//! The session keeps one view in place (`open_chat`, `open_topic`,
//! `threads.thread`, the Saved Messages sublist and tag filter): the main
//! window's, or a chat window's while that window is drawn or handles
//! input. The others wait in [`WindowViews`]. During `ingest` the main
//! window's view is in place, so an update path that reads those fields
//! as "what is on screen" misses every chat window: its new messages
//! graded for the chat list only, its reply thread with no live rows, its
//! taps with no effect (review round 11, area A).
//!
//! Update paths ask this module instead. [`Session::views_shown`] reads
//! every view where it is kept; [`Session::in_every_view`] runs an edit of
//! the view's own rows with each view in place in turn. Both see the main
//! window's view and every chat window's.
use super::*;

/// One window's view, read where it is kept.
#[derive(Clone, Copy, Debug)]
pub struct ShownView<'a> {
    /// The chat window it belongs to (`None`: the main window).
    pub window: Option<ChatId>,
    pub open_chat: Option<ChatId>,
    pub open_topic: Option<i32>,
    pub thread: Option<&'a ThreadView>,
    pub saved_sublist: Option<&'a SavedSublistView>,
    pub saved_tag_search: Option<&'a SavedTagSearch>,
}

impl<'a> ShownView<'a> {
    /// Whether the conversation pane draws `chat_id`: the view's chat, or
    /// the comment thread shown in its place (the chat of its replies, or
    /// the chat of the post it comments, which its header names).
    pub fn draws_chat(&self, chat_id: ChatId) -> bool {
        self.open_chat == Some(chat_id)
            || self
                .thread
                .is_some_and(|thread| thread.chat_id == chat_id || thread.origin_chat_id == chat_id)
    }

    /// The rows this view keeps beside the chat's own history: the comment
    /// thread's (in `thread.chat_id`) and the Saved Messages sublist's and
    /// tag filter's (in Saved Messages, `saved_chat`).
    fn layered_histories(
        self,
        saved_chat: Option<ChatId>,
    ) -> impl Iterator<Item = (ChatId, &'a TopicHistory)> {
        let thread = self.thread.map(|thread| (thread.chat_id, &thread.history));
        let saved = saved_chat.into_iter().flat_map(move |chat| {
            self.saved_sublist
                .map(|view| (chat, &view.history))
                .into_iter()
                .chain(self.saved_tag_search.map(|search| (chat, &search.history)))
        });
        thread.into_iter().chain(saved)
    }
}

impl Session {
    /// Every view on screen: the one in place, then each one kept in
    /// [`WindowViews`]. Each says which window it is.
    pub fn views_shown(&self) -> impl Iterator<Item = ShownView<'_>> {
        let in_place_window = self.view_in_place();
        let in_place = ShownView {
            window: in_place_window,
            open_chat: self.open_chat,
            open_topic: self.open_topic,
            thread: self.threads.thread.as_ref(),
            saved_sublist: self.threads.saved.sublist.as_ref(),
            saved_tag_search: self.threads.saved.tag_search.as_ref(),
        };
        // While a chat window's view is in place, its entry holds the
        // main window's view (`Session::enter_view`).
        let stored = self
            .window_chats
            .stored()
            .map(move |(key, view)| ShownView {
                window: (Some(*key) != in_place_window).then_some(*key),
                open_chat: view.open_chat,
                open_topic: view.open_topic,
                thread: view.thread.as_ref(),
                saved_sublist: view.saved_sublist.as_ref(),
                saved_tag_search: view.saved_tag_search.as_ref(),
            });
        std::iter::once(in_place).chain(stored)
    }

    /// Whether any window's conversation pane draws `chat_id`
    /// ([`ShownView::draws_chat`]).
    pub fn chat_drawn(&self, chat_id: ChatId) -> bool {
        self.views_shown().any(|view| view.draws_chat(chat_id))
    }

    /// Every row a window can draw: the loaded history of each chat on
    /// screen ([`Self::chats_shown`]) and each view's comment thread and
    /// Saved Messages rows.
    pub fn shown_messages(&self) -> impl Iterator<Item = &HistoryMessage> {
        let saved_chat = self.my_user_id.map(ChatId);
        let chats = self
            .chats_shown()
            .into_iter()
            .filter_map(|chat| self.histories.get(&chat.0))
            .flat_map(|history| history.messages.values());
        let layered = self
            .views_shown()
            .flat_map(move |view| view.layered_histories(saved_chat))
            .flat_map(|(_, history)| history.messages.values());
        chats.chain(layered)
    }

    /// Run `f` once with each view in the session's place: the main
    /// window's, then each chat window's, and put back the view that was.
    /// For edits of a view's own rows (`threads.thread`, the Saved
    /// Messages views); requests `f` registers are stamped with the view
    /// it ran in. Inside a closed window's throwaway view
    /// (`apply_payload_in_asking_view`) only that view runs: the main
    /// window's is held outside the session then.
    pub(crate) fn in_every_view(&mut self, mut f: impl FnMut(&mut Self)) {
        let outer = self.view_in_place();
        if self.window_chats.is_empty()
            || outer.is_some_and(|window| !self.window_chats.contains(&window))
        {
            f(self);
            return;
        }
        let windows: Vec<ChatId> = self.window_chats.iter().copied().collect();
        for view in std::iter::once(None).chain(windows.into_iter().map(Some)) {
            if self.enter_view(view) {
                f(self);
            }
        }
        self.enter_view(outer);
    }

    /// Apply `edit` to the in-place view's copies of one message: its
    /// comment thread's row and its Saved Messages sublist and tag filter
    /// rows. Returns whether a copy was found. Run it through
    /// [`Self::in_every_view`] (`edit_loaded_message` does).
    pub(crate) fn edit_view_copy(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        edit: &mut dyn FnMut(&mut HistoryMessage),
    ) -> bool {
        let saved = self.is_saved_messages(chat_id);
        let thread = self
            .threads
            .thread
            .as_mut()
            .filter(|thread| thread.chat_id == chat_id)
            .map(|thread| &mut thread.history);
        let saved_views = &mut self.threads.saved;
        let sublist = saved_views.sublist.as_mut().map(|view| &mut view.history);
        let tags = saved_views
            .tag_search
            .as_mut()
            .map(|search| &mut search.history);
        let saved_rows = sublist.into_iter().chain(tags).filter(|_| saved);
        let mut found = false;
        for history in thread.into_iter().chain(saved_rows) {
            if let Some(message) = history.messages.get_mut(&message_id.0) {
                edit(message);
                found = true;
            }
        }
        found
    }

    /// A sent message got its server id (or failed): the in-place view's
    /// comment thread swaps its temporary row for `row`, as the chat's own
    /// history does. Run it through [`Self::in_every_view`].
    pub(crate) fn replace_view_copy(&mut self, old_message_id: MessageId, row: &HistoryMessage) {
        if let Some(thread) = self.threads.thread.as_mut()
            && thread.chat_id == row.chat_id
            && thread.history.messages.contains_key(&old_message_id.0)
        {
            thread.history.replace_id(old_message_id, row.clone());
        }
    }

    /// Messages deleted for good: an in-chat search jump of the in-place
    /// view that waits for one of them finds it missing. Run it through
    /// [`Self::in_every_view`].
    pub(crate) fn miss_search_jump(&mut self, deleted: &[MessageId]) {
        let jump = &mut self.search.chat_search.jump;
        if let ChatSearchJump::Ready { message_id } | ChatSearchJump::Loading { message_id } = *jump
            && deleted.contains(&message_id)
        {
            *jump = ChatSearchJump::Missing { message_id };
        }
    }

    /// The loaded row of `message_id` in `chat_id`, wherever it is kept:
    /// the chat's history, a loaded forum topic, or a view's comment
    /// thread or Saved Messages rows, in any window. The first copy found.
    pub fn loaded_message(
        &self,
        chat_id: ChatId,
        message_id: MessageId,
    ) -> Option<&HistoryMessage> {
        if let Some(message) = self
            .histories
            .get(&chat_id.0)
            .and_then(|history| history.messages.get(&message_id.0))
        {
            return Some(message);
        }
        let topics = self
            .threads
            .topic_histories
            .iter()
            .filter(|((topic_chat, _), _)| *topic_chat == chat_id.0)
            .map(|(_, topic)| topic);
        if let Some(message) = topics
            .filter_map(|topic| topic.messages.get(&message_id.0))
            .next()
        {
            return Some(message);
        }
        let saved_chat = self.my_user_id.map(ChatId);
        self.views_shown()
            .flat_map(|view| view.layered_histories(saved_chat))
            .filter(|(chat, _)| *chat == chat_id)
            .find_map(|(_, history)| history.messages.get(&message_id.0))
    }
}
