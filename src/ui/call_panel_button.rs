//! The call window's round buttons (tdesktop `Ui::CallButton`).
#![deny(clippy::indexing_slicing)]

use crate::ui::app_theme::call_chrome;
use gpui_kit::assets::IconName;
use gpui_kit::component::{Icon, Sizable};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use std::time::Duration;

/// One round control: a 44 pt disc and an 11 pt label (tdesktop
/// `CallButton`, 68 × 79).
pub(super) fn call_button(
    id: &'static str,
    icon: IconName,
    label: &'static str,
    style: ButtonLook,
    on_click: impl Fn(&mut App) + 'static,
) -> impl IntoElement {
    let (bg, fg, rotate) = match style {
        ButtonLook::Plain => (call_chrome::wash(), call_chrome::text(), false),
        ButtonLook::Active => (call_chrome::active(), call_chrome::on_active(), false),
        ButtonLook::Answer => (call_chrome::answer_fill(), white(), false),
        ButtonLook::Hangup => (call_chrome::hangup_fill(), white(), true),
    };
    let mut icon = Icon::new(icon).with_size(px(22.)).text_color(fg);
    if rotate {
        icon = icon.rotate(radians(std::f32::consts::PI * 0.75));
    }
    div()
        .id(id)
        .w(px(68.))
        .flex()
        .flex_col()
        .items_center()
        .gap(px(6.))
        .cursor_pointer()
        .role(Role::Button)
        .aria_label(label)
        .on_click(move |_, _, cx| on_click(cx))
        .child(
            div()
                .size(px(44.))
                .rounded_full()
                .bg(bg)
                .flex()
                .items_center()
                .justify_center()
                .hover(|style| style.opacity(0.88))
                .child(icon)
                // The answer button breathes while it rings (tdesktop's
                // outer ring).
                .when(style == ButtonLook::Answer, |this| {
                    let ring = div()
                        .absolute()
                        .inset(px(-6.))
                        .rounded_full()
                        .border_2()
                        .border_color(call_chrome::answer_ring());
                    // Battery and animations: a still ring.
                    let ring = if quill::power_saving::on(quill::power_saving::Flag::Calls) {
                        ring.opacity(0.6).into_any_element()
                    } else {
                        ring.with_animation(
                            "answer-pulse",
                            Animation::new(Duration::from_millis(1400))
                                .repeat()
                                .with_easing(pulsating_between(0.2, 1.0)),
                            |ring, delta| ring.opacity(delta),
                        )
                        .into_any_element()
                    };
                    this.relative().child(ring)
                }),
        )
        .child(
            div()
                .text_size(px(11.))
                .text_color(call_chrome::text())
                .child(label),
        )
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ButtonLook {
    Plain,
    /// Switched off (muted mic, camera off look in tdesktop is the
    /// inverse: white disc, dark icon).
    Active,
    Answer,
    Hangup,
}
