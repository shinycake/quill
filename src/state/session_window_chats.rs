//! Chats shown in a window of their own (`parity:platform-new-window`).
//!
//! Telegram Desktop gives every separate chat window its own
//! `SessionController` with its own active chat; the data (history,
//! drafts, read state) is shared. Quill keeps one `Session`, so the main
//! window's chat stays `open_chat` and the chats shown in other windows are
//! listed here. A window chat stays open in TDLib while the main window
//! switches chats, its history pages apply whatever `open_chat` is, and its
//! rows report as viewed like the main window's.
use super::*;

impl Session {
    /// Whether `chat_id` is on screen: the main window's chat or one shown
    /// in its own window.
    pub fn chat_is_shown(&self, chat_id: ChatId) -> bool {
        self.open_chat == Some(chat_id) || self.window_chats.contains(&chat_id)
    }

    /// Whether a history page answers a view that is gone. The main
    /// window's pages die with its `view_generation` (bumped on every chat
    /// switch); a window chat's pages outlive those switches, only a
    /// replaced window (`reset_history_window`) drops them.
    pub(crate) fn history_view_stale(&self, pending: &PendingRequest) -> bool {
        if pending
            .chat_id
            .is_some_and(|chat_id| self.window_chats.contains(&chat_id))
        {
            return false;
        }
        pending.view_generation != Some(self.view_generation)
    }

    /// Record that `chat_id` is shown in its own window and start a fresh
    /// view of its history (unread anchor, nothing viewed yet). Returns
    /// whether the chat was not a window chat before.
    pub fn open_window_chat(&mut self, chat_id: ChatId) -> bool {
        let added = self.window_chats.insert(chat_id);
        if added {
            self.prepare_history_view(chat_id);
            self.prepare_history_window(chat_id);
        }
        added
    }

    /// The window showing `chat_id` closed. Returns whether it was a
    /// window chat.
    pub fn close_window_chat(&mut self, chat_id: ChatId) -> bool {
        self.window_chats.remove(&chat_id)
    }

    /// The main window shows no chat any more (Telegram Desktop
    /// `clearSectionStack` when the active chat moves to its own window):
    /// its topic, thread, Saved Messages views, search and gallery close
    /// with it; the history itself stays loaded.
    pub fn leave_open_chat(&mut self) -> Option<ChatId> {
        let prev = self.open_chat.take()?;
        self.search.chat_search.close();
        self.media.shared_media.close();
        self.open_topic = None;
        self.threads.saved.close_views();
        if self
            .threads
            .thread
            .as_ref()
            .is_some_and(|thread| thread.chat_id == prev)
        {
            self.threads.thread = None;
        }
        self.view_generation.bump();
        Some(prev)
    }
}

/// The part of the session that is a window's view rather than the
/// account's data: what the window looks at and the views layered on it.
/// Telegram Desktop keeps these per `SessionController`; Quill keeps the
/// main window's in `Session` and swaps a chat window's in while that
/// window is drawn or handles input (`ui::chat_window`).
#[derive(Debug, Default)]
pub struct SessionView {
    pub open_chat: Option<ChatId>,
    pub open_topic: Option<i32>,
    pub thread: Option<ThreadView>,
    pub saved_sublist: Option<SavedSublistView>,
    pub saved_tag_search: Option<SavedTagSearch>,
    pub chat_search: ChatSearchState,
    pub shared_media: SharedMediaState,
}

impl SessionView {
    /// A fresh view of `chat_id`: its plain history, nothing layered.
    pub fn of_chat(chat_id: ChatId) -> Self {
        Self {
            open_chat: Some(chat_id),
            ..Self::default()
        }
    }
}

impl Session {
    /// Exchange the session's view with `view` (symmetric: calling it
    /// twice restores the original).
    pub fn swap_view(&mut self, view: &mut SessionView) {
        std::mem::swap(&mut self.open_chat, &mut view.open_chat);
        std::mem::swap(&mut self.open_topic, &mut view.open_topic);
        std::mem::swap(&mut self.threads.thread, &mut view.thread);
        std::mem::swap(&mut self.threads.saved.sublist, &mut view.saved_sublist);
        std::mem::swap(
            &mut self.threads.saved.tag_search,
            &mut view.saved_tag_search,
        );
        std::mem::swap(&mut self.search.chat_search, &mut view.chat_search);
        std::mem::swap(&mut self.media.shared_media, &mut view.shared_media);
    }
}
