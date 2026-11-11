//! The "New Thread" card under the "All" history of a bot that lets users
//! create threads (tdesktop `GenerateNewBotThread`): a dotted line across
//! the history, then a service bubble with the new-thread icon, the title,
//! "Type any message to create a new thread." and an arrow down to the
//! compose field. Sending from there starts the thread
//! (`composer::bot_thread`).
#![deny(clippy::indexing_slicing)]

use crate::ui::app::QuillApp;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::lang::{keys, tr};
use quill::state::HistoryMessage;
use quill::telegram::envelope::{MessageContent, ServiceAction};

/// tdesktop `newThreadAboutMaxWidth` plus the bubble's side margins.
const CARD_MAX_WIDTH: f32 = 202.;
/// tdesktop `newThreadAboutIconOuter`.
const ICON_OUTER: f32 = 60.;
/// The glyph inside it (`chat/new_thread-40x40`, drawn at 40 of 72).
const ICON_SIZE: f32 = 34.;
/// tdesktop `newBotThreadTopSkip`: room above the bubble, the dotted line
/// in its middle.
const TOP_SKIP: f32 = 24.;
/// tdesktop `kLabelOpacity` for the arrow.
const ARROW_OPACITY: f32 = 0.7;

pub(in crate::ui) fn new_bot_thread_row(
    message: &HistoryMessage,
    cx: &mut Context<QuillApp>,
) -> Option<AnyElement> {
    let MessageContent::Action(action) = &message.content else {
        return None;
    };
    if **action != ServiceAction::NewBotThread {
        return None;
    }
    let bg = crate::ui::chat_palette::service_bg(cx.theme().secondary.opacity(0.85));
    let fg = cx.theme().secondary_foreground;
    let line = div()
        .w_full()
        .h(px(TOP_SKIP / 2.))
        .border_b_2()
        .border_dashed()
        .border_color(bg);
    let icon = div()
        .size(px(ICON_OUTER))
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .bg(fg.opacity(0.12))
        .child(
            Icon::new(gpui_kit::assets::IconName::MessageCirclePlus)
                .size(px(ICON_SIZE))
                .text_color(fg),
        );
    let card = div()
        .flex()
        .flex_col()
        .items_center()
        .max_w(px(CARD_MAX_WIDTH))
        .px_3()
        .pt_4()
        .pb_1()
        .rounded_xl()
        .bg(bg)
        .text_color(fg)
        .text_sm()
        .text_center()
        .child(icon)
        .child(
            div()
                .pt_4()
                .pb_1()
                .font_semibold()
                .child(tr(keys::BOT_NEW_THREAD_TITLE)),
        )
        .child(
            div()
                .whitespace_normal()
                .child(tr(keys::BOT_NEW_THREAD_ABOUT)),
        )
        .child(
            Icon::new(gpui_kit::assets::IconName::ChevronDown)
                .size(px(20.))
                .text_color(fg.opacity(ARROW_OPACITY)),
        );
    Some(
        div()
            .id(("new-bot-thread", message.id.0 as u64))
            .flex()
            .flex_col()
            .items_center()
            .gap(px(TOP_SKIP / 2.))
            .pb_2()
            .child(line)
            .child(card)
            .into_any_element(),
    )
}
