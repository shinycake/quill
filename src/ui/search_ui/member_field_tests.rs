//! The "From:" picker filters members with a field of its own, so the
//! message query typed before it opens is never replaced or reused.

use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::telegram::envelope::MessageSender;

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    let root = quill::scratch::process_dir("quill-member-field");
    let _ = std::fs::create_dir_all(&root);
    quill::settings::use_isolated_app_root(root);
    cx.update(gpui_kit::init);
    cx.update(crate::ui::bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(800.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(
                window,
                cx,
                None,
                Some(ScreenshotDemo::named("ready-search-in-chat")),
            )
        });
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some(view);
        Root::new(shell, window, cx)
    });
    let app = slot.borrow().clone().expect("app view");
    let vcx = VisualTestContext::from_window(handle.into(), cx);
    (app, vcx)
}

fn texts(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> (String, String) {
    app.read_with(vcx, |app, cx| {
        (
            app.search_ui.chat_input.read(cx).value().to_string(),
            app.search_ui.member_input.read(cx).value().to_string(),
        )
    })
}

#[gpui_kit::test]
fn picking_a_sender_leaves_the_message_query_and_each_picker_starts_empty(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    app.update_in(vcx, |app, window, cx| {
        app.search_ui
            .chat_input
            .update(cx, |input, cx| input.set_value("hello", window, cx));
        app.toggle_chat_search_picker(window, cx);
        app.search_ui
            .member_input
            .update(cx, |input, cx| input.set_value("ann", window, cx));
    });
    assert_eq!(texts(&app, vcx), ("hello".into(), "ann".into()));

    app.update_in(vcx, |app, window, cx| {
        app.chat_search_pick_sender(Some(MessageSender::User { user_id: 5 }), window, cx);
    });
    assert_eq!(texts(&app, vcx).0, "hello");

    app.update_in(vcx, |app, window, cx| {
        app.toggle_chat_search_picker(window, cx);
    });
    assert_eq!(texts(&app, vcx), ("hello".into(), String::new()));
}
