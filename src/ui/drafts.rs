//! cross-chat draft lifecycle.

use super::app::QuillApp;
use gpui_kit::*;
use quill::composer::{ComposerReplyTo, QuoteSelection, draft_text_to_store};
use quill::connect::DraftSaveOutcome;
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::ChatId;
use quill::state::{Session, effective_preview};
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

    pub(super) fn note_open_draft(&mut self, delayed: bool, cx: &mut Context<Self>) {
        if self.pending_edit.is_some() {
            return;
        }
        let Some(chat_id) = self.open_chat_id() else {
            return;
        };
        let text = self.composer_markup(cx);
        let reply = self
            .pending_reply
            .as_ref()
            .and_then(|reply| reply.send_reply(chat_id));
        self.save_chat_draft(chat_id, &text, reply, delayed, cx);
    }

    pub(super) fn leaving_draft_parts(
        &self,
        cx: &Context<Self>,
    ) -> (String, Option<quill::telegram::SendReply>, u64) {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        let Some(chat_id) = self.open_chat_id() else {
            return (String::new(), None, now_ms);
        };
        if self.pending_edit.is_some() {
            let reply = self
                .saved_edit_reply
                .as_ref()
                .and_then(|saved| saved.send_reply(chat_id));
            return (self.saved_edit_draft.clone(), reply, now_ms);
        }
        (
            self.composer_markup(cx),
            self.pending_reply
                .as_ref()
                .and_then(|reply| reply.send_reply(chat_id)),
            now_ms,
        )
    }

    pub(super) fn dismiss_cross_chat_state(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        if self
            .pending_reply
            .as_ref()
            .is_some_and(|reply| reply.chat_id != chat_id)
        {
            self.pending_reply = None;
        }
        if self
            .pending_edit
            .as_ref()
            .is_some_and(|edit| edit.chat_id != chat_id)
        {
            self.pending_edit = None;
            self.saved_edit_draft.clear();
            self.saved_edit_reply = None;
        }
        if self
            .pending_delete
            .as_ref()
            .is_some_and(|confirm| confirm.chat_id != chat_id)
        {
            self.pending_delete = None;
        }
        // B4: a pending stop-poll confirm belongs to its own chat.
        if self
            .pending_stop_poll
            .is_some_and(|(id, _, _)| id != chat_id)
        {
            self.pending_stop_poll = None;
        }
        self.dismiss_forward_for_chat(chat_id);
        // Phase 3.3: the `/` menu never survives a chat switch.
        self.command_menu_open = false;
        self.command_menu_selected = 0;
        // Bots slice: neither does the inline-results dropdown.
        self.inline_results_open = false;
        self.inline_results_selected = 0;
        self.inline_query_armed = None;
        if self.recording_active() {
            self.cancel_recording(cx);
        }
    }

    pub(super) fn flush_leaving_draft(&mut self, cx: &mut Context<Self>) {
        let Some(chat_id) = self.open_chat_id() else {
            return;
        };
        let (text, reply) = if self.pending_edit.is_some() {
            let reply = self
                .saved_edit_reply
                .as_ref()
                .and_then(|saved| saved.send_reply(chat_id));
            (self.saved_edit_draft.clone(), reply)
        } else {
            (
                self.composer_markup(cx),
                self.pending_reply
                    .as_ref()
                    .and_then(|reply| reply.send_reply(chat_id)),
            )
        };
        self.save_chat_draft(chat_id, &text, reply, false, cx);
    }

    pub(super) fn save_chat_draft(
        &mut self,
        chat_id: ChatId,
        text: &str,
        reply_to: Option<quill::telegram::SendReply>,
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
                    .note_composer_draft(chat_id, text, reply_to, now_ms, delayed)
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
        if !session.accepts_composer_draft(chat_id) {
            return;
        }
        let stored = draft_text_to_store(text, reply_to.is_some()).map(str::to_string);
        let reply_to = stored.as_ref().and(reply_to);
        let draft = stored.map(|text| ChatDraft {
            text,
            reply_to_message_id: reply_to.as_ref().map(|reply| reply.message_id),
            quote: reply_to.and_then(|reply| reply.quote),
        });
        session.store_composer_draft(chat_id, draft);
    }

    pub(super) fn schedule_draft_commit(
        &mut self,
        token: u64,
        delay: Duration,
        cx: &mut Context<Self>,
    ) {
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(delay).await;
            this.update(cx, |this, _cx| {
                if let Some(live) = this.live.as_mut() {
                    let _ = live.driver.commit_debounced_draft(token);
                }
            })
            .ok();
        })
        .detach();
    }

    pub(super) fn forget_local_draft(&mut self, chat_id: ChatId) {
        if let Some(live) = self.live.as_mut() {
            live.driver.cancel_pending_draft();
            live.driver.session.store_composer_draft(chat_id, None);
        } else if let Some(session) = self.demo_session.as_mut() {
            session.store_composer_draft(chat_id, None);
        }
    }

    pub(super) fn restore_open_draft(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.pending_edit.is_some() {
            return;
        }
        let Some(chat_id) = self.open_chat_id() else {
            return;
        };
        let restored = self.session().and_then(|session| {
            if session.draft_is_dirty(chat_id) {
                return None;
            }
            if !session.accepts_composer_draft(chat_id) {
                return Some((None, None));
            }
            let draft = session
                .chats
                .get(&chat_id.0)
                .and_then(|chat| chat.draft.clone());
            let preview = draft
                .as_ref()
                .and_then(|draft| draft.reply_to_message_id)
                .map(|id| {
                    let text = session
                        .histories
                        .get(&chat_id.0)
                        .and_then(|history| history.messages.get(&id.0))
                        .map(effective_preview)
                        .filter(|text| !text.is_empty())
                        .unwrap_or_else(|| "message".into());
                    (id, text)
                });
            Some((draft, preview))
        });
        let Some((draft, preview)) = restored else {
            return;
        };
        let text = draft
            .as_ref()
            .map(|draft| draft.text.clone())
            .unwrap_or_default();
        self.pending_reply = preview.map(|(id, preview)| {
            // Slice G1: a draft saved with a partial quote restores the
            // quote picker state, not just the replied-to message.
            match draft.as_ref().and_then(|draft| draft.quote.clone()) {
                Some((text, position)) => ComposerReplyTo::with_quote(
                    chat_id,
                    id,
                    preview,
                    QuoteSelection { text, position },
                ),
                None => ComposerReplyTo::new(chat_id, id, preview),
            }
        });
        self.set_composer_markup(&text, window, cx);
        // `set_value` emits no Change: re-check the restored draft.
        let text = self.composer.read(cx).value().to_string();
        self.composer_prev_text = text.clone();
        self.sync_spellcheck(&text, cx);
        self.sync_suggest_menu(cx);
    }

    pub(super) fn sync_composer_typing(&mut self, text: &str) {
        let Some(live) = self.live.as_mut() else {
            return;
        };
        let editing = self.pending_edit.is_some();
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        let _ = live.driver.sync_outgoing_typing(text, editing, now_ms);
    }
}
