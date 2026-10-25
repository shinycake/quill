//! Chats shown in a window of their own (`parity:platform-new-window`).
//!
//! Telegram Desktop gives every separate chat window its own
//! `SessionController` with its own active chat; the data (history,
//! drafts, read state) is shared. Quill keeps one `Session`, so the main
//! window's chat stays `open_chat` and the chats shown in other windows are
//! listed here. A window chat stays open in TDLib while the main window
//! switches chats, its history pages apply whatever `open_chat` is, and its
//! rows report as viewed like the main window's.
//!
//! Each window's view (its search, gallery, thread, Saved Messages views)
//! is kept here by chat ([`WindowViews`]) and put in the session's place
//! while that window is drawn or handles input ([`Session::enter_view`]).
//! A request remembers whose view was in place when it was registered
//! (`PendingRequest::window`), and an answer that lands in a view applies
//! to that one, whichever view is in place when it arrives.
use super::*;
use std::collections::BTreeMap;

impl Session {
    /// Whether `chat_id` is on screen: the main window's chat or one shown
    /// in its own window, whichever view is in place.
    pub fn chat_is_shown(&self, chat_id: ChatId) -> bool {
        self.open_chat == Some(chat_id)
            || self.window_chats.contains(&chat_id)
            || self.window_chats.shows_chat(chat_id)
    }

    /// Every chat on screen: the main window's and each chat window's,
    /// whichever view is in place.
    pub fn chats_shown(&self) -> Vec<ChatId> {
        let mut chats: Vec<ChatId> = self
            .open_chat
            .into_iter()
            .chain(self.window_chats.iter().copied())
            .chain(
                self.window_chats
                    .views
                    .values()
                    .filter_map(|view| view.open_chat),
            )
            .collect();
        chats.sort_unstable_by_key(|chat| chat.0);
        chats.dedup();
        chats
    }

    /// Record that `chat_id` is shown in its own window and start a fresh
    /// view of its history (unread anchor, nothing viewed yet). Returns
    /// whether the chat was not a window chat before.
    pub fn open_window_chat(&mut self, chat_id: ChatId) -> bool {
        if self.window_chats.contains(&chat_id) {
            return false;
        }
        self.window_chats
            .views
            .insert(chat_id, SessionView::of_chat(chat_id));
        self.prepare_history_view(chat_id);
        self.prepare_history_window(chat_id);
        true
    }

    /// The window showing `chat_id` closed: its view goes. Returns whether
    /// it was a window chat.
    pub fn close_window_chat(&mut self, chat_id: ChatId) -> bool {
        if self.view_in_place() == Some(chat_id) {
            self.enter_view(None);
        }
        let was_window = self.window_chats.views.remove(&chat_id).is_some();
        if was_window {
            self.release_chat_history(chat_id);
        }
        was_window
    }

    /// The chat window whose view is in the session's place (`None`: the
    /// main window's).
    pub fn view_in_place(&self) -> Option<ChatId> {
        self.requests.window
    }

    /// Put `target`'s view in the session's place (`None`: the main
    /// window's). Returns whether it is in place: `false` for a chat with
    /// no window, which leaves the main window's view in place and
    /// records a diagnostic (the UI asks only for windows it has, so a
    /// miss means the two disagree).
    pub fn enter_view(&mut self, target: Option<ChatId>) -> bool {
        if self.requests.window == target {
            return true;
        }
        if let Some(current) = self.requests.window.take() {
            self.swap_stored_view(current);
        }
        match target {
            Some(chat_id) if self.window_chats.contains(&chat_id) => {
                self.swap_stored_view(chat_id);
                self.requests.window = Some(chat_id);
                true
            }
            Some(chat_id) => {
                self.diagnostics.record(Diagnostic {
                    category: "window",
                    type_name: None,
                    extra: Some(chat_id.0.unsigned_abs()),
                    seq: None,
                    note: "no-window-view",
                });
                false
            }
            None => true,
        }
    }

    /// Run `f` with `window`'s view in place (`None`: the main window's),
    /// then put back the view that was. `None`, without running `f`, when
    /// that window is gone: its work must not land in another view.
    pub fn with_view<R>(
        &mut self,
        window: Option<ChatId>,
        f: impl FnOnce(&mut Self) -> R,
    ) -> Option<R> {
        if window.is_some_and(|chat_id| !self.window_chats.contains(&chat_id)) {
            return None;
        }
        let outer = self.view_in_place();
        self.enter_view(window);
        let result = f(self);
        self.enter_view(outer);
        Some(result)
    }

    /// Apply an answer with the view that asked for it in place. An answer
    /// that lands in a view ([`lands_in_view`]) goes to the view that was
    /// in place when its request was registered; a window closed since
    /// gets a throwaway view, so the answer lands in no window's view
    /// while the account's data it carries (messages, files) still applies;
    /// requests registered meanwhile are stamped with the closed window
    /// too, so their answers land nowhere either.
    pub(crate) fn apply_payload_in_asking_view(
        &mut self,
        payload: EnvelopePayload,
        pending: Option<&PendingRequest>,
        extra: Option<RequestId>,
        seq: u64,
    ) {
        let Some(asked_in) = pending
            .filter(|pending| lands_in_view(pending.purpose))
            .map(|pending| pending.window)
        else {
            return self.apply_payload(payload, pending, extra, seq);
        };
        let outer = self.view_in_place();
        if asked_in.is_none_or(|chat_id| self.window_chats.contains(&chat_id)) {
            self.enter_view(asked_in);
            self.apply_payload(payload, pending, extra, seq);
        } else {
            self.enter_view(None);
            let mut detached = SessionView::default();
            self.swap_view(&mut detached);
            self.requests.window = asked_in;
            self.apply_payload(payload, pending, extra, seq);
            self.requests.window = None;
            self.swap_view(&mut detached);
        }
        self.enter_view(outer);
    }

    /// Exchange the session's view with the one kept for `chat_id`'s
    /// window (symmetric).
    fn swap_stored_view(&mut self, chat_id: ChatId) {
        if let Some(mut view) = self.window_chats.views.remove(&chat_id) {
            self.swap_view(&mut view);
            self.window_chats.views.insert(chat_id, view);
        }
    }

    /// The main window shows no chat any more (Telegram Desktop
    /// `clearSectionStack` when the active chat moves to its own window):
    /// its topic, thread, Saved Messages views, search and gallery close
    /// with it; the history itself stays loaded.
    pub fn leave_open_chat(&mut self) -> Option<ChatId> {
        let prev = self.open_chat.take()?;
        self.chat_list.psa_about.hide();
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
        self.release_chat_history(prev);
        Some(prev)
    }
}

/// Whether the answer to a `purpose` request lands in a window's view
/// (`SessionView`): the in-chat search and its jumps, the gallery, a
/// comment thread, a Saved Messages sublist or tag search, and the
/// sponsored rows (kept by chat, but accepted only for the open one).
fn lands_in_view(purpose: RequestPurpose) -> bool {
    matches!(
        purpose,
        RequestPurpose::SearchChatMessages
            | RequestPurpose::SearchChatMessagesMore
            | RequestPurpose::SearchFromMembers
            | RequestPurpose::GetChatMessageByDate
            | RequestPurpose::Search(SearchPurpose::GetChatMessageCalendar { .. })
            | RequestPurpose::GetHistoryAround
            | RequestPurpose::GetSharedMedia { .. }
            | RequestPurpose::Media(MediaPurpose::GetSharedMediaMore { .. })
            | RequestPurpose::Media(MediaPurpose::GetProfileGifts { .. })
            | RequestPurpose::GetChatPostedToChatPageStories
            | RequestPurpose::GetMessageThread { .. }
            | RequestPurpose::GetMessageThreadHistory { .. }
            | RequestPurpose::Threads(ThreadsPurpose::GetMessageThreadHistoryNewer { .. })
            | RequestPurpose::GetSavedMessagesTopicHistory { .. }
            | RequestPurpose::SearchSavedMessages { .. }
            | RequestPurpose::GetChatSponsoredMessages
    )
}

/// The chat windows' views, by chat (`Session::window_chats`): the keys
/// are the chats shown in a window of their own. While a window's view is
/// in the session's place (`Session::enter_view`) its entry holds the main
/// window's view instead (the exchange is symmetric).
#[derive(Debug, Default)]
pub struct WindowViews {
    views: BTreeMap<ChatId, SessionView>,
}

impl WindowViews {
    /// Whether `chat_id` is shown in a window of its own.
    pub fn contains(&self, chat_id: &ChatId) -> bool {
        self.views.contains_key(chat_id)
    }

    /// The chats shown in a window of their own.
    pub fn iter(&self) -> impl Iterator<Item = &ChatId> {
        self.views.keys()
    }

    pub fn is_empty(&self) -> bool {
        self.views.is_empty()
    }

    /// Whether a stored view (a window's, or the main window's while a
    /// window's is in place) shows `chat_id`.
    pub fn shows_chat(&self, chat_id: ChatId) -> bool {
        self.views
            .values()
            .any(|view| view.open_chat == Some(chat_id))
    }

    /// Whether a stored view shows `topic` of `chat_id`.
    pub fn shows_topic(&self, chat_id: ChatId, topic: i32) -> bool {
        self.views
            .values()
            .any(|view| view.open_chat == Some(chat_id) && view.open_topic == Some(topic))
    }

    pub fn len(&self) -> usize {
        self.views.len()
    }
}

/// The part of the session that is a window's view rather than the
/// account's data: what the window looks at and the views layered on it.
/// Telegram Desktop keeps these per `SessionController`; Quill keeps the
/// main window's in `Session` and each chat window's in [`WindowViews`],
/// exchanged into place by [`Session::enter_view`].
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
    pub(crate) fn swap_view(&mut self, view: &mut SessionView) {
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
