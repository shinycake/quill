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
use gpui_kit::component::*;
use gpui_kit::*;
use quill::ids::{ChatId, MessageId};
use quill::state::{DeepLinkAction, DeepLinkState};
use quill::telegram::envelope::AuthorizationState;
use std::cell::RefCell;
use std::rc::Rc;

impl QuillApp {
    /// Driver-facing half of the deep-link flow, run from `poll_live`.
    /// Fires `getDeepLinkInfo` once auth is Ready, then turns each
    /// terminal session state into the next step: follow-up request,
    /// info dialog, or a deferred chat open for render.
    pub(super) fn pump_deep_link(&mut self, cx: &mut Context<Self>) {
        let Some(live) = self.live.as_mut() else {
            return;
        };
        if !matches!(live.driver.session.auth, AuthorizationState::Ready) {
            return;
        }
        if let Some(link) = self.pending_deep_link.take()
            && live.driver.request_deep_link_info(&link).is_err()
        {
            self.status_note = "couldn't open the link".into();
        }
        // Take-once: each state is consumed here and never re-processed.
        let state = live.driver.session.deep_link.take();
        match state {
            None
            | Some(DeepLinkState::ResolvingInfo { .. })
            | Some(DeepLinkState::ResolvingChat { .. }) => {
                live.driver.session.deep_link = state;
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
                        self.deep_link_dialog = Some(text);
                    }
                    (false, Some(action)) => {
                        if live.driver.resolve_deep_link(action).is_err() {
                            self.status_note = "couldn't open the link".into();
                        }
                    }
                    // Unknown link: TDLib's info text is the confirmation
                    // copy written for exactly this case.
                    (false, None) => {
                        self.deep_link_dialog = Some(text);
                    }
                }
            }
            Some(DeepLinkState::ChatReady { chat_id, action }) => {
                self.pending_deep_link_open = Some((chat_id, action));
            }
            Some(DeepLinkState::ShowText(text)) => {
                self.deep_link_dialog = Some(text);
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
        let (text, reply, now_ms) = self.leaving_draft_parts(cx);
        if let Some(live) = self.live.as_mut() {
            self.status_note = match live
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
                _ => None,
            };
            if let Some(message_id) = jump_to
                && live
                    .driver
                    .jump_to_replied_message(MessageId(message_id))
                    .is_err()
            {
                self.status_note = "opened chat, couldn't jump to the message".into();
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
        if let DeepLinkAction::OpenUsername {
            story_id: Some(story_id),
            ..
        } = action
        {
            self.open_story_viewer(chat_id, *story_id, cx);
        }
        cx.notify();
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
                this.deep_link_dialog = None;
                cx.notify();
            });
        app.update(cx, |this, cx| {
            let text = this.deep_link_dialog.clone().unwrap_or_default();
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
                        this.deep_link_dialog = None;
                        cx.notify();
                        this.close_kit_dialog_if_done(DialogKind::DeepLinkInfo, window, cx);
                    })),
            );
            dialog
                .overlay(true)
                .title("Telegram link")
                .content({
                    let body = Rc::new(RefCell::new(Some(body)));
                    move |content, _, _| {
                        let body = body
                            .borrow_mut()
                            .take()
                            .unwrap_or_else(|| div().into_any_element());
                        content.child(body)
                    }
                })
                .footer(footer)
                .on_close(on_close)
        })
    }
}
