//! forwarding: pickers, banners, strips.

use super::app::QuillApp;
use super::pressable::PressableDiv;
use super::*;
use gpui_kit::component::button::*;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::input::Textarea;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::composer::{ForwardDraft, cancel_forward_draft};
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::{ChatId, MessageId};
use quill::state::{ForwardResult, RequestPurpose, Session};
use quill::telegram::client::copy_and_parse;
use quill::telegram::envelope::ChatKind;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
pub(super) fn apply_ready_forward(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let incoming = r#"{"@type":"updateNewMessage","message":{"id":101,"chat_id":11,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"Hello from injected JSON.","entities":[]}},"forward_info":{"@type":"messageForwardInfo","origin":{"@type":"messageOriginHiddenUser","sender_name":"Ada Lovelace"},"date":1700000000,"source":null,"public_service_announcement_type":""}}}"#;
    if let Some(owned) = copy_and_parse(incoming, seq, &dyn_sink) {
        session.apply(owned);
    }
    let extra = session.request(RequestPurpose::ForwardMessages, Some(ChatId(12)));
    session.in_flight_forward = Some(quill::state::ForwardFlight {
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

fn forward_dest_row(id: ChatId, title: String, cx: &mut Context<QuillApp>) -> impl IntoElement {
    div()
        .id(("forward-dest", id.0 as u64))
        .px_2()
        .py_2()
        .rounded_md()
        .role(gpui_kit::Role::Button)
        .aria_label(format!("Forward to {title}"))
        .tab_index(0)
        .cursor_pointer()
        .pressable(cx.theme())
        .bg(cx.theme().sidebar)
        .on_click(cx.listener(move |this, _, _, cx| {
            this.submit_forward_to(id, cx);
        }))
        .child(div().font_medium().child(title))
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
            self.status_note = "cannot forward this message".into();
            cx.notify();
            return;
        };
        if let Some(existing) = self.pending_forward.as_mut()
            && existing.from_chat_id == chat_id
            && self.forward_picker_open
        {
            existing.toggle(chat_id, message_id, pending);
            if existing.is_empty() {
                self.close_forward_picker(window, cx);
            }
            cx.notify();
            return;
        }
        self.pending_forward = Some(single);
        self.open_forward_picker(window, cx);
    }

    pub(super) fn toggle_forward_select(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        pending: bool,
        cx: &mut Context<Self>,
    ) {
        match self.pending_forward.as_mut() {
            Some(draft) => {
                draft.toggle(chat_id, message_id, pending);
                if draft.is_empty() {
                    self.pending_forward = None;
                    self.forward_picker_open = false;
                }
            }
            None => {
                self.pending_forward = ForwardDraft::from_message(chat_id, message_id, pending);
            }
        }
        self.status_note = match self.pending_forward.as_ref().map(|d| d.count()) {
            Some(1) => "1 message selected".into(),
            Some(n) => format!("{n} messages selected"),
            None => "selection cleared".into(),
        };
        cx.notify();
    }

    pub(super) fn open_forward_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.pending_forward.as_ref().is_none_or(|d| d.is_empty()) {
            return;
        }
        self.forward_picker_open = true;
        self.forward_search_input.update(cx, |input, cx| {
            input.focus(window, cx);
        });
        self.status_note = "forward to…".into();
        cx.notify();
    }

    pub(super) fn close_forward_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.forward_picker_open = false;
        self.forward_search_input
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.status_note = "forward picker closed".into();
        cx.notify();
    }

    pub(super) fn clear_forward(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let _ = cancel_forward_draft(self.pending_forward.take());
        self.forward_picker_open = false;
        self.forward_search_input
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.status_note = "forward cancelled".into();
        cx.notify();
    }

    pub(super) fn activate_first_forward_destination(&mut self, cx: &mut Context<Self>) {
        let query = self.forward_search_input.read(cx).value().to_string();
        let dest = self
            .session()
            .and_then(|session| session.forward_destinations(&query).into_iter().next())
            .map(|chat| chat.id);
        if let Some(dest) = dest {
            self.submit_forward_to(dest, cx);
        }
    }

    pub(super) fn submit_forward_to(&mut self, dest: ChatId, cx: &mut Context<Self>) {
        let Some(draft) = self.pending_forward.clone() else {
            return;
        };
        // Phase S1: messages cannot be forwarded to secret chats (TGX
        // `SecretChatForwardError`, verbatim). The picker stays open so
        // the user can pick another destination.
        let dest_is_secret = self
            .session()
            .and_then(|s| s.chats.get(&dest.0))
            .is_some_and(|chat| matches!(chat.kind, ChatKind::Secret { .. }));
        if dest_is_secret {
            self.status_note = "This message cannot be forwarded to secret chats.".into();
            cx.notify();
            return;
        }
        // Phase A1: slow-mode gate applies to forwards — forwarding sends
        // messages to the destination chat.
        if self.slow_mode_blocked(dest, cx) {
            return;
        }
        if self.live.is_some() {
            let result = self
                .live
                .as_mut()
                .expect("live")
                .driver
                .forward_messages(dest, &draft);
            self.status_note = match result {
                Ok(_) => "forwarding…".into(),
                Err(_) => "could not forward messages".into(),
            };
            cx.notify();
            return;
        }
        if self.demo_session.is_some() {
            self.apply_demo_forward(dest, &draft);
            if let Some(result) = self
                .demo_session
                .as_mut()
                .and_then(|session| session.last_forward.take())
            {
                self.present_forward_result(result, cx);
            }
        }
    }

    pub(super) fn present_forward_result(&mut self, result: ForwardResult, cx: &mut Context<Self>) {
        self.status_note = result.success_label();
        self.forward_result = Some(result);
        self.pending_forward = None;
        self.forward_picker_open = false;
        cx.notify();
    }

    pub(super) fn forward_success_banner(
        &self,
        result: &ForwardResult,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let label = result.success_label();
        let detail = format!(
            "{} → {} (ids {})",
            result.from_chat_id.0,
            result.dest_title,
            result
                .forwarded_ids
                .iter()
                .map(|id| id.0.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        );
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
                div()
                    .flex()
                    .flex_col()
                    .min_w_0()
                    .child(
                        div()
                            .text_xs()
                            .font_medium()
                            .text_color(success())
                            .child(label),
                    )
                    .child(div().text_sm().text_color(text_primary()).child(detail)),
            )
            .child(
                Button::new("dismiss-forward-success")
                    .label("Dismiss")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.forward_result = None;
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
            "1 message selected".to_string()
        } else {
            format!("{count} messages selected")
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
                            .label("Clear")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.clear_forward(window, cx);
                            })),
                    )
                    .child(
                        Button::new("open-forward-picker")
                            .label("Forward")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.open_forward_picker(window, cx);
                            })),
                    ),
            )
    }

    pub(super) fn forward_picker_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let query = self.forward_search_input.read(cx).value().to_string();
        let draft = self.pending_forward.clone();
        let count = draft.as_ref().map(|d| d.count()).unwrap_or(0);
        let from_title = draft
            .as_ref()
            .and_then(|d| {
                self.session()
                    .and_then(|s| s.chats.get(&d.from_chat_id.0).map(|c| c.title.clone()))
            })
            .unwrap_or_else(|| "this chat".into());
        let dests: Vec<(ChatId, String)> = self
            .session()
            .map(|session| {
                session
                    .forward_destinations(&query)
                    .into_iter()
                    .map(|chat| (chat.id, chat.title.clone()))
                    .collect()
            })
            .unwrap_or_default();
        let heading = if count == 1 {
            format!("Forward 1 message from {from_title}")
        } else {
            format!("Forward {count} messages from {from_title}")
        };
        let mut list = div()
            .id("forward-dest-list")
            .flex()
            .flex_col()
            .gap_1()
            .max_h(px(220.));
        if dests.is_empty() {
            list = list.child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("No matching loaded chats."),
            );
        } else {
            for (id, title) in dests {
                list = list.child(forward_dest_row(id, title, cx));
            }
        }
        div()
            .id("forward-picker")
            .flex()
            .flex_col()
            .gap_2()
            .px_3()
            .py_2()
            .rounded_md()
            .border_1()
            .border_color(accent())
            .bg(bg_canvas())
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_sm()
                            .font_semibold()
                            .text_color(accent())
                            .child("Forward to…"),
                    )
                    .child(
                        Button::new("cancel-forward-picker")
                            .label("Cancel")
                            .ghost()
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.close_forward_picker(window, cx);
                            })),
                    ),
            )
            .child(div().text_xs().text_color(text_primary()).child(heading))
            .child(
                Textarea::new(&self.forward_search_input)
                    .aria_label("Search forwarding destinations")
                    .h(px(36.)),
            )
            // M1: `forwardMessages.send_copy` ("Hide sender name", TGX)
            // and `forwardMessages.remove_caption` (only applies to
            // send_copy copies — the checkbox disables itself otherwise).
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        // Phase 6: kit Checkboxes (were: ghost buttons with
                        // ☑/☐ labels). Controlled: write the requested value;
                        // the send_copy=false side effect is preserved, and
                        // remove_caption still disables itself without
                        // send_copy.
                        Checkbox::new("forward-send-copy")
                            .label("Hide sender name")
                            .checked(draft.as_ref().is_some_and(|d| d.send_copy))
                            .on_click(cx.listener(|this, &on, _, cx| {
                                if let Some(draft) = this.pending_forward.as_mut() {
                                    draft.send_copy = on;
                                    if !on {
                                        draft.remove_caption = false;
                                    }
                                }
                                cx.notify();
                            })),
                    )
                    .child(
                        Checkbox::new("forward-remove-caption")
                            .label("Remove caption")
                            .checked(draft.as_ref().is_some_and(|d| d.remove_caption))
                            .disabled(!draft.as_ref().is_some_and(|d| d.send_copy))
                            .on_click(cx.listener(|this, &on, _, cx| {
                                if let Some(draft) = this.pending_forward.as_mut()
                                    && draft.send_copy
                                {
                                    draft.remove_caption = on;
                                }
                                cx.notify();
                            })),
                    ),
            )
            .child(list)
    }
}
