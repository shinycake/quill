//! The Star chip under a message (tdesktop `InlineList` paid button): a
//! click sends one Star, a right-click opens the amount box.

use super::app::QuillApp;
use super::chat_theme::{bg_subtle, text_primary, warning};
use super::pressable::PressableDiv;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::premium_hub::group_digits;
use quill::state::HistoryMessage;
use quill::telegram::envelope::MessageReaction;

/// The Star reaction as the row shows it. TDLib already folds Stars
/// waiting for their Undo window into the count it sends (and marks the
/// reaction chosen), so nothing is added locally. A message with other
/// reactions in a chat that allows Star reactions gets an empty chip to
/// start from; tdesktop does the same.
pub(super) fn row_paid_reaction(
    message: &HistoryMessage,
    session: Option<&quill::state::Session>,
    has_other_chips: bool,
) -> Option<MessageReaction> {
    if let Some(paid) = message.paid_reaction() {
        return Some(paid.clone());
    }
    let enabled = session.is_some_and(|s| s.paid_reactions_enabled(message.chat_id));
    (has_other_chips && enabled).then(|| MessageReaction {
        reaction_type: quill::telegram::envelope::ReactionType::Paid,
        total_count: 0,
        is_chosen: false,
        recent_senders: Vec::new(),
    })
}

/// The chip for `paid`; mine (the account has sent Stars) is filled.
pub(super) fn paid_reaction_chip(
    message: &HistoryMessage,
    paid: &MessageReaction,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let (chat_id, message_id) = (message.chat_id, message.id);
    let mine = paid.is_chosen;
    let count = group_digits(i64::from(paid.total_count));
    div()
        .id(("paid-reaction-chip", message_id.0 as u64))
        .flex()
        .items_center()
        .gap_1()
        .px_2()
        .py_1()
        .rounded_full()
        .text_xs()
        .role(gpui_kit::Role::Button)
        .aria_label(format!(
            "Star reaction, {count} Stars. Click to send a Star."
        ))
        .tab_index(0)
        .cursor_pointer()
        .pressable(cx.theme())
        .border_1()
        .when(mine, |this| {
            this.bg(warning().opacity(0.25)).border_color(warning())
        })
        .when(!mine, |this| {
            this.bg(bg_subtle()).border_color(warning().opacity(0.5))
        })
        .text_color(text_primary())
        .tooltip(|window, cx| {
            gpui_kit::component::tooltip::Tooltip::new("Send a Star. Right-click for more.")
                .build(window, cx)
        })
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                this.open_paid_box(chat_id, message_id, cx);
            }),
        )
        .on_click(cx.listener(move |this, _, _, cx| {
            this.tap_paid_reaction(chat_id, message_id, cx);
        }))
        .child("\u{2B50}")
        .child(count)
        .into_any_element()
}
