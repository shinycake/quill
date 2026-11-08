//! forwarding: pickers, banners, strips.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::*;
use crate::ui::status_note::StatusNote;
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
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
            self.connection.set_note(StatusNote::error(tr(
                keys::Q_MISC_CANNOT_FORWARD_THIS_MESSAGE,
            )));
            cx.notify();
            return;
        };
        self.drop_deleted_selection();
        // A new forward is no viewer share at a time (that one sets it
        // again after this).
        self.viewer.extra.share_at = None;
        if let Some(existing) = self.screen.share.pending_forward.as_mut()
            && existing.from_chat_id == chat_id
            && self.screen.share.forward_picker_open
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
        self.drop_deleted_selection();
        match self.share.pending_forward.as_mut() {
            Some(draft) => {
                draft.toggle(chat_id, message_id, pending);
                if draft.is_empty() {
                    self.share.pending_forward = None;
                    self.chat_ui.selection_focus = None;
                    self.share.forward_picker_open = false;
                }
            }
            None => {
                self.share.pending_forward =
                    ForwardDraft::from_message(chat_id, message_id, pending);
            }
        }
        self.screen
            .connection
            .set_note(match self.forward_draft().map(|d| d.count()) {
                Some(1) => StatusNote::silent(tr(keys::Q_MISC_1_MESSAGE_SELECTED)),
                Some(n) => StatusNote::silent(trn(keys::Q_MISC_MESSAGES_SELECTED, n)),
                None => StatusNote::silent(tr(keys::Q_MISC_SELECTION_CLEARED)),
            });
        cx.notify();
    }

    /// The selection in `chat_id` may leave it: nothing leaves a secret chat
    /// (TDLib `can_forward_message`).
    pub(super) fn selection_forwardable(&self, chat_id: ChatId) -> bool {
        !self.session().is_some_and(|s| s.is_secret_chat(chat_id))
    }

    pub(super) fn open_forward_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(draft) = self.forward_draft() else {
            return;
        };
        if !self.selection_forwardable(draft.from_chat_id) {
            return;
        }
        self.share.forward_picker_open = true;
        self.share.focus_search = true;
        self.share.selection.clear();
        self.share
            .comment_input
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.share.search_input.update(cx, |input, cx| {
            input.set_value("", window, cx);
            input.focus(window, cx);
        });
        cx.notify();
    }

    pub(super) fn close_forward_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self
            .chat_ui
            .schedule_picker
            .as_ref()
            .is_some_and(|picker| picker.target == super::scheduled::ScheduleTarget::Share)
        {
            self.close_schedule_picker(cx);
        }
        self.share.forward_picker_open = false;
        self.share.selection.clear();
        // "Share at 1:23" lasts as long as the picker it opened.
        self.viewer.extra.share_at = None;
        self.share
            .search_input
            .update(cx, |input, cx| input.set_value("", window, cx));
        cx.notify();
    }

    pub(super) fn clear_forward(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let _ = cancel_forward_draft(self.share.pending_forward.take());
        self.chat_ui.selection_focus = None;
        self.share.forward_picker_open = false;
        self.share.forward_bar_dest = None;
        self.share.selection.clear();
        self.share
            .search_input
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.connection
            .set_note(StatusNote::silent(tr(keys::Q_MISC_FORWARD_CANCELLED)));
        cx.notify();
    }

    /// Chat switch: a forward draft stays only for its source chat (the
    /// selection) and for the chat the forward bar waits in.
    pub(super) fn dismiss_forward_for_chat(&mut self, chat_id: ChatId) {
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

    /// A forward's answer from TDLib (`poll_live`): shown in the window
    /// that shows its source chat, the chat's own window when it has one,
    /// else the main window.
    pub(super) fn present_forward_answer(&mut self, result: ForwardResult, cx: &mut Context<Self>) {
        let source = result.from_chat_id;
        let window = self
            .windows
            .open_windows()
            .any(|(chat, _)| chat == source)
            .then_some(source);
        self.in_window_scope(window, cx, |app, cx| app.present_forward_result(result, cx));
    }

    /// Show `result` in the window in place. What that window was
    /// building for this forward (its selection, bar and picker) goes;
    /// another forward being built there stays.
    pub(super) fn present_forward_result(&mut self, result: ForwardResult, cx: &mut Context<Self>) {
        let label = result.success_label();
        let host = self.windows.scope;
        self.connection.set_note_in(
            host,
            if result.forwarded_ids.is_empty() {
                StatusNote::error(label)
            } else {
                StatusNote::info(label)
            },
        );
        let this_forward = self
            .share
            .pending_forward
            .as_ref()
            .is_none_or(|draft| draft.from_chat_id == result.from_chat_id);
        if this_forward {
            self.share.pending_forward = None;
            self.share.forward_picker_open = false;
            if self.share.forward_bar_dest == Some(result.dest_chat_id) {
                self.share.forward_bar_dest = None;
            }
        }
        self.share.forward_result = Some(result);
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
                    .when(self.selection_forwardable(draft.from_chat_id), |this| {
                        this.child(
                            Button::new("open-forward-picker")
                                .label(tr(keys::CONTEXT_FORWARD_MSG))
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.open_forward_picker(window, cx);
                                })),
                        )
                    }),
            )
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "forward_tests.rs"]
mod tests;
