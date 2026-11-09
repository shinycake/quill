//! Each window shows its own kit dialog (`dialog_hosts`): a box stays in
//! the window it opened in, through a lock and an unlock, and closes with
//! that window instead of moving to the main one. A box asked for again
//! from another window moves there.
#![cfg(feature = "demo-capture")]

use super::tests::{Harness, OPEN, OTHER, new_app};
use crate::ui::shell::{DialogKind, QuillShell};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, TestAppContext};

fn draw(window: AnyWindowHandle, cx: &mut TestAppContext) {
    cx.update_window(window, |_, window, cx| window.render_frame(cx))
        .unwrap();
    cx.run_until_parked();
}

fn chat_window_shell(h: &Harness, cx: &mut TestAppContext) -> Entity<QuillShell> {
    let view = h
        .app
        .read_with(cx, |app, _| app.windows.view_for(OTHER))
        .expect("the chat window's view");
    view.read_with(cx, |view, _| view.shell.clone())
}

fn shown(shell: &Entity<QuillShell>, cx: &mut TestAppContext) -> Option<DialogKind> {
    shell.read_with(cx, |shell, _| shell.open_dialog)
}

/// The chat window's Mute box, asked for by its own input.
fn open_mute_in_chat_window(h: &Harness, chat_window: AnyWindowHandle, cx: &mut TestAppContext) {
    h.app.update(cx, |app, cx| {
        app.windows.last_input = Some(OTHER);
        app.in_window_scope(Some(OTHER), cx, |app, cx| app.open_mute_menu(cx));
    });
    draw(chat_window, cx);
    draw(h.main, cx);
}

#[gpui_kit::test]
fn a_box_shows_only_in_the_window_that_asked_for_it(cx: &mut TestAppContext) {
    let h = new_app(cx);
    let chat_window = h.open_in_new_window(OTHER, cx);
    open_mute_in_chat_window(&h, chat_window, cx);
    let in_chat_window = chat_window_shell(&h, cx);
    assert_eq!(shown(&in_chat_window, cx), Some(DialogKind::MuteMenu));
    assert_eq!(shown(&h.shell, cx), None, "the main window shows no box");
    // Input in the main window does not move it.
    h.app.update(cx, |app, _| app.windows.last_input = None);
    draw(h.main, cx);
    assert_eq!(shown(&h.shell, cx), None, "still not the main window's");
    assert_eq!(shown(&in_chat_window, cx), Some(DialogKind::MuteMenu));
}

/// Finding 17, the window close: the box goes with its window.
#[gpui_kit::test]
fn a_chat_windows_box_closes_with_it_and_does_not_move(cx: &mut TestAppContext) {
    let h = new_app(cx);
    let chat_window = h.open_in_new_window(OTHER, cx);
    open_mute_in_chat_window(&h, chat_window, cx);
    cx.update_window(chat_window, |_, window, _| window.remove_window())
        .unwrap();
    cx.run_until_parked();
    draw(h.main, cx);
    draw(h.main, cx);
    assert_eq!(
        shown(&h.shell, cx),
        None,
        "the box did not move to the main window"
    );
    h.app.read_with(cx, |app, _| {
        assert!(!app.notify.mute_menu_open, "its own close ran");
        assert!(app.notify.mute_chat.is_none());
    });
}

/// Finding 17, the unlock: typing the passcode in the main window does
/// not bring the chat window's box there.
#[gpui_kit::test]
fn a_chat_windows_box_stays_there_through_a_lock(cx: &mut TestAppContext) {
    let h = new_app(cx);
    let chat_window = h.open_in_new_window(OTHER, cx);
    open_mute_in_chat_window(&h, chat_window, cx);
    h.app.update(cx, |app, cx| {
        app.set_window_scope(None);
        app.set_locked(true, cx);
    });
    draw(h.main, cx);
    draw(chat_window, cx);
    // The passcode is typed in the main window.
    h.app.update(cx, |app, cx| {
        app.windows.last_input = None;
        app.set_locked(false, cx);
    });
    draw(h.main, cx);
    draw(chat_window, cx);
    assert_eq!(shown(&h.shell, cx), None, "the main window shows no box");
    let in_chat_window = chat_window_shell(&h, cx);
    assert_eq!(shown(&in_chat_window, cx), Some(DialogKind::MuteMenu));
}

/// Finding 53: New Topic in a chat window while the main window shows the
/// manage box for another forum. The box opens in the chat window, for its
/// forum; the main window's box does not turn into it.
#[gpui_kit::test]
fn new_topic_in_a_chat_window_opens_there(cx: &mut TestAppContext) {
    let h = new_app(cx);
    let chat_window = h.open_in_new_window(OTHER, cx);
    cx.update_window(h.main, |_, window, cx| {
        h.app.update(cx, |app, cx| {
            app.windows.last_input = None;
            app.set_window_scope(None);
            app.open_forum_manage_dialog(OPEN, window, cx);
        });
    })
    .unwrap();
    draw(h.main, cx);
    assert_eq!(shown(&h.shell, cx), Some(DialogKind::ForumManage));
    cx.update_window(chat_window, |_, window, cx| {
        h.app.update(cx, |app, cx| {
            app.windows.last_input = Some(OTHER);
            app.in_window_scope(Some(OTHER), cx, |app, cx| {
                app.open_forum_topic_editor(OTHER, None, window, cx);
            });
        });
    })
    .unwrap();
    draw(h.main, cx);
    draw(chat_window, cx);
    assert_eq!(shown(&h.shell, cx), None, "the main window's box left it");
    let in_chat_window = chat_window_shell(&h, cx);
    assert_eq!(shown(&in_chat_window, cx), Some(DialogKind::ForumManage));
    h.app.read_with(cx, |app, _| {
        let dialog = app.admin.forum_manage_dialog.as_ref().expect("the box");
        assert_eq!(dialog.chat_id, OTHER, "the chat window's forum");
        assert!(dialog.editor.is_some(), "on the new topic's editor");
    });
}

/// Finding 47, the other half: a Create Bot box whose chat window closes
/// does not move to the main window.
#[gpui_kit::test]
fn a_create_bot_box_closes_with_its_chat_window(cx: &mut TestAppContext) {
    let h = new_app(cx);
    let chat_window = h.open_in_new_window(OTHER, cx);
    cx.update_window(chat_window, |_, window, cx| {
        h.app.update(cx, |app, cx| {
            app.windows.last_input = Some(OTHER);
            app.in_window_scope(Some(OTHER), cx, |app, cx| {
                app.open_bot_create(
                    7,
                    "Helper".into(),
                    "helper_bot".into(),
                    false,
                    None,
                    window,
                    cx,
                );
            });
        });
    })
    .unwrap();
    draw(chat_window, cx);
    draw(h.main, cx);
    let in_chat_window = chat_window_shell(&h, cx);
    assert_eq!(shown(&in_chat_window, cx), Some(DialogKind::BotCreate));
    cx.update_window(chat_window, |_, window, _| window.remove_window())
        .unwrap();
    cx.run_until_parked();
    draw(h.main, cx);
    draw(h.main, cx);
    assert_eq!(shown(&h.shell, cx), None, "the box did not move");
    h.app.read_with(cx, |app, _| {
        assert!(app.dialogs.bot_create_dialog.is_none(), "its own close ran");
    });
}
