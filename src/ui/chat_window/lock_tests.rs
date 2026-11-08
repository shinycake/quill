//! Locking Quill with a chat window open: each window shows the lock
//! screen with a field of its own, and the windows go idle.
#![cfg(feature = "demo-capture")]

use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, AppContext, Entity, Focusable, TestAppContext, px, size};
use quill::ids::ChatId;

const OTHER: ChatId = ChatId(12);

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, AnyWindowHandle) {
    cx.update(gpui_kit::init);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(700.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(window, cx, None, Some(ScreenshotDemo::named("ready-chats")))
        });
        let focus = view.focus_handle(cx);
        window.focus(&focus, cx);
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some(view);
        Root::new(shell, window, cx)
    });
    let app = slot.borrow().clone().expect("the app view");
    let main: AnyWindowHandle = handle.into();
    cx.run_until_parked();
    (app, main)
}

fn open_chat_window(
    app: &Entity<QuillApp>,
    main: AnyWindowHandle,
    cx: &mut TestAppContext,
) -> AnyWindowHandle {
    cx.update_window(main, |_, window, cx| {
        app.update(cx, |app, cx| app.open_chat_in_new_window(OTHER, window, cx));
    })
    .expect("the main window");
    cx.run_until_parked();
    cx.windows()
        .into_iter()
        .find(|handle| handle.window_id() != main.window_id())
        .expect("the chat window opened")
}

fn lock(app: &Entity<QuillApp>, cx: &mut TestAppContext) {
    app.update(cx, |app, cx| {
        app.set_window_scope(None);
        app.account.passcode.enabled = true;
        app.lock_by_passcode(cx);
    });
}

fn typed(app: &Entity<QuillApp>, scope: Option<ChatId>, cx: &mut TestAppContext) -> String {
    app.update(cx, |app, cx| {
        app.in_window_scope(scope, cx, |app, cx| {
            app.account.passcode.lock.input.read(cx).value().to_string()
        })
    })
    .expect("the window is open")
}

/// The windows sized apart is what made a shared field's paint look like
/// a layout change to the other window, every frame.
#[gpui_kit::test]
fn locking_with_a_chat_window_open_lets_both_windows_go_idle(cx: &mut TestAppContext) {
    let (app, main) = new_app(cx);
    let chat_window = open_chat_window(&app, main, cx);
    lock(&app, cx);
    cx.run_until_parked();
    for window in [main, chat_window] {
        cx.update_window(window, |_, window, cx| {
            window.render_frame(cx);
            assert!(
                window.try_find("lock-submit").is_some(),
                "every window shows the passcode box"
            );
        })
        .expect("the window");
        cx.run_until_parked();
    }
    app.read_with(cx, |app, _| assert!(app.account.passcode.locked));
}

/// tdesktop's lock widget is per window: what one window's field holds
/// is not the other's.
#[gpui_kit::test]
fn each_window_types_into_its_own_lock_field(cx: &mut TestAppContext) {
    let (app, main) = new_app(cx);
    let chat_window = open_chat_window(&app, main, cx);
    lock(&app, cx);
    cx.run_until_parked();
    cx.update_window(chat_window, |_, window, cx| {
        app.update(cx, |app, cx| {
            app.in_window_scope(Some(OTHER), cx, |app, cx| {
                let input = app.account.passcode.lock.input.clone();
                input.update(cx, |input, cx| input.set_value("1234", window, cx));
            })
        })
    })
    .expect("the chat window");
    cx.run_until_parked();
    assert_eq!(typed(&app, Some(OTHER), cx), "1234");
    assert_eq!(
        typed(&app, None, cx),
        "",
        "the main window's field is its own"
    );
}
