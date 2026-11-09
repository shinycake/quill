//! `t.me` / `tg:` deep-link handling (`parity:platform-deep-links`).
//!
//! `main.rs` stashes the launch link in `QuillApp::pending_deep_link`.
//! [`QuillApp::pump_deep_link`] (called from `poll_live`, no window
//! needed) fires `getDeepLinkInfo` once auth is Ready and drives the
//! driver-facing half of the flow; the chat open itself happens in
//! render via [`QuillApp::open_deep_link_chat`], which has the `Window`
//! (same take-once pattern as `pending_story_open`).
#![deny(clippy::indexing_slicing)]
use super::app::QuillApp;
use super::links_state::PendingOpen;
use super::shell::{DialogKind, QuillShell};
use super::*;
use crate::ui::status_note::StatusNote;
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::*;
use quill::agent::ui_driver::guard;
use quill::ids::{ChatId, MessageId};
use quill::lang::{keys, tr};
use quill::state::{DeepLinkAction, DeepLinkState, HistoryOpen};
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

/// Whether `link`, taken from `QuillApp::pending_deep_link`, is the one an
/// agent's step queued. Taken once; a link written there any other way
/// (the launch argument, the OS) is the user's.
/// The note after a t.me link opened its chat: none when it did, as
/// Telegram Desktop shows nothing then.
fn chat_link_note(opened: bool) -> Option<StatusNote> {
    (!opened).then(|| StatusNote::error(tr(keys::Q_MISC_COULDNT_OPEN_THE_CHAT)))
}

fn queued_by_agent(agent_link: &mut Option<String>, link: &str) -> bool {
    agent_link.take().is_some_and(|queued| queued == link)
}

/// The chat window `link`, taken from `QuillApp::pending_deep_link`, was
/// clicked in. Taken once; a link queued anywhere else has none.
fn queued_in_window(window_link: &mut Option<(String, ChatId)>, link: &str) -> Option<ChatId> {
    window_link
        .take()
        .and_then(|(queued, window)| (queued == link).then_some(window))
}

impl QuillApp {
    /// Driver-facing half of the deep-link flow, run from `poll_live`.
    /// Fires `getDeepLinkInfo` once auth is Ready, then turns each
    /// terminal session state into the next step: follow-up request,
    /// info dialog, or a deferred chat open for render.
    pub(super) fn pump_deep_link(&mut self, cx: &mut Context<Self>) {
        self.tick_media_seek(std::time::Instant::now(), cx);
        self.pump_start_files(cx);
        // A link that arrives while Quill is locked waits for the unlock,
        // as tdesktop's start links do (`checkStartUrls`): nothing it
        // opens may show over the lock.
        if self.refused_while_locked() {
            return;
        }
        // `tg://proxy` / `tg://socks` (and the t.me forms) are parsed
        // locally and need no sign-in: a user who is blocked from
        // Telegram can only get in through them.
        if let Some(link) = self
            .pending_deep_link
            .take_if(|link| quill::proxy::parse_proxy_link(link).is_some())
        {
            // Its box belongs to the window the link was clicked in, not
            // to the main window this poll runs in.
            let asker = queued_in_window(&mut self.screen.links.window_pending_link, &link);
            self.as_asked_by(asker, |this| this.handle_proxy_link(&link, cx));
            return;
        }
        let Some(live) = self.live.as_mut() else {
            return;
        };
        if !matches!(live.driver.session.auth, AuthorizationState::Ready) {
            return;
        }
        // One flow at a time: a link waits while the previous one is being
        // resolved. An invite preview waiting for the user's answer gives
        // way to a new link: its box closes, as it is modal only in its
        // own window and a click in another must not wait for it.
        let invite_waits = matches!(
            live.driver.session.chats_state.deep_link,
            Some(DeepLinkState::InvitePreview { .. })
        );
        if (live.driver.session.chats_state.deep_link.is_none() || invite_waits)
            && let Some(link) = self.pending_deep_link.take()
        {
            if invite_waits {
                live.driver.session.chats_state.deep_link = None;
                self.screen.links.deep_link_invite = None;
            }
            let from_agent = queued_by_agent(&mut self.screen.links.agent_pending_link, &link);
            self.screen.links.flow_window =
                queued_in_window(&mut self.screen.links.window_pending_link, &link);
            let _ = live.driver.request_deep_link_info_from(&link, from_agent);
        }
        // Consume terminal states once, retaining the invite preview until a decision.
        let state = live.driver.session.chats_state.deep_link.take();
        // The flow's origin is taken with its state and put back only while
        // the flow goes on, so every terminal state (and no flow at all,
        // after an error or a closed box) leaves none behind.
        let from_agent = std::mem::take(&mut live.driver.session.chats_state.deep_link_from_agent)
            && state.is_some();
        let from_window = self
            .screen
            .links
            .flow_window
            .take()
            .filter(|_| state.is_some());
        if !deep_link_step_redraws(state.as_ref(), self.screen.links.deep_link_invite.as_ref()) {
            // Nothing new (this runs on every poll): put the state back and
            // don't redraw.
            live.driver.session.chats_state.deep_link = state;
            live.driver.session.chats_state.deep_link_from_agent = from_agent;
            self.screen.links.flow_window = from_window;
            return;
        }
        match state {
            None
            | Some(DeepLinkState::ResolvingInfo { .. })
            | Some(DeepLinkState::ResolvingChat { .. }) => {
                live.driver.session.chats_state.deep_link = state;
                live.driver.session.chats_state.deep_link_from_agent = from_agent;
                self.screen.links.flow_window = from_window;
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
                        self.screen
                            .windows
                            .open_box_in(DialogKind::DeepLinkInfo, from_window);
                    }
                    (false, Some(action)) => {
                        live.driver.session.chats_state.deep_link_from_agent = from_agent;
                        self.screen.links.flow_window = from_window;
                        let _ = live.driver.continue_deep_link(action);
                    }
                    // Unknown link: TDLib's info text is the confirmation
                    // copy written for exactly this case.
                    (false, None) => {
                        self.links.deep_link_dialog = Some(text);
                        self.screen
                            .windows
                            .open_box_in(DialogKind::DeepLinkInfo, from_window);
                    }
                }
            }
            // The user decides on the invite; Join starts its own origin.
            Some(preview @ DeepLinkState::InvitePreview { .. }) => {
                // A preview already up is the same flow's: it stays where
                // it shows. A new one belongs to the window the link was
                // clicked in, not to the window the pointer is over now.
                if self.screen.links.deep_link_invite.is_none() {
                    self.screen
                        .windows
                        .open_box_in(DialogKind::DeepLinkInvite, from_window);
                }
                self.screen.links.deep_link_invite = Some(preview.clone());
                live.driver.session.chats_state.deep_link = Some(preview);
            }
            Some(DeepLinkState::ChatReady { chat_id, action }) => {
                self.links.pending_deep_link_open = Some(PendingOpen {
                    chat_id,
                    action,
                    from_agent,
                });
                self.hold_resolved_link(from_window, cx);
            }
            Some(DeepLinkState::ShowText(text)) => {
                self.links.deep_link_dialog = Some(text);
                self.screen
                    .windows
                    .open_box_in(DialogKind::DeepLinkInfo, from_window);
            }
            // Typed links (`deep_link_types`): the UI half runs in render.
            // An agent's step gets none of them (settings, proxies, folders,
            // sticker sets, backgrounds): nothing an agent needs, all of it
            // the user's.
            Some(DeepLinkState::Ui(ui)) => {
                if from_agent {
                    guard::refuse_typed_link();
                } else {
                    self.links.pending_deep_link_ui = Some(ui);
                    self.hold_resolved_link(from_window, cx);
                }
            }
            // TDLib has no type for it: `getDeepLinkInfo` explains it.
            Some(DeepLinkState::Unknown { link }) => {
                live.driver.session.chats_state.deep_link_from_agent = from_agent;
                self.screen.links.flow_window = from_window;
                let _ = live.driver.request_deep_link_text(&link);
            }
        }
        cx.notify();
    }

    /// The window whose frame runs the resolved link waiting for render
    /// (`None`: the main window), and whose boxes it opens: the window it
    /// was clicked in, which is on screen (a chat with a window of its
    /// own goes there through the router); the main window for what opens
    /// in it, a chat with no window of its own or a settings page.
    pub(in crate::ui) fn resolved_link_frame(&self) -> Option<ChatId> {
        let in_main = self
            .links
            .pending_deep_link_open
            .as_ref()
            .is_some_and(|open| !self.windows.has_window(open.chat_id))
            || self
                .links
                .pending_deep_link_ui
                .as_ref()
                .is_some_and(super::deep_link_routes::opens_in_main_window);
        if in_main {
            return None;
        }
        self.links
            .pending_flow_window
            .filter(|chat| self.windows.has(*chat))
    }

    /// A resolved link waits for the frame of [`Self::resolved_link_frame`].
    /// Asked from a chat window for something the main window opens, the
    /// main window comes forward now, so its next frame runs it even from
    /// minimized or hidden.
    pub(in crate::ui) fn hold_resolved_link(
        &mut self,
        from_window: Option<ChatId>,
        cx: &mut Context<Self>,
    ) {
        self.links.pending_flow_window = from_window;
        if from_window.is_some() && self.resolved_link_frame().is_none() {
            self.raise_main_window_later(cx);
        }
    }

    /// Render-time half of a resolved link: the chat it names, or the box
    /// of a typed link. Every window's frame asks (`per_window_upkeep`);
    /// it runs in [`Self::resolved_link_frame`]'s, so a link clicked in a
    /// chat window does not wait for a minimized main window. Held while
    /// Quill is locked, then run at the first frame after the unlock.
    pub(super) fn run_resolved_deep_link(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.refused_while_locked()
            || (self.links.pending_deep_link_open.is_none()
                && self.links.pending_deep_link_ui.is_none())
        {
            return;
        }
        let frame = self.resolved_link_frame();
        if frame != self.windows.scope {
            return;
        }
        self.links.pending_flow_window = None;
        self.as_asked_by(frame, |this| {
            if let Some(open) = this.links.pending_deep_link_open.take() {
                this.open_deep_link_chat(open, window, cx);
                this.drop_shared_card(cx);
            }
            if let Some(ui) = this.links.pending_deep_link_ui.take() {
                this.run_deep_link_ui(ui, window, cx);
            }
        });
    }

    /// Starts the deep-link flow for `link` at the next poll, remembering
    /// whether the handler asking is a step the UI driver injected, and
    /// the chat window that asked (`asking_window`: the window of the
    /// click, or the window an answer's request carried; none for a link
    /// from the OS or a mini app, which the main window takes). The
    /// origin rides with the link: a later link queued by someone else
    /// replaces both, and the flow keeps its own in the session
    /// (`ChatsState::deep_link_from_agent`).
    pub(super) fn queue_deep_link(&mut self, link: String) {
        self.links.agent_pending_link = guard::injecting().then(|| link.clone());
        self.links.window_pending_link = self
            .asking_window()
            .flatten()
            .map(|window| (link.clone(), window));
        self.pending_deep_link = Some(link);
        quill::wake::UPDATES.poke();
    }

    /// Bring the main window forward at the end of this update, restoring
    /// it when minimized or hidden, so its next frame runs.
    fn raise_main_window_later(&self, cx: &mut Context<Self>) {
        let Some(main) = self.main_window.handle else {
            return;
        };
        cx.defer(move |cx| {
            let _ = main.update(cx, |_, window, cx| {
                super::link_inbox::raise_main_window(window, cx);
            });
        });
    }

    /// Render-time half: open the resolved chat (has the `Window` for
    /// draft restore and composer prefill). Consumed once via
    /// `pending_deep_link_open`. A chat an agent's step asked for opens as
    /// part of that step: the step ended when the agent clicked, TDLib
    /// resolved the link later, and the open is checked again now.
    pub(super) fn open_deep_link_chat(
        &mut self,
        open: PendingOpen,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let PendingOpen {
            chat_id,
            action,
            from_agent,
        } = open;
        self.agent_step(from_agent, |this| {
            if !from_agent || this.agent_may_open(chat_id) {
                this.open_deep_link_chat_now(chat_id, &action, window, cx);
            }
        });
    }

    fn open_deep_link_chat_now(
        &mut self,
        chat_id: ChatId,
        action: &DeepLinkAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.run_managed_bot_link(chat_id, action, window, cx) {
            return;
        }
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
        // A boost link shows the boost box over the current view; like
        // tdesktop's `resolveBoostState` it does not open the channel.
        if matches!(action, DeepLinkAction::OpenChannelBoost { .. }) {
            self.open_boost_box_for(chat_id, cx);
            return;
        }
        let (text, reply, now_ms) = self.leaving_draft_parts(cx);
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
        // The inline results button's start, from whichever window
        // pressed it.
        let inline_return = self.take_waiting_inline_return(chat_id);
        // A chat in a window of its own opens there, with what the link
        // asks for.
        if self.windows.has_window(chat_id) {
            let premium = self.take_premium_auto_start(chat_id, action, cx);
            let mut routed =
                super::chat_window::ChatOpen::new(chat_id).with_inline_return(inline_return);
            if let Some(message_id) = jump_to {
                routed = routed.at(MessageId::from_server_id(message_id));
            }
            if !premium
                && let DeepLinkAction::OpenUsername {
                    start_param: Some(param),
                    ..
                } = action
            {
                routed = routed.compose(format!("/start {param}"));
            }
            // A link's draft goes into that window's composer too.
            let draft = match action {
                DeepLinkAction::OpenPublicChatDraft { draft, .. }
                | DeepLinkAction::OpenUserDraft { draft, .. } => Some(draft),
                DeepLinkAction::ShareDraft { text } => Some(text),
                _ => None,
            };
            match draft {
                Some(text) => {
                    if !text.is_empty() {
                        routed = routed.compose(text.clone());
                    }
                    let _ = self.route_chat_open(routed, cx);
                    if matches!(action, DeepLinkAction::ShareDraft { .. }) {
                        self.in_window_scope(Some(chat_id), cx, |app, cx| {
                            app.attach_shared_qr_card(chat_id, cx);
                        });
                    }
                }
                None => {
                    let _ = self.route_chat_open(routed, cx);
                    self.finish_typed_link(chat_id, action, window, cx);
                }
            }
            cx.notify();
            return;
        }
        if let Some(ret) = inline_return {
            self.composer_ui.inline_return = Some(ret);
        }
        // A linked message wins over the chat's saved scroll position (and
        // drops it); a plain chat link opens at the saved position.
        let open = match self.chat_open_at(chat_id, jump_to.map(MessageId::from_server_id)) {
            HistoryOpen::Message(_) => HistoryOpen::Default,
            open => open,
        };
        if let Some(live) = self.live.as_mut() {
            let opened = live
                .driver
                .select_search_chat_open(chat_id, open, text, reply, now_ms)
                .is_ok();
            if let Some(note) = chat_link_note(opened) {
                self.screen.connection.set_note(note);
            }
            if let Some(message_id) = jump_to
                && live
                    .driver
                    .jump_to_replied_message(MessageId::from_server_id(message_id))
                    .is_err()
            {
                self.connection.set_note(StatusNote::error(tr(
                    keys::Q_MISC_OPENED_CHAT_COULDNT_JUMP_TO,
                )));
            }
        }
        self.enter_chat_ui(chat_id, cx);
        self.restore_open_draft(window, cx);
        // The inline results button's start goes out right away (the user
        // pressed it); a link's `start=` only prefills `/start <param>`,
        // the user confirms.
        let started = self.take_inline_start(chat_id, cx)
            || self.take_premium_auto_start(chat_id, action, cx);
        if !started
            && let DeepLinkAction::OpenUsername {
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
            self.open_story_viewer(
                chat_id,
                *story_id,
                quill::story_viewer::StoryOpenScope::Link,
                cx,
            );
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
                body = body.child(
                    div()
                        .text_sm()
                        .text_color(text_muted())
                        .child(tr(keys::Q_MISC_JOINING_SENDS_A_REQUEST_AN)),
                );
            }
            let footer = div()
                .flex()
                .justify_end()
                .gap_2()
                .child(
                    Button::new("deep-link-invite-cancel")
                        .label(tr(keys::CANCEL))
                        .ghost()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.cancel_deep_link_invite(cx);
                            this.close_kit_dialog_if_done(DialogKind::DeepLinkInvite, window, cx);
                        })),
                )
                .child(
                    Button::new("deep-link-invite-join")
                        .label(tr(keys::Q_MISC_JOIN))
                        .primary()
                        .on_click(cx.listener(|this, _, window, cx| {
                            if let Some(DeepLinkState::InvitePreview { generation, .. }) =
                                this.links.deep_link_invite.take()
                                && let Some(live) = this.live.as_mut()
                                && let Ok(Some(_)) =
                                    live.driver.confirm_deep_link_invite(generation)
                            {
                                // Join starts its own flow, from the window
                                // of this box: the joined chat and the
                                // request-sent or error box go there.
                                this.screen.links.flow_window = this.asking_window().flatten();
                            }
                            cx.notify();
                            this.close_kit_dialog_if_done(DialogKind::DeepLinkInvite, window, cx);
                        })),
                );
            dialog
                .overlay(true)
                .title(crate::ui::shell::dialog_title(if channel {
                    tr(keys::Q_MISC_JOIN_CHANNEL)
                } else {
                    tr(keys::Q_MISC_JOIN_GROUP)
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
                    .label(tr(keys::BOX_OK))
                    .primary()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.links.deep_link_dialog = None;
                        cx.notify();
                        this.close_kit_dialog_if_done(DialogKind::DeepLinkInfo, window, cx);
                    })),
            );
            dialog
                .overlay(true)
                .title(crate::ui::shell::dialog_title(tr(
                    keys::Q_MISC_TELEGRAM_LINK,
                )))
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
    )
    .close(|app, _, cx| {
            app.links.deep_link_dialog = None;
            cx.notify();
        }),

    DeepLinkInvite => DialogSpec::new(
        2800,
        |app| app.links.deep_link_invite.is_some(),
        QuillApp::build_deep_link_invite_dialog,
    )
    .close(|app, _, cx| app.cancel_deep_link_invite(cx)),
}

#[cfg(test)]
mod pump_tests {
    use super::{deep_link_step_redraws, queued_by_agent};
    use quill::state::DeepLinkState;

    #[test]
    fn a_queued_link_is_the_agents_only_if_the_agent_queued_that_link() {
        let mut agent = Some("https://t.me/agentpick".to_string());
        assert!(queued_by_agent(&mut agent, "https://t.me/agentpick"));
        assert_eq!(agent, None, "taken once");
        assert!(!queued_by_agent(&mut agent, "https://t.me/agentpick"));
        // The launch argument or the OS replaced the agent's link.
        let mut agent = Some("https://t.me/agentpick".to_string());
        assert!(!queued_by_agent(&mut agent, "https://t.me/userpick"));
        assert_eq!(agent, None);
    }

    /// A link clicked in a chat window keeps that window as its origin
    /// only while it is the link being resolved: the main window comes
    /// forward for it, not for a later link queued from elsewhere.
    #[test]
    fn a_queued_link_keeps_the_chat_window_it_was_clicked_in() {
        use quill::ids::ChatId;
        let mut origin = Some(("https://t.me/pick".to_string(), ChatId(12)));
        assert_eq!(
            super::queued_in_window(&mut origin, "https://t.me/pick"),
            Some(ChatId(12))
        );
        assert_eq!(origin, None, "taken once");
        let mut origin = Some(("https://t.me/pick".to_string(), ChatId(12)));
        assert_eq!(
            super::queued_in_window(&mut origin, "https://t.me/os"),
            None
        );
    }

    #[test]
    fn a_link_that_opened_its_chat_shows_no_note() {
        assert!(super::chat_link_note(true).is_none());
        let failed = super::chat_link_note(false).unwrap();
        assert_eq!(failed.kind(), crate::ui::status_note::NoteKind::Error);
    }

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
