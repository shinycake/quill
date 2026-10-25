//! Cross-chat draft lifecycle: save the composer into the slot it was
//! typed in (chat, forum topic or reply thread — tdesktop's
//! `Data::DraftKey`), restore the slot's draft when it opens again.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::draft_timer::DraftTimerStep;
use gpui_kit::*;
use quill::composer::{ComposerReplyTo, DraftContent, DraftTopic, draft_text_to_store};
use quill::connect::DraftSaveOutcome;
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::ChatId;
use quill::rich::RichDraft;
use quill::state::Session;
use quill::telegram::SendReply;
use quill::telegram::client::copy_and_parse;
use quill::telegram::envelope::ChatDraft;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::time::Duration;
pub(super) fn apply_ready_drafts(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let json = r#"{"@type":"updateChatDraftMessage","chat_id":11,"draft_message":{"@type":"draftMessage","reply_to":{"@type":"inputMessageReplyToMessage","message_id":101,"quote":null,"checklist_task_id":0,"poll_option_id":""},"date":1700000000,"content":{"@type":"draftMessageContentText","text":{"@type":"formattedText","text":"meet at 6","entities":[]},"link_preview_options":null},"effect_id":"0","suggested_post_info":null},"positions":[{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"30","is_pinned":true}]}"#;
    if let Some(owned) = copy_and_parse(json, seq, &dyn_sink) {
        session.apply(owned);
    }
}

impl QuillApp {
    pub(super) fn open_chat_id(&self) -> Option<ChatId> {
        self.live
            .as_ref()
            .and_then(|live| live.driver.session.open_chat)
            .or_else(|| {
                self.demo_session
                    .as_ref()
                    .and_then(|session| session.open_chat)
            })
    }

    /// The slot the open composer writes to (`Session::open_draft_topic`).
    pub(super) fn open_draft_topic(&self, chat_id: ChatId) -> DraftTopic {
        self.session()
            .map(|session| session.open_draft_topic(chat_id))
            .unwrap_or_default()
    }

    pub(super) fn note_open_draft(&mut self, delayed: bool, cx: &mut Context<Self>) {
        if self.composer_ui.pending_edit.is_some() {
            return;
        }
        let Some(chat_id) = self.open_chat_id() else {
            return;
        };
        let topic = self.open_draft_topic(chat_id);
        let (content, reply) = self.composer_draft_parts(chat_id, cx);
        self.save_chat_draft(chat_id, topic, content, reply, delayed, cx);
    }

    /// The stored rich draft of the open slot, if its draft is one.
    pub(super) fn stored_rich_draft(&self) -> Option<&RichDraft> {
        let chat_id = self.open_chat_id()?;
        let session = self.session()?;
        let topic = session.open_draft_topic(chat_id);
        session.draft_for(chat_id, topic)?.rich.as_ref()
    }

    /// What the composer saves for the open slot, and the reply it keeps:
    /// the rich editor's blocks, a stored rich draft (shown in place of
    /// the field, or kept aside while a message is edited), or the field.
    pub(super) fn composer_draft_parts(
        &self,
        chat_id: ChatId,
        cx: &App,
    ) -> (DraftContent, Option<SendReply>) {
        if self.composer_ui.pending_edit.is_some() {
            let reply = self.draft_reply(self.composer_ui.saved_edit_reply.as_ref(), chat_id);
            let content = match self.stored_rich_draft() {
                Some(rich) => DraftContent::Rich(rich.clone()),
                None => DraftContent::Text(self.composer_ui.saved_edit_draft.clone()),
            };
            return (content, reply);
        }
        let reply = self.draft_reply(self.composer_ui.reply(), chat_id);
        let markup = self.composer_markup(cx);
        let content = if self.composer_ui.rich_editor_open {
            RichDraft::from_markup(&markup).map_or(DraftContent::Text(markup), DraftContent::Rich)
        } else if let Some(rich) = self.stored_rich_draft() {
            DraftContent::Rich(rich.clone())
        } else {
            DraftContent::Text(markup)
        };
        (content, reply)
    }

    pub(super) fn leaving_draft_parts(
        &self,
        cx: &Context<Self>,
    ) -> (DraftContent, Option<SendReply>, u64) {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        let Some(chat_id) = self.open_chat_id() else {
            return (DraftContent::Text(String::new()), None, now_ms);
        };
        let (content, reply) = self.composer_draft_parts(chat_id, cx);
        (content, reply, now_ms)
    }

    /// Close what was open for another chat before `chat_id` shows: menus,
    /// confirmations and a recording. The next send's attachments and
    /// options need nothing here, they are per chat (`composer_sends.rs`).
    pub(super) fn dismiss_cross_chat_state(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        // Phase B4: the TTL picker belongs to the previous chat.
        self.notify.ttl_picker_open = false;
        if self
            .composer_ui
            .reply()
            .is_some_and(|reply| !reply.belongs_to(chat_id))
        {
            self.composer_ui.set_reply(None);
        }
        if self
            .composer_ui
            .pending_edit
            .as_ref()
            .is_some_and(|edit| edit.chat_id != chat_id)
        {
            self.composer_ui.pending_edit = None;
            self.composer_ui.saved_edit_draft.clear();
            self.composer_ui.saved_edit_reply = None;
        }
        if self
            .message_ui
            .pending_delete
            .as_ref()
            .is_some_and(|confirm| confirm.chat_id != chat_id)
        {
            self.message_ui.pending_delete = None;
        }
        // B4: a pending stop-poll confirm belongs to its own chat.
        if self
            .message_ui
            .pending_stop_poll
            .is_some_and(|(id, _, _)| id != chat_id)
        {
            self.message_ui.pending_stop_poll = None;
        }
        self.dismiss_forward_for_chat(chat_id);
        // Phase 3.3: the `/` menu never survives a chat switch.
        self.composer_ui.command_menu_open = false;
        self.composer_ui.command_menu_selected = 0;
        // Bots slice: neither does the inline-results dropdown.
        self.composer_ui.inline_results_open = false;
        self.composer_ui.inline_results_selected = 0;
        self.composer_ui.inline_query_armed = None;
        if self.recording_here() {
            self.cancel_recording(cx);
        }
    }

    /// Save the composer before the view moves to another chat, topic or
    /// thread. Call it before the switch: the slot is read from the state
    /// as it is now (tdesktop `saveCurrentDraftToCloud` on leaving).
    pub(super) fn flush_leaving_draft(&mut self, cx: &mut Context<Self>) {
        let Some(chat_id) = self.open_chat_id() else {
            return;
        };
        let topic = self.open_draft_topic(chat_id);
        let (content, reply) = self.composer_draft_parts(chat_id, cx);
        self.save_chat_draft(chat_id, topic, content, reply, false, cx);
    }

    /// Ask for the open slot's draft to be put in the composer on the next
    /// frame (the input needs `&mut Window`, which topic and thread
    /// switches triggered from menus don't have).
    pub(super) fn request_draft_restore(&mut self) {
        self.composer_ui.draft_restore_pending = true;
    }

    /// Frame upkeep: run a requested restore.
    pub(super) fn restore_draft_if_pending(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if std::mem::take(&mut self.composer_ui.draft_restore_pending) {
            self.restore_open_draft(window, cx);
        }
    }

    /// A draft another client saved for the slot this composer shows goes
    /// into the field, unless the user has unsaved changes there or is
    /// editing a message (tdesktop `HistoryWidget::applyCloudDraft`). Runs
    /// once per remote change, in the scope of the window it updates, and
    /// acts only when that slot's own draft changed.
    pub(super) fn adopt_remote_draft(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(revision) = self.session().map(Session::remote_draft_revision) else {
            return;
        };
        if std::mem::replace(&mut self.composer_ui.remote_draft_seen, revision) == revision
            || self.composer_ui.pending_edit.is_some()
        {
            return;
        }
        let Some(chat_id) = self.open_chat_id() else {
            return;
        };
        let Some((topic, draft, held)) = self.session().map(|session| {
            let topic = session.open_draft_topic(chat_id);
            let draft = session.draft_for(chat_id, topic).cloned();
            let held = session.draft_is_dirty(chat_id, topic)
                || !session.accepts_composer_draft(chat_id, topic);
            // The rich editor holds what the user is writing.
            let held = held || self.composer_ui.rich_editor_open;
            (topic, draft, held)
        }) else {
            return;
        };
        // A change to the draft of the slot the field was filled from
        // (`restore_open_draft`) replaces it. A slot the field was not
        // filled from is only filled while the field is empty, so text a
        // link put there stays.
        let previous = self
            .composer_ui
            .shown_draft
            .replace((chat_id, topic, draft.clone()));
        if held {
            return;
        }
        let reply = self.composer_ui.reply().map(|reply| reply.message_id);
        let markup = self.composer_markup(cx);
        let shown = draft_text_to_store(&markup, reply.is_some());
        let take = match previous {
            Some((shown_chat, shown_topic, shown_draft))
                if shown_chat == chat_id && shown_topic == topic =>
            {
                shown_draft != draft
            }
            _ => shown.is_none(),
        };
        if !take {
            return;
        }
        // A rich draft shows in place of the field, which it empties.
        if draft.as_ref().is_some_and(|draft| draft.rich.is_some())
            || draft.as_ref().map(|draft| draft.text.as_str()) != shown
            || draft.and_then(|draft| draft.reply_to_message_id) != shown.and(reply)
        {
            self.restore_open_draft(window, cx);
        }
    }

    pub(super) fn save_chat_draft(
        &mut self,
        chat_id: ChatId,
        topic: DraftTopic,
        content: DraftContent,
        reply_to: Option<SendReply>,
        delayed: bool,
        cx: &mut Context<Self>,
    ) {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        if self.live.is_some() {
            let outcome = self.live.as_mut().and_then(|live| {
                live.driver
                    .note_composer_content_in(chat_id, topic, content, reply_to, now_ms, delayed)
                    .ok()
            });
            if let Some(DraftSaveOutcome::Debounced { token, delay }) = outcome {
                self.schedule_draft_commit(token, delay, cx);
            }
            return;
        }
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        if !session.accepts_composer_draft(chat_id, topic) {
            return;
        }
        session.store_composer_draft(chat_id, topic, demo_draft(content, reply_to));
    }

    pub(super) fn schedule_draft_commit(
        &mut self,
        token: u64,
        delay: Duration,
        cx: &mut Context<Self>,
    ) {
        if !self
            .frame
            .draft_timer
            .arm(token, delay, std::time::Instant::now())
        {
            return;
        }
        // window-scope: the pending draft names its own chat and topic (and
        // its token), so the commit needs no window's state.
        cx.spawn(async move |this, cx| {
            loop {
                let step = this
                    .update(cx, |this, _cx| {
                        this.frame.draft_timer.step(std::time::Instant::now())
                    })
                    .unwrap_or(DraftTimerStep::Idle);
                match step {
                    DraftTimerStep::Idle => break,
                    DraftTimerStep::Sleep(wait) => cx.background_executor().timer(wait).await,
                    DraftTimerStep::Fire(token) => {
                        this.update(cx, |this, _cx| {
                            if let Some(live) = this.live.as_mut() {
                                let _ = live.driver.commit_debounced_draft(token);
                            }
                        })
                        .ok();
                        break;
                    }
                }
            }
        })
        .detach();
    }

    pub(super) fn forget_local_draft(&mut self, chat_id: ChatId) {
        let topic = self.open_draft_topic(chat_id);
        if let Some(live) = self.live.as_mut() {
            live.driver.cancel_pending_draft();
            live.driver
                .session
                .store_composer_draft(chat_id, topic, None);
        } else if let Some(session) = self.demo_session.as_mut() {
            session.store_composer_draft(chat_id, topic, None);
        }
    }

    pub(super) fn restore_open_draft(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.composer_ui.pending_edit.is_some() {
            return;
        }
        let Some(chat_id) = self.open_chat_id() else {
            return;
        };
        let restored = self.session().and_then(|session| {
            let topic = session.open_draft_topic(chat_id);
            if session.draft_is_dirty(chat_id, topic) {
                return None;
            }
            if !session.accepts_composer_draft(chat_id, topic) {
                return Some((None, None));
            }
            let draft = session.draft_for(chat_id, topic).cloned();
            let preview = draft.as_ref().and_then(|draft| draft.reply_to_message_id);
            Some((draft, preview))
        });
        let Some((draft, preview)) = restored else {
            return;
        };
        self.composer_ui.shown_draft = self
            .session()
            .map(|session| (chat_id, session.open_draft_topic(chat_id), draft.clone()));
        let text = draft
            .as_ref()
            .map(|draft| draft.text.clone())
            .unwrap_or_default();
        let item = draft
            .as_ref()
            .map(|draft| draft.item.clone())
            .unwrap_or_default();
        let restored_reply = preview.map(|id| {
            // Slice G1: a draft saved with a partial quote restores the
            // quote picker state, not just the replied-to message.
            match draft.as_ref().and_then(|draft| draft.quote.clone()) {
                Some(quote) => ComposerReplyTo::with_quote(chat_id, id, quote),
                None => ComposerReplyTo::new(chat_id, id),
            }
            .to_item(item)
        });
        self.composer_ui.set_reply(restored_reply);
        if draft.as_ref().is_some_and(|draft| draft.rich.is_some()) {
            self.composer_ui.rich_editor_open = false;
        }
        self.set_composer_markup(&text, window, cx);
        // `set_value` emits no Change: re-check the restored draft.
        let text = self.composer.read(cx).value().to_string();
        self.composer_ui.prev_text = text.clone();
        self.sync_spellcheck(&text, cx);
        self.sync_suggest_menu(cx);
    }

    pub(super) fn sync_composer_typing(&mut self, text: &str) {
        let Some(live) = self.live.as_mut() else {
            return;
        };
        let editing = self.composer_ui.pending_edit.is_some();
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        let _ = live.driver.sync_outgoing_typing(text, editing, now_ms);
    }
}

/// The draft the demo session stores for `content` (the live driver's
/// `send_draft` stores the same after `setChatDraftMessage`).
pub(super) fn demo_draft(content: DraftContent, reply_to: Option<SendReply>) -> Option<ChatDraft> {
    let content = content.to_store(reply_to.is_some())?;
    Some(ChatDraft {
        text: content.text().to_string(),
        rich: content.rich().cloned(),
        reply_to_message_id: reply_to.as_ref().map(|reply| reply.message_id),
        item: reply_to
            .as_ref()
            .map(|reply| reply.item.clone())
            .unwrap_or_default(),
        quote: reply_to.and_then(|reply| reply.quote),
        suggested_post: None,
    })
}
