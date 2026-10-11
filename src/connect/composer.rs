//! Connect driver: composer drafts and open-chat bookkeeping.
use super::*;
use crate::composer::{
    DraftSaveClock, DraftSaveStep, DraftTopic, draft_text_to_store, schedule_draft_save,
};
use crate::ids::{ChatId, MessageId, RequestId};
use crate::state::RequestPurpose;
use crate::telegram::envelope::ChatDraft;
use crate::telegram::requests::{
    SendReply, close_chat, open_chat, set_chat_draft_message, view_messages,
};
use std::time::Duration;

impl<S: JsonSender> ConnectDriver<S> {
    /// Composer edit in the open chat, saved to the slot the composer is
    /// writing to now (`Session::open_draft_topic`): the chat, the open
    /// forum topic or the open reply thread. `delayed` follows tdesktop
    /// `saveDraft(true)` (1s quiet, 5s cap). `delayed == false` is the
    /// flush on leaving the chat, topic or thread.
    pub fn note_composer_draft(
        &mut self,
        chat_id: ChatId,
        text: &str,
        reply_to: Option<SendReply>,
        now_ms: u64,
        delayed: bool,
    ) -> Result<DraftSaveOutcome, ConnectSendError> {
        let topic = self.session.open_draft_topic(chat_id);
        self.note_composer_draft_in(chat_id, topic, text, reply_to, now_ms, delayed)
    }

    /// [`Self::note_composer_draft`] for an explicit slot. The UI passes
    /// the slot it read before switching away, so a flush after the
    /// switch still lands where the text was typed.
    pub fn note_composer_draft_in(
        &mut self,
        chat_id: ChatId,
        topic: DraftTopic,
        text: &str,
        reply_to: Option<SendReply>,
        now_ms: u64,
        delayed: bool,
    ) -> Result<DraftSaveOutcome, ConnectSendError> {
        if !self.chats_path_active() || !self.session.accepts_composer_draft(chat_id, topic) {
            return Ok(DraftSaveOutcome::Skipped);
        }
        let stored = draft_text_to_store(text, reply_to.is_some()).map(str::to_string);
        let reply_to = stored.as_ref().and(reply_to);
        if self.draft_matches(chat_id, topic, stored.as_deref(), reply_to.as_ref()) {
            self.pending_draft = None;
            self.draft_clock = DraftSaveClock::idle();
            let existing = self.session.draft_for(chat_id, topic).cloned();
            self.session.store_composer_draft(chat_id, topic, existing);
            return Ok(DraftSaveOutcome::Skipped);
        }
        self.session.mark_draft_dirty(chat_id, topic);
        match schedule_draft_save(self.draft_clock, now_ms, delayed) {
            DraftSaveStep::Wait {
                delay_ms,
                started_ms,
            } => {
                self.draft_clock = DraftSaveClock {
                    started_ms: Some(started_ms),
                };
                self.draft_save_token = self.draft_save_token.saturating_add(1);
                let token = self.draft_save_token;
                self.pending_draft = Some(PendingDraft {
                    token,
                    chat_id,
                    topic,
                    text: stored.unwrap_or_default(),
                    reply_to,
                });
                Ok(DraftSaveOutcome::Debounced {
                    token,
                    delay: Duration::from_millis(delay_ms),
                })
            }
            DraftSaveStep::Write => {
                self.pending_draft = None;
                self.draft_clock = DraftSaveClock::idle();
                self.send_draft(chat_id, topic, stored.as_deref(), reply_to.as_ref())?;
                Ok(DraftSaveOutcome::Sent)
            }
        }
    }

    /// Timer fired. Sends only if `token` is still the latest quiet window.
    pub fn commit_debounced_draft(
        &mut self,
        token: u64,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        let Some(pending) = self.pending_draft.take() else {
            return Ok(None);
        };
        if pending.token != token {
            self.pending_draft = Some(pending);
            return Ok(None);
        }
        self.draft_clock = DraftSaveClock::idle();
        let text = draft_text_to_store(&pending.text, pending.reply_to.is_some());
        let reply_to = text.and(pending.reply_to);
        if self.draft_matches(pending.chat_id, pending.topic, text, reply_to.as_ref()) {
            let existing = self
                .session
                .draft_for(pending.chat_id, pending.topic)
                .cloned();
            self.session
                .store_composer_draft(pending.chat_id, pending.topic, existing);
            return Ok(None);
        }
        self.send_draft(pending.chat_id, pending.topic, text, reply_to.as_ref())
            .map(Some)
    }

    /// Successful send into `topic` of `chat_id`: drop that slot's draft
    /// when the composer is still idle.
    pub fn clear_draft_after_send(
        &mut self,
        chat_id: ChatId,
        topic: DraftTopic,
        composer_idle: bool,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !composer_idle || self.session.draft_is_dirty(chat_id, topic) {
            return Ok(None);
        }
        if !self.session.accepts_composer_draft(chat_id, topic) {
            return Ok(None);
        }
        if self.session.draft_for(chat_id, topic).is_none() {
            return Ok(None);
        }
        self.pending_draft = None;
        self.draft_clock = DraftSaveClock::idle();
        self.send_draft(chat_id, topic, None, None).map(Some)
    }

    pub fn cancel_pending_draft(&mut self) {
        self.pending_draft = None;
        self.draft_clock = DraftSaveClock::idle();
    }

    fn draft_matches(
        &self,
        chat_id: ChatId,
        topic: DraftTopic,
        text: Option<&str>,
        reply_to: Option<&SendReply>,
    ) -> bool {
        let current = self.session.draft_for(chat_id, topic);
        match (current, text) {
            (None, None) => true,
            (Some(draft), Some(text)) => {
                draft.text == text
                    && draft.reply_to_message_id == reply_to.map(|reply| reply.message_id)
                    && draft.quote == reply_to.and_then(|reply| reply.quote.clone())
            }
            _ => false,
        }
    }

    fn send_draft(
        &mut self,
        chat_id: ChatId,
        topic: DraftTopic,
        text: Option<&str>,
        reply_to: Option<&SendReply>,
    ) -> Result<RequestId, ConnectSendError> {
        let extra = self
            .session
            .request(RequestPurpose::SetChatDraftMessage, Some(chat_id));
        match self.sender.send_json(&set_chat_draft_message(
            extra, chat_id, topic, text, reply_to,
        )) {
            Ok(()) => {
                let draft = text
                    .filter(|text| !text.trim().is_empty() || reply_to.is_some())
                    .map(|text| ChatDraft {
                        text: text.to_string(),
                        reply_to_message_id: reply_to.map(|reply| reply.message_id),
                        quote: reply_to.and_then(|reply| reply.quote.clone()),
                    });
                self.session.store_composer_draft(chat_id, topic, draft);
                Ok(extra)
            }
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    pub(crate) fn close_open_chat(&mut self) -> Result<(), ConnectSendError> {
        let Some(prev) = self.session.open_chat else {
            return Ok(());
        };
        // A chat that is also shown in its own window stays open in TDLib
        // (`openChat` is a per-chat flag there: one `closeChat` would end it
        // for that window too). That window's close sends it.
        if self.session.window_chats.contains(&prev) {
            return Ok(());
        }
        // No `supported()` gate: `select_chat` sends `openChat` for every
        // chat, so every opened chat gets its paired `closeChat` (Telegram
        // X `Tdlib.closeChatImpl`).
        let extra = self.session.request(RequestPurpose::CloseChat, Some(prev));
        self.sender.send_json(&close_chat(extra, prev))?;
        Ok(())
    }

    pub(crate) fn send_open_chat(
        &mut self,
        chat_id: ChatId,
    ) -> Result<RequestId, ConnectSendError> {
        let extra = self
            .session
            .request(RequestPurpose::OpenChat, Some(chat_id));
        self.sender.send_json(&open_chat(extra, chat_id))?;
        Ok(extra)
    }

    /// `viewMessages` for the open chat's due rows (TDLib 1.8.67): the
    /// newest message until the UI reports visible rows, then only those
    /// (`Session::message_ids_to_view`). Unread counts change only when
    /// `updateChatReadInbox` arrives.
    pub fn maybe_view_open_messages(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        let Some(chat_id) = self.session.open_chat else {
            return Ok(None);
        };
        self.maybe_view_messages_for(chat_id)
    }

    /// [`Self::maybe_view_open_messages`] for a chat on screen: the open
    /// chat or one shown in its own window.
    pub fn maybe_view_messages_for(
        &mut self,
        chat_id: ChatId,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() || !self.session.chat_is_shown(chat_id) {
            return Ok(None);
        }
        if !self
            .session
            .chats
            .get(&chat_id.0)
            .is_some_and(|chat| chat.supported())
        {
            return Ok(None);
        }
        if self
            .session
            .requests
            .has_purpose_for_chat(RequestPurpose::ViewMessages, chat_id)
        {
            return Ok(None);
        }
        let ids = self.session.message_ids_to_view(chat_id);
        if ids.is_empty() {
            return Ok(None);
        }
        let extra = self
            .session
            .request(RequestPurpose::ViewMessages, Some(chat_id));
        // A topic view reads its rows as topic history
        // (`messageSourceForumTopicHistory`, schema 1.8.67 line 3213), so
        // TDLib advances that topic's read position — as Telegram Desktop
        // does when a topic's messages are on screen.
        let source = if self.session.thread_for_chat(chat_id).is_some() {
            "messageSourceMessageThreadHistory"
        } else if self.session.open_chat == Some(chat_id) && self.session.open_topic.is_some() {
            "messageSourceForumTopicHistory"
        } else {
            "messageSourceChatHistory"
        };
        match self
            .sender
            .send_json(&view_messages(extra, chat_id, &ids, source, true))
        {
            Ok(()) => {
                self.session.begin_viewing(chat_id, &ids);
                Ok(Some(extra))
            }
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// The UI reports the rows of the open chat it is showing (call it when
    /// the visible range settles: after layout, after a scroll stops, and
    /// when a new row appears on screen). The ids become due for
    /// `viewMessages` (`force_read`, `messageSourceChatHistory`) and are
    /// sent now, or after the in-flight `viewMessages` answers. From the
    /// first report on, this open generation views only reported rows
    /// (Telegram X `MessagesManager.viewMessages` →
    /// `TdlibMessageViewer.Viewport.viewMessages`). Reports for a chat that
    /// is not open are ignored (`Ok(None)`).
    pub fn view_messages(
        &mut self,
        chat_id: ChatId,
        message_ids: &[MessageId],
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() || !self.session.report_visible_messages(chat_id, message_ids)
        {
            return Ok(None);
        }
        self.maybe_view_messages_for(chat_id)
    }

    /// Persist the open chat's composer before a search result switches chats.
    pub(crate) fn flush_leaving_composer(
        &mut self,
        next_chat: ChatId,
        leaving_text: &str,
        leaving_reply: Option<SendReply>,
        now_ms: u64,
    ) -> Result<(), ConnectSendError> {
        let Some(prev) = self.session.open_chat else {
            return Ok(());
        };
        if prev == next_chat {
            return Ok(());
        }
        self.note_composer_draft(prev, leaving_text, leaving_reply, now_ms, false)?;
        Ok(())
    }
}

impl<S: JsonSender> ConnectDriver<S> {
    /// The composer's `@query` for the open chat changed: search its members
    /// (groups only; private chats and channels have no one to mention).
    /// `None` clears the suggestions.
    pub fn search_mentions(&mut self, query: Option<&str>) -> Result<(), ConnectSendError> {
        let Some(chat_id) = self.session.open_chat else {
            self.session.messages.mention_search = None;
            return Ok(());
        };
        let is_group = self.session.chats.get(&chat_id.0).is_some_and(|chat| {
            matches!(
                chat.kind,
                crate::telegram::envelope::ChatKind::BasicGroup { .. }
                    | crate::telegram::envelope::ChatKind::Supergroup {
                        is_channel: false,
                        ..
                    }
            )
        });
        let Some(query) = query.filter(|_| is_group) else {
            self.session.messages.mention_search = None;
            return Ok(());
        };
        if self
            .session
            .messages
            .mention_search
            .as_ref()
            .is_some_and(|s| s.chat_id == chat_id && s.query == query)
        {
            return Ok(());
        }
        let extra = self
            .session
            .request(RequestPurpose::SearchMentionMembers, Some(chat_id));
        let previous = self.session.messages.mention_search.take();
        self.session.messages.mention_search = Some(crate::state::MentionSearch {
            chat_id,
            query: query.to_string(),
            // Keep showing the previous matches while the new query loads.
            user_ids: previous
                .filter(|s| s.chat_id == chat_id)
                .map(|s| s.user_ids)
                .unwrap_or_default(),
            request: Some(extra),
        });
        if let Err(err) = self
            .sender
            .send_json(&crate::telegram::requests::search_chat_members(
                extra, chat_id, query, 20,
            ))
        {
            self.session.requests.take(extra);
            self.session.messages.mention_search = None;
            return Err(err);
        }
        Ok(())
    }
}

/// Status note for a rich AI answer. Create / fix / rewrite must not share
/// one label — only create actually created the draft.
pub(super) fn ai_rich_draft_note(purpose: RequestPurpose) -> &'static str {
    match purpose {
        RequestPurpose::FixRichMessageWithAi => "AI fixed the draft",
        RequestPurpose::ComposeRichMessageWithAi => "AI rewrote the draft",
        RequestPurpose::CreateRichMessageWithAi => "AI created the draft",
        _ => "AI updated the draft",
    }
}
