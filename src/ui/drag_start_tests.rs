#![cfg(test)]
#![cfg(feature = "demo-capture")]
//! How far a press moves before a drag starts. The vendored GPUI starts
//! every `on_drag` at 2 px; message drags ask for tdesktop's 10 px
//! (`drag_start_distance`), and nothing else is changed by that. A button on
//! a draggable tile lets its press through, so the tile can lift from it,
//! and stops its click so the tile does not also take it.

use gpui_kit::component::button::Button;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    AppContext, Context, InteractiveElement, IntoElement, Modifiers, MouseButton, ParentElement,
    Render, StatefulInteractiveElement, Styled, TestAppContext, Window, div, point, px,
};

struct Ghost;

impl Render for Ghost {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().size(px(4.))
    }
}

#[derive(Clone)]
struct Payload;

#[derive(Default)]
struct Probe {
    far: bool,
    tile_clicks: u32,
    button_clicks: u32,
}

impl Render for Probe {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let far = self.far;
        div().size_full().child(
            div()
                .id("tile")
                .w(px(200.))
                .h(px(100.))
                .on_drag(Payload, |_: &Payload, _, _, cx| cx.new(|_| Ghost))
                .when(far, |tile| tile.drag_start_distance(px(10.)))
                .on_click(cx.listener(|this, _, _, _| this.tile_clicks += 1))
                .child(Button::new("button").label("Play").on_click(cx.listener(
                    |this, _, _, cx| {
                        cx.stop_propagation();
                        this.button_clicks += 1;
                    },
                ))),
        )
    }
}

fn dragging_after(far: bool, dx: f32, dy: f32, cx: &mut TestAppContext) -> bool {
    cx.update(gpui_kit::init);
    let (_, vcx) = cx.add_window_view(|_, _| Probe {
        far,
        ..Probe::default()
    });
    vcx.run_until_parked();
    let none = Modifiers::default();
    let from = point(px(8.), px(8.));
    vcx.simulate_mouse_move(from, None, none);
    vcx.simulate_mouse_down(from, MouseButton::Left, none);
    vcx.simulate_mouse_move(
        point(from.x + px(dx), from.y + px(dy)),
        Some(MouseButton::Left),
        none,
    );
    vcx.update(|_, cx| cx.has_active_drag())
}

#[gpui_kit::test]
fn an_ordinary_drag_source_starts_after_a_few_pixels(cx: &mut TestAppContext) {
    assert!(dragging_after(false, 4., 0., cx));
}

#[gpui_kit::test]
fn a_far_drag_source_waits_for_ten_pixels_in_manhattan_distance(cx: &mut TestAppContext) {
    assert!(!dragging_after(true, 4., 0., cx));
    assert!(!dragging_after(true, 4., 4., cx), "8 px across and down");
    assert!(dragging_after(true, 6., 6., cx), "12 px across and down");
    assert!(dragging_after(true, 12., 0., cx));
}

#[gpui_kit::test]
fn a_button_press_lifts_the_tile_under_it(cx: &mut TestAppContext) {
    // The button sits at the tile's top left, where the press lands.
    assert!(dragging_after(true, 14., 0., cx));
}

#[gpui_kit::test]
fn a_button_click_is_the_buttons_alone(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let (view, vcx) = cx.add_window_view(|_, _| Probe {
        far: true,
        ..Probe::default()
    });
    vcx.run_until_parked();
    vcx.simulate_click(point(px(8.), px(8.)), Modifiers::default());
    let (tile, button) = view.read_with(vcx, |probe, _| (probe.tile_clicks, probe.button_clicks));
    assert_eq!((tile, button), (0, 1));
}
