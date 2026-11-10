//! A context menu closes when its message or chat leaves the session.

use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::menu_states::{ChatMenuState, MessageMenuState};
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, Focusable, TestAppContext, VisualTestContext, point, px, size};
use quill::ids::{ChatId, MessageId};

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

fn frames(vcx: &mut VisualTestContext) {
    for _ in 0..2 {
        vcx.update(|window, cx| window.render_frame(cx));
    }
}

#[gpui_kit::test]
fn message_menu_closes_when_its_message_is_removed(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    let (chat, message) = app.read_with(vcx, |app, _| {
        let session = app.session().expect("demo session");
        let open = session.open_chat.expect("the demo opens a chat");
        let history = session.histories.get(&open.0).expect("history");
        let id = *history.messages.keys().next().expect("a message");
        (open, MessageId(id))
    });
    app.update(vcx, |app, cx| {
        app.message_ui.menu = Some(MessageMenuState {
            chat_id: chat,
            message_id: message,
            position: point(px(400.), px(300.)),
        });
        cx.notify();
    });
    frames(vcx);
    assert!(app.read_with(vcx, |app, _| app.message_ui.menu.is_some()));
    app.update(vcx, |app, cx| {
        if let Some(history) = app.session_mut().and_then(|s| s.histories.get_mut(&chat.0)) {
            history.messages.remove(&message.0);
        }
        cx.notify();
    });
    frames(vcx);
    assert!(app.read_with(vcx, |app, _| app.message_ui.menu.is_none()));
}

#[gpui_kit::test]
fn chat_menu_closes_when_its_chat_is_removed(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    let chat = app.read_with(vcx, |app, _| {
        let session = app.session().expect("demo session");
        let open = session.open_chat.expect("the demo opens a chat");
        session
            .chats
            .keys()
            .find(|id| **id != open.0)
            .map(|id| ChatId(*id))
            .expect("another chat")
    });
    app.update(vcx, |app, cx| {
        app.chat_list.menu = Some(ChatMenuState::at(chat, point(px(100.), px(200.))));
        cx.notify();
    });
    frames(vcx);
    assert!(app.read_with(vcx, |app, _| app.chat_list.menu.is_some()));
    app.update(vcx, |app, cx| {
        if let Some(session) = app.session_mut() {
            session.chats.remove(&chat.0);
        }
        cx.notify();
    });
    frames(vcx);
    assert!(app.read_with(vcx, |app, _| app.chat_list.menu.is_none()));
}

fn other_chat(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> (ChatId, ChatId) {
    app.read_with(vcx, |app, _| {
        let session = app.session().expect("demo session");
        let open = session.open_chat.expect("the demo opens a chat");
        let other = session
            .chats
            .keys()
            .find(|id| **id != open.0)
            .map(|id| ChatId(*id))
            .expect("another chat");
        (open, other)
    })
}

#[gpui_kit::test]
fn a_pinned_list_open_for_a_chat_is_closed_when_another_chat_shows(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    let (open, other) = other_chat(&app, vcx);
    app.update(vcx, |app, cx| {
        // The list was left open in the chat that is no longer shown.
        app.set_pinned_list_open(other, true);
        assert!(app.history.pinned_list_open.is_some());
        // Closing the list of a chat that is not the open one leaves a
        // list open elsewhere alone.
        app.set_pinned_list_open(open, false);
        assert!(app.history.pinned_list_open.is_some());
        cx.notify();
    });
    frames(vcx);
    assert!(
        app.read_with(vcx, |app, _| app.history.pinned_list_open.is_none()),
        "the panel does not follow the user into another chat"
    );
}

#[gpui_kit::test]
fn the_inline_bot_warning_stash_stays_with_the_chat_that_asked(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    let (_, other) = other_chat(&app, vcx);
    app.update_in(vcx, |app, window, cx| {
        app.chat_ui.pending_inline_bot_alert = Some("query".into());
        app.select_listed_chat(other, window, cx);
    });
    frames(vcx);
    assert_eq!(
        app.read_with(vcx, |app, _| app.chat_ui.pending_inline_bot_alert.clone()),
        None,
        "another chat's composer never receives it"
    );
}

#[gpui_kit::test]
fn an_open_html_picker_ends_with_the_chat_its_selection_was_in(cx: &mut TestAppContext) {
    use crate::ui::selection_html::HtmlSave;
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    let (_, other) = other_chat(&app, vcx);
    app.update_in(vcx, |app, window, cx| {
        app.chat_ui.html_save = HtmlSave::Picking(gpui_kit::Task::ready(()));
        app.select_listed_chat(other, window, cx);
    });
    frames(vcx);
    assert!(
        app.read_with(vcx, |app, _| matches!(
            app.chat_ui.html_save,
            HtmlSave::Idle
        )),
        "the picker's stage went with the chat's value"
    );
}
