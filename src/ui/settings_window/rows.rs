//! Row building blocks of settings pages: a titled group and a row that
//! opens something, with its current value on the right.
#![deny(clippy::indexing_slicing)]

use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::group_box::{GroupBox, GroupBoxVariants};
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

/// A titled group of rows on an outlined surface (the fill is too close to
/// the page in dark themes).
pub(super) fn group(
    title: Option<&'static str>,
    rows: impl IntoIterator<Item = AnyElement>,
) -> GroupBox {
    GroupBox::new()
        .outline()
        .content_style(StyleRefinement::default().p_1().gap_0())
        .when_some(title, |this, title| this.title(title))
        .children(rows)
}

/// A row that opens a screen or a page: title, current value, chevron. A
/// kit button, for its hover fill, focus ring and keyboard press.
pub(super) fn link_row(
    id: impl Into<ElementId>,
    title: impl Into<SharedString>,
    value: Option<SharedString>,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    cx: &App,
) -> AnyElement {
    let title: SharedString = title.into();
    Button::new(id)
        .ghost()
        .w_full()
        .h(px(40.))
        .accessibility_label(title.clone())
        .child(
            div()
                .flex()
                .items_center()
                .gap_3()
                .w_full()
                .child(div().flex_1().min_w_0().text_sm().truncate().child(title))
                .when_some(value, |this, value| {
                    this.child(
                        div()
                            .flex_none()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(value),
                    )
                })
                .child(
                    Icon::new(IconName::ChevronRight)
                        .size_4()
                        .text_color(cx.theme().muted_foreground),
                ),
        )
        .on_click(on_click)
        .into_any_element()
}

/// A row that acts at once (Log out): icon and title in the danger color,
/// no chevron.
pub(super) fn danger_row(
    id: impl Into<ElementId>,
    icon: IconName,
    title: &'static str,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    cx: &App,
) -> AnyElement {
    Button::new(id)
        .ghost()
        .w_full()
        .h(px(40.))
        .accessibility_label(title)
        .child(
            div()
                .flex()
                .items_center()
                .gap_3()
                .w_full()
                .text_sm()
                .text_color(cx.theme().danger)
                .child(Icon::new(icon).size_4())
                .child(title),
        )
        .on_click(on_click)
        .into_any_element()
}
