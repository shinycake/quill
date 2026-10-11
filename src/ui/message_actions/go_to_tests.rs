//! UI integration test: the peek preview's "Go To Message" menu opens on a
//! right-click and its click is not eaten by the preview overlay.

use crate::ui::app::QuillApp;
use crate::ui::chat_row::ChatPreviewState;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AppContext, Entity, Focusable, Modifiers, MouseButton, TestAppContext, VisualTestContext,
    point, px, size,
};
use quill::ids::ChatId;

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
    let app: Entity<QuillApp> = slot.borrow().clone().unwrap();
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    for _ in 0..2 {
        vcx.update(|window, cx| window.render_frame(cx));
    }
    vcx.run_until_parked();
    (app, vcx)
}

#[gpui_kit::test]
fn preview_row_menu_opens_the_chat(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    let (open, other) = app.read_with(vcx, |app, _| {
        let session = app.session().expect("demo session");
        let open = session.open_chat.expect("the demo opens a chat");
        let other = session
            .histories
            .iter()
            .find(|(id, history)| **id != open.0 && !history.messages.is_empty())
            .map(|(id, _)| ChatId(*id))
            .expect("another chat has messages");
        (open, other)
    });
    assert_ne!(open, other);
    app.update(vcx, |app, cx| {
        app.chat_list.preview = Some(ChatPreviewState {
            chat_id: other,
            anchor: point(px(300.), px(300.)),
        });
        cx.notify();
    });
    for _ in 0..2 {
        vcx.update(|window, cx| window.render_frame(cx));
    }
    let row = vcx
        .debug_bounds("chat-preview-row")
        .expect("the preview paints its rows");
    let press = row.center();
    vcx.simulate_mouse_down(press, MouseButton::Right, Modifiers::default());
    vcx.simulate_mouse_up(press, MouseButton::Right, Modifiers::default());
    vcx.run_until_parked();
    for _ in 0..2 {
        vcx.update(|window, cx| window.render_frame(cx));
    }
    // The menu opens at the press; its first item sits inside the padding.
    let item = point(press.x + px(30.), press.y + px(17.));
    vcx.simulate_mouse_move(item, None, Modifiers::default());
    vcx.simulate_click(item, Modifiers::default());
    vcx.run_until_parked();
    app.read_with(vcx, |app, _| {
        assert!(app.chat_list.preview.is_none(), "the preview closes");
        assert_eq!(app.session().and_then(|s| s.open_chat), Some(other));
    });
}
