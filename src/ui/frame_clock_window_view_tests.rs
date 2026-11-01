//! A view in a window of its own (the voice chat window) ticks while that
//! window is active, and moves nothing in the main window meanwhile (needs
//! the `demo-capture` feature for gpui-kit's test support).
use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use gpui_kit::component::Root;
use gpui_kit::{
    AppContext, Context, Entity, IntoElement, ParentElement, Render, TestAppContext,
    VisualTestContext, Window, div, px, size,
};

use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;

struct Blank;

impl Render for Blank {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().child("")
    }
}

fn app_window(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    let slot = Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(700.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(window, cx, None, Some(ScreenshotDemo::named("ready-chats")))
        });
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some(view);
        Root::new(shell, window, cx)
    });
    let app: Entity<QuillApp> = slot.borrow().clone().expect("demo app");
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    vcx.update(|window, _| window.activate_window());
    vcx.run_until_parked();
    (app, vcx)
}

/// Counts the redraws the clock asks of `view`.
fn redraws(view: &Entity<Blank>, vcx: &mut VisualTestContext) -> Rc<Cell<u32>> {
    let count = Rc::new(Cell::new(0));
    let counted = count.clone();
    vcx.update(|_, cx| {
        cx.observe(view, move |_, _| counted.set(counted.get() + 1))
            .detach();
    });
    count
}

#[gpui_kit::test]
fn the_call_windows_tick_leaves_the_inactive_main_window_still(cx: &mut TestAppContext) {
    let (app, mut vcx) = app_window(cx);
    let slice = vcx.update(|_, cx| cx.new(|_| Blank));
    let panel = vcx.update(|_, cx| cx.new(|_| Blank));
    let slice_redraws = redraws(&slice, &mut vcx);
    let panel_redraws = redraws(&panel, &mut vcx);

    app.update_in(&mut vcx, |app, window, cx| {
        assert!(window.is_window_active(), "the panel's own window");
        app.frame.window_active.set(true);
        app.request_animation_tick_for(Some(slice.entity_id()), 30, cx);
        // The main window goes behind the voice chat window.
        app.frame.window_active.set(false);
        app.request_window_view_tick(panel.entity_id(), window, 30, cx);
    });
    vcx.executor().advance_clock(Duration::from_millis(100));
    vcx.run_until_parked();

    assert!(panel_redraws.get() > 0, "the call window's panel moves");
    assert_eq!(slice_redraws.get(), 0, "the main window holds still");
}
