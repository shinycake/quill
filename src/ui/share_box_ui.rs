//! Share box logic (the box itself is `forward_box`), forward bar and
//! "send as" picker.
//!
//! Mirrors tdesktop's `ShareBox` (boxes/share_box.cpp), `ForwardPanel`
//! (history/view/controls/history_view_forward_panel.cpp) and
//! `ChooseSendAsBox` (ui/chat/choose_send_as.cpp); see
//! `docs/decisions/codex-share-box.md`.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::*;
use crate::ui::status_note::StatusNote;
use gpui_kit::component::button::*;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::composer::ComposerSnapshot;
use quill::ids::ChatId;
use quill::lang::{keys, tr, tr_with};
use quill::share_box::{ShareSend, forward_bar_lines, share_done_label};
use quill::telegram::envelope::ChatKind;

use super::forward_stage::ForwardStage;
impl QuillApp {
    pub(super) fn toggle_share_destination(&mut self, id: ChatId, cx: &mut Context<Self>) {
        self.share.selection.toggle(id);
        cx.notify();
    }

    /// The typed query changed: ask the server too (`searchChatsOnServer`).
    pub(super) fn sync_share_search(&mut self, text: &str, cx: &mut Context<Self>) {
        if !self.share.forward.picker_open() && !self.share.reply_elsewhere_open {
            return;
        }
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.search_share_chats(text);
        }
        cx.notify();
    }

    /// Enter in the share box search toggles the first match, which is the
    /// only result once the query narrows the list (tdesktop
    /// `ShareBox::Inner::selectActive`). Ctrl/Cmd+Enter sends instead
    /// (`setSubmittedCallback`), see `submit_share_from_search`.
    pub(super) fn activate_first_forward_destination(&mut self, cx: &mut Context<Self>) {
        let query = self.share.search_input.read(cx).value().to_string();
        let first = self
            .session()
            .and_then(|session| session.share_destinations(&query).into_iter().next())
            .map(|chat| chat.id);
        if let Some(first) = first {
            self.toggle_share_destination(first, cx);
        }
    }

    /// Ctrl/Cmd+Enter in the share box search: send to the ticked chats.
    pub(super) fn submit_share_from_search(&mut self, cx: &mut Context<Self>) {
        if !self.share.selection.is_empty() {
            self.submit_share(ShareSend::Normal, cx);
        }
    }

    /// Why the draft cannot go to `dest` right now, if it cannot.
    pub(in crate::ui) fn share_dest_refusal(
        &mut self,
        dest: ChatId,
        cx: &mut Context<Self>,
    ) -> Option<String> {
        // Phase S1: messages cannot be forwarded to secret chats (TGX
        // `SecretChatForwardError`, verbatim).
        let dest_is_secret = self
            .session()
            .and_then(|s| s.chats.get(&dest.0))
            .is_some_and(|chat| matches!(chat.kind, ChatKind::Secret { .. }));
        if dest_is_secret {
            return Some(tr(keys::Q_OPS_THIS_MESSAGE_CANNOT_BE_FORWARDED).into());
        }
        // Phase A1: slow-mode gate applies to forwards — forwarding sends
        // messages to the destination chat.
        if self.slow_mode_blocked(dest, cx) {
            return Some(self.connection.status_note().to_string());
        }
        None
    }

    /// Send the draft to every ticked chat: the comment first (a plain
    /// message), then the forwards with the chosen sound / schedule.
    pub(super) fn submit_share(&mut self, mode: ShareSend, cx: &mut Context<Self>) {
        let Some(draft) = self.forward_draft() else {
            return;
        };
        let dests: Vec<ChatId> = self.share.selection.chats().to_vec();
        if dests.is_empty() {
            self.set_note(StatusNote::silent(tr(keys::Q_OPS_CHOOSE_A_CHAT_TO_FORWARD)));
            cx.notify();
            return;
        }
        for dest in &dests {
            if let Some(note) = self.share_dest_refusal(*dest, cx) {
                self.set_note(StatusNote::error(note));
                cx.notify();
                return;
            }
        }
        let options = mode.options();
        let comment = self.share.comment_input.read(cx).value().trim().to_string();
        let titles: Vec<String> = dests
            .iter()
            .map(|dest| {
                self.session()
                    .and_then(|s| s.chats.get(&dest.0).map(|chat| chat.title.clone()))
                    .unwrap_or_else(|| format!("chat {}", dest.0))
            })
            .collect();
        let mut failed = 0;
        if let Some(live) = self.live.as_mut() {
            let view_generation = live.driver.session.view_generation;
            for dest in &dests {
                if !comment.is_empty() {
                    let snap = ComposerSnapshot::capture(*dest, view_generation, comment.clone())
                        .with_send_options(options);
                    if live.driver.send_snapshot(&snap).is_err() {
                        failed += 1;
                        continue;
                    }
                }
                if live
                    .driver
                    .forward_messages_with_options(*dest, &draft, &options)
                    .is_err()
                {
                    failed += 1;
                }
            }
            self.set_note(if failed == 0 {
                StatusNote::info(share_done_label(draft.count(), &titles))
            } else {
                StatusNote::error(tr_with(
                    keys::Q_OPS_COULD_NOT_FORWARD_TO_CHAT,
                    &[("failed", &failed.to_string())],
                ))
            });
        } else if self.demo_session.is_some() {
            for dest in &dests {
                self.apply_demo_forward(*dest, &draft);
            }
            if let Some(result) = self
                .demo_session
                .as_mut()
                .and_then(|session| session.messages.last_forward.take())
            {
                self.present_forward_result(result, cx);
            }
        }
        if failed < dests.len() {
            self.share.forward = ForwardStage::None;
            self.share.selection.clear();
        }
        cx.notify();
    }

    /// Copy the t.me link of the single selected message
    /// (`getMessageProperties` then `getMessageLink`).
    pub(super) fn share_copy_link(&mut self, cx: &mut Context<Self>) {
        let Some((chat_id, message_id)) = self
            .forward_draft()
            .filter(|draft| draft.count() == 1)
            .and_then(|draft| Some((draft.from_chat_id, *draft.message_ids.first()?)))
        else {
            return;
        };
        if !self.share_viewer_link_at_time(chat_id, message_id, cx) {
            self.share_message_link(chat_id, message_id, cx);
        }
    }

    /// One destination chosen: open it with the forward bar above its
    /// composer (the typed comment moves into the composer). The forward
    /// and the comment ride with the open to the window that shows the
    /// chat; the box's window keeps none of it.
    pub(super) fn share_continue_in_chat(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(dest) = self.share.selection.single() else {
            return;
        };
        if let Some(note) = self.share_dest_refusal(dest, cx) {
            self.set_note(StatusNote::error(note));
            cx.notify();
            return;
        }
        let comment = self.share.comment_input.read(cx).value().trim().to_string();
        let draft = self.share.forward.take_draft();
        self.share.selection.clear();
        self.share
            .search_input
            .update(cx, |input, cx| input.set_value("", window, cx));
        let mut open = super::chat_window::ChatOpen::new(dest);
        if let Some(draft) = draft {
            open = open.forwarding(draft);
        }
        if !comment.is_empty() {
            open = open.compose(comment);
        }
        self.select_chat_request(open, window, cx);
        cx.notify();
    }

    /// The forward bar is showing in the open chat.
    pub(super) fn forward_bar_here(&self) -> bool {
        let Some(dest) = self.share.forward.bar_shown() else {
            return false;
        };
        !self.chat_ui.is_editing()
            && self.forward_draft().is_some()
            && self.session().and_then(|s| s.open_chat) == Some(dest)
    }

    /// Composer Send with the forward bar up: the typed text goes first as a
    /// normal message, then the forward (tdesktop sends the comment before
    /// the forwarded messages).
    pub(super) fn submit_forward_bar(
        &mut self,
        text: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (Some(dest), Some(draft)) = (self.share.forward.bar_shown(), self.forward_draft())
        else {
            return;
        };
        if let Some(note) = self.share_dest_refusal(dest, cx) {
            self.set_note(StatusNote::error(note));
            cx.notify();
            return;
        }
        let options = self.composer_send_options();
        if !text.trim().is_empty() || !self.pending_attachments().is_empty() {
            // The comment goes as a plain message: the bar is lowered to
            // the selection while it does, so the composer does not route
            // it back here.
            self.share.forward.drop_bar();
            self.submit_composer(text, window, cx);
            self.share.forward = match self.share.forward.take_draft() {
                Some(draft) => ForwardStage::Bar { draft, dest },
                None => ForwardStage::None,
            };
            // The comment did not go out (the composer keeps it): stop.
            if !self.composer.read(cx).value().trim().is_empty()
                || !self.pending_attachments().is_empty()
            {
                return;
            }
        }
        if let Some(live) = self.live.as_mut() {
            let result = live
                .driver
                .forward_messages_with_options(dest, &draft, &options);
            match result {
                Ok(_) => {
                    self.set_note(StatusNote::silent(tr(keys::Q_OPS_FORWARDING)));
                    self.share.forward = ForwardStage::None;
                    self.update_pending_send(|send| {
                        send.scheduling = quill::composer::ComposerScheduling::None
                    });
                }
                Err(_) => self.set_note(StatusNote::error(tr(
                    keys::Q_OPS_COULD_NOT_FORWARD_MESSAGES,
                ))),
            }
        } else if self.demo_session.is_some() {
            self.apply_demo_forward(dest, &draft);
            if let Some(result) = self
                .demo_session
                .as_mut()
                .and_then(|session| session.messages.last_forward.take())
            {
                self.present_forward_result(result, cx);
            }
        }
        cx.notify();
    }

    /// "Hide sender name": forward as copies (`forwardMessages.send_copy`);
    /// showing names again shows captions too.
    pub(super) fn set_forward_send_copy(&mut self, on: bool, cx: &mut Context<Self>) {
        if let Some(draft) = self.share.forward.draft_mut() {
            draft.send_copy = on;
            if !on {
                draft.remove_caption = false;
            }
        }
        cx.notify();
    }

    fn forward_remove_caption(&self) -> bool {
        self.share
            .forward
            .draft()
            .is_some_and(|draft| draft.remove_caption)
    }

    /// "Hide captions" (`remove_caption`), which only copies can drop.
    pub(super) fn toggle_forward_remove_caption(&mut self, cx: &mut Context<Self>) {
        if let Some(draft) = self.share.forward.draft_mut()
            && draft.send_copy
        {
            draft.remove_caption = !draft.remove_caption;
        }
        cx.notify();
    }

    /// "Hide sender name" / "Hide captions" (`forwardMessages.send_copy` /
    /// `remove_caption`; captions only drop on copies).
    fn forward_option_checkboxes(&self, prefix: &'static str, cx: &mut Context<Self>) -> Div {
        let send_copy = self
            .share
            .forward
            .draft()
            .is_some_and(|draft| draft.send_copy);
        let remove_caption = self
            .share
            .forward
            .draft()
            .is_some_and(|draft| draft.remove_caption);
        let multiple = self.forward_draft().is_some_and(|draft| draft.count() > 1);
        div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_3()
            .child(
                Checkbox::new((prefix, 0usize))
                    .label(if multiple {
                        tr(keys::Q_OPS_HIDE_SENDER_NAMES)
                    } else {
                        tr(keys::Q_OPS_HIDE_SENDER_NAME)
                    })
                    .checked(send_copy)
                    .on_click(cx.listener(|this, &on, _, cx| {
                        this.set_forward_send_copy(on, cx);
                    })),
            )
            .child(
                Checkbox::new((prefix, 1usize))
                    .label(if multiple {
                        tr(keys::Q_OPS_HIDE_CAPTIONS)
                    } else {
                        tr(keys::Q_OPS_HIDE_CAPTION)
                    })
                    .checked(remove_caption)
                    .disabled(!send_copy)
                    .on_click(cx.listener(|this, &on, _, cx| {
                        if this.forward_remove_caption() != on {
                            this.toggle_forward_remove_caption(cx);
                        }
                    })),
            )
    }

    /// The bar above the composer after one destination was chosen:
    /// who the messages are from, a preview, and the forward options.
    pub(super) fn forward_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let draft = self.forward_draft();
        let count = draft.as_ref().map(|d| d.count()).unwrap_or(0);
        let hide_sender = draft.as_ref().is_some_and(|d| d.send_copy);
        let (senders, preview) = match (&draft, self.session()) {
            (Some(draft), Some(session)) => (
                session.forward_bar_senders(draft.from_chat_id, &draft.message_ids),
                draft
                    .message_ids
                    .first()
                    .map(|id| session.forward_bar_preview(draft.from_chat_id, *id))
                    .unwrap_or_default(),
            ),
            _ => (Vec::new(), String::new()),
        };
        let (from, text) = forward_bar_lines(&senders, count, hide_sender, &preview);
        div()
            .id("forward-bar")
            .flex()
            .flex_col()
            .gap_1()
            .px_3()
            .py_2()
            .rounded_md()
            .border_l_2()
            .border_color(accent())
            .bg(bg_subtle())
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .min_w_0()
                            .child(
                                div()
                                    .text_xs()
                                    .font_semibold()
                                    .text_color(accent())
                                    .child(tr(keys::Q_OPS_FORWARD)),
                            )
                            .child(
                                div()
                                    .truncate()
                                    .text_sm()
                                    .font_medium()
                                    .text_color(text_primary())
                                    .child(super::bidi_line::name_line(from)),
                            )
                            .child(
                                div()
                                    .truncate()
                                    .text_sm()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(super::bidi_line::one_line_plain(text)),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .gap_1()
                            .child(
                                Button::new("forward-bar-change")
                                    .label(tr(keys::Q_OPS_CHANGE_RECIPIENT))
                                    .ghost()
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.change_forward_recipient(window, cx);
                                    })),
                            )
                            .child(
                                Button::new("forward-bar-cancel")
                                    .icon(gpui_kit::assets::IconName::X)
                                    .xsmall()
                                    .ghost()
                                    .accessibility_label(tr(keys::Q_OPS_CANCEL_FORWARD))
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.clear_forward(window, cx);
                                    })),
                            ),
                    ),
            )
            .child(self.forward_option_checkboxes("forward-bar-option", cx))
    }

    /// "Change recipient": reopen the share box with the current chat
    /// ticked; Cancel there returns to the bar.
    pub(super) fn change_forward_recipient(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let dest = self.share.forward.bar_shown();
        self.open_forward_picker(window, cx);
        if let Some(dest) = dest {
            self.share.selection.toggle(dest);
        }
        cx.notify();
    }
}
