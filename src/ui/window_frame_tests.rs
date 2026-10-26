//! The bar above the window content under both frames, and the settings
//! switch that only the platforms with a choice show.

use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::window_frame::{
    FrameTitleBar, set_native, supported, titlebar_hidden, use_native, window_decorations,
};
use gpui_kit::WindowDecorations;
use gpui_kit::component::Root;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AppContext, Context, InteractiveElement, IntoElement, ParentElement, Render, Styled,
    TestAppContext, VisualTestContext, Window, div, px, size,
};

struct BarView {
    native: bool,
    menu: bool,
}

impl Render for BarView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(
            FrameTitleBar::new()
                .system_frame(self.native)
                .caption(
                    div()
                        .id("caption")
                        .debug_selector(|| "caption".into())
                        .child("Quill"),
                )
                .child(
                    div()
                        .id("action")
                        .debug_selector(|| "action".into())
                        .child("Search"),
                )
                .when(self.menu, |bar| {
                    bar.menu(
                        div()
                            .id("menu")
                            .debug_selector(|| "menu".into())
                            .child("File"),
                    )
                }),
        )
    }
}

fn bar(cx: &mut TestAppContext, native: bool) -> VisualTestContext {
    bar_with(cx, native, false)
}

fn bar_with(cx: &mut TestAppContext, native: bool, menu: bool) -> VisualTestContext {
    cx.update(gpui_kit::init);
    let handle = cx.open_window(size(px(600.), px(200.)), move |window, cx| {
        let view = cx.new(|_| BarView { native, menu });
        Root::new(view, window, cx)
    });
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    vcx.run_until_parked();
    vcx.update(|window, cx| window.render_frame(cx));
    vcx
}

#[gpui_kit::test]
fn own_frame_shows_the_title_and_the_actions(cx: &mut TestAppContext) {
    let mut vcx = bar(cx, false);
    assert!(vcx.debug_bounds("caption").is_some());
    assert!(vcx.debug_bounds("action").is_some());
}

#[gpui_kit::test]
fn system_frame_keeps_only_the_actions_and_no_caption(cx: &mut TestAppContext) {
    let mut vcx = bar(cx, true);
    assert!(
        vcx.debug_bounds("caption").is_none(),
        "the OS shows the title"
    );
    let action = vcx.debug_bounds("action").expect("actions stay");
    // Right-aligned in the slim row.
    assert!(action.origin.x > px(300.));
}

#[gpui_kit::test]
fn own_frame_puts_the_menu_in_the_title_row(cx: &mut TestAppContext) {
    let mut vcx = bar_with(cx, false, true);
    let menu = vcx.debug_bounds("menu").expect("menu in the bar");
    let action = vcx.debug_bounds("action").expect("actions stay");
    assert!(
        vcx.debug_bounds("caption").is_none(),
        "menu replaces caption"
    );
    assert_eq!(menu.center().y, action.center().y, "one row");
    assert!(menu.origin.x < action.origin.x);
}

#[gpui_kit::test]
fn system_frame_puts_the_menu_beside_the_actions(cx: &mut TestAppContext) {
    let mut vcx = bar_with(cx, true, true);
    let menu = vcx.debug_bounds("menu").expect("menu in the row");
    let action = vcx.debug_bounds("action").expect("actions stay");
    assert_eq!(menu.center().y, action.center().y, "one row");
    assert!(menu.origin.x < px(100.));
    assert!(action.origin.x > px(300.));
}

/// The tags an agent's pointer meets at `at` (`redact::point_allowed`).
fn tags_at(vcx: &mut VisualTestContext, at: gpui_kit::Point<gpui_kit::Pixels>) -> Vec<String> {
    vcx.simulate_mouse_move(at, None, gpui_kit::Modifiers::none());
    vcx.update(|window, cx| window.render_frame(cx));
    vcx.update(|window, _| {
        window
            .hovered_a11y_tags()
            .into_iter()
            .map(|tag| tag.to_string())
            .collect()
    })
}

#[gpui_kit::test]
fn own_frame_keeps_agents_off_the_bar_but_not_off_its_actions(cx: &mut TestAppContext) {
    use quill::agent::ui_driver::redact::{WINDOW_CHROME, WINDOW_CHROME_CONTENT, on_window_chrome};
    let mut vcx = bar_with(cx, false, true);
    let (Some(action), Some(menu), Some(drag_point)) = (
        vcx.debug_bounds("action"),
        vcx.debug_bounds("menu"),
        // The drag area between the menu and the actions.
        vcx.debug_bounds("action")
            .map(|action| gpui_kit::point(action.origin.x - px(40.), action.center().y)),
    ) else {
        panic!("bar drawn");
    };
    let drag = tags_at(&mut vcx, drag_point);
    assert_eq!(drag, [WINDOW_CHROME], "the drag area");
    assert!(on_window_chrome(&drag));
    for (name, at) in [("action", action.center()), ("menu", menu.center())] {
        let tags = tags_at(&mut vcx, at);
        assert_eq!(tags, [WINDOW_CHROME_CONTENT, WINDOW_CHROME], "{name}");
        assert!(!on_window_chrome(&tags), "{name}");
    }
}

#[gpui_kit::test]
fn the_switch_is_offered_only_where_the_platform_allows_it(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let demo = ScreenshotDemo::named("ready-chats");
    let handle = cx.open_window(size(px(900.), px(600.)), move |window, cx| {
        let view = cx.new(|cx| QuillApp::new_with_demo(window, cx, None, Some(demo)));
        *slot_in.borrow_mut() = Some(view.clone());
        Root::new(view, window, cx)
    });
    let app = slot.borrow().clone().unwrap();
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    app.update_in(&mut vcx, |app, _, cx| {
        assert!(app.native_frame_section(false, cx).is_none());
        assert!(app.native_frame_section(true, cx).is_some());
    });
}

/// One test: the choice is a process-wide static.
#[test]
fn native_choice_follows_the_saved_value_where_the_platform_allows_it() {
    set_native(false);
    assert!(set_native(true));
    assert!(!set_native(true));
    assert_eq!(use_native(), supported());
    assert_eq!(titlebar_hidden(), !supported());
    assert_eq!(
        window_decorations(),
        cfg!(target_os = "linux").then_some(WindowDecorations::Server)
    );
    assert!(set_native(false));
    assert!(!use_native());
    assert!(titlebar_hidden());
    assert_eq!(
        window_decorations(),
        cfg!(target_os = "linux").then_some(WindowDecorations::Client)
    );
}
