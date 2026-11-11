//! Send in the "All" view of a bot that lets users create threads
//! (tdesktop `HistoryWidget::prepareSendAction` with
//! `Data::IsBotUserCreatesTopics`): the message goes into a thread of its
//! own. Quill asks TDLib for a "New Chat" thread first (tdesktop reserves
//! one locally, `Data::Forum::reserveNewBotTopic`), then opens it and sends
//! there, the tray and reply going along. The text stays in the field
//! until then, so a failure or a chat change loses nothing.
#![deny(clippy::indexing_slicing)]

use crate::ui::app::QuillApp;
use crate::ui::composer_sends::SendOnce;
use crate::ui::status_note::StatusNote;
use gpui_kit::*;
use quill::ids::{ChatId, RequestId};
use quill::lang::{keys, tr};

/// The view's send that waits for its new thread. Lives in the view's
/// `ChatUi`, so leaving the view drops it (tdesktop discards the reserved
/// topic when the chat changes before the send).
#[derive(Debug, Default)]
pub(in crate::ui) enum BotThreadStart {
    #[default]
    Idle,
    /// `createForumTopic` is out as `request`; `text` goes into the thread
    /// it answers, with the one-time options of the Send that asked.
    Asked {
        request: RequestId,
        text: String,
        once: SendOnce,
    },
}

impl QuillApp {
    /// Whether this Send in `chat_id` waits for a new thread instead of
    /// going now: asks for the thread, or says one is being made.
    pub(super) fn send_waits_for_bot_thread(
        &mut self,
        chat_id: ChatId,
        text: &str,
        cx: &mut Context<Self>,
    ) -> bool {
        if text.trim().is_empty() && self.pending_attachments().is_empty() {
            return false;
        }
        let reply = self
            .chat_ui
            .reply()
            .filter(|reply| reply.chat_id == chat_id)
            .map(|reply| reply.message_id);
        if !self
            .session()
            .is_some_and(|session| session.send_starts_bot_thread(chat_id, reply))
        {
            return false;
        }
        if matches!(self.chat_ui.bot_thread, BotThreadStart::Asked { .. }) {
            cx.notify();
            return true;
        }
        let name = tr(keys::BOT_NEW_CHAT);
        let asked = match self.live.as_mut() {
            Some(live) => live.driver.start_bot_thread(chat_id, name).ok().flatten(),
            None => self.demo_start_bot_thread(chat_id, name),
        };
        match asked {
            Some(request) => {
                self.chat_ui.bot_thread = BotThreadStart::Asked {
                    request,
                    text: text.to_string(),
                    once: self.send_once(),
                };
            }
            None => self.set_note(StatusNote::error(tr(
                keys::Q_CHAT_COULDN_T_START_A_NEW_THREAD,
            ))),
        }
        cx.notify();
        true
    }

    /// Frame upkeep: the view's new thread was answered. On success the
    /// view moves into the thread, taking the tray along, and the waiting
    /// text is sent there; on failure the text stays in the field.
    pub(in crate::ui) fn advance_bot_thread(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let BotThreadStart::Asked { request, .. } = self.chat_ui.bot_thread else {
            return;
        };
        let Some(outcome) = self
            .session_mut()
            .and_then(|session| session.take_bot_thread_start(request))
        else {
            return;
        };
        let BotThreadStart::Asked { text, once, .. } = std::mem::take(&mut self.chat_ui.bot_thread)
        else {
            return;
        };
        let topic = match outcome {
            Ok(topic) => topic,
            Err(line) => {
                self.set_note(StatusNote::error(line));
                cx.notify();
                return;
            }
        };
        // The field leaves "All" empty: the draft saved there on the way
        // out is nothing.
        self.composer
            .update(cx, |input, cx| input.set_value("", window, cx));
        let tray = self
            .send_slot()
            .and_then(|slot| self.composer_sends.finish(&slot));
        self.select_topic_ui(topic, cx);
        if let Some(tray) = tray {
            self.update_pending_send(|send| *send = tray);
        }
        self.with_send_once(once, |this| this.submit_composer(text, window, cx));
    }

    /// The demo has no TDLib: the thread is made from the answer TDLib
    /// would give.
    fn demo_start_bot_thread(&mut self, chat_id: ChatId, name: &str) -> Option<RequestId> {
        let session = self.demo_session.as_mut()?;
        let request = session.request(quill::state::RequestPurpose::StartBotThread, Some(chat_id));
        let topic = session
            .threads
            .forum_topics
            .get(&chat_id.0)
            .and_then(|topics| topics.iter().map(|topic| topic.forum_topic_id).max())
            .unwrap_or(1)
            + 1;
        let answer = serde_json::json!({
            "@type": "forumTopicInfo",
            "@extra": request.as_extra(),
            "chat_id": chat_id.0,
            "forum_topic_id": topic,
            "name": name,
            "icon": {"@type": "forumTopicIcon", "color": 0x6FB9F0, "custom_emoji_id": "0"},
            "creation_date": 0,
            "creator_id": {"@type": "messageSenderUser", "user_id": 777},
            "is_general": false,
            "is_outgoing": true,
            "is_closed": false,
            "is_hidden": false,
            "is_name_implicit": true,
        })
        .to_string();
        self.demo_ui
            .seq
            .fetch_max(session.last_seq, std::sync::atomic::Ordering::Relaxed);
        let sink: std::sync::Arc<dyn quill::diagnostics::DiagnosticSink> =
            self.demo_ui.sink.clone();
        let owned = quill::telegram::client::copy_and_parse(&answer, &self.demo_ui.seq, &sink)?;
        session.apply(owned);
        Some(request)
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "bot_thread_tests.rs"]
mod tests;
