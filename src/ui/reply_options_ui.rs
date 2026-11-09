//! The composer reply bar's options: Update Quote, Reply in Another Chat,
//! Show in Chat and Do Not Reply (tdesktop `EditDraftOptions`,
//! `history/view/controls/history_view_draft_options.cpp`), the chat
//! chooser behind "Reply in Another Chat" and the quote picker. See
//! `docs/decisions/codex-reply-elsewhere.md`.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::pressable::PressableDiv;
use super::*;
use crate::ui::status_note::StatusNote;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::composer::{ComposerReplyTo, QuoteSelection};
use quill::ids::{ChatId, MessageId};
use quill::lang::{keys, tr};
use quill::quote_source::QuotableText;
use quill::reply_options::{ReplyOption, reply_options};
use quill::telegram::envelope::{ChatKind, effective_content};

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
        let Some(reply) = self.chat_ui.reply().cloned() else {
            return;
        };
        match option {
            ReplyOption::UpdateQuote => {
                self.share.forward.close_reply_chooser();
                self.chat_ui.open_quote_picker();
                self.set_note(StatusNote::silent(tr(keys::Q_OPS_PICK_THE_PART_TO_QUOTE)));
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
        if self.chat_ui.reply().is_none() {
            return;
        }
        self.chat_ui.close_quote_picker();
        // One stage with the forward box: that box closes as this opens.
        self.share.forward.open_reply_chooser();
        self.share.focus_search = true;
        self.share
            .search_input
            .update(cx, |input, cx| input.set_value("", window, cx));
        cx.notify();
    }

    pub(super) fn close_reply_panels(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let was_open = self.share.forward.close_reply_chooser();
        self.chat_ui.close_quote_picker();
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
        let Some(reply) = self.chat_ui.reply().cloned() else {
            return;
        };
        let secret = self
            .session()
            .and_then(|s| s.chats.get(&dest.0))
            .is_some_and(|chat| matches!(chat.kind, ChatKind::Secret { .. }));
        if secret {
            self.set_note(StatusNote::error(tr(
                keys::Q_OPS_REPLIES_FROM_ANOTHER_CHAT_CANT,
            )));
            cx.notify();
            return;
        }
        if self.slow_mode_blocked(dest, cx) {
            cx.notify();
            return;
        }
        self.share.forward.close_reply_chooser();
        self.share
            .search_input
            .update(cx, |input, cx| input.set_value("", window, cx));
        // The reply leaves this composer and rides with the open to the
        // window that shows `dest`.
        self.chat_ui.set_reply(None);
        let open = crate::ui::chat_window::ChatOpen::new(dest).with_reply(reply.into_chat(dest));
        if self.select_chat_request(open, window, cx) {
            self.composer
                .update(cx, |input, cx| input.focus(window, cx));
        }
        self.set_note(StatusNote::silent(tr(
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
        let Some(reply) = self.chat_ui.take_reply() else {
            return;
        };
        let same = reply.quote.as_ref() == Some(&quote);
        self.chat_ui
            .set_reply(Some(reply.with_new_quote((!same).then_some(quote))));
        self.note_open_draft(true, cx);
        cx.notify();
    }

    /// The quote picker is open: Update Quote opened it for the message
    /// the composer replies to now.
    pub(in crate::ui) fn reply_quote_open(&self) -> bool {
        self.chat_ui.quote_picker_open()
    }

    /// The quote picker: the parts of the message, one click each.
    pub(super) fn reply_quote_panel(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.reply_quote_open() {
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
        if !self.reply_quote_open() {
            return None;
        }
        let reply = self.chat_ui.reply()?;
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
