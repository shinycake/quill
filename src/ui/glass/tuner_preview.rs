//! The tuning panel's live preview: a slice of a chat (a photo, message
//! bubbles) under the glass Quill really draws there, built the way the app
//! builds it: the chat list's segmented tabs with their glass selection,
//! the floating date pill, a message context menu (`menu_surface`), the
//! emoji bar over the composer, the jump-to-latest button, and one empty
//! slab for reference. Each takes the glass the app draws now, so a dial
//! shows its effect on the real surfaces before the person leaves Settings.
#![deny(clippy::indexing_slicing)]

use super::{GlassRole, GlassSurface as _};
use crate::ui::QuillApp;
use crate::ui::chat_theme::{
    accent_strong, bg_bubble_incoming, danger_bright, text_menu, text_on_fill, text_primary,
};
use crate::ui::menu_surface::MenuSurface as _;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::tab::{Tab, TabBar};
use gpui_kit::component::theme::ActiveTheme;
use gpui_kit::component::{Icon, Sizable as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use quill::lang::{Key, keys, tr};

const HEIGHT: f32 = 340.0;
/// The emoji bar's suggestions.
const EMOJI: [&str; 7] = ["👍", "❤️", "😂", "🔥", "🎉", "😮", "🙏"];

/// A message bubble with `text`, outgoing in the accent colour.
fn bubble(text: Key, outgoing: bool) -> Div {
    let (bg, fg) = if outgoing {
        (accent_strong(), text_on_fill())
    } else {
        (bg_bubble_incoming(), text_primary())
    };
    div()
        .max_w(px(230.))
        .px_3()
        .py_1p5()
        .rounded_xl()
        .bg(bg)
        .text_color(fg)
        .text_sm()
        .child(tr(text))
}

/// A photo in an incoming bubble: a sunset over two hills.
fn photo() -> Div {
    let hill = |top: f32, left: f32, color: u32| {
        div()
            .absolute()
            .top(px(top))
            .left(px(left))
            .size(px(240.))
            .rounded_full()
            .bg(rgb(color))
    };
    div()
        .relative()
        .w(px(220.))
        .h(px(110.))
        .rounded_xl()
        .overflow_hidden()
        .bg(linear_gradient(
            180.,
            linear_color_stop(rgb(0xf7b267), 0.),
            linear_color_stop(rgb(0xf4845f), 1.),
        ))
        .child(
            div()
                .absolute()
                .top(px(22.))
                .left(px(140.))
                .size(px(34.))
                .rounded_full()
                .bg(rgb(0xfff1c1)),
        )
        .child(hill(62., -110., 0xd9634a))
        .child(hill(80., 60., 0xa8432f))
}

impl QuillApp {
    pub(super) fn glass_tuner_preview(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let dark = theme.background.l < 0.5;
        let (from, to) = if dark {
            (rgb(0x1b2733), rgb(0x2b1f3a))
        } else {
            (rgb(0xdfe8d0), rgb(0xc9dcef))
        };
        let history = div()
            .absolute()
            .inset_0()
            .px_4()
            .pb(px(60.))
            .flex()
            .flex_col()
            .justify_end()
            .gap_2()
            .child(
                div()
                    .p_1()
                    .rounded_xl()
                    .bg(bg_bubble_incoming())
                    .w(px(228.))
                    .child(photo()),
            )
            .child(
                div()
                    .flex()
                    .justify_end()
                    .child(bubble(keys::Q_PREFS_GLASS_SAMPLE_OUT, true)),
            )
            .child(bubble(keys::Q_PREFS_GLASS_SAMPLE_IN, false));
        let tabs = div()
            .absolute()
            .top(px(10.))
            .left(px(12.))
            .w(px(240.))
            .child(
                TabBar::new("glass-preview-tabs")
                    .w_full()
                    .segmented()
                    .glass_selection()
                    .small()
                    .selected_index(0)
                    .child(Tab::new().flex_grow(1.).label(tr(keys::RECENT_CHATS)))
                    .child(Tab::new().flex_grow(1.).label(tr(keys::MENU_CONTACTS)))
                    .child(Tab::new().flex_grow(1.).label(tr(keys::MENU_CALLS))),
            );
        let date =
            crate::ui::conversation::pill_label(tr(keys::Q_CHATLIST_TODAY), cx).map(|pill| {
                match super::lens(GlassRole::Control, cx) {
                    Some(glass) => pill
                        .bg(transparent_black())
                        .text_color(cx.theme().foreground)
                        .backdrop_glass(Some(glass)),
                    None => pill,
                }
            });
        let date = div()
            .absolute()
            .top(px(14.))
            .left(px(270.))
            .right_0()
            .flex()
            .justify_center()
            .child(date);
        let row = |icon: IconName, label: Key, danger: bool| {
            div()
                .flex()
                .items_center()
                .gap_3()
                .px_3()
                .py_1p5()
                .rounded_md()
                .text_sm()
                .text_color(if danger { danger_bright() } else { text_menu() })
                .child(Icon::new(icon).size(px(16.)))
                .child(tr(label))
        };
        let menu = div()
            .absolute()
            .left(px(150.))
            .top(px(56.))
            .w(px(190.))
            .flex()
            .flex_col()
            .p_1()
            .menu_surface(self.look(), cx)
            .child(row(IconName::Reply, keys::Q_CHAT_REPLY, false))
            .child(row(IconName::Copy, keys::Q_AV_COPY, false))
            .child(row(IconName::Forward, keys::CONTEXT_FORWARD_MSG, false))
            .child(row(IconName::Pin, keys::CONTEXT_PIN_MSG, false))
            .child(row(IconName::Delete, keys::BOX_DELETE, true));
        let theme = cx.theme();
        let emoji = div()
            .absolute()
            .left(px(12.))
            .bottom(px(12.))
            .flex()
            .gap_1()
            .p_1()
            .rounded_lg()
            .border_1()
            .glass_or(GlassRole::Panel, cx, |this| {
                this.border_color(theme.border)
                    .bg(theme.popover)
                    .shadow_md()
            })
            .children(EMOJI.iter().map(|emoji| {
                div()
                    .size(px(32.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(20.))
                    .child(*emoji)
            }));
        let slab = div()
            .absolute()
            .right(px(16.))
            .top(px(96.))
            .w(px(120.))
            .h(px(72.))
            .rounded_xl()
            .glass_or(GlassRole::Panel, cx, |this| {
                this.bg(theme.popover).border_1().border_color(theme.border)
            });
        let jump = div()
            .absolute()
            .right(px(16.))
            .bottom(px(12.))
            .child(super::glass_button(
                Button::new("glass-preview-jump")
                    .secondary()
                    .icon(IconName::ChevronDown)
                    .rounded_full()
                    .border_1(),
                super::lens(GlassRole::Control, cx),
            ));
        div()
            .id("glass-tuning-preview")
            .test_support()
            .relative()
            .w_full()
            .h(px(HEIGHT))
            .rounded_lg()
            .overflow_hidden()
            .bg(linear_gradient(
                160.,
                linear_color_stop(from, 0.),
                linear_color_stop(to, 1.),
            ))
            .child(history)
            .child(tabs)
            .child(date)
            .child(slab)
            .child(menu)
            .child(emoji)
            .child(jump)
            .into_any_element()
    }
}
