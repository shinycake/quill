use super::{MENU_RADIUS, menu_shadow, note_drawn_window};
use gpui_kit::component::popover_shape::popover_shape;
use gpui_kit::{TestAppContext, px};
use quill::layout_style::Clip;

#[gpui_kit::test]
fn menus_take_web_k_shape_only_while_a_floating_window_draws(cx: &mut TestAppContext) {
    cx.update(|cx| {
        assert_eq!(popover_shape(cx), None, "no window has drawn");
        note_drawn_window(Clip::Rounded, cx);
        let shape = popover_shape(cx);
        assert_eq!(
            shape.as_ref().map(|shape| shape.radius),
            Some(px(MENU_RADIUS))
        );
        assert_eq!(shape.map(|shape| shape.shadow), Some(menu_shadow()));
        note_drawn_window(Clip::Square, cx);
        assert_eq!(popover_shape(cx), None, "a Classic window draws next");
    });
}
