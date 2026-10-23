#![cfg(test)]
#![cfg(feature = "demo-capture")]
//! History pages are asked for three screens before the loaded window ends
//! (tdesktop `kPreloadHeightsCount`), measured on the kit scroller.

use super::history_fx::preload_due;
use gpui_kit::component::message_scroller::{MessageScroller, MessageScrollerState};
use gpui_kit::{
    AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render, Styled as _,
    TestAppContext, VisualTestContext, Window, div, px,
};

struct Harness {
    state: Entity<MessageScrollerState>,
}

impl Render for Harness {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size(px(400.)).child(
            MessageScroller::new("messages", self.state.clone(), |_, _, _| {
                div().h(px(40.)).into_any_element()
            })
            .scrollbar(false)
            .jump_button(false),
        )
    }
}

#[gpui_kit::test]
fn distance_to_edges_counts_rows_not_laid_out_yet(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let state = cx.new(|cx| MessageScrollerState::new(30, cx));
    let (_, cx) = cx.add_window_view({
        let state = state.clone();
        move |_, _| Harness { state }
    });
    let draw = |cx: &mut VisualTestContext| {
        cx.run_until_parked();
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });
    };
    let edges = |cx: &mut VisualTestContext| {
        cx.update(|_, cx| state.read(cx).distance_to_edges())
            .map(|(above, below, viewport)| {
                (f32::from(above), f32::from(below), f32::from(viewport))
            })
    };
    draw(cx);

    // At the tail only the last rows were laid out; the ones above still
    // count, at the mean height (a row is 40px plus the 32px gap after it).
    let (above, below, viewport) = edges(cx).expect("laid out");
    assert!(below < 1., "at the tail: {below}");
    assert!(viewport > 300. && viewport <= 400., "viewport: {viewport}");
    assert!(
        (above - (30. * 72. - viewport)).abs() < 120.,
        "above the tail: {above}"
    );

    cx.update(|_, cx| state.update(cx, |state, cx| state.scroll_to_item(10, cx)));
    draw(cx);
    let (above, below, viewport) = edges(cx).expect("laid out");
    assert!((above - 720.).abs() < 40., "above row 10: {above}");
    assert!(
        (below - (30. * 72. - 720. - viewport)).abs() < 80.,
        "below: {below}"
    );
}

#[test]
fn a_page_is_due_three_screens_before_either_end() {
    let screen = px(500.);
    assert_eq!(preload_due(px(1500.), px(1501.), screen), (true, false));
    assert_eq!(preload_due(px(1501.), px(1500.), screen), (false, true));
    assert_eq!(preload_due(px(4000.), px(4000.), screen), (false, false));
    assert_eq!(preload_due(px(0.), px(0.), screen), (true, true));
}
