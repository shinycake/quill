//! The in-chat search hits list folds once the user jumps to a hit and the
//! "N of M" counter brings it back (tdesktop `ComposeSearch` hides its list
//! on every activation), over the `ready-chats` demo.
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, Focusable, TestAppContext, VisualTestContext, px, size};
use quill::ids::MessageId;

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
    frames(&mut vcx);
    (app, vcx)
}

fn frames(cx: &mut VisualTestContext) {
    for _ in 0..2 {
        cx.update(|window, cx| window.render_frame(cx));
    }
    cx.run_until_parked();
}

/// Open the bar and search the demo chat for "e" (three hits).
fn search(app: &Entity<QuillApp>, cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.open_chat_search_ui(window, cx);
            app.sync_chat_search_query("e", cx);
        });
    });
    frames(cx);
}

fn list_shown(app: &Entity<QuillApp>, cx: &mut VisualTestContext) -> bool {
    app.read_with(cx, |app, _| {
        app.session()
            .is_some_and(|session| session.search.chat_search.list_shown())
    })
}

fn search_open(app: &Entity<QuillApp>, cx: &mut VisualTestContext) -> bool {
    app.read_with(cx, |app, _| app.chat_search_is_open())
}

fn hit(app: &Entity<QuillApp>, cx: &mut VisualTestContext, index: usize) -> MessageId {
    app.read_with(cx, |app, _| {
        app.session()
            .and_then(|session| session.search.chat_search.hits().get(index))
            .map(|hit| hit.message_id)
    })
    .expect("the demo chat has hits for \"e\"")
}

fn click(cx: &mut VisualTestContext, id: &'static str) {
    cx.update(|window, cx| window.click(id, cx));
    frames(cx);
}

fn escape(cx: &mut VisualTestContext) {
    cx.update(|window, cx| window.press("escape", cx));
    frames(cx);
}

#[gpui_kit::test]
fn a_new_search_lists_its_hits(cx: &mut TestAppContext) {
    let (app, mut cx) = new_app(cx);
    search(&app, &mut cx);
    assert!(app.read_with(&cx, |app, _| {
        app.session()
            .is_some_and(|session| session.search.chat_search.hits().len() > 1)
    }));
    // Landing on the first hit is the search's, not the user's pick.
    assert!(list_shown(&app, &mut cx));
}

#[gpui_kit::test]
fn picking_a_hit_folds_the_list_and_keeps_the_bar(cx: &mut TestAppContext) {
    let (app, mut cx) = new_app(cx);
    search(&app, &mut cx);
    let second = hit(&app, &mut cx, 1);
    app.update(&mut cx, |app, cx| app.jump_chat_search_message(second, cx));
    frames(&mut cx);
    assert!(!list_shown(&app, &mut cx));
    assert!(search_open(&app, &mut cx));
    let position = app.read_with(&cx, |app, _| {
        app.session()
            .map(|session| session.search.chat_search.position_label())
    });
    assert_eq!(position.as_deref(), Some("2 of 3"));
}

#[gpui_kit::test]
fn the_arrows_fold_the_list(cx: &mut TestAppContext) {
    let (app, mut cx) = new_app(cx);
    search(&app, &mut cx);
    click(&mut cx, "chat-search-older");
    assert!(!list_shown(&app, &mut cx));
    assert!(search_open(&app, &mut cx));
}

#[gpui_kit::test]
fn the_counter_brings_the_list_back(cx: &mut TestAppContext) {
    let (app, mut cx) = new_app(cx);
    search(&app, &mut cx);
    click(&mut cx, "chat-search-older");
    click(&mut cx, "chat-search-position");
    assert!(list_shown(&app, &mut cx));
    click(&mut cx, "chat-search-position");
    assert!(!list_shown(&app, &mut cx));
}

#[gpui_kit::test]
fn escape_folds_the_list_before_closing_the_search(cx: &mut TestAppContext) {
    let (app, mut cx) = new_app(cx);
    search(&app, &mut cx);
    escape(&mut cx);
    assert!(!list_shown(&app, &mut cx));
    assert!(search_open(&app, &mut cx));
    escape(&mut cx);
    assert!(!search_open(&app, &mut cx));
}
