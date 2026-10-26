//! The composer reply bar's options: Update Quote, Reply in Another Chat,
//! Show in Chat and Do Not Reply (tdesktop `EditDraftOptions`,
//! `history/view/controls/history_view_draft_options.cpp`), the chat
//! chooser behind "Reply in Another Chat" and the quote picker. See
//! `docs/decisions/codex-reply-elsewhere.md`.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::chat_row::chat_avatar;
use super::pressable::PressableDiv;
use super::*;
use crate::ui::status_note::StatusNote;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::input::Textarea;
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::agent::ui_driver::redact;
use quill::composer::{ComposerReplyTo, QuoteSelection};
use quill::ids::{ChatId, MessageId};
use quill::lang::{keys, tr, tr_with};
use quill::quote_source::QuotableText;
use quill::reply_options::{ReplyOption, reply_options};
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
        .a11y_tag(redact::chat_tag(id.0))
        .aria_label(tr_with(
            keys::FORUM_REPLY_IN,
            &[("topic", &title.to_string())],
        ))
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
    /// The quotable text of a loaded message, with its formatting.
    pub(in crate::ui) fn message_quotable(
        &self,
        chat_id: ChatId,
        message_id: MessageId,
    ) -> Option<QuotableText> {
        let message = self
            .session()?
            .histories
            .get(&chat_id.0)?
            .messages
            .get(&message_id.0)?;
        let content = effective_content(&message.content, message.ephemeral.as_ref());
        QuotableText::for_reply(content, Self::message_copyable_text(content))
    }

    /// The text of the message a reply points at, if it is loaded.
    pub(in crate::ui) fn reply_message_text(
        &self,
        reply: &ComposerReplyTo,
    ) -> Option<QuotableText> {
        self.message_quotable(reply.chat_id, reply.message_id)
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
            .tooltip(tr(keys::Q_OPS_REPLY_OPTIONS))
            .accessibility_label(tr(keys::Q_OPS_REPLY_OPTIONS))
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
        let Some(reply) = self.composer_ui.reply().cloned() else {
            return;
        };
        match option {
            ReplyOption::UpdateQuote => {
                self.share.reply_elsewhere_open = false;
                self.share.reply_quote_open = true;
                self.connection
                    .set_note(StatusNote::silent(tr(keys::Q_OPS_PICK_THE_PART_TO_QUOTE)));
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
        if self.composer_ui.reply().is_none() {
            return;
        }
        self.share.reply_quote_open = false;
        self.share.reply_elsewhere_open = true;
        self.share.forward_picker_open = false;
        self.share.search_input.update(cx, |input, cx| {
            input.set_value("", window, cx);
            input.focus(window, cx);
        });
        self.connection
            .set_note(StatusNote::silent(tr(keys::Q_OPS_REPLY_IN)));
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
        let Some(reply) = self.composer_ui.reply().cloned() else {
            return;
        };
        let secret = self
            .session()
            .and_then(|s| s.chats.get(&dest.0))
            .is_some_and(|chat| matches!(chat.kind, ChatKind::Secret { .. }));
        if secret {
            self.connection.set_note(StatusNote::error(tr(
                keys::Q_OPS_REPLIES_FROM_ANOTHER_CHAT_CANT,
            )));
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
        self.composer_ui.set_reply(Some(reply.into_chat(dest)));
        self.select_listed_chat(dest, window, cx);
        self.composer
            .update(cx, |input, cx| input.focus(window, cx));
        self.connection.set_note(StatusNote::silent(tr(
            keys::Q_OPS_REPLYING_FROM_ANOTHER_CHAT,
        )));
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
        let Some(reply) = self.composer_ui.take_reply() else {
            return;
        };
        let same = reply.quote.as_ref() == Some(&quote);
        self.composer_ui
            .set_reply(Some(reply.with_new_quote((!same).then_some(quote))));
        self.note_open_draft(true, cx);
        cx.notify();
    }

    /// The chat chooser above the composer: the message's author first,
    /// then the chat list (tdesktop `ShowReplyToChatBox`).
    pub(super) fn reply_elsewhere_panel(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.share.reply_elsewhere_open {
            return None;
        }
        self.reply_elsewhere_panel_body(cx)
    }

    /// `reply_elsewhere_panel` past its cheap check. A debug build reserves this
    /// builder's whole frame on entry, so frames that show nothing skip
    /// it (docs/decisions/main-thread-stack.md).
    /// The outer function repeats this body's first check; keep the two in step.
    #[inline(never)]
    fn reply_elsewhere_panel_body(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.share.reply_elsewhere_open {
            return None;
        }
        let reply = self.composer_ui.reply()?;
        let session = self.session()?;
        let query = self.share.search_input.read(cx).value().to_string();
        let searching = session.messages.share_search.is_searching();
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
                        tr(keys::SAVED_MESSAGES).to_string()
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
                        tr(keys::Q_OPS_SEARCHING)
                    } else {
                        tr(keys::Q_OPS_NO_MATCHING_CHATS)
                    }),
            );
        } else {
            for (id, title, photo) in dests {
                list = list.child(reply_dest_row(id, title, photo, cx));
            }
        }
        // The chooser lists chats, so it is a list to the UI driver: only
        // the tagged rows of granted chats and these controls survive the
        // granted scope (`redact::LIST`, `redact::CONTROLS`).
        Some(
            div()
                .id("reply-elsewhere")
                .role(gpui_kit::Role::Group)
                .a11y_tag(redact::LIST)
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
                        .id("reply-elsewhere-controls")
                        .role(gpui_kit::Role::Group)
                        .a11y_tag(redact::CONTROLS)
                        .flex()
                        .flex_col()
                        .gap_2()
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
                                        .child(tr(keys::REPLY_IN_ANOTHER_TITLE)),
                                )
                                .child(
                                    Button::new("reply-elsewhere-cancel")
                                        .label(tr(keys::CANCEL))
                                        .ghost()
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.close_reply_panels(window, cx);
                                        })),
                                ),
                        )
                        .child(
                            Textarea::new(&self.share.search_input)
                                .aria_label(tr(keys::Q_OPS_SEARCH_CHATS))
                                .h(px(36.)),
                        ),
                )
                .when_some(author, |this, (id, title, photo)| {
                    this.child(section_label(tr(keys::REPLY_IN_AUTHOR), cx))
                        .child(reply_dest_row(id, title, photo, cx))
                })
                .child(section_label(tr(keys::REPLY_IN_CHATS_LIST), cx))
                .child(list)
                .into_any_element(),
        )
    }

    /// The quote picker: the parts of the message, one click each.
    pub(super) fn reply_quote_panel(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.share.reply_quote_open {
            return None;
        }
        self.reply_quote_panel_body(cx)
    }

    /// `reply_quote_panel` past its cheap check. A debug build reserves this
    /// builder's whole frame on entry, so frames that show nothing skip
    /// it (docs/decisions/main-thread-stack.md).
    /// The outer function repeats this body's first check; keep the two in step.
    #[inline(never)]
    fn reply_quote_panel_body(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.share.reply_quote_open {
            return None;
        }
        let reply = self.composer_ui.reply()?;
        let text = self.reply_message_text(reply)?;
        let picked = reply.quote.clone();
        let mut list = div()
            .id("reply-quote-list")
            .flex()
            .flex_col()
            .gap_1()
            .max_h(px(220.))
            .overflow_y_scroll();
        for (index, part) in text.segments().into_iter().enumerate() {
            let selected = picked
                .as_ref()
                .is_some_and(|picked| picked.same_part(&part));
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
                    // Selection is a filled row (plus the check), not a
                    // leading bar that squares the rounded corners.
                    .when(selected, |row| row.bg(cx.theme().primary.opacity(0.25)))
                    .text_sm()
                    .role(gpui_kit::Role::Button)
                    .aria_label(format!(
                        "{}: {label}",
                        if selected {
                            tr(keys::Q_OPS_QUOTED)
                        } else {
                            tr(keys::Q_OPS_QUOTE)
                        }
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
                                tr(keys::REPLY_OPTIONS_QUOTE)
                            } else {
                                tr(keys::Q_OPS_QUOTE_PART_OF_MESSAGE)
                            },
                        ))
                        .child(
                            Button::new("reply-quote-done")
                                .label(tr(keys::BOX_DONE))
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
                        .child(tr(keys::REPLY_ABOUT_QUOTE)),
                )
                .into_any_element(),
        )
    }
}
