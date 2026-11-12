//! The send box belongs to the window whose chat holds the files: picked
//! in a chat's own window, it shows there and the main window shows none.
#![cfg(feature = "demo-capture")]

use super::tests::{OTHER, new_app};
use crate::ui::send_box_tests::fixture;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, TestAppContext};
use quill::composer::{AttachmentKind, ComposerAttachment};

#[gpui_kit::test]
fn files_picked_in_a_chat_window_show_their_box_in_that_window_only(cx: &mut TestAppContext) {
    let h = new_app(cx);
    let chat_window = h.open_in_new_window(OTHER, cx);
    let view = h
        .app
        .read_with(cx, |app, _| app.windows.view_for(OTHER))
        .expect("view");
    let window_shell = view.read_with(cx, |view, _| view.shell.clone());
    let app = h.app.clone();
    cx.update_window(chat_window, |_, window, cx| {
        app.update(cx, |app, cx| {
            app.in_window_of(window, cx, |app, _, cx| {
                let photo =
                    ComposerAttachment::pick(&fixture("demo-thumb.png"), AttachmentKind::Photo)
                        .expect("fixture");
                app.update_pending_attachments(|tray| tray.push(photo))
                    .expect("the window's chat");
                assert!(app.send_box_shown(), "the window's tray holds a file");
                cx.notify();
            });
        });
    })
    .unwrap();
    for window in [h.main, chat_window, h.main] {
        cx.update_window(window, |_, window, cx| window.render_frame(cx))
            .unwrap();
        cx.run_until_parked();
    }
    assert!(
        window_shell.read_with(cx, |shell, _| shell.dialog_open()),
        "the chat window shows the box"
    );
    assert!(
        !h.shell.read_with(cx, |shell, _| shell.dialog_open()),
        "the main window does not"
    );
    h.settle(cx);
    h.app.read_with(cx, |app, _| {
        assert!(app.pending_attachments().is_empty());
        assert!(
            !app.send_box_shown(),
            "the main window's chat holds nothing"
        );
    });

    // Cancel in the window drops its files and closes its box.
    cx.update_window(chat_window, |_, window, cx| {
        app.update(cx, |app, cx| {
            app.in_window_of(window, cx, |app, window, cx| {
                app.cancel_send_box(window, cx)
            });
        });
    })
    .unwrap();
    cx.update_window(chat_window, |_, window, cx| window.render_frame(cx))
        .unwrap();
    assert!(!window_shell.read_with(cx, |shell, _| shell.dialog_open()));
}
