//! Connect driver: chats shown in a window of their own
//! (`parity:platform-new-window`, Telegram Desktop
//! `Core::App().ensureSeparateWindowFor(peer)`).
//!
//! One TDLib client serves every window. The main window's chat is
//! `Session::open_chat`; a chat window adds its chat to
//! `Session::window_chats`, which keeps it open in TDLib (`openChat` is a
//! per-chat flag there) and lets its history page and its rows report as
//! viewed while the main window shows something else.
use super::*;
use crate::composer::DraftSaveClock;
use crate::ids::{ChatId, RequestId};
use crate::state::RequestPurpose;
use crate::telegram::requests::close_chat;

impl<S: JsonSender> ConnectDriver<S> {
    /// A window opened for `chat_id`: open it in TDLib unless the main
    /// window already has it open, then load its first history page.
    /// Returns the page request, `None` when the history is complete or a
    /// page is already in flight.
    pub fn open_chat_window(
        &mut self,
        chat_id: ChatId,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let in_main = self.session.open_chat == Some(chat_id);
        if self.session.open_window_chat(chat_id) && !in_main {
            self.send_open_chat(chat_id)?;
        }
        if in_main {
            return self.fetch_history_for(chat_id);
        }
        // The same follow-ups `select_chat` runs, with the window's chat as
        // the open one for their duration.
        self.with_window_chat(chat_id, |driver| driver.fetch_open_chat_extras(chat_id))?;
        self.fetch_pinned_messages(chat_id)?;
        self.fetch_history_for(chat_id)
    }

    /// Run `f` with `chat_id` as the session's open chat (the driver's
    /// open-chat methods then address the window's chat), restoring the
    /// main window's view afterwards.
    pub fn with_window_chat<R>(&mut self, chat_id: ChatId, f: impl FnOnce(&mut Self) -> R) -> R {
        let mut view = crate::state::SessionView::of_chat(chat_id);
        self.session.swap_view(&mut view);
        let result = f(self);
        self.session.swap_view(&mut view);
        result
    }

    /// The window showing `chat_id` closed: `closeChat` pairs the
    /// window's `openChat`, unless the main window still shows the chat
    /// (its own switch away sends the close then).
    pub fn close_chat_window(&mut self, chat_id: ChatId) -> Result<(), ConnectSendError> {
        if !self.session.close_window_chat(chat_id) {
            return Ok(());
        }
        if self.session.open_chat == Some(chat_id) || !self.chats_path_active() {
            return Ok(());
        }
        let extra = self
            .session
            .request(RequestPurpose::CloseChat, Some(chat_id));
        self.sender.send_json(&close_chat(extra, chat_id))?;
        Ok(())
    }

    /// The main window shows no chat any more (Telegram Desktop clears its
    /// section stack when the active chat moves to its own window). The
    /// chat is closed in TDLib unless a window still shows it.
    pub fn deselect_chat(&mut self) -> Result<(), ConnectSendError> {
        if self.session.open_chat.is_none() {
            return Ok(());
        }
        self.cancel_outgoing_typing()?;
        self.close_open_chat()?;
        self.draft_clock = DraftSaveClock::idle();
        self.pending_draft = None;
        self.session.leave_open_chat();
        Ok(())
    }
}
