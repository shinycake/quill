//! The composer reply bar's options: Update Quote, Reply in Another Chat,
//! Show in Chat and Do Not Reply (tdesktop `EditDraftOptions`,
//! `history/view/controls/history_view_draft_options.cpp`), the chat
//! chooser behind "Reply in Another Chat" and the quote picker. See
//! `docs/decisions/codex-reply-elsewhere.md`.

use super::app::QuillApp;
use super::chat_row::chat_avatar;
use super::pressable::PressableDiv;
use super::*;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::input::Textarea;
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::composer::{ComposerReplyTo, QuoteSelection};
use quill::ids::{ChatId, MessageId};
use quill::reply_options::{ReplyOption, quote_segments, reply_options};
use quill::telegram::envelope::{ChatKind, MessageSender, effective_content};
use std::path::PathBuf;

/// One row of the chat chooser: avatar and title, one click picks it.
fn reply_dest_row(
    id: ChatId,
    title: String,
    photo: Option<PathBuf>,
    cx: &mut Context<QuillApp>,
) -> impl IntoElement {
    div()
        .id(("reply-dest", id.0 as u64))
        .flex()
        .items_center()
        .gap_2()
        .px_2()
        .py_1()
        .rounded_md()
        .role(gpui_kit::Role::Button)
        .aria_label(format!("Reply in {title}"))
        .tab_index(0)
        .cursor_pointer()
        .pressable(cx.theme())
        .on_click(cx.listener(move |this, _, window, cx| {
            this.choose_reply_chat(id, window, cx);
        }))
        .child(chat_avatar(&title, photo.as_deref(), 28.))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .font_medium()
                .child(super::bidi_line::one_line_plain(title)),
        )
}

fn section_label(text: &'static str, cx: &App) -> Div {
    div()
        .px_2()
        .text_xs()
        .font_semibold()
        .text_color(cx.theme().muted_foreground)
        .child(text)
}

impl QuillApp {
    /// The text of the message a reply points at, if it is loaded.
    fn reply_message_text(&self, reply: &ComposerReplyTo) -> Option<String> {
        let message = self
            .session()?
            .histories
            .get(&reply.chat_id.0)?
            .messages
            .get(&reply.message_id.0)?;
        Self::message_copyable_text(effective_content(
            &message.content,
            message.ephemeral.as_ref(),
        ))
        .filter(|text| !text.trim().is_empty())
    }

    /// Whether the reply's message may be answered in another chat:
    /// tdesktop offers it when the message allows forwarding (not in a
    /// protected chat) and never from or into secret chats. The message
    /// menu asks TDLib's `can_be_replied_in_another_chat` instead.
    pub(super) fn reply_elsewhere_allowed(&self, chat_id: ChatId, message_id: MessageId) -> bool {
        let Some(session) = self.session() else {
            return false;
        };
        message_id.0 > 0
            && !session.chat_has_protected_content(chat_id)
            && !session
                .chats
                .get(&chat_id.0)
                .is_some_and(|chat| matches!(chat.kind, ChatKind::Secret { .. }))
    }

    /// The "..." button of the reply bar, opening the options menu.
    pub(super) fn reply_options_button(
        &self,
        reply: &ComposerReplyTo,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let same_chat = reply.target_chat.is_none() && self.open_chat_id() == Some(reply.chat_id);
        let options = reply_options(
            self.reply_message_text(reply).is_some(),
            self.reply_elsewhere_allowed(reply.chat_id, reply.message_id),
            same_chat,
        );
        let has_quote = reply.quote.is_some();
        let owner = cx.entity().downgrade();
        Button::new("reply-options")
            .icon(IconName::EllipsisVertical)
            .ghost()
            .small()
            .tooltip("Reply options")
            .accessibility_label("Reply options")
            .dropdown_menu(move |mut menu, _, _| {
                for option in &options {
                    let option = *option;
                    let owner = owner.clone();
                    let icon = match option {
                        ReplyOption::UpdateQuote => IconName::TextQuote,
                        ReplyOption::ReplyInAnotherChat => IconName::Replace,
                        ReplyOption::ShowInChat => IconName::Locate,
                        ReplyOption::DoNotReply => IconName::Trash,
                    };
                    menu = menu.item(
                        PopupMenuItem::new(option.label(has_quote))
                            .icon(icon)
                            .on_click(move |_, window, cx| {
                                let _ = owner.update(cx, |this, cx| {
                                    this.apply_reply_option(option, window, cx);
                                });
                            }),
                    );
                }
                menu
            })
    }

    pub(super) fn apply_reply_option(
        &mut self,
        option: ReplyOption,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(reply) = self.composer_ui.pending_reply.clone() else {
            return;
        };
        match option {
            ReplyOption::UpdateQuote => {
                self.share.reply_elsewhere_open = false;
                self.share.reply_quote_open = true;
                self.connection.status_note = "pick the part to quote".into();
            }
            ReplyOption::ReplyInAnotherChat => {
                self.open_reply_elsewhere(window, cx);
            }
            ReplyOption::ShowInChat => {
                self.jump_to_replied_message(reply.message_id, cx);
            }
            ReplyOption::DoNotReply => {
                self.clear_reply(cx);
            }
        }
        cx.notify();
    }

    /// "Reply in Another Chat" from the message menu: reply to the message,
    /// then ask where.
    pub(super) fn begin_reply_elsewhere(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.begin_reply_from_message(chat_id, message_id, window, cx);
        self.open_reply_elsewhere(window, cx);
    }

    /// Show the chat chooser for the composer's reply.
    pub(super) fn open_reply_elsewhere(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.composer_ui.pending_reply.is_none() {
            return;
        }
        self.share.reply_quote_open = false;
        self.share.reply_elsewhere_open = true;
        self.share.forward_picker_open = false;
        self.share.search_input.update(cx, |input, cx| {
            input.set_value("", window, cx);
            input.focus(window, cx);
        });
        self.connection.status_note = "reply in…".into();
        cx.notify();
    }

    pub(super) fn close_reply_panels(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let was_open = self.share.reply_elsewhere_open;
        self.share.reply_elsewhere_open = false;
        self.share.reply_quote_open = false;
        if was_open {
            self.share
                .search_input
                .update(cx, |input, cx| input.set_value("", window, cx));
        }
        cx.notify();
    }

    /// A chat was picked: open it with the reply attached to its composer.
    pub(super) fn choose_reply_chat(
        &mut self,
        dest: ChatId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(reply) = self.composer_ui.pending_reply.clone() else {
            return;
        };
        let secret = self
            .session()
            .and_then(|s| s.chats.get(&dest.0))
            .is_some_and(|chat| matches!(chat.kind, ChatKind::Secret { .. }));
        if secret {
            self.connection.status_note =
                "Replies from another chat can't go to secret chats.".into();
            cx.notify();
            return;
        }
        if self.slow_mode_blocked(dest, cx) {
            cx.notify();
            return;
        }
        self.share.reply_elsewhere_open = false;
        self.share
            .search_input
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.composer_ui.pending_reply = Some(reply.into_chat(dest));
        self.select_listed_chat(dest, window, cx);
        self.composer
            .update(cx, |input, cx| input.focus(window, cx));
        self.connection.status_note = "replying from another chat".into();
        cx.notify();
    }

    /// Enter in the chooser's search box picks the first match.
    pub(super) fn choose_first_reply_chat(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let query = self.share.search_input.read(cx).value().to_string();
        let first = self
            .session()
            .and_then(|session| session.share_destinations(&query).into_iter().next())
            .map(|chat| chat.id);
        if let Some(first) = first {
            self.choose_reply_chat(first, window, cx);
        }
    }

    /// The part of the message was picked (or unpicked, back to the whole
    /// message).
    fn pick_reply_quote(&mut self, quote: QuoteSelection, cx: &mut Context<Self>) {
        let Some(reply) = self.composer_ui.pending_reply.take() else {
            return;
        };
        let same = reply.quote.as_ref() == Some(&quote);
        self.composer_ui.pending_reply = Some(reply.with_new_quote((!same).then_some(quote)));
        self.note_open_draft(true, cx);
        cx.notify();
    }

    /// The chat chooser above the composer: the message's author first,
    /// then the chat list (tdesktop `ShowReplyToChatBox`).
    pub(super) fn reply_elsewhere_panel(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.share.reply_elsewhere_open {
            return None;
        }
        let reply = self.composer_ui.pending_reply.as_ref()?;
        let session = self.session()?;
        let query = self.share.search_input.read(cx).value().to_string();
        let searching = session.share_search.is_searching();
        let secret = |id: ChatId| {
            session
                .chats
                .get(&id.0)
                .is_some_and(|chat| matches!(chat.kind, ChatKind::Secret { .. }))
        };
        // The author of a message from a group or channel has a chat of
        // their own to answer in.
        let author: Option<(ChatId, String, Option<PathBuf>)> = (query.trim().is_empty())
            .then(|| {
                let message = session
                    .histories
                    .get(&reply.chat_id.0)?
                    .messages
                    .get(&reply.message_id.0)?;
                let MessageSender::User { user_id } = message.sender? else {
                    return None;
                };
                let chat = session.chats.get(&user_id)?;
                let listed = session
                    .share_destinations("")
                    .iter()
                    .any(|candidate| candidate.id == chat.id);
                (chat.id != reply.chat_id && !secret(chat.id) && listed).then(|| {
                    (
                        chat.id,
                        chat.title.clone(),
                        session.chat_photo_path(chat.id).map(PathBuf::from),
                    )
                })
            })
            .flatten();
        let dests: Vec<(ChatId, String, Option<PathBuf>)> = session
            .share_destinations(&query)
            .into_iter()
            .filter(|chat| !secret(chat.id))
            .map(|chat| {
                (
                    chat.id,
                    if session.is_saved_messages(chat.id) {
                        "Saved Messages".to_string()
                    } else {
                        chat.title.clone()
                    },
                    session.chat_photo_path(chat.id).map(PathBuf::from),
                )
            })
            .collect();
        let mut list = div()
            .id("reply-dest-list")
            .flex()
            .flex_col()
            .gap_1()
            .max_h(px(220.))
            .overflow_y_scroll();
        if dests.is_empty() {
            list = list.child(
                div()
                    .px_2()
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
                list = list.child(reply_dest_row(id, title, photo, cx));
            }
        }
        Some(
            div()
                .id("reply-elsewhere")
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
                                .child("Reply in…"),
                        )
                        .child(
                            Button::new("reply-elsewhere-cancel")
                                .label("Cancel")
                                .ghost()
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.close_reply_panels(window, cx);
                                })),
                        ),
                )
                .child(
                    Textarea::new(&self.share.search_input)
                        .aria_label("Search chats")
                        .h(px(36.)),
                )
                .when_some(author, |this, (id, title, photo)| {
                    this.child(section_label("Message author", cx))
                        .child(reply_dest_row(id, title, photo, cx))
                })
                .child(section_label("Your chats", cx))
                .child(list)
                .into_any_element(),
        )
    }

    /// The quote picker: the parts of the message, one click each.
    pub(super) fn reply_quote_panel(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.share.reply_quote_open {
            return None;
        }
        let reply = self.composer_ui.pending_reply.as_ref()?;
        let text = self.reply_message_text(reply)?;
        let picked = reply.quote.clone();
        let mut list = div()
            .id("reply-quote-list")
            .flex()
            .flex_col()
            .gap_1()
            .max_h(px(220.))
            .overflow_y_scroll();
        for (index, part) in quote_segments(&text).into_iter().enumerate() {
            let selected = picked.as_ref() == Some(&part);
            let label = part.text.clone();
            list = list.child(
                div()
                    .id(("reply-quote-part", index as u64))
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .border_l_2()
                    .border_color(if selected { accent() } else { bg_canvas() })
                    .text_sm()
                    .role(gpui_kit::Role::Button)
                    .aria_label(format!(
                        "{}: {label}",
                        if selected { "Quoted" } else { "Quote" }
                    ))
                    .tab_index(0)
                    .cursor_pointer()
                    .pressable(cx.theme())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.pick_reply_quote(part.clone(), cx);
                    }))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(super::bidi_line::one_line_plain(
                                super::search_ui::one_line_preview(&label),
                            )),
                    )
                    .when(selected, |row| {
                        row.child(Icon::new(IconName::Check).small().text_color(accent()))
                    }),
            );
        }
        Some(
            div()
                .id("reply-quote")
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
                        .child(div().text_sm().font_semibold().text_color(accent()).child(
                            if picked.is_some() {
                                "Update Quote"
                            } else {
                                "Quote Part of Message"
                            },
                        ))
                        .child(
                            Button::new("reply-quote-done")
                                .label("Done")
                                .primary()
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.close_reply_panels(window, cx);
                                })),
                        ),
                )
                .child(list)
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child("You can select a specific part to quote."),
                )
                .into_any_element(),
        )
    }
}
