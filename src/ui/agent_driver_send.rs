//! A send the composer was asked for while the UI driver injected input.
//! The window never sends it itself: it builds the request as it would
//! have sent it, keeps the draft, and hands it to the tool thread
//! (`UiReply::Done.ui_send`), which sends it through the agent write path
//! (grants, pacing, "Confirm each send", the activity log) and reports
//! back with `UiCommand::ComposerSent` so the box clears.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use gpui_kit::*;
use quill::agent::ui_driver::bridge::UiSend;
use quill::agent::ui_driver::guard;
use quill::composer::SendOptions;
use quill::ids::RequestId;
use quill::telegram::requests::send_text;
use serde_json::Value;

impl QuillApp {
    /// `submit_composer` under the driver: a plain text send of the open
    /// chat becomes a [`UiSend`]; everything else the composer could send
    /// (an edit, a forward, attachments, rich blocks) is refused.
    pub(super) fn agent_route_send(&mut self, text: String, cx: &mut Context<Self>) {
        if self.composer_ui.pending_edit.is_some() {
            guard::refuse_if_injecting("edit a message");
            return;
        }
        if self.forward_bar_here() {
            guard::refuse_if_injecting("forward messages");
            return;
        }
        if self.composer_ui.rich_editor_open {
            guard::refuse_if_injecting("send formatted blocks");
            return;
        }
        if !self.pending_attachments().is_empty() {
            guard::refuse_if_injecting("send files");
            return;
        }
        let Some(chat_id) = self.open_chat_id() else {
            return;
        };
        if text.trim().is_empty() {
            return;
        }
        let reply = self.send_reply_for(chat_id, cx);
        let topic_id = self.session().and_then(|session| session.open_topic);
        // A plain send: the composer's link preview choices travel, nothing
        // else the box could carry does (a schedule, a silent send, an
        // effect, a paid message, suggested-post terms). The tool path
        // checks the request again before sending.
        let composer = self.composer_send_options();
        let options = SendOptions {
            link_preview_disabled: composer.link_preview_disabled,
            link_preview_above_text: composer.link_preview_above_text,
            link_preview_link: composer.link_preview_link,
            link_preview_media: composer.link_preview_media,
            ..SendOptions::default()
        };
        let request = send_text(RequestId(0), chat_id, topic_id, &text, reply, &options);
        let mut request: Value = serde_json::from_str(&request).unwrap_or(Value::Null);
        if let Some(map) = request.as_object_mut() {
            map.remove("@extra");
        }
        let chat_title = self
            .session()
            .and_then(|session| session.chats.get(&chat_id.0))
            .map(|chat| chat.title.clone())
            .unwrap_or_default();
        self.agent_ui.ui_send = Some(UiSend {
            chat_id: chat_id.0,
            chat_title,
            text,
            request,
        });
        cx.notify();
    }

    /// The agent path delivered a [`UiSend`]: clear the box if it still
    /// holds that text for that chat.
    pub(super) fn agent_composer_sent(
        &mut self,
        chat_id: i64,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.open_chat_id().map(|chat| chat.0) != Some(chat_id) {
            return;
        }
        if self.composer_markup(cx) != text {
            return;
        }
        self.composer
            .update(cx, |input, cx| input.set_value("", window, cx));
        cx.notify();
    }
}
