//! UI integration tests for the Quick Replies dialog against the demo
//! `QuillApp` (gpui-kit's test support needs the `demo-capture` feature).
use super::View;
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, Focusable, TestAppContext, VisualTestContext, px, size};
use quill::quick_replies::{NAME_TAKEN_LINE, QuickRepliesUpdate, QuickReplyShortcut};

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

fn seed(app: &Entity<QuillApp>, cx: &mut VisualTestContext) {
    app.update(cx, |app, _| {
        let session = app.demo_session.as_mut().expect("demo session");
        for (id, name, count) in [(1, "hours", 2), (2, "hello", 1)] {
            session.apply_quick_replies(QuickRepliesUpdate::Shortcut(QuickReplyShortcut {
                id,
                name: name.into(),
                message_count: count,
            }));
        }
        session.apply_quick_replies(QuickRepliesUpdate::Order {
            shortcut_ids: vec![2, 1],
        });
    });
}

fn open(app: &Entity<QuillApp>, cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        app.update(cx, |app, cx| app.open_quick_replies(window, cx));
    });
    for _ in 0..2 {
        cx.update(|window, cx| window.render_frame(cx));
    }
    cx.run_until_parked();
}

fn view(app: &Entity<QuillApp>, cx: &mut VisualTestContext) -> View {
    app.read_with(cx, |app, _| {
        app.dialogs
            .quick_replies_dialog
            .as_ref()
            .unwrap()
            .view
            .clone()
    })
}

fn error(app: &Entity<QuillApp>, cx: &mut VisualTestContext) -> Option<String> {
    app.read_with(cx, |app, _| {
        app.dialogs
            .quick_replies_dialog
            .as_ref()
            .unwrap()
            .local_error
            .clone()
    })
}

fn type_name(app: &Entity<QuillApp>, cx: &mut VisualTestContext, name: &str) {
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            let input = app
                .dialogs
                .quick_replies_dialog
                .as_ref()
                .unwrap()
                .name_input
                .clone();
            input.update(cx, |input, cx| {
                input.set_value(name.to_string(), window, cx)
            });
        });
    });
}

fn submit_name(app: &Entity<QuillApp>, cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        app.update(cx, |app, cx| app.submit_quick_reply_name(window, cx));
    });
    cx.run_until_parked();
}

fn go(app: &Entity<QuillApp>, cx: &mut VisualTestContext, view: View) {
    cx.update(|window, cx| {
        app.update(cx, |app, cx| app.quick_replies_go(view, window, cx));
    });
    for _ in 0..2 {
        cx.update(|window, cx| window.render_frame(cx));
    }
    cx.run_until_parked();
}

#[gpui_kit::test]
fn the_dialog_opens_on_the_list_and_every_screen_renders(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    seed(&app, vcx);
    open(&app, vcx);
    assert_eq!(view(&app, vcx), View::List);
    go(&app, vcx, View::Name { rename_of: None });
    go(&app, vcx, View::Name { rename_of: Some(1) });
    go(
        &app,
        vcx,
        View::Shortcut {
            name: "hours".into(),
            id: Some(1),
        },
    );
    go(&app, vcx, View::List);
}

#[gpui_kit::test]
fn a_new_name_is_checked_before_anything_is_sent(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    seed(&app, vcx);
    open(&app, vcx);
    go(&app, vcx, View::Name { rename_of: None });

    submit_name(&app, vcx);
    assert_eq!(error(&app, vcx).as_deref(), Some("Enter a shortcut."));

    type_name(&app, vcx, "has space");
    submit_name(&app, vcx);
    assert!(error(&app, vcx).unwrap().contains("letters, digits"));

    // A fresh name opens an empty shortcut that exists only here.
    type_name(&app, vcx, "prices");
    submit_name(&app, vcx);
    assert_eq!(
        view(&app, vcx),
        View::Shortcut {
            name: "prices".into(),
            id: None
        }
    );
}

#[gpui_kit::test]
fn an_existing_name_opens_that_shortcut_and_a_rename_cannot_take_it(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    seed(&app, vcx);
    open(&app, vcx);
    go(&app, vcx, View::Name { rename_of: None });
    type_name(&app, vcx, "HOURS");
    submit_name(&app, vcx);
    assert_eq!(
        view(&app, vcx),
        View::Shortcut {
            name: "hours".into(),
            id: Some(1)
        }
    );

    go(&app, vcx, View::Name { rename_of: Some(1) });
    type_name(&app, vcx, "hello");
    submit_name(&app, vcx);
    assert_eq!(error(&app, vcx).as_deref(), Some(NAME_TAKEN_LINE));
}

#[gpui_kit::test]
fn adding_a_message_without_a_live_session_says_so(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    seed(&app, vcx);
    open(&app, vcx);
    go(
        &app,
        vcx,
        View::Shortcut {
            name: "hours".into(),
            id: Some(1),
        },
    );
    vcx.update(|window, cx| {
        app.update(cx, |app, cx| {
            let input = app
                .dialogs
                .quick_replies_dialog
                .as_ref()
                .unwrap()
                .text_input
                .clone();
            input.update(cx, |input, cx| input.set_value("Open at 9", window, cx));
            app.submit_quick_reply_text(window, cx);
        });
    });
    assert_eq!(
        error(&app, vcx).as_deref(),
        Some("Quick replies need a live session.")
    );
}
