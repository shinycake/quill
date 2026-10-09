//! Share box (multi-destination forward), forward bar and "send as" picker.
//!
//! Mirrors tdesktop's `ShareBox` (boxes/share_box.cpp), `ForwardPanel`
//! (history/view/controls/history_view_forward_panel.cpp) and
//! `ChooseSendAsBox` (ui/chat/choose_send_as.cpp); see
//! `docs/decisions/codex-share-box.md`.

use super::app::QuillApp;
use super::chat_row::chat_avatar;
use super::pressable::PressableDiv;
use super::scheduled::ScheduleTarget;
use super::*;
use gpui_kit::component::button::*;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::input::Textarea;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::composer::ComposerSnapshot;
use quill::ids::ChatId;
use quill::schedule::ScheduleKind;
use quill::share_box::{ShareSend, forward_bar_lines, share_done_label};
use quill::telegram::envelope::{ChatKind, MessageSender};
use std::path::PathBuf;

/// Tick circle of a share box row.
fn share_check(id: ChatId, checked: bool) -> impl IntoElement {
    div()
        .id(("share-check", id.0 as u64))
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .rounded_full()
        .border_1()
        .border_color(accent_strong())
        .bg(if checked {
            accent_strong()
        } else {
            bg_black().opacity(0.0)
        })
        .text_color(text_bright())
        .size(px(20.))
        .when(checked, |this| {
            this.child(Icon::new(gpui_kit::assets::IconName::Check).small())
        })
}

fn share_dest_row(
    id: ChatId,
    title: String,
    photo: Option<PathBuf>,
    checked: bool,
    cx: &mut Context<QuillApp>,
) -> impl IntoElement {
    div()
        .id(("share-dest", id.0 as u64))
        .flex()
        .items_center()
        .gap_2()
        .px_2()
        .py_1()
        .rounded_md()
        .role(gpui_kit::Role::Button)
        .aria_label(format!(
            "{title}, {}",
            if checked { "selected" } else { "not selected" }
        ))
        .tab_index(0)
        .cursor_pointer()
        .pressable(cx.theme())
        .on_click(cx.listener(move |this, _, _, cx| {
            this.toggle_share_destination(id, cx);
        }))
        .child(share_check(id, checked))
        .child(chat_avatar(&title, photo.as_deref(), 28.))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .font_medium()
                .child(title),
        )
}

impl QuillApp {
    pub(super) fn toggle_share_destination(&mut self, id: ChatId, cx: &mut Context<Self>) {
        self.share_selection.toggle(id);
        cx.notify();
    }

    /// The typed query changed: ask the server too (`searchChatsOnServer`).
    pub(super) fn sync_share_search(&mut self, text: &str, cx: &mut Context<Self>) {
        if !self.forward_picker_open {
            return;
        }
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.search_share_chats(text);
        }
        cx.notify();
    }

    /// Enter in the share box search: tick the first match, or send when
    /// something is already ticked (tdesktop `ShareBox::keyPressEvent`).
    pub(super) fn activate_first_forward_destination(&mut self, cx: &mut Context<Self>) {
        if !self.share_selection.is_empty() {
            self.submit_share(ShareSend::Normal, cx);
            return;
        }
        let query = self.forward_search_input.read(cx).value().to_string();
        let first = self
            .session()
            .and_then(|session| session.share_destinations(&query).into_iter().next())
            .map(|chat| chat.id);
        if let Some(first) = first {
            self.toggle_share_destination(first, cx);
        }
    }

    /// Why the draft cannot go to `dest` right now, if it cannot.
    fn share_dest_refusal(&mut self, dest: ChatId, cx: &mut Context<Self>) -> Option<String> {
        // Phase S1: messages cannot be forwarded to secret chats (TGX
        // `SecretChatForwardError`, verbatim).
        let dest_is_secret = self
            .session()
            .and_then(|s| s.chats.get(&dest.0))
            .is_some_and(|chat| matches!(chat.kind, ChatKind::Secret { .. }));
        if dest_is_secret {
            return Some("This message cannot be forwarded to secret chats.".into());
        }
        // Phase A1: slow-mode gate applies to forwards — forwarding sends
        // messages to the destination chat.
        if self.slow_mode_blocked(dest, cx) {
            return Some(self.status_note.clone());
        }
        None
    }

    /// Send the draft to every ticked chat: the comment first (a plain
    /// message), then the forwards with the chosen sound / schedule.
    pub(super) fn submit_share(&mut self, mode: ShareSend, cx: &mut Context<Self>) {
        let Some(draft) = self.pending_forward.clone() else {
            return;
        };
        let dests: Vec<ChatId> = self.share_selection.chats().to_vec();
        if dests.is_empty() {
            self.status_note = "choose a chat to forward to".into();
            cx.notify();
            return;
        }
        for dest in &dests {
            if let Some(note) = self.share_dest_refusal(*dest, cx) {
                self.status_note = note;
                cx.notify();
                return;
            }
        }
        let options = mode.options();
        let comment = self.share_comment_input.read(cx).value().trim().to_string();
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
            self.status_note = if failed == 0 {
                share_done_label(draft.count(), &titles)
            } else {
                format!("could not forward to {failed} chat(s)")
            };
        } else if self.demo_session.is_some() {
            for dest in &dests {
                self.apply_demo_forward(*dest, &draft);
            }
            if let Some(result) = self
                .demo_session
                .as_mut()
                .and_then(|session| session.last_forward.take())
            {
                self.present_forward_result(result, cx);
            }
        }
        if failed < dests.len() {
            self.pending_forward = None;
            self.forward_bar_dest = None;
            self.forward_picker_open = false;
            self.share_selection.clear();
        }
        cx.notify();
    }

    /// Copy the t.me link of the single selected message
    /// (`getMessageProperties` then `getMessageLink`).
    pub(super) fn share_copy_link(&mut self, cx: &mut Context<Self>) {
        let Some((chat_id, message_id)) = self
            .pending_forward
            .as_ref()
            .filter(|draft| draft.count() == 1)
            .map(|draft| (draft.from_chat_id, draft.message_ids[0]))
        else {
            return;
        };
        self.share_message_link(chat_id, message_id, cx);
    }

    /// One destination chosen: open it with the forward bar above its
    /// composer (the typed comment moves into the composer).
    pub(super) fn share_continue_in_chat(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(dest) = self.share_selection.single() else {
            return;
        };
        if let Some(note) = self.share_dest_refusal(dest, cx) {
            self.status_note = note;
            cx.notify();
            return;
        }
        let comment = self.share_comment_input.read(cx).value().trim().to_string();
        self.forward_bar_dest = Some(dest);
        self.forward_picker_open = false;
        self.share_selection.clear();
        self.forward_search_input
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.select_listed_chat(dest, window, cx);
        if !comment.is_empty() {
            self.composer
                .update(cx, |input, cx| input.set_value(comment, window, cx));
        }
        self.status_note = "forward bar ready — press Send".into();
        cx.notify();
    }

    /// The forward bar is showing in the open chat.
    pub(super) fn forward_bar_here(&self) -> bool {
        let Some(dest) = self.forward_bar_dest else {
            return false;
        };
        !self.forward_picker_open
            && self.pending_edit.is_none()
            && self
                .pending_forward
                .as_ref()
                .is_some_and(|draft| !draft.is_empty())
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
        let (Some(dest), Some(draft)) = (self.forward_bar_dest, self.pending_forward.clone())
        else {
            return;
        };
        if let Some(note) = self.share_dest_refusal(dest, cx) {
            self.status_note = note;
            cx.notify();
            return;
        }
        let options = self.composer_send_options();
        if !text.trim().is_empty() || !self.pending_attachments.is_empty() {
            self.forward_bar_dest = None;
            self.submit_composer(text, window, cx);
            self.forward_bar_dest = Some(dest);
            // The comment did not go out (the composer keeps it): stop.
            if !self.composer.read(cx).value().trim().is_empty()
                || !self.pending_attachments.is_empty()
            {
                return;
            }
        }
        if self.live.is_some() {
            let result = self
                .live
                .as_mut()
                .expect("live")
                .driver
                .forward_messages_with_options(dest, &draft, &options);
            match result {
                Ok(_) => {
                    self.status_note = "forwarding…".into();
                    self.pending_forward = None;
                    self.forward_bar_dest = None;
                    self.composer_scheduling = quill::composer::ComposerScheduling::None;
                }
                Err(_) => self.status_note = "could not forward messages".into(),
            }
        } else if self.demo_session.is_some() {
            self.apply_demo_forward(dest, &draft);
            if let Some(result) = self
                .demo_session
                .as_mut()
                .and_then(|session| session.last_forward.take())
            {
                self.present_forward_result(result, cx);
            }
        }
        cx.notify();
    }

    /// "Hide sender name" / "Hide captions" (`forwardMessages.send_copy` /
    /// `remove_caption`; captions only drop on copies).
    fn forward_option_checkboxes(&self, prefix: &'static str, cx: &mut Context<Self>) -> Div {
        let send_copy = self
            .pending_forward
            .as_ref()
            .is_some_and(|draft| draft.send_copy);
        let remove_caption = self
            .pending_forward
            .as_ref()
            .is_some_and(|draft| draft.remove_caption);
        let multiple = self
            .pending_forward
            .as_ref()
            .is_some_and(|draft| draft.count() > 1);
        div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_3()
            .child(
                Checkbox::new((prefix, 0usize))
                    .label(if multiple {
                        "Hide sender names"
                    } else {
                        "Hide sender name"
                    })
                    .checked(send_copy)
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
                Checkbox::new((prefix, 1usize))
                    .label(if multiple {
                        "Hide captions"
                    } else {
                        "Hide caption"
                    })
                    .checked(remove_caption)
                    .disabled(!send_copy)
                    .on_click(cx.listener(|this, &on, _, cx| {
                        if let Some(draft) = this.pending_forward.as_mut()
                            && draft.send_copy
                        {
                            draft.remove_caption = on;
                        }
                        cx.notify();
                    })),
            )
    }

    /// The share box: search, ticked destinations, comment and the send
    /// choices (Send, Send without sound, Schedule, Copy link).
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
        let session = self.session();
        let dests: Vec<(ChatId, String, Option<PathBuf>)> = session
            .map(|session| {
                session
                    .share_destinations(&query)
                    .into_iter()
                    .map(|chat| {
                        let photo = session.chat_photo_path(chat.id).map(PathBuf::from);
                        (chat.id, chat.title.clone(), photo)
                    })
                    .collect()
            })
            .unwrap_or_default();
        let searching = session.is_some_and(|s| s.share_search.is_searching());
        let selected = self.share_selection.len();
        let single_saved = self
            .share_selection
            .single()
            .and_then(|id| session.map(|s| s.is_saved_messages(id)))
            .unwrap_or(false);
        let schedule_label = match ScheduleKind::for_saved_messages(single_saved) {
            ScheduleKind::Reminder => "Remind me",
            ScheduleKind::Schedule => "Schedule",
        };
        let heading = if count == 1 {
            format!("Forward 1 message from {from_title}")
        } else {
            format!("Forward {count} messages from {from_title}")
        };
        let summary = match selected {
            0 => "Choose one or more chats".to_string(),
            1 => "1 chat selected".to_string(),
            n => format!("{n} chats selected"),
        };
        let mut list = div()
            .id("forward-dest-list")
            .flex()
            .flex_col()
            .gap_1()
            .max_h(px(220.))
            .overflow_y_scroll();
        if dests.is_empty() {
            list = list.child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(if searching {
                        "Searching…"
                    } else {
                        "No matching chats."
                    }),
            );
        } else {
            for (id, title, photo) in dests {
                let checked = self.share_selection.contains(id);
                list = list.child(share_dest_row(id, title, photo, checked, cx));
            }
        }
        let can_link = count == 1 && self.live.is_some();
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
            .child(list)
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(summary),
            )
            .child(
                Textarea::new(&self.share_comment_input)
                    .aria_label("Comment")
                    .h(px(36.)),
            )
            .child(self.forward_option_checkboxes("forward-option", cx))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_2()
                    .when(can_link, |row| {
                        row.child(
                            Button::new("share-copy-link")
                                .label("Copy link")
                                .ghost()
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.share_copy_link(cx);
                                })),
                        )
                    })
                    .child(div().flex_1())
                    .child(
                        Button::new("share-schedule")
                            .label(schedule_label)
                            .ghost()
                            .disabled(selected == 0)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.open_schedule_picker(ScheduleTarget::Share, window, cx);
                            })),
                    )
                    .child(
                        Button::new("share-silent")
                            .label("Send without sound")
                            .ghost()
                            .disabled(selected == 0)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.submit_share(ShareSend::Silent, cx);
                            })),
                    )
                    .when(selected == 1, |row| {
                        row.child(Button::new("share-continue").label("Open chat").on_click(
                            cx.listener(|this, _, window, cx| {
                                this.share_continue_in_chat(window, cx);
                            }),
                        ))
                    })
                    .child(
                        Button::new("share-send")
                            .label("Send")
                            .primary()
                            .disabled(selected == 0)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.submit_share(ShareSend::Normal, cx);
                            })),
                    ),
            )
    }

    /// The bar above the composer after one destination was chosen:
    /// who the messages are from, a preview, and the forward options.
    pub(super) fn forward_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let draft = self.pending_forward.clone();
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
                                    .child("Forward"),
                            )
                            .child(
                                div()
                                    .truncate()
                                    .text_sm()
                                    .font_medium()
                                    .text_color(text_primary())
                                    .child(from),
                            )
                            .child(
                                div()
                                    .truncate()
                                    .text_sm()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(text),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .gap_1()
                            .child(
                                Button::new("forward-bar-change")
                                    .label("Change recipient")
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
                                    .accessibility_label("Cancel forward")
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
        let dest = self.forward_bar_dest;
        self.open_forward_picker(window, cx);
        if let Some(dest) = dest {
            self.share_selection.toggle(dest);
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
                .aria_label(format!("Send as {name}"))
                .tab_index(0)
                .cursor_pointer()
                .child(chat_avatar(&name, photo.as_deref(), 28.))
                .on_click(cx.listener(|this, _, _, cx| this.toggle_send_as(cx)))
                .into_any_element(),
        )
    }

    pub(super) fn toggle_send_as(&mut self, cx: &mut Context<Self>) {
        self.send_as_open = !self.send_as_open;
        if self.send_as_open {
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
            self.status_note =
                "Subscribe to Telegram Premium to be able to comment on behalf of your channels in group chats."
                    .into();
            cx.notify();
            return;
        }
        if let Some(live) = self.live.as_mut() {
            self.status_note = match live.driver.set_chat_message_sender(chat_id, sender) {
                Ok(()) => "changing sender…".into(),
                Err(_) => "could not change the sender".into(),
            };
        } else {
            self.apply_demo_message_sender(chat_id, sender);
        }
        self.send_as_open = false;
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
                    .child("Loading…"),
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
                    .aria_label(format!("Send as {title}, {status}"))
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
                            .child(div().truncate().font_medium().child(title))
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
                    .child(div().text_sm().font_semibold().child("Send message as…"))
                    .child(
                        Button::new("send-as-close")
                            .icon(gpui_kit::assets::IconName::X)
                            .xsmall()
                            .ghost()
                            .accessibility_label("Close")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.send_as_open = false;
                                cx.notify();
                            })),
                    ),
            )
            .child(list)
    }
}
