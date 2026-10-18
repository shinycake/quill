//! Keyboard reach of the archive row, story tile and playlist track
//! context menus: Tab lands on an item inside the menu (Enter is checked in the real-app capture)
//! (needs the `demo-capture` feature for gpui-kit's test support).
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::{AppContext, Entity, Focusable, TestAppContext, VisualTestContext, point, px, size};

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(700.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(window, cx, None, Some(ScreenshotDemo::named("ready-chats")))
        });
        *slot_in.borrow_mut() = Some(view.clone());
        let focus = view.focus_handle(cx);
        window.focus(&focus, cx);
        let shell = cx.new(|_| QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let app: Entity<QuillApp> = slot
        .borrow()
        .clone()
        .expect("the window builder fills the slot");
    let vcx = VisualTestContext::from_window(handle.into(), cx);
    (app, vcx)
}

/// After Tab the focus sits on an item inside the menu, not on its frame.
pub(super) fn tab_lands_on_an_item(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) {
    vcx.run_until_parked();
    vcx.simulate_keystrokes("tab");
    let (inside, on_frame) = vcx.update(|window, cx| {
        let menu = app.read(cx).frame.context_menu_focus.clone();
        (menu.contains_focused(window, cx), menu.is_focused(window))
    });
    assert!(inside, "focus stays in the menu");
    assert!(!on_frame, "Tab moved off the menu frame onto an item");
}

#[gpui_kit::test]
fn tab_reaches_the_archive_menu_items(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    app.update_in(vcx, |app, _, cx| {
        app.chat_list.archive_menu = Some(point(px(40.), px(80.)));
        cx.notify();
    });
    tab_lands_on_an_item(&app, vcx);
}

#[gpui_kit::test]
fn tab_reaches_the_story_menu_items(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    app.update_in(vcx, |app, _, cx| {
        app.open_story_menu(1, point(px(40.), px(80.)), cx)
    });
    tab_lands_on_an_item(&app, vcx);
}
