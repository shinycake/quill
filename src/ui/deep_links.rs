//! `t.me` / `tg:` deep-link handling (`parity:platform-deep-links`).
//!
//! `main.rs` stashes the launch link in `QuillApp::pending_deep_link`.
//! [`QuillApp::pump_deep_link`] (called from `poll_live`, no window
//! needed) fires `getDeepLinkInfo` once auth is Ready and drives the
//! driver-facing half of the flow; the chat open itself happens in
//! render via [`QuillApp::open_deep_link_chat`], which has the `Window`
//! (same take-once pattern as `pending_story_open`).
use super::app::QuillApp;
use super::shell::{DialogKind, QuillShell};
use super::*;
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::*;
use quill::ids::{ChatId, MessageId};
use quill::state::{DeepLinkAction, DeepLinkState};
use quill::telegram::envelope::AuthorizationState;
use std::cell::RefCell;
use std::rc::Rc;

/// Whether one `pump_deep_link` step has something to show: a terminal
/// state to consume, or an invite preview not on screen yet. No flow, a
/// request in flight, and the invite already shown change nothing.
fn deep_link_step_redraws(
    state: Option<&DeepLinkState>,
    shown_invite: Option<&DeepLinkState>,
) -> bool {
    match state {
        None | Some(DeepLinkState::ResolvingInfo { .. } | DeepLinkState::ResolvingChat { .. }) => {
            false
        }
        Some(preview @ DeepLinkState::InvitePreview { .. }) => shown_invite != Some(preview),
        Some(_) => true,
    }
}

impl QuillApp {
    /// Driver-facing half of the deep-link flow, run from `poll_live`.
    /// Fires `getDeepLinkInfo` once auth is Ready, then turns each
    /// terminal session state into the next step: follow-up request,
    /// info dialog, or a deferred chat open for render.
    pub(super) fn pump_deep_link(&mut self, cx: &mut Context<Self>) {
        self.tick_media_seek(cx);
        // `tg://proxy` / `tg://socks` (and the t.me forms) are parsed
        // locally and need no sign-in: a user who is blocked from
        // Telegram can only get in through them.
        if let Some(link) = self
            .pending_deep_link
            .take_if(|link| quill::proxy::parse_proxy_link(link).is_some())
        {
            self.handle_proxy_link(&link, cx);
            return;
        }
        let Some(live) = self.live.as_mut() else {
            return;
        };
        if !matches!(live.driver.session.auth, AuthorizationState::Ready) {
            return;
        }
        // One flow at a time: a link waits until the previous one has been
        // consumed (an invite preview stays up until the user decides).
        if live.driver.session.chats_state.deep_link.is_none()
            && let Some(link) = self.pending_deep_link.take()
        {
            let _ = live.driver.request_deep_link_info(&link);
        }
        // Consume terminal states once, retaining the invite preview until a decision.
        let state = live.driver.session.chats_state.deep_link.take();
        if !deep_link_step_redraws(state.as_ref(), self.links.deep_link_invite.as_ref()) {
            // Nothing new (this runs on every poll, ~8×/s when idle): put
            // the state back and don't redraw.
            live.driver.session.chats_state.deep_link = state;
            return;
        }
        match state {
            None
            | Some(DeepLinkState::ResolvingInfo { .. })
            | Some(DeepLinkState::ResolvingChat { .. }) => {
                live.driver.session.chats_state.deep_link = state;
            }
            Some(DeepLinkState::Info {
                text,
                need_update,
                action,
                ..
            }) => {
                match (need_update, action) {
                    // The real in-app updater is a queued future slice; the
                    // honest UI is TDLib's own text saying so.
                    (true, _) => {
                        self.links.deep_link_dialog = Some(text);
                    }
                    (false, Some(action)) => {
                        let _ = live.driver.resolve_deep_link(action);
                    }
                    // Unknown link: TDLib's info text is the confirmation
                    // copy written for exactly this case.
                    (false, None) => {
                        self.links.deep_link_dialog = Some(text);
                    }
                }
            }
            Some(preview @ DeepLinkState::InvitePreview { .. }) => {
                self.links.deep_link_invite = Some(preview.clone());
                live.driver.session.chats_state.deep_link = Some(preview);
            }
            Some(DeepLinkState::ChatReady { chat_id, action }) => {
                self.links.pending_deep_link_open = Some((chat_id, action));
            }
            Some(DeepLinkState::ShowText(text)) => {
                self.links.deep_link_dialog = Some(text);
            }
            // Typed links (`deep_link_types`): the UI half runs in render.
            Some(DeepLinkState::Ui(ui)) => {
                self.links.pending_deep_link_ui = Some(ui);
            }
            // TDLib has no type for it: `getDeepLinkInfo` explains it.
            Some(DeepLinkState::Unknown { link }) => {
                let _ = live.driver.request_deep_link_text(&link);
            }
        }
        cx.notify();
    }

    /// Render-time half: open the resolved chat (has the `Window` for
    /// draft restore and composer prefill). Consumed once via
    /// `pending_deep_link_open`.
    pub(super) fn open_deep_link_chat(
        &mut self,
        chat_id: ChatId,
        action: &DeepLinkAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Game and add-bot links open a picker over the current view; the
        // bot's own chat stays closed (tdesktop shows a box, not the chat).
        if matches!(
            action,
            DeepLinkAction::ShareGame { .. }
                | DeepLinkAction::AddBot { .. }
                | DeepLinkAction::OpenWebAppLink { .. }
                | DeepLinkAction::OpenMainWebApp { .. }
                | DeepLinkAction::OpenAttachmentBot { .. }
        ) && self.run_bot_link(chat_id, action, cx)
        {
            return;
        }
        let (text, reply, now_ms) = self.leaving_draft_parts(cx);
        if let Some(live) = self.live.as_mut() {
            self.connection.status_note = match live
                .driver
                .select_search_chat(chat_id, &text, reply, now_ms)
            {
                Ok(_) => "opened link".into(),
                Err(_) => "couldn't open the chat".into(),
            };
            // Jump to the linked message / post once the chat is open.
            let jump_to = match action {
                DeepLinkAction::OpenUsername {
                    post: Some(post), ..
                } => Some(*post),
                DeepLinkAction::OpenMessage { message_id, .. } => Some(*message_id),
                DeepLinkAction::OpenChannelPost { post, .. } => Some(*post),
                // Already a TDLib message id.
                DeepLinkAction::OpenChatById { message_id, .. } if *message_id > 0 => {
                    Some(*message_id >> 20)
                }
                _ => None,
            };
            if let Some(message_id) = jump_to
                && live
                    .driver
                    .jump_to_replied_message(MessageId::from_server_id(message_id))
                    .is_err()
            {
                self.connection.status_note = "opened chat, couldn't jump to the message".into();
            }
        }
        self.dismiss_cross_chat_state(chat_id, cx);
        self.restore_open_draft(window, cx);
        // Bot `start=`: prefill `/start <param>` — never auto-send, the
        // user confirms.
        if let DeepLinkAction::OpenUsername {
            start_param: Some(param),
            ..
        } = action
        {
            let prefill = format!("/start {param}");
            self.composer
                .update(cx, |input, cx| input.set_value(&prefill, window, cx));
        }
        self.finish_typed_link(chat_id, action, window, cx);
        if let DeepLinkAction::OpenUsername {
            story_id: Some(story_id),
            ..
        } = action
        {
            self.open_story_viewer(chat_id, *story_id, cx);
        }
        cx.notify();
    }

    fn cancel_deep_link_invite(&mut self, cx: &mut Context<Self>) {
        if let Some(DeepLinkState::InvitePreview { generation, .. }) =
            self.links.deep_link_invite.take()
            && let Some(live) = self.live.as_mut()
            && matches!(live.driver.session.chats_state.deep_link, Some(DeepLinkState::InvitePreview { generation: slot, .. }) if slot == generation)
        {
            live.driver.session.chats_state.deep_link = None;
        }
        cx.notify();
    }

    pub(super) fn build_deep_link_invite_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::DeepLinkInvite, |this, _, cx| {
                this.cancel_deep_link_invite(cx);
            });
        app.update(cx, |this, cx| {
            let (title, count, request, channel) = match this.links.deep_link_invite.as_ref() {
                Some(DeepLinkState::InvitePreview {
                    title,
                    member_count,
                    creates_join_request,
                    is_channel,
                    ..
                }) => (
                    title.clone(),
                    *member_count,
                    *creates_join_request,
                    *is_channel,
                ),
                _ => (String::new(), 0, false, false),
            };
            let mut body = div()
                .flex()
                .flex_col()
                .gap_3()
                .child(div().child(title))
                .child(div().text_sm().text_color(text_muted()).child(format!(
                    "{count} {}",
                    if channel { "subscribers" } else { "members" }
                )));
            if request {
                body = body.child(div().text_sm().text_color(text_muted()).child(
                    "Joining sends a request. An admin must approve it before you can enter.",
                ));
            }
            let footer = div()
                .flex()
                .justify_end()
                .gap_2()
                .child(
                    Button::new("deep-link-invite-cancel")
                        .label("Cancel")
                        .ghost()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.cancel_deep_link_invite(cx);
                            this.close_kit_dialog_if_done(DialogKind::DeepLinkInvite, window, cx);
                        })),
                )
                .child(
                    Button::new("deep-link-invite-join")
                        .label("Join")
                        .primary()
                        .on_click(cx.listener(|this, _, window, cx| {
                            if let Some(DeepLinkState::InvitePreview { generation, .. }) =
                                this.links.deep_link_invite.take()
                                && let Some(live) = this.live.as_mut()
                            {
                                let _ = live.driver.confirm_deep_link_invite(generation);
                            }
                            cx.notify();
                            this.close_kit_dialog_if_done(DialogKind::DeepLinkInvite, window, cx);
                        })),
                );
            dialog
                .overlay(true)
                .title(crate::ui::shell::dialog_title(if channel {
                    "Join channel?"
                } else {
                    "Join group?"
                }))
                .content(crate::ui::shell::scrollable_dialog_content({
                    let body = Rc::new(RefCell::new(Some(body.into_any_element())));
                    move |content, _, _| {
                        content.child(
                            body.borrow_mut()
                                .take()
                                .unwrap_or_else(|| div().into_any_element()),
                        )
                    }
                }))
                .footer(footer)
                .on_close(on_close)
        })
    }

    /// kit `Dialog` for TDLib's deep-link info / error text, via
    /// `window.open_dialog` like every other dialog (`DialogKind::DeepLinkInfo`).
    pub(super) fn build_deep_link_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::DeepLinkInfo, |this, _, cx| {
                this.links.deep_link_dialog = None;
                cx.notify();
            });
        app.update(cx, |this, cx| {
            let text = this.links.deep_link_dialog.clone().unwrap_or_default();
            let body = div()
                .flex()
                .flex_col()
                .gap_3()
                .child(div().text_sm().text_color(text_muted()).child(text))
                .into_any_element();
            let footer = div().flex().justify_end().gap_2().child(
                Button::new("deep-link-ok")
                    .label("OK")
                    .primary()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.links.deep_link_dialog = None;
                        cx.notify();
                        this.close_kit_dialog_if_done(DialogKind::DeepLinkInfo, window, cx);
                    })),
            );
            dialog
                .overlay(true)
                .title(crate::ui::shell::dialog_title("Telegram link"))
                .content(crate::ui::shell::scrollable_dialog_content({
                    let body = Rc::new(RefCell::new(Some(body)));
                    move |content, _, _| {
                        let body = body
                            .borrow_mut()
                            .take()
                            .unwrap_or_else(|| div().into_any_element());
                        content.child(body)
                    }
                }))
                .footer(footer)
                .on_close(on_close)
        })
    }
}

crate::ui::shell::register_dialogs! {
    /// `parity:platform-deep-links`: TDLib's deep-link info / error text.
    DeepLinkInfo => DialogSpec::new(
        // `parity:platform-deep-links`: link info sits with the other
        // low-priority informational dialogs.
        2700,
        |app| app.links.deep_link_dialog.is_some(),
        QuillApp::build_deep_link_dialog,
    ),

    DeepLinkInvite => DialogSpec::new(
        2800,
        |app| app.links.deep_link_invite.is_some(),
        QuillApp::build_deep_link_invite_dialog,
    ),
}

#[cfg(test)]
mod pump_tests {
    use super::deep_link_step_redraws;
    use quill::state::DeepLinkState;

    #[test]
    fn an_idle_poll_does_not_redraw() {
        // No deep-link flow at all: the case on every idle poll.
        assert!(!deep_link_step_redraws(None, None));
        let resolving = DeepLinkState::ResolvingInfo { generation: 1 };
        assert!(!deep_link_step_redraws(Some(&resolving), None));
    }

    #[test]
    fn an_invite_redraws_once() {
        let invite = DeepLinkState::InvitePreview {
            hash: "h".into(),
            title: "Group".into(),
            member_count: 3,
            creates_join_request: false,
            is_channel: false,
            generation: 1,
        };
        assert!(deep_link_step_redraws(Some(&invite), None));
        assert!(!deep_link_step_redraws(Some(&invite), Some(&invite)));
        let text = DeepLinkState::ShowText("Unknown link".into());
        assert!(deep_link_step_redraws(Some(&text), None));
    }
}
