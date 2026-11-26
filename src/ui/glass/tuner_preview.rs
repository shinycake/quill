//! The tuning panel's live preview: a bright, busy backdrop (colour bars, a
//! gradient, large type) under a menu slab and two floating buttons, each
//! drawn with the glass the app draws now, so a dial shows its effect
//! before the person leaves Settings.
#![deny(clippy::indexing_slicing)]

use super::{GlassRole, GlassSurface as _};
use crate::ui::QuillApp;
use gpui_kit::assets::IconName;
use gpui_kit::component::Icon;
use gpui_kit::component::theme::ActiveTheme;
use gpui_kit::*;
use quill::lang::{keys, tr};

const HEIGHT: f32 = 168.0;
/// The demo's colour bars, as on its Editorial scene.
const BARS: [u32; 7] = [
    0xff453a, 0xff9f0a, 0xffd60a, 0x34c759, 0x32ade6, 0x007aff, 0xaf52de,
];

pub(super) fn preview(cx: &mut Context<QuillApp>) -> AnyElement {
    let theme = cx.theme();
    let dark = theme.background.l < 0.5;
    let (from, to) = if dark {
        (rgb(0x15122b), rgb(0x0a84ff))
    } else {
        (rgb(0xf4f2ed), rgb(0x5ac8fa))
    };
    let bars = div()
        .absolute()
        .left_0()
        .right_0()
        .top(px(18.))
        .flex()
        .gap_2()
        .px_4()
        .children(
            BARS.iter()
                .map(|&color| div().flex_1().h(px(132.)).rounded_lg().bg(rgb(color))),
        );
    let title = div()
        .absolute()
        .left_0()
        .right_0()
        .top(px(52.))
        .flex()
        .justify_center()
        .text_size(px(56.))
        .font_weight(FontWeight::BOLD)
        .text_color(gpui_kit::white())
        .child("Quill");
    let row = |icon: IconName, label: SharedString| {
        div()
            .flex()
            .items_center()
            .gap_2()
            .px_3()
            .h(px(28.))
            .text_sm()
            .child(Icon::new(icon).size(px(14.)))
            .child(label)
    };
    let menu = div()
        .absolute()
        .left(px(28.))
        .top(px(30.))
        .w(px(168.))
        .py_1()
        .rounded(px(12.))
        .text_color(theme.popover_foreground)
        .glass_or(GlassRole::Menu, cx, |this| {
            this.bg(theme.popover).border_1().border_color(theme.border)
        })
        .child(row(IconName::Copy, tr(keys::Q_AV_COPY).into()))
        .child(row(IconName::Forward, tr(keys::Q_AV_FORWARD).into()))
        .child(row(IconName::Pin, tr(keys::CONTEXT_PIN_MSG).into()));
    let button = |size: f32| {
        div()
            .size(px(size))
            .rounded_full()
            .flex()
            .items_center()
            .justify_center()
            .text_color(theme.foreground)
            .glass_or(GlassRole::Control, cx, |this| {
                this.bg(theme.background)
                    .border_1()
                    .border_color(theme.border)
            })
    };
    let buttons = div()
        .absolute()
        .right(px(36.))
        .top(px(46.))
        .flex()
        .items_center()
        .gap_4()
        .child(button(44.).child(Icon::new(IconName::Heart).size(px(18.))))
        .child(button(64.).child(Icon::new(IconName::Star).size(px(24.))));
    div()
        .id("glass-tuning-preview")
        .test_support()
        .relative()
        .w_full()
        .h(px(HEIGHT))
        .rounded_lg()
        .overflow_hidden()
        .bg(linear_gradient(
            135.,
            linear_color_stop(from, 0.),
            linear_color_stop(to, 1.),
        ))
        .child(bars)
        .child(title)
        .child(menu)
        .child(buttons)
        .into_any_element()
}
