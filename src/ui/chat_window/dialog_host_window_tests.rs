//! Round 13 (area B): a box belongs to the window that opened it, from the
//! moment it opens (`dialog_hosts`).
//!
//! - A session-wide box asked for again from another window moves there,
//!   for that window's target (finding 1).
//! - A per-window box shows in each window that has its own flag, and a
//!   closing window closes only its own (finding 2).
//! - A box opened by an answer shows in the window that asked, wherever
//!   the pointer went meanwhile (finding 3).
#![cfg(feature = "demo-capture")]

use super::tests::{Harness, OTHER, new_app};
use crate::ui::shell::{DialogKind, QuillShell};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, TestAppContext};

fn draw(window: AnyWindowHandle, cx: &mut TestAppContext) {
    cx.update_window(window, |_, window, cx| window.render_frame(cx))
        .unwrap();
    cx.run_until_parked();
}

fn draw_both(h: &Harness, chat_window: AnyWindowHandle, cx: &mut TestAppContext) {
    for _ in 0..2 {
        draw(h.main, cx);
        draw(chat_window, cx);
    }
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

/// Close the chat window from inside it (its title bar close), so its own
/// window runs its boxes' closes.
fn close_from_inside(h: &Harness, chat_window: AnyWindowHandle, cx: &mut TestAppContext) {
    let view = h
        .app
        .read_with(cx, |app, _| app.windows.view_for(OTHER))
        .expect("the chat window's view");
    cx.update_window(chat_window, |_, window, cx| {
        view.update(cx, |view, cx| view.close(window, cx));
    })
    .unwrap();
    cx.run_until_parked();
}

/// Finding 1: Mute in the main window, then Mute in the chat window. The
/// box moves to the chat window, for its chat; the main window's box does
/// not turn into the other chat's.
#[gpui_kit::test]
fn mute_asked_again_from_a_chat_window_moves_there(cx: &mut TestAppContext) {
    let h = new_app(cx);
    let chat_window = h.open_in_new_window(OTHER, cx);
    h.app.update(cx, |app, cx| {
        app.note_window_press(None);
        app.open_mute_menu(cx);
    });
    draw_both(&h, chat_window, cx);
    assert_eq!(shown(&h.shell, cx), Some(DialogKind::MuteMenu));
    let in_chat_window = chat_window_shell(&h, cx);
    assert_eq!(shown(&in_chat_window, cx), None);
    h.app.update(cx, |app, cx| {
        app.note_window_press(Some(OTHER));
        app.open_mute_menu(cx);
    });
    draw_both(&h, chat_window, cx);
    assert_eq!(shown(&h.shell, cx), None, "the main window's box left it");
    assert_eq!(
        shown(&in_chat_window, cx),
        Some(DialogKind::MuteMenu),
        "the box shows where it was asked for"
    );
    h.app.read_with(cx, |app, _| {
        assert_eq!(
            app.notify.mute_chat,
            Some(OTHER),
            "for the asking window's chat"
        );
    });
}

/// Finding 1, from the other side: a box in a chat window, asked for again
/// in the main window, comes back to the main window. An open made with a
/// window's state in place names it whatever the last press was.
#[gpui_kit::test]
fn an_open_names_the_window_in_place_not_the_last_press(cx: &mut TestAppContext) {
    let h = new_app(cx);
    let chat_window = h.open_in_new_window(OTHER, cx);
    h.app.update(cx, |app, cx| {
        app.note_window_press(Some(OTHER));
        app.open_mute_menu(cx);
    });
    draw_both(&h, chat_window, cx);
    let in_chat_window = chat_window_shell(&h, cx);
    assert_eq!(shown(&in_chat_window, cx), Some(DialogKind::MuteMenu));
    // The main window's open runs with its state in place, while the last
    // press is still the chat window's.
    h.app.update(cx, |app, cx| {
        app.set_window_scope(None);
        app.open_mute_menu(cx);
    });
    draw_both(&h, chat_window, cx);
    assert_eq!(shown(&h.shell, cx), Some(DialogKind::MuteMenu));
    assert_eq!(shown(&in_chat_window, cx), None);
}

/// Finding 2: a per-window box in the main window does not hold back the
/// same kind in a chat window; each shows its own at once.
#[gpui_kit::test]
fn each_window_shows_its_own_per_window_box(cx: &mut TestAppContext) {
    let h = new_app(cx);
    let chat_window = h.open_in_new_window(OTHER, cx);
    h.app.update(cx, |app, _| {
        app.set_window_scope(None);
        app.chat_ui.scheduled_dialog_open = true;
    });
    draw_both(&h, chat_window, cx);
    assert_eq!(shown(&h.shell, cx), Some(DialogKind::Scheduled));
    let in_chat_window = chat_window_shell(&h, cx);
    assert_eq!(shown(&in_chat_window, cx), None, "its own flag is clear");
    h.app.update(cx, |app, cx| {
        app.in_window_scope(Some(OTHER), cx, |app, _| {
            app.chat_ui.scheduled_dialog_open = true;
        });
    });
    draw_both(&h, chat_window, cx);
    assert_eq!(shown(&h.shell, cx), Some(DialogKind::Scheduled));
    assert_eq!(
        shown(&in_chat_window, cx),
        Some(DialogKind::Scheduled),
        "the chat window shows its own box beside the main window's"
    );
}

/// Finding 2, the close: a chat window that closes runs its own boxes'
/// closes against its own state. The main window's box of the same kind,
/// and its flag, stay.
#[gpui_kit::test]
fn a_closing_window_closes_only_its_own_boxes(cx: &mut TestAppContext) {
    let h = new_app(cx);
    let chat_window = h.open_in_new_window(OTHER, cx);
    h.app.update(cx, |app, cx| {
        app.set_window_scope(None);
        app.chat_ui.scheduled_dialog_open = true;
        app.in_window_scope(Some(OTHER), cx, |app, _| {
            app.chat_ui.scheduled_dialog_open = true;
        });
    });
    draw_both(&h, chat_window, cx);
    close_from_inside(&h, chat_window, cx);
    draw(h.main, cx);
    draw(h.main, cx);
    assert_eq!(shown(&h.shell, cx), Some(DialogKind::Scheduled));
    h.app.update(cx, |app, _| {
        app.set_window_scope(None);
        assert!(
            app.chat_ui.scheduled_dialog_open,
            "the main window's own box was not closed"
        );
    });
}

/// The close from inside also closes the session-wide box the window
/// showed, with its own close, and it does not move to the main window.
#[gpui_kit::test]
fn a_session_box_closes_with_the_window_that_shows_it(cx: &mut TestAppContext) {
    let h = new_app(cx);
    let chat_window = h.open_in_new_window(OTHER, cx);
    h.app.update(cx, |app, cx| {
        app.note_window_press(Some(OTHER));
        app.open_mute_menu(cx);
    });
    draw_both(&h, chat_window, cx);
    close_from_inside(&h, chat_window, cx);
    h.app.read_with(cx, |app, _| {
        assert!(!app.notify.mute_menu_open, "its own close ran at once");
    });
    draw(h.main, cx);
    assert_eq!(shown(&h.shell, cx), None);
}

/// Finding 3: the window that asks names the box before its answer comes.
/// The pointer then crosses the main window (and an inactive window gets
/// moves too); the answer, applied with the main window's state in place,
/// still shows in the window that asked.
#[gpui_kit::test]
fn an_answer_shows_in_the_window_that_asked(cx: &mut TestAppContext) {
    let h = new_app(cx);
    let chat_window = h.open_in_new_window(OTHER, cx);
    h.app.update(cx, |app, cx| {
        app.note_window_press(Some(OTHER));
        app.in_window_scope(Some(OTHER), cx, |app, _| {
            app.open_box(DialogKind::DeepLinkInfo);
        });
        app.note_window_pointer(None);
        // The answer, from the poll loop.
        app.set_window_scope(None);
        app.links.deep_link_dialog = Some("TDLib's text".into());
    });
    draw_both(&h, chat_window, cx);
    assert_eq!(shown(&h.shell, cx), None, "not where the pointer went");
    let in_chat_window = chat_window_shell(&h, cx);
    assert_eq!(shown(&in_chat_window, cx), Some(DialogKind::DeepLinkInfo));
}

/// Finding 3: pointer moves and scrolls are not input for the boxes. An
/// open no path named shows in the window of the last press, not the one
/// the pointer last moved over.
#[gpui_kit::test]
fn a_pointer_move_is_not_a_press(cx: &mut TestAppContext) {
    let h = new_app(cx);
    let chat_window = h.open_in_new_window(OTHER, cx);
    h.app.update(cx, |app, _| {
        app.note_window_press(None);
        app.note_window_pointer(Some(OTHER));
        assert_eq!(app.windows.last_press, None);
        assert_eq!(app.connection.note_window, None, "toasts stay too");
        app.set_window_scope(None);
        app.links.deep_link_dialog = Some("TDLib's text".into());
    });
    draw_both(&h, chat_window, cx);
    assert_eq!(shown(&h.shell, cx), Some(DialogKind::DeepLinkInfo));
    let in_chat_window = chat_window_shell(&h, cx);
    assert_eq!(shown(&in_chat_window, cx), None);
}

/// Finding 3: a typed link resolved for a chat window runs in the main
/// window's frame, and the box it opens is still the chat window's.
#[gpui_kit::test]
fn a_typed_link_opens_its_box_in_the_window_it_was_clicked_in(cx: &mut TestAppContext) {
    let h = new_app(cx);
    let chat_window = h.open_in_new_window(OTHER, cx);
    h.app.update(cx, |app, _| {
        app.note_window_pointer(None);
        app.set_window_scope(None);
        app.links.pending_deep_link_ui = Some(quill::deep_link_types::DeepLinkUi::ConferenceCall {
            link: "https://t.me/call/abc".into(),
        });
        app.links.pending_flow_window = Some(OTHER);
    });
    let app = h.app.clone();
    cx.update_window(h.main, |_, window, cx| {
        app.update(cx, |app, cx| app.run_resolved_deep_link(window, cx));
    })
    .unwrap();
    draw_both(&h, chat_window, cx);
    assert_eq!(shown(&h.shell, cx), None);
    let in_chat_window = chat_window_shell(&h, cx);
    assert_eq!(shown(&in_chat_window, cx), Some(DialogKind::ConferenceLink));
}
