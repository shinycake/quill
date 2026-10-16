//! forwarding: pickers, banners, strips.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::*;
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::composer::{ForwardDraft, cancel_forward_draft};
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::{ChatId, MessageId};
use quill::lang::{keys, tr, trn};
use quill::state::{ForwardResult, RequestPurpose, Session};
use quill::telegram::client::copy_and_parse;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
pub(super) fn apply_ready_forward(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let incoming = r#"{"@type":"updateNewMessage","message":{"id":101,"chat_id":11,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"Hello from injected JSON.","entities":[]}},"forward_info":{"@type":"messageForwardInfo","origin":{"@type":"messageOriginHiddenUser","sender_name":"Ada Lovelace"},"date":1700000000,"source":null,"public_service_announcement_type":""}}}"#;
    if let Some(owned) = copy_and_parse(incoming, seq, &dyn_sink) {
        session.apply(owned);
    }
    let extra = session.request(RequestPurpose::ForwardMessages, Some(ChatId(12)));
    session.messages.in_flight_forward = Some(quill::state::ForwardFlight {
        extra,
        dest_chat_id: ChatId(12),
        from_chat_id: ChatId(11),
        requested: 2,
    });
    let json = format!(
        r#"{{"@type":"messages","@extra":"{}","total_count":2,"messages":[{{"id":80,"chat_id":12,"is_outgoing":true,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"Hello from injected JSON.","entities":[]}}}},"forward_info":{{"@type":"messageForwardInfo","origin":{{"@type":"messageOriginUser","sender_user_id":11}},"date":1}}}},{{"id":81,"chat_id":12,"is_outgoing":true,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"Reply from the session reducer.","entities":[]}}}},"forward_info":{{"@type":"messageForwardInfo","origin":{{"@type":"messageOriginUser","sender_user_id":11}},"date":1}}}}]}}"#,
        extra.0
    );
    if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
        session.apply(owned);
    }
}

#[allow(dead_code)]
pub(super) fn forward_from_strip(row_id: MessageId, label: String) -> AnyElement {
    div()
        .id(("forward-from", row_id.0 as u64))
        .mt_1()
        .mb_1()
        .px_2()
        .py_1()
        .rounded_md()
        .border_l_2()
        .border_color(success())
        .bg(bg_canvas())
        .child(
            div()
                .text_xs()
                .font_medium()
                .text_color(success())
                .child(label),
        )
        .into_any_element()
}

impl QuillApp {
    pub(super) fn begin_forward_one(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        pending: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(single) = ForwardDraft::from_message(chat_id, message_id, pending) else {
            self.connection.status_note = tr(keys::Q_MISC_CANNOT_FORWARD_THIS_MESSAGE).into();
            cx.notify();
            return;
        };
        if let Some(existing) = self.share.pending_forward.as_mut()
            && existing.from_chat_id == chat_id
            && self.share.forward_picker_open
        {
            existing.toggle(chat_id, message_id, pending);
            if existing.is_empty() {
                self.close_forward_picker(window, cx);
            }
            cx.notify();
            return;
        }
        self.share.pending_forward = Some(single);
        self.open_forward_picker(window, cx);
    }

    pub(super) fn toggle_forward_select(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        pending: bool,
        cx: &mut Context<Self>,
    ) {
        match self.share.pending_forward.as_mut() {
            Some(draft) => {
                draft.toggle(chat_id, message_id, pending);
                if draft.is_empty() {
                    self.share.pending_forward = None;
                    self.message_ui.selection_focus = None;
                    self.share.forward_picker_open = false;
                }
            }
            None => {
                self.share.pending_forward =
                    ForwardDraft::from_message(chat_id, message_id, pending);
            }
        }
        self.connection.status_note = match self.share.pending_forward.as_ref().map(|d| d.count()) {
            Some(1) => tr(keys::Q_MISC_1_MESSAGE_SELECTED).into(),
            Some(n) => trn(keys::Q_MISC_MESSAGES_SELECTED, n),
            None => tr(keys::Q_MISC_SELECTION_CLEARED).into(),
        };
        cx.notify();
    }

    pub(super) fn open_forward_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self
            .share
            .pending_forward
            .as_ref()
            .is_none_or(|d| d.is_empty())
        {
            return;
        }
        self.share.forward_picker_open = true;
        self.share.selection.clear();
        self.share
            .comment_input
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.share.search_input.update(cx, |input, cx| {
            input.set_value("", window, cx);
            input.focus(window, cx);
        });
        self.connection.status_note = tr(keys::Q_MISC_FORWARD_TO_2).into();
        cx.notify();
    }

    pub(super) fn close_forward_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.share.forward_picker_open = false;
        self.share.selection.clear();
        self.share
            .search_input
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.connection.status_note = tr(keys::Q_MISC_FORWARD_PICKER_CLOSED).into();
        cx.notify();
    }

    pub(super) fn clear_forward(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let _ = cancel_forward_draft(self.share.pending_forward.take());
        self.message_ui.selection_focus = None;
        self.share.forward_picker_open = false;
        self.share.forward_bar_dest = None;
        self.share.selection.clear();
        self.share
            .search_input
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.connection.status_note = tr(keys::Q_MISC_FORWARD_CANCELLED).into();
        cx.notify();
    }

    /// Chat switch: a forward draft stays only for its source chat (the
    /// selection) and for the chat the forward bar waits in.
    pub(super) fn dismiss_forward_for_chat(&mut self, chat_id: ChatId) {
        // The send-as list belongs to the chat it was opened in.
        self.composer_ui.send_as_open = false;
        if self
            .share
            .forward_bar_dest
            .is_some_and(|dest| dest != chat_id)
        {
            self.share.forward_bar_dest = None;
        }
        if self.share.forward_bar_dest != Some(chat_id)
            && self
                .share
                .pending_forward
                .as_ref()
                .is_some_and(|draft| draft.from_chat_id != chat_id)
        {
            self.share.pending_forward = None;
            self.share.forward_picker_open = false;
        }
    }

    pub(super) fn present_forward_result(&mut self, result: ForwardResult, cx: &mut Context<Self>) {
        self.connection.status_note = result.success_label();
        self.share.forward_result = Some(result);
        self.share.pending_forward = None;
        self.share.forward_bar_dest = None;
        self.share.forward_picker_open = false;
        cx.notify();
    }

    pub(super) fn forward_success_banner(
        &self,
        result: &ForwardResult,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let label = result.success_label();
        div()
            .id("forward-success")
            .flex()
            .items_center()
            .justify_between()
            .gap_2()
            .px_3()
            .py_2()
            .rounded_md()
            .border_1()
            .border_color(success())
            .bg(success_bg_subtle())
            .child(
                div().flex().flex_col().min_w_0().child(
                    div()
                        .text_sm()
                        .font_medium()
                        .text_color(success())
                        .truncate()
                        .child(super::bidi_line::one_line_plain(label)),
                ),
            )
            .child(
                Button::new("dismiss-forward-success")
                    .label(tr(keys::Q_MISC_DISMISS))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.share.forward_result = None;
                        cx.notify();
                    })),
            )
    }

    pub(super) fn forward_selection_banner(
        &self,
        draft: &ForwardDraft,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let count = draft.count();
        let label = if count == 1 {
            tr(keys::Q_MISC_1_MESSAGE_SELECTED).to_string()
        } else {
            trn(keys::Q_MISC_MESSAGES_SELECTED_2, count)
        };
        div()
            .id("forward-selection")
            .flex()
            .items_center()
            .justify_between()
            .gap_2()
            .px_3()
            .py_2()
            .rounded_md()
            .border_1()
            .border_color(accent())
            .bg(bg_subtle())
            .child(div().text_sm().text_color(text_primary()).child(label))
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        Button::new("clear-forward-selection")
                            .label(tr(keys::Q_MISC_CLEAR))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.clear_forward(window, cx);
                            })),
                    )
                    .child(
                        Button::new("delete-selection")
                            .label(tr(keys::BOX_DELETE))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.confirm_delete_selection(window, cx);
                            })),
                    )
                    .child(
                        Button::new("open-forward-picker")
                            .label(tr(keys::CONTEXT_FORWARD_MSG))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.open_forward_picker(window, cx);
                            })),
                    ),
            )
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "forward_tests.rs"]
mod tests;
