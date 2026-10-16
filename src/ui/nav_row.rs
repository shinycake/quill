//! The navigation row used where a settings page opens a sub-page: label on
//! the left, a chevron on the right, the kit button's hover fill and focus
//! ring.

use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::*;

/// A full-width row that navigates when pressed.
pub(super) fn nav_row<T: 'static>(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    cx: &mut Context<T>,
    on_press: impl Fn(&mut T, &mut Window, &mut Context<T>) + 'static,
) -> Button {
    let label: SharedString = label.into();
    Button::new(id)
        .ghost()
        .w_full()
        .accessibility_label(label.clone())
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .w_full()
                .child(div().text_sm().child(label))
                .child(Icon::new(IconName::ChevronRight).size_4()),
        )
        .on_click(cx.listener(move |this, _, window, cx| on_press(this, window, cx)))
}
