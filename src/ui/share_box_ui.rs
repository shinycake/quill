//! Share box logic (the box itself is `forward_box`), forward bar and
//! "send as" picker.
//!
//! Mirrors tdesktop's `ShareBox` (boxes/share_box.cpp), `ForwardPanel`
//! (history/view/controls/history_view_forward_panel.cpp) and
//! `ChooseSendAsBox` (ui/chat/choose_send_as.cpp); see
//! `docs/decisions/codex-share-box.md`.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::chat_row::chat_avatar;
use super::pressable::PressableDiv;
use super::*;
use crate::ui::status_note::StatusNote;
use gpui_kit::component::button::*;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::composer::ComposerSnapshot;
use quill::ids::ChatId;
use quill::lang::{keys, tr, tr_with};
use quill::share_box::{ShareSend, forward_bar_lines, share_done_label};
use quill::telegram::envelope::{ChatKind, MessageSender};
use std::path::PathBuf;

impl QuillApp {
    pub(super) fn toggle_share_destination(&mut self, id: ChatId, cx: &mut Context<Self>) {
        self.share.selection.toggle(id);
        cx.notify();
    }

    /// The typed query changed: ask the server too (`searchChatsOnServer`).
    pub(super) fn sync_share_search(&mut self, text: &str, cx: &mut Context<Self>) {
        if !self.share.forward_picker_open && !self.share.reply_elsewhere_open {
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
            self.connection
                .set_note(StatusNote::silent(tr(keys::Q_OPS_CHOOSE_A_CHAT_TO_FORWARD)));
            cx.notify();
            return;
        }
        for dest in &dests {
            if let Some(note) = self.share_dest_refusal(*dest, cx) {
                self.connection.set_note(StatusNote::error(note));
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
            self.connection.set_note(if failed == 0 {
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
            self.share.pending_forward = None;
            self.share.forward_bar_dest = None;
            self.share.forward_picker_open = false;
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
    /// composer (the typed comment moves into the composer).
    pub(super) fn share_continue_in_chat(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(dest) = self.share.selection.single() else {
            return;
        };
        if let Some(note) = self.share_dest_refusal(dest, cx) {
            self.connection.set_note(StatusNote::error(note));
            cx.notify();
            return;
        }
        let comment = self.share.comment_input.read(cx).value().trim().to_string();
        self.share.forward_bar_dest = Some(dest);
        self.share.forward_picker_open = false;
        self.share.selection.clear();
        self.share
            .search_input
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.select_listed_chat(dest, window, cx);
        if !comment.is_empty() {
            self.set_composer_markup(&comment, window, cx);
        }
        self.connection.set_note(StatusNote::silent(tr(
            keys::Q_OPS_FORWARD_BAR_READY_PRESS_SEND,
        )));
        cx.notify();
    }

    /// The forward bar is showing in the open chat.
    pub(super) fn forward_bar_here(&self) -> bool {
        let Some(dest) = self.share.forward_bar_dest else {
            return false;
        };
        !self.share.forward_picker_open
            && self.composer_ui.pending_edit.is_none()
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
        let (Some(dest), Some(draft)) = (self.share.forward_bar_dest, self.forward_draft()) else {
            return;
        };
        if let Some(note) = self.share_dest_refusal(dest, cx) {
            self.connection.set_note(StatusNote::error(note));
            cx.notify();
            return;
        }
        let options = self.composer_send_options();
        if !text.trim().is_empty() || !self.pending_attachments().is_empty() {
            self.share.forward_bar_dest = None;
            self.submit_composer(text, window, cx);
            self.share.forward_bar_dest = Some(dest);
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
                    self.connection
                        .set_note(StatusNote::silent(tr(keys::Q_OPS_FORWARDING)));
                    self.share.pending_forward = None;
                    self.share.forward_bar_dest = None;
                    self.update_pending_send(|send| {
                        send.scheduling = quill::composer::ComposerScheduling::None
                    });
                }
                Err(_) => self.connection.set_note(StatusNote::error(tr(
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
        if let Some(draft) = self.share.pending_forward.as_mut() {
            draft.send_copy = on;
            if !on {
                draft.remove_caption = false;
            }
        }
        cx.notify();
    }

    fn forward_remove_caption(&self) -> bool {
        self.share
            .pending_forward
            .as_ref()
            .is_some_and(|draft| draft.remove_caption)
    }

    /// "Hide captions" (`remove_caption`), which only copies can drop.
    pub(super) fn toggle_forward_remove_caption(&mut self, cx: &mut Context<Self>) {
        if let Some(draft) = self.share.pending_forward.as_mut()
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
            .pending_forward
            .as_ref()
            .is_some_and(|draft| draft.send_copy);
        let remove_caption = self
            .share
            .pending_forward
            .as_ref()
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
        let dest = self.share.forward_bar_dest;
        self.open_forward_picker(window, cx);
        if let Some(dest) = dest {
            self.share.selection.toggle(dest);
        }
        cx.notify();
    }

    // ---- send as ---------------------------------------------------

    /// Composer button showing the current "send as" identity; only for
    /// chats where TDLib reports a selectable sender.
    pub(super) fn send_as_button(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let session = self.session()?;
        let chat_id = session.open_chat?;
        let sender = session.selected_message_sender(chat_id)?;
        let name = session.message_sender_title(sender);
        let photo = match sender {
            MessageSender::Chat { chat_id } => session.chat_photo_path(ChatId(chat_id)),
            MessageSender::User { user_id } => session
                .user_photo_path(user_id)
                .or_else(|| session.chat_photo_path(ChatId(user_id))),
        }
        .map(PathBuf::from);
        Some(
            div()
                .id("composer-send-as")
                .flex_none()
                .rounded_full()
                .role(gpui_kit::Role::Button)
                .aria_label(tr_with(keys::Q_OPS_SEND_AS, &[("name", &name.to_string())]))
                .tab_index(0)
                .cursor_pointer()
                .child(chat_avatar(&name, photo.as_deref(), 28.))
                .on_click(cx.listener(|this, _, _, cx| this.toggle_send_as(cx)))
                .into_any_element(),
        )
    }

    pub(super) fn toggle_send_as(&mut self, cx: &mut Context<Self>) {
        self.composer_ui.send_as_open = !self.composer_ui.send_as_open;
        if self.composer_ui.send_as_open {
            let chat = self.session().and_then(|s| s.open_chat);
            if let (Some(chat), Some(live)) = (chat, self.live.as_mut()) {
                let _ = live.driver.get_chat_available_message_senders(chat);
            }
        }
        cx.notify();
    }

    fn choose_message_sender(
        &mut self,
        chat_id: ChatId,
        sender: MessageSender,
        locked: bool,
        cx: &mut Context<Self>,
    ) {
        if locked {
            self.connection.set_note(StatusNote::info(tr(
                keys::Q_OPS_SUBSCRIBE_TO_TELEGRAM_PREMIUM_TO,
            )));
            cx.notify();
            return;
        }
        if let Some(live) = self.live.as_mut() {
            self.connection
                .set_note(match live.driver.set_chat_message_sender(chat_id, sender) {
                    Ok(()) => StatusNote::silent(tr(keys::Q_OPS_CHANGING_SENDER)),
                    Err(_) => StatusNote::error(tr(keys::Q_OPS_COULD_NOT_CHANGE_THE_SENDER)),
                });
        } else {
            self.apply_demo_message_sender(chat_id, sender);
        }
        self.composer_ui.send_as_open = false;
        cx.notify();
    }

    /// "Send message as…": one row per identity (`chatMessageSender`),
    /// the current one ticked, premium-only ones dimmed.
    pub(super) fn send_as_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let session = self.session();
        let chat_id = session.and_then(|s| s.open_chat);
        let current = chat_id.and_then(|id| session.and_then(|s| s.selected_message_sender(id)));
        let mut list = div()
            .id("send-as-list")
            .flex()
            .flex_col()
            .gap_1()
            .max_h(px(240.))
            .overflow_y_scroll();
        let senders = match (session, chat_id) {
            (Some(session), Some(chat_id)) => session.available_message_senders(chat_id),
            _ => &[],
        };
        if senders.is_empty() {
            list = list.child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(tr(keys::Q_OPS_LOADING)),
            );
        }
        for (index, entry) in senders.iter().enumerate() {
            let (Some(session), Some(chat_id)) = (session, chat_id) else {
                break;
            };
            let sender = entry.sender;
            let title = session.message_sender_title(sender);
            let status = session.message_sender_status(chat_id, entry);
            let photo = match sender {
                MessageSender::Chat { chat_id } => session.chat_photo_path(ChatId(chat_id)),
                MessageSender::User { user_id } => session
                    .user_photo_path(user_id)
                    .or_else(|| session.chat_photo_path(ChatId(user_id))),
            }
            .map(PathBuf::from);
            let locked = entry.needs_premium;
            let selected = current == Some(sender);
            list = list.child(
                div()
                    .id(("send-as-row", index))
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .role(gpui_kit::Role::Button)
                    .aria_label(tr_with(
                        keys::Q_OPS_SEND_AS_2,
                        &[("title", &title.to_string()), ("status", status)],
                    ))
                    .tab_index(0)
                    .cursor_pointer()
                    .pressable(cx.theme())
                    .when(locked, |row| row.opacity(0.6))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.choose_message_sender(chat_id, sender, locked, cx);
                    }))
                    .child(chat_avatar(&title, photo.as_deref(), 32.))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .truncate()
                                    .font_medium()
                                    .child(super::bidi_line::name_line(title)),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(status),
                            ),
                    )
                    .when(selected, |row| {
                        row.child(Icon::new(gpui_kit::assets::IconName::Check).small())
                    }),
            );
        }
        div()
            .id("send-as-panel")
            .flex()
            .flex_col()
            .gap_2()
            .px_3()
            .py_2()
            .rounded_md()
            .border_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().popover)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_sm()
                            .font_semibold()
                            .child(tr(keys::Q_OPS_SEND_MESSAGE_AS)),
                    )
                    .child(
                        Button::new("send-as-close")
                            .icon(gpui_kit::assets::IconName::X)
                            .xsmall()
                            .ghost()
                            .accessibility_label(tr(keys::CLOSE))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.composer_ui.send_as_open = false;
                                cx.notify();
                            })),
                    ),
            )
            .child(list)
    }
}
