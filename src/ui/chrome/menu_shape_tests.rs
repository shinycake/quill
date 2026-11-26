use super::{MENU_RADIUS, menu_shadow, note_drawn_window};
use gpui_kit::component::popover_shape::popover_shape;
use gpui_kit::{AnyWindowHandle, AppContext as _, Empty, TestAppContext, px};
use quill::layout_style::Look;

fn open(cx: &mut TestAppContext) -> AnyWindowHandle {
    cx.add_window(|_, _| Empty).into()
}

#[gpui_kit::test]
fn a_menu_takes_the_shape_of_the_window_it_is_in(cx: &mut TestAppContext) {
    let cards = open(cx);
    let lines = open(cx);
    let shape_in = |handle: AnyWindowHandle, cx: &mut TestAppContext| {
        cx.update_window(handle, |_, window, cx| popover_shape(window, cx))
            .ok()
            .flatten()
    };
    assert_eq!(shape_in(cards, cx), None, "no window has drawn");
    cx.update(|cx| {
        note_drawn_window(cards.window_id(), Look::Cards, cx);
        note_drawn_window(lines.window_id(), Look::Lines, cx);
    });
    let shape = shape_in(cards, cx);
    assert_eq!(
        shape.as_ref().map(|shape| shape.radius),
        Some(px(MENU_RADIUS))
    );
    assert_eq!(shape.map(|shape| shape.shadow), Some(menu_shadow(false)));
    assert_eq!(shape_in(lines, cx), None, "the Classic window beside it");
}
