//! Connect driver: channel comments and group reply threads.
use super::*;
use crate::composer::DraftSaveClock;
use crate::ids::{ChatId, MessageId, RequestId};
use crate::state::{RequestPurpose, ThreadStatus};
use crate::telegram::requests::{
    get_message_thread, get_message_thread_history, route_into_thread,
};

/// Page size of `getMessageThreadHistory` (TDLib may return fewer).
const THREAD_PAGE: i32 = 50;

impl<S: JsonSender> ConnectDriver<S> {
    /// Open the comment / reply thread of `message_id` in `chat_id`
    /// (`getMessageThread`). The thread view appears once TDLib answers;
    /// the UI then moves to the thread's chat (`switch_to_thread_chat`).
    pub fn open_thread(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() || message_id.0 <= 0 {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self.session.thread.as_ref().is_some_and(|thread| {
            thread.origin_chat_id == chat_id
                && thread.origin_message_id == message_id
                && !matches!(thread.status, ThreadStatus::Failed(_))
        }) {
            return Ok(None);
        }
        let _ = self.cancel_outgoing_typing();
        self.session.begin_thread(chat_id, message_id);
        let purpose = RequestPurpose::GetMessageThread {
            message_id: message_id.0,
        };
        let extra = self.session.request(purpose, Some(chat_id));
        if let Err(err) = self
            .sender
            .send_json(&get_message_thread(extra, chat_id, message_id))
        {
            self.session.requests.take(extra);
            self.session.thread = None;
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Retry a failed thread open.
    pub fn retry_thread(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        let Some((chat_id, message_id)) = self
            .session
            .thread
            .as_ref()
            .map(|thread| (thread.origin_chat_id, thread.origin_message_id))
        else {
            return Ok(None);
        };
        self.session.thread = None;
        self.open_thread(chat_id, message_id)
    }

    /// The thread lives in another chat than the open one: close the open
    /// chat in TDLib and open the thread's (the discussion group of a
    /// channel post). Called by the UI after it has saved the composer
    /// draft of the chat being left.
    pub fn switch_to_thread_chat(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        let Some(chat_id) = self
            .session
            .thread
            .as_ref()
            .filter(|thread| thread.needs_chat_switch)
            .map(|thread| thread.chat_id)
        else {
            return Ok(None);
        };
        if !self.session.chats.contains_key(&chat_id.0) {
            if let Some(thread) = self.session.thread.as_mut() {
                thread.needs_chat_switch = false;
                thread.status = ThreadStatus::Failed("The discussion group is unavailable.".into());
            }
            return Ok(None);
        }
        self.cancel_outgoing_typing()?;
        self.close_open_chat()?;
        self.draft_clock = DraftSaveClock::idle();
        self.pending_draft = None;
        self.session.open_chat(chat_id);
        if let Some(thread) = self.session.thread.as_mut() {
            thread.needs_chat_switch = false;
        }
        self.send_open_chat(chat_id)?;
        self.begin_thread_reading(chat_id);
        self.fetch_thread_history()
    }

    /// The thread is in the open chat already (a group message's replies):
    /// start reading it as thread history and load the first page.
    pub fn start_thread_in_open_chat(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        let Some(chat_id) = self
            .session
            .thread
            .as_ref()
            .filter(|thread| !thread.needs_chat_switch && thread.thread_id != 0)
            .map(|thread| thread.chat_id)
        else {
            return Ok(None);
        };
        if self.session.open_chat != Some(chat_id) {
            return Ok(None);
        }
        self.begin_thread_reading(chat_id);
        self.fetch_thread_history()
    }

    fn begin_thread_reading(&mut self, chat_id: ChatId) {
        self.session.prepare_thread_marks(chat_id);
        if let Some(thread) = self.session.thread.as_mut() {
            thread.reading_started = true;
        }
    }

    /// Whether the open thread still needs its chat opened or its first
    /// page requested (the UI polls this each frame).
    pub fn thread_needs_start(&self) -> bool {
        self.session.thread.as_ref().is_some_and(|thread| {
            thread.thread_id != 0
                && matches!(thread.status, ThreadStatus::LoadingHistory)
                && !thread.reading_started
        })
    }

    /// Load the next page of the open thread: the newest page first, then
    /// older replies. Deduped while one is in flight; stops once the root
    /// was reached.
    pub fn fetch_thread_history(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let Some(thread) = self.session.thread.as_ref() else {
            return Ok(None);
        };
        if thread.thread_id == 0 || thread.history.loaded_complete {
            return Ok(None);
        }
        let (origin_chat, origin_message) = (thread.origin_chat_id, thread.origin_message_id);
        let from = thread.history.next_from_message_id;
        let purpose = RequestPurpose::GetMessageThreadHistory {
            message_id: origin_message.0,
        };
        if self
            .session
            .requests
            .has_purpose_for_chat(purpose, origin_chat)
        {
            return Ok(None);
        }
        let extra = self.session.request(purpose, Some(origin_chat));
        if let Err(err) = self.sender.send_json(&get_message_thread_history(
            extra,
            origin_chat,
            origin_message,
            from,
            THREAD_PAGE,
        )) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Leave the thread view, back to where it was opened from. Returns the
    /// chat to re-select when the thread lived in another chat.
    pub fn close_thread(&mut self) -> Option<ChatId> {
        let thread = self.session.close_thread()?;
        (thread.chat_id != thread.origin_chat_id
            && self.session.open_chat == Some(thread.chat_id)
            && self.session.chats.contains_key(&thread.origin_chat_id.0))
        .then_some(thread.origin_chat_id)
    }

    /// Address a prebuilt send / typing request to the open thread when it
    /// lives in `chat_id`; any other request is returned untouched.
    pub(crate) fn thread_routed(&self, chat_id: ChatId, json: String) -> String {
        match self.session.thread_send_target(chat_id) {
            Some((thread_id, _)) => route_into_thread(&json, thread_id),
            None => json,
        }
    }
}
