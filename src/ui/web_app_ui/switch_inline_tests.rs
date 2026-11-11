//! A mini app's switch-inline text lands in the chat the app was opened
//! from, wherever that chat shows.

use super::super::RenderAction;
use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::{AnyWindowHandle, AppContext, Entity, TestAppContext, px, size};
use quill::ids::ChatId;

const FROM: ChatId = ChatId(12);
const QUERY: &str = "@bot pizza";

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, AnyWindowHandle) {
    cx.update(gpui_kit::init);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(700.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(window, cx, None, Some(ScreenshotDemo::named("ready-chats")))
        });
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some(view);
        Root::new(shell, window, cx)
    });
    let app = slot.borrow().clone().expect("the app view");
    cx.run_until_parked();
    (app, handle.into())
}

fn switch_inline(app: &Entity<QuillApp>, main: AnyWindowHandle, cx: &mut TestAppContext) {
    cx.update_window(main, |_, window, cx| {
        app.update(cx, |app, cx| {
            app.mini_apps.render_actions.push(RenderAction::SwitchInline {
                query: QUERY.into(),
                chat: Some(FROM),
            });
            app.run_pending_mini_app_action(window, cx);
        });
    })
    .expect("the main window");
    cx.run_until_parked();
}

fn composer(app: &Entity<QuillApp>, cx: &mut TestAppContext) -> String {
    app.read_with(cx, |app, cx| app.composer.read(cx).value().to_string())
}

/// The main window shows another chat: it opens the app's chat first, so
/// the text is never a draft for the wrong chat.
#[gpui_kit::test]
fn the_text_goes_to_the_chat_the_app_was_opened_from(cx: &mut TestAppContext) {
    let (app, main) = new_app(cx);
    assert_ne!(app.read_with(cx, |app, _| app.open_chat_id()), Some(FROM));
    switch_inline(&app, main, cx);
    assert_eq!(app.read_with(cx, |app, _| app.open_chat_id()), Some(FROM));
    assert!(composer(&app, cx).contains(QUERY));
}

/// That chat has a window of its own: the text lands in its composer, and
/// the main window's stays as it was.
#[gpui_kit::test]
fn the_text_goes_to_the_chats_own_window(cx: &mut TestAppContext) {
    let (app, main) = new_app(cx);
    cx.update_window(main, |_, window, cx| {
        app.update(cx, |app, cx| app.open_chat_in_new_window(FROM, window, cx));
    })
    .expect("the main window");
    cx.run_until_parked();
    let before = app.read_with(cx, |app, _| app.open_chat_id());
    switch_inline(&app, main, cx);
    assert_eq!(app.read_with(cx, |app, _| app.open_chat_id()), before);
    assert!(!composer(&app, cx).contains(QUERY));
    let in_window = app.update(cx, |app, cx| {
        app.in_window_scope(Some(FROM), cx, |app, cx| {
            app.composer.read(cx).value().to_string()
        })
    });
    assert!(in_window.is_some_and(|text| text.contains(QUERY)));
}
