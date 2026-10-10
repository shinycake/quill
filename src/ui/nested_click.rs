//! Clickable children inside clickable parents.
//!
//! GPUI delivers a mouse press to every element under the pointer, and kit
//! `Button`s do not stop it, so a button (or any `on_click` div) nested in
//! a clickable parent fires both. Swallowing the press on the nested
//! control keeps the parent from recording it, which is what
//! [`SwallowPress::swallow_press`] does. tdesktop does the same implicitly:
//! a pressed `ClickHandler` suppresses the surrounding widget's press action.

use gpui_kit::{InteractiveElement, MouseButton};

/// Keeps a left press on this element from reaching clickable ancestors.
/// Put it on the nested control, next to its own `on_click`.
pub(super) trait SwallowPress: InteractiveElement + Sized {
    fn swallow_press(self) -> Self {
        self.on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
    }
}

impl<T: InteractiveElement> SwallowPress for T {}

#[cfg(all(test, feature = "demo-capture"))]
mod tests {
    use super::SwallowPress;
    use gpui_kit::component::button::Button;
    use gpui_kit::prelude::FluentBuilder;
    use gpui_kit::{
        Context, InteractiveElement, IntoElement, Modifiers, ParentElement, Render,
        StatefulInteractiveElement, Styled, TestAppContext, Window, div, point, px,
    };

    #[derive(Default)]
    struct Probe {
        parent: u32,
        child: u32,
        swallow: bool,
    }

    impl Render for Probe {
        fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let swallow = self.swallow;
            div().size_full().child(
                div()
                    .id("parent")
                    .w(px(200.))
                    .h(px(100.))
                    .on_click(cx.listener(|this, _, _, _| this.parent += 1))
                    .child(
                        Button::new("child")
                            .label("Play")
                            .when(swallow, |button| button.swallow_press())
                            .on_click(cx.listener(|this, _, _, _| this.child += 1)),
                    ),
            )
        }
    }

    fn click_child(swallow: bool, cx: &mut TestAppContext) -> (u32, u32) {
        cx.update(gpui_kit::init);
        let (view, vcx) = cx.add_window_view(|_, _| Probe {
            swallow,
            ..Probe::default()
        });
        vcx.run_until_parked();
        // Top-left of the parent holds the (auto-sized) button.
        vcx.simulate_click(point(px(8.), px(8.)), Modifiers::default());
        // A later press elsewhere must not replay the parent's click.
        vcx.simulate_click(point(px(250.), px(180.)), Modifiers::default());
        view.read_with(vcx, |probe, _| (probe.parent, probe.child))
    }

    #[gpui_kit::test]
    fn plain_button_also_clicks_its_parent(cx: &mut TestAppContext) {
        // Documents the hazard the helper exists for.
        assert_eq!(click_child(false, cx), (1, 1));
    }

    #[gpui_kit::test]
    fn swallowed_press_keeps_the_parent_quiet(cx: &mut TestAppContext) {
        assert_eq!(click_child(true, cx), (0, 1));
    }
}
