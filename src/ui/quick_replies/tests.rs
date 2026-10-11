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

/// Open a private chat with a person and type `/` in the composer.
fn type_slash(app: &Entity<QuillApp>, cx: &mut VisualTestContext, text: &str) {
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            let session = app.demo_session.as_mut().expect("demo session");
            let chat = session.open_chat.expect("open chat");
            let user_id = quill::ids::UserId(chat.0);
            session.chats.get_mut(&chat.0).unwrap().kind =
                quill::telegram::envelope::ChatKind::Private { user_id };
            app.composer.update(cx, |input, cx| {
                input.set_value(text.to_string(), window, cx)
            });
            app.sync_command_menu(cx);
        });
    });
    cx.run_until_parked();
}

#[gpui_kit::test]
fn the_slash_list_starts_below_the_edit_row_and_tab_inserts(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    seed(&app, vcx);
    type_slash(&app, vcx, "/");
    let (items, selected) = app.update(vcx, |app, cx| {
        (
            app.command_menu_state(cx).expect("menu is open").1,
            app.composer_ui.command_menu_selected,
        )
    });
    assert_eq!(items[0].quick_reply, Some(quill::quick_replies::EDIT_ROW));
    assert_eq!(items[1].command, "hello");
    assert_eq!(selected, 1);

    vcx.update(|window, cx| {
        app.update(cx, |app, cx| {
            assert!(app.insert_command_menu_selection(window, cx));
        });
    });
    let text = app.read_with(vcx, |app, cx| app.composer.read(cx).value().to_string());
    assert_eq!(text, "/hello ");
}

#[gpui_kit::test]
fn a_prefix_filters_the_shortcuts_and_the_edit_row_stays(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    seed(&app, vcx);
    type_slash(&app, vcx, "/ho");
    let items = app.update(vcx, |app, cx| app.command_menu_state(cx).unwrap().1);
    let names: Vec<_> = items.iter().map(|i| i.command.as_str()).collect();
    assert_eq!(names, ["", "hours"]);
}

#[gpui_kit::test]
fn the_edit_row_opens_the_dialog_and_clears_the_slash(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    seed(&app, vcx);
    type_slash(&app, vcx, "/");
    vcx.update(|window, cx| {
        app.update(cx, |app, cx| app.send_command_menu_index(0, window, cx));
    });
    vcx.run_until_parked();
    assert!(app.read_with(vcx, |app, _| app.dialogs.quick_replies_dialog.is_some()));
    let text = app.read_with(vcx, |app, cx| app.composer.read(cx).value().to_string());
    assert!(text.is_empty());
}
