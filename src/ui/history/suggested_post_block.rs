//! A suggested post in the history (Telegram Desktop
//! `history_view_suggest_decision.cpp`, `HistoryMessageSuggestion`): the
//! service block above the bubble with the offer's terms, and the
//! Accept / Decline / Suggest Changes buttons in the bubble.

use crate::ui::app::QuillApp;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::local_time::{civil_local, now_unix};
use quill::state::{HistoryMessage, Session};
use quill::suggested_posts::{
    BLOCK_CHECK_BELOW, Changes, SuggestionButtons, block_labels, block_time_label, block_title,
    price_label,
};

/// tdesktop `st::chatSuggestInfoWidth`.
const BLOCK_MAX_WIDTH: f32 = 236.;

/// What a counter-offer changed against the offer it replies to; `None`
/// for an original offer (or when that offer isn't loaded).
fn resolve_changes(session: &Session, message: &HistoryMessage) -> Option<Changes> {
    let now = message.extras.suggested_post?.terms();
    let reply = message.reply_to.as_ref()?;
    let original = session
        .histories
        .get(&reply.chat_id.0)?
        .messages
        .get(&reply.message_id.0)?;
    let was = original.extras.suggested_post?.terms();
    Changes::resolve(Some(was), now, original.content == message.content)
}

/// The centered block above a suggested post.
pub(in crate::ui) fn suggested_post_block(
    session: &Session,
    message: &HistoryMessage,
    cx: &App,
) -> Option<AnyElement> {
    let info = message.extras.suggested_post?;
    let changes = resolve_changes(session, message);
    let from = crate::ui::suggested_posts_ui::sender_short_name(session, message.sender);
    let title = block_title(message.is_outgoing, &from, changes);
    let (price_title, time_title) = block_labels(changes);
    let time = if info.send_date > 0 {
        block_time_label(
            &civil_local(i64::from(info.send_date)),
            &civil_local(now_unix()),
        )
    } else {
        "Anytime".to_string()
    };
    let fg = cx.theme().secondary_foreground;
    let faded = fg.opacity(0.7);
    let row = |label: &'static str, value: String| {
        div()
            .flex()
            .justify_between()
            .gap_3()
            .child(div().text_color(faded).child(label))
            .child(div().font_semibold().child(value))
    };
    Some(
        div()
            .id(("suggested-post-block", message.id.0 as u64))
            .flex()
            .justify_center()
            .py_1()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .max_w(px(BLOCK_MAX_WIDTH))
                    .px_4()
                    .pt_3()
                    .pb_2()
                    .rounded_xl()
                    .bg(crate::ui::chat_palette::service_bg(
                        cx.theme().secondary.opacity(0.85),
                    ))
                    .text_xs()
                    .text_color(fg)
                    .child(
                        div()
                            .pb_1()
                            .font_semibold()
                            .text_center()
                            .whitespace_normal()
                            .child(title),
                    )
                    .child(row(price_title, price_label(info.price)))
                    .child(row(time_title, time))
                    .when(changes.is_some_and(|c| c.message), |this| {
                        this.child(
                            div()
                                .pt_1()
                                .text_center()
                                .text_color(faded)
                                .child(BLOCK_CHECK_BELOW),
                        )
                    }),
            )
            .into_any_element(),
    )
}

/// The decision buttons, laid out like an inline keyboard: [Decline |
/// Accept] then [Suggest Changes]; [Decline] alone when that's all the
/// viewer may do.
pub(in crate::ui) fn suggestion_buttons(
    message: &HistoryMessage,
    cx: &mut Context<QuillApp>,
) -> Option<AnyElement> {
    let info = message.extras.suggested_post?;
    let buttons = SuggestionButtons::for_info(&info);
    if !buttons.any() {
        return None;
    }
    let (chat_id, message_id) = (message.chat_id, message.id);
    let id = message.id.0 as u64;
    let can_edit = quill::suggested_posts::edit_source(&message.content).is_some();
    let mut first = div().flex().gap_1();
    if buttons.decline {
        first = first.child(
            Button::new(("suggest-decline", id))
                .label("Decline")
                .ghost()
                .flex_1()
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.open_suggest_decline(chat_id, message_id, window, cx);
                })),
        );
    }
    if buttons.accept {
        first = first.child(
            Button::new(("suggest-accept", id))
                .label("Accept")
                .ghost()
                .flex_1()
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.begin_suggest_accept(chat_id, message_id, window, cx);
                })),
        );
    }
    let owner = cx.entity().downgrade();
    let change = buttons.change.then(|| {
        Button::new(("suggest-change", id))
            .label("Suggest Changes")
            .ghost()
            .w_full()
            .dropdown_menu(move |mut menu, _, _| {
                if can_edit {
                    let owner = owner.clone();
                    menu = menu.item(
                        PopupMenuItem::new("Edit Message")
                            .icon(IconName::Pencil)
                            .on_click(move |_, window, cx| {
                                let _ = owner.update(cx, |this, cx| {
                                    this.begin_suggest_edit(chat_id, message_id, window, cx);
                                });
                            }),
                    );
                }
                let price_owner = owner.clone();
                let time_owner = owner.clone();
                menu.item(
                    PopupMenuItem::new("Edit Price")
                        .icon(IconName::Coins)
                        .on_click(move |_, window, cx| {
                            let _ = price_owner.update(cx, |this, cx| {
                                this.suggest_edit_price(chat_id, message_id, window, cx);
                            });
                        }),
                )
                .item(
                    PopupMenuItem::new("Edit Time")
                        .icon(IconName::Clock)
                        .on_click(move |_, window, cx| {
                            let _ = time_owner.update(cx, |this, cx| {
                                this.suggest_edit_time(chat_id, message_id, window, cx);
                            });
                        }),
                )
            })
    });
    Some(
        div()
            .id(("suggestion-buttons", id))
            .flex()
            .flex_col()
            .gap_1()
            .mt_2()
            .child(first)
            .children(change)
            .into_any_element(),
    )
}
