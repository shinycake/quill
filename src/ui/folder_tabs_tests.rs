//! The folder tabs' sliding indicator: it springs when the selection moves
//! and jumps when only the selected tab's width changes (a badge appearing),
//! which is what tdesktop does (`DiscreteSlider::setSelectedSection` animates,
//! `ChatsFiltersTabs::setUnreadCount` does not).

use gpui_kit::IntoElement;
use gpui_kit::component::Root;
use gpui_kit::component::tab::{Tab, TabBar};
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AppContext, Context, Entity, ParentElement, Render, Styled, TestAppContext, VisualTestContext,
    Window, div, px, size,
};
use std::time::Duration;

struct Strip {
    selected: usize,
    badge: bool,
}

impl Render for Strip {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let badge_width = if self.badge { 40. } else { 4. };
        div().w(px(500.)).child(
            TabBar::new("test-folder-tabs")
                .underline()
                .selected_index(self.selected)
                .child(
                    Tab::new()
                        .label("All")
                        .suffix(div().w(px(badge_width)).h(px(10.))),
                )
                .child(Tab::new().label("Work"))
                .child(Tab::new().label("News")),
        )
    }
}

fn frame(cx: &mut VisualTestContext) {
    cx.executor().advance_clock(Duration::from_millis(16));
    cx.run_until_parked();
    cx.update(|window, cx| window.render_frame(cx));
}

fn open(cx: &mut TestAppContext) -> Option<(VisualTestContext, Entity<Strip>)> {
    cx.update(|cx| {
        gpui_kit::init(cx);
        cx.set_reduce_motion(false);
    });
    let mut view = None;
    let handle = cx.open_window(size(px(600.), px(200.)), |window, cx| {
        let strip = cx.new(|_| Strip {
            selected: 0,
            badge: false,
        });
        view = Some(strip.clone());
        Root::new(strip, window, cx)
    });
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    for _ in 0..4 {
        frame(&mut vcx);
    }
    view.map(|strip| (vcx, strip))
}

#[gpui_kit::test]
fn indicator_jumps_when_a_badge_resizes_the_selected_tab(cx: &mut TestAppContext) {
    let Some((mut vcx, strip)) = open(cx) else {
        panic!("window did not open");
    };
    let before = vcx.debug_bounds("tab-indicator").expect("indicator drawn");
    strip.update(&mut vcx, |s, cx| {
        s.badge = true;
        cx.notify();
    });
    // Only a few 16 ms frames pass: a 280 ms spring would still be near the
    // old width.
    for _ in 0..4 {
        frame(&mut vcx);
    }
    let after = vcx.debug_bounds("tab-indicator").expect("indicator drawn");
    assert!(
        (after.size.width - (before.size.width + px(36.))).abs() < px(1.),
        "indicator width {:?} should match the resized tab (was {:?})",
        after.size.width,
        before.size.width
    );
}

#[gpui_kit::test]
fn indicator_still_springs_when_the_selection_moves(cx: &mut TestAppContext) {
    let Some((mut vcx, strip)) = open(cx) else {
        panic!("window did not open");
    };
    let start = vcx.debug_bounds("tab-indicator").expect("indicator drawn");
    strip.update(&mut vcx, |s, cx| {
        s.selected = 2;
        cx.notify();
    });
    for _ in 0..3 {
        frame(&mut vcx);
    }
    let mid = vcx.debug_bounds("tab-indicator").expect("indicator drawn");
    assert!(mid.origin.x > start.origin.x, "indicator moves");
    for _ in 0..60 {
        frame(&mut vcx);
    }
    let end = vcx.debug_bounds("tab-indicator").expect("indicator drawn");
    assert!(
        end.origin.x > mid.origin.x,
        "still travelling after 3 frames"
    );
}
