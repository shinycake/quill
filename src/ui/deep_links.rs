//! `t.me` / `tg:` deep-link handling (`parity:platform-deep-links`).
//!
//! `main.rs` stashes the launch link in `QuillApp::pending_deep_link`.
//! [`QuillApp::pump_deep_link`] (called from `poll_live`, no window
//! needed) fires `getDeepLinkInfo` once auth is Ready and drives the
//! driver-facing half of the flow; the chat open itself happens in
//! render via [`QuillApp::open_deep_link_chat`], which has the `Window`
//! (same take-once pattern as `StoryOpen::Waiting`).
#![deny(clippy::indexing_slicing)]
use super::app::QuillApp;
use super::chat_window::AskWaits;
use super::links_state::{PendingOpen, QueuedLink, ResolvedLink, ResolvedStep};
use super::shell::{DialogKind, QuillShell};
use super::*;
use crate::ui::status_note::StatusNote;
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::*;
use quill::agent::ui_driver::guard;
use quill::ids::{ChatId, MessageId, RequestId};
use quill::lang::{keys, tr};
use quill::state::{DeepLinkAction, DeepLinkFlow, DeepLinkState, HistoryOpen};
use quill::telegram::envelope::AuthorizationState;

/// Whether one `pump_deep_link` step has something to show: a terminal
/// state to consume, or an invite preview not on screen yet. No flow, a
/// request in flight, and the invite already shown (its box is up for that
/// flow's request) change nothing.
fn deep_link_step_redraws(state: Option<&DeepLinkState>, shown_invite: Option<RequestId>) -> bool {
    match state {
        None | Some(DeepLinkState::ResolvingInfo { .. } | DeepLinkState::ResolvingChat { .. }) => {
            false
        }
        Some(DeepLinkState::InvitePreview { request, .. }) => shown_invite != Some(*request),
        Some(_) => true,
    }
}

/// The note after a t.me link opened its chat: none when it did, as
/// Telegram Desktop shows nothing then.
fn chat_link_note(opened: bool) -> Option<StatusNote> {
    (!opened).then(|| StatusNote::error(tr(keys::Q_MISC_COULDNT_OPEN_THE_CHAT)))
}

impl QuillApp {
    /// Driver-facing half of the deep-link flow, run from `poll_live`.
    /// Fires `getDeepLinkInfo` once auth is Ready, then turns each
    /// terminal session state into the next step: follow-up request,
    /// info dialog, or a deferred chat open for render.
    pub(super) fn pump_deep_link(&mut self, cx: &mut Context<Self>) {
        // A link that waited in the OS inbox behind the one a step took
        // comes in once the slot is free: the link pump sleeps instead of
        // polling for it, and this is where it is told.
        if self.pending_deep_link.is_none() && quill::deep_link_inbox::has_pending_links() {
            quill::wake::LINKS.poke();
        }
        self.pump_start_files(cx);
        // A link that arrives while Quill is locked waits for the unlock,
        // as tdesktop's start links do (`checkStartUrls`): nothing it
        // opens may show over the lock. A linked media timestamp waits
        // too: its seek plays or opens media, so it stays pending (until
        // its deadline) and runs after the unlock.
        if self.refused_while_locked() {
            return;
        }
        self.tick_media_seek(cx);
        // A link clicked in an account that left the screen goes with it:
        // it names that account's chats, bots and posts.
        let on_screen = self.session().map(|session| session.account.clone());
        if self
            .pending_deep_link
            .as_ref()
            .is_some_and(|queued| !queued.for_screen(on_screen.as_ref()) && on_screen.is_some())
        {
            self.pending_deep_link = None;
        }
        // `tg://proxy` / `tg://socks` (and the t.me forms) are parsed
        // locally and need no sign-in: a user who is blocked from
        // Telegram can only get in through them.
        if let Some(queued) = self
            .pending_deep_link
            .take_if(|queued| quill::proxy::parse_proxy_link(&queued.link).is_some())
        {
            // Its box belongs to the window the link was clicked in, not
            // to the main window this poll runs in.
            self.as_asked_by(queued.window, |this| {
                this.handle_proxy_link(&queued.link, cx)
            });
            return;
        }
        let Some(live) = self.live.as_mut() else {
            return;
        };
        if !matches!(live.driver.session.auth, AuthorizationState::Ready) {
            return;
        }
        // What a box opened by this step waits for: the requests out once
        // the step has sent its own (read by the next dialog sync).
        let waits = AskWaits::Unread;
        // One flow at a time: a link waits while the previous one is being
        // resolved. An invite preview waiting for the user's answer gives
        // way to a new link: its box closes, as it is modal only in its
        // own window and a click in another must not wait for it.
        let invite_waits = matches!(
            live.driver.session.chats_state.deep_link_stage(),
            Some(DeepLinkState::InvitePreview { .. })
        );
        if (live.driver.session.chats_state.deep_link.is_none() || invite_waits)
            && let Some(queued) = self
                .pending_deep_link
                .take_if(|queued| queued.for_screen(Some(&live.driver.session.account)))
        {
            if invite_waits {
                live.driver.session.chats_state.end_deep_link_flow();
            }
            // A new flow gives way to an invite box still up.
            self.screen.links.invite_box.take();
            // A link whose send fails leaves a note: it was taken from its
            // slot, and nothing else would say it did not go out.
            if live
                .driver
                .request_deep_link_info_from(&queued.link, queued.by_agent)
                .is_err()
            {
                self.screen.connection.set_note_for(
                    &self.screen.windows,
                    StatusNote::error(tr(keys::Q_CHAT_COULD_NOT_OPEN_LINK)),
                );
            }
            live.driver
                .session
                .chats_state
                .set_deep_link_window(queued.window);
        }
        // Consume terminal states once, retaining the invite preview until a
        // decision. The flow carries its origin window, so every terminal
        // state (and no flow at all, after an error or a closed box) leaves
        // none behind.
        let shown_invite = self.screen.links.invite_box.request();
        let waiting = live
            .driver
            .session
            .chats_state
            .deep_link_stage()
            .is_some_and(|stage| deep_link_step_redraws(Some(stage), shown_invite));
        if !waiting {
            // Nothing new (this runs on every poll).
            return;
        }
        let Some(DeepLinkFlow {
            stage,
            from_agent,
            window: from_window,
        }) = live.driver.session.chats_state.take_deep_link_step()
        else {
            return;
        };
        match stage {
            // `take_deep_link_step` leaves a request in flight in place.
            DeepLinkState::ResolvingInfo { .. } | DeepLinkState::ResolvingChat { .. } => {}
            DeepLinkState::Info {
                text,
                need_update,
                action,
                ..
            } => {
                match (need_update, action) {
                    // The real in-app updater is a queued future slice; the
                    // honest UI is TDLib's own text saying so.
                    (true, _) => {
                        self.cancel_inline_opening();
                        self.show_deep_link_info(text, from_window, cx);
                    }
                    (false, Some(action)) => {
                        let _ = live.driver.continue_deep_link(action, from_agent);
                        live.driver
                            .session
                            .chats_state
                            .set_deep_link_window(from_window);
                    }
                    // Unknown link: TDLib's info text is the confirmation
                    // copy written for exactly this case.
                    (false, None) => {
                        self.cancel_inline_opening();
                        self.show_deep_link_info(text, from_window, cx);
                    }
                }
            }
            // The user decides on the invite; Join starts its own origin.
            DeepLinkState::InvitePreview { request, .. } => {
                // A preview already up is the same flow's: it stays where
                // it shows. A new one belongs to the window the link was
                // clicked in, not to the window the pointer is over now.
                // The preview itself stays the session flow's stage.
                if !self.screen.links.invite_box.is_open() {
                    self.screen.windows.open_box_in(
                        DialogKind::DeepLinkInvite,
                        from_window,
                        waits.clone(),
                    );
                }
                self.screen.links.invite_box.show(request);
                self.cancel_inline_opening();
            }
            DeepLinkState::ChatReady { chat_id, action } => {
                self.cancel_inline_opening_unless_chat(chat_id);
                self.hold_resolved_link(
                    ResolvedStep::Open(PendingOpen {
                        chat_id,
                        action,
                        from_agent,
                    }),
                    from_window,
                    cx,
                );
            }
            DeepLinkState::ShowText(text) => {
                self.cancel_inline_opening();
                self.show_deep_link_info(text, from_window, cx);
            }
            // Typed links (`deep_link_types`): the UI half runs in render.
            // An agent's step gets none of them (settings, proxies, folders,
            // sticker sets, backgrounds): nothing an agent needs, all of it
            // the user's.
            DeepLinkState::Ui(ui) => {
                self.cancel_inline_opening();
                if from_agent {
                    guard::refuse_typed_link();
                } else {
                    self.hold_resolved_link(ResolvedStep::Ui(ui), from_window, cx);
                }
            }
            // TDLib has no type for it: `getDeepLinkInfo` explains it.
            DeepLinkState::Unknown { link } => {
                let _ = live.driver.request_deep_link_text_from(&link, from_agent);
                live.driver
                    .session
                    .chats_state
                    .set_deep_link_window(from_window);
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
        let resolved = self.links.resolved.as_ref()?;
        let in_main = match &resolved.step {
            ResolvedStep::Open(open) => !self.windows.has_window(open.chat_id),
            ResolvedStep::Ui(ui) => super::deep_link_routes::opens_in_main_window(ui),
        };
        if in_main {
            return None;
        }
        resolved.window.filter(|chat| self.windows.has(*chat))
    }

    /// A resolved link waits for the frame of [`Self::resolved_link_frame`].
    /// Asked from a chat window for something the main window opens, the
    /// main window comes forward now, so its next frame runs it even from
    /// minimized or hidden.
    pub(in crate::ui) fn hold_resolved_link(
        &mut self,
        step: ResolvedStep,
        from_window: Option<ChatId>,
        cx: &mut Context<Self>,
    ) {
        self.links.resolved = Some(ResolvedLink {
            window: from_window,
            step,
        });
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
        if self.refused_while_locked() || self.links.resolved.is_none() {
            return;
        }
        let frame = self.resolved_link_frame();
        if frame != self.windows.scope {
            return;
        }
        let Some(resolved) = self.links.resolved.take() else {
            return;
        };
        self.as_asked_by(frame, |this| match resolved.step {
            ResolvedStep::Open(open) => {
                this.open_deep_link_chat(open, window, cx);
                this.drop_shared_card(cx);
            }
            ResolvedStep::Ui(ui) => this.run_deep_link_ui(ui, window, cx),
        });
    }

    /// Starts the deep-link flow for `link` at the next poll, remembering
    /// the account on screen, whether the handler asking is a step the UI
    /// driver injected, and the chat window that asked (`asking_window`:
    /// the window of the click, or the window an answer's request carried;
    /// none for a mini app's link, which the main window takes). The
    /// origin rides with the link (`QueuedLink`): a later link replaces
    /// it whole, and the flow keeps its own in the session
    /// (`DeepLinkFlow::from_agent`).
    pub(super) fn queue_deep_link(&mut self, link: String) {
        self.pending_deep_link = Some(QueuedLink {
            link,
            account: self.session().map(|session| session.account.clone()),
            window: self.asking_window().flatten(),
            by_agent: guard::injecting(),
        });
        quill::wake::UPDATES.poke();
    }

    /// Resolve `action` (a box's or toast's "open @username") as a flow of
    /// the window that asks, as [`Self::queue_deep_link`] does for a link:
    /// the chat it resolves opens there, and the main window comes forward
    /// for it when another window asked (`hold_resolved_link`). The one way
    /// UI code starts a resolve; `scripts/main_frame_queues.py` rejects a
    /// bare `driver.resolve_deep_link` elsewhere in src/ui (round 18 B #8).
    /// `None` without a live session, else whether the request went out.
    pub(in crate::ui) fn resolve_deep_link_here(&mut self, action: DeepLinkAction) -> Option<bool> {
        let window = self.asking_window().flatten();
        let live = self.live.as_mut()?;
        let sent = live.driver.resolve_deep_link(action).is_ok();
        if sent {
            live.driver.session.chats_state.set_deep_link_window(window);
            self.screen.links.invite_box.take();
        }
        Some(sent)
    }

    /// A link from outside Quill (the OS, a second launch): whichever
    /// account is on screen takes it, in the main window.
    pub(super) fn queue_outside_link(&mut self, link: String) {
        self.pending_deep_link = Some(QueuedLink::outside(link));
        quill::wake::UPDATES.poke();
    }

    /// Bring the main window forward at the end of this update, restoring
    /// it when minimized or hidden, so its next frame runs. Every queue
    /// that only the main window's frame drains calls this when it fills
    /// for a press that may have come from another window (a chat window,
    /// a mini app, a call window): a minimized or hidden main window draws
    /// no frame, and `cx.activate` alone does not restore it
    /// (`scripts/check-window-scope.py`, "main-frame queues").
    pub(in crate::ui) fn raise_main_window_later(&self, cx: &mut Context<Self>) {
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
                    // Its thread and media timestamp are that window's,
                    // not the asking frame's.
                    self.in_window_scope(Some(chat_id), cx, |app, cx| {
                        app.finish_typed_link(chat_id, action, window, cx);
                    });
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
                self.screen
                    .connection
                    .set_note_for(&self.screen.windows, note);
            }
            if let Some(message_id) = jump_to
                && live
                    .driver
                    .jump_to_replied_message(MessageId::from_server_id(message_id))
                    .is_err()
            {
                self.set_note(StatusNote::error(tr(
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

    /// Join from the invite box. Join starts its own flow, from the window of
    /// this box: the joined chat and the request-sent or error box go there,
    /// a "Couldn't reach Telegram" too, so the window is set whatever the
    /// send returns. A send refused before the flow changed (the client is
    /// not ready) leaves the invite as it was: it is dropped with a note.
    fn join_deep_link_invite(&mut self, cx: &mut Context<Self>) {
        let Some(request) = self.links.invite_box.take() else {
            cx.notify();
            return;
        };
        let from = self.asking_window().flatten();
        let mut refused = false;
        if let Some(live) = self.live.as_mut() {
            refused = live.driver.confirm_deep_link_invite(request).is_err()
                && matches!(
                    live.driver.session.chats_state.deep_link_stage(),
                    Some(DeepLinkState::InvitePreview { request: shown, .. }) if *shown == request
                );
            if refused {
                live.driver
                    .session
                    .chats_state
                    .cancel_deep_link_invite(request);
            }
            live.driver.session.chats_state.set_deep_link_window(from);
        }
        if refused {
            self.set_note(StatusNote::error(tr(keys::Q_CHAT_COULD_NOT_OPEN_LINK)));
        }
        cx.notify();
    }

    fn cancel_deep_link_invite(&mut self, cx: &mut Context<Self>) {
        if let Some(request) = self.links.invite_box.take()
            && let Some(live) = self.live.as_mut()
        {
            live.driver
                .session
                .chats_state
                .cancel_deep_link_invite(request);
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
            let (title, count, request, channel) = match this
                .session()
                .and_then(|session| session.chats_state.deep_link_stage())
            {
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
                            this.join_deep_link_invite(cx);
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
                .content(crate::ui::shell::one_shot_dialog_content(
                    body.into_any_element(),
                ))
                .footer(footer)
                .on_close(on_close)
        })
    }

    /// TDLib's text for a link opens in the window the link was clicked in
    /// (the main window once that one has closed): every window holds its
    /// own, so two links resolved for two windows each keep their text.
    pub(in crate::ui) fn show_deep_link_info(
        &mut self,
        text: String,
        window: Option<quill::ids::ChatId>,
        cx: &mut Context<Self>,
    ) {
        let shown = self
            .in_window_scope(window, cx, |this, _| {
                this.message_ui.deep_link_info = Some(text.clone());
            })
            .is_some();
        if !shown {
            self.in_window_scope(None, cx, |this, _| {
                this.message_ui.deep_link_info = Some(text);
            });
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
                this.message_ui.deep_link_info = None;
                cx.notify();
            });
        app.update(cx, |this, cx| {
            let text = this.message_ui.deep_link_info.clone().unwrap_or_default();
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
                        this.message_ui.deep_link_info = None;
                        cx.notify();
                        this.close_kit_dialog_if_done(DialogKind::DeepLinkInfo, window, cx);
                    })),
            );
            dialog
                .overlay(true)
                .title(crate::ui::shell::dialog_title(tr(
                    keys::Q_MISC_TELEGRAM_LINK,
                )))
                .content(crate::ui::shell::one_shot_dialog_content(body))
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
        |app| app.message_ui.deep_link_info.is_some(),
        QuillApp::build_deep_link_dialog,
    )
    .per_window()
    .close(|app, _, cx| {
            app.message_ui.deep_link_info = None;
            cx.notify();
        }),

    DeepLinkInvite => DialogSpec::new(
        2800,
        |app| app.links.invite_box.is_open(),
        QuillApp::build_deep_link_invite_dialog,
    )
    .close(|app, _, cx| app.cancel_deep_link_invite(cx)),
}

#[cfg(test)]
mod pump_tests {
    use super::QueuedLink;
    use super::deep_link_step_redraws;
    use quill::ids::{AccountKey, RequestId};
    use quill::state::DeepLinkState;

    /// Round 15 (area B, finding 5): a link clicked in an account goes
    /// with it; one from outside Quill waits for whichever account is on
    /// screen.
    #[test]
    fn a_queued_link_is_followed_only_by_the_account_it_was_clicked_in() {
        let a = AccountKey("a".into());
        let b = AccountKey("b".into());
        let clicked = QueuedLink {
            link: "https://t.me/c/1/2".into(),
            account: Some(a.clone()),
            window: None,
            by_agent: true,
        };
        assert!(clicked.for_screen(Some(&a)));
        assert!(!clicked.for_screen(Some(&b)));
        assert!(!clicked.for_screen(None), "waits for its account");
        let outside = QueuedLink::outside("https://t.me/durov".into());
        assert!(outside.for_screen(Some(&a)) && outside.for_screen(Some(&b)));
        assert!(outside.for_screen(None));
        assert!(!outside.by_agent);
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
        let resolving = DeepLinkState::ResolvingInfo {
            request: RequestId(1),
            original: String::new(),
        };
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
            request: RequestId(1),
        };
        assert!(deep_link_step_redraws(Some(&invite), None));
        assert!(!deep_link_step_redraws(Some(&invite), Some(RequestId(1))));
        assert!(
            deep_link_step_redraws(Some(&invite), Some(RequestId(2))),
            "a newer invite is another box's"
        );
        let text = DeepLinkState::ShowText("Unknown link".into());
        assert!(deep_link_step_redraws(Some(&text), None));
    }
}
