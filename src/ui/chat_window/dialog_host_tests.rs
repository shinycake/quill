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
        app.windows.last_press = Some(OTHER);
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
    h.app.update(cx, |app, _| app.windows.last_press = None);
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
        app.windows.last_press = None;
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
            app.windows.last_press = None;
            app.set_window_scope(None);
            app.open_forum_manage_dialog(OPEN, window, cx);
        });
    })
    .unwrap();
    draw(h.main, cx);
    assert_eq!(shown(&h.shell, cx), Some(DialogKind::ForumManage));
    cx.update_window(chat_window, |_, window, cx| {
        h.app.update(cx, |app, cx| {
            app.windows.last_press = Some(OTHER);
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

/// The same forum's manage box open in the main window: New Topic in the
/// chat window for that forum moves the box there too.
#[gpui_kit::test]
fn new_topic_for_the_main_windows_forum_opens_in_the_chat_window(cx: &mut TestAppContext) {
    let h = new_app(cx);
    let chat_window = h.open_in_new_window(OTHER, cx);
    cx.update_window(h.main, |_, window, cx| {
        h.app.update(cx, |app, cx| {
            app.windows.last_press = None;
            app.set_window_scope(None);
            app.open_forum_manage_dialog(OTHER, window, cx);
        });
    })
    .unwrap();
    draw(h.main, cx);
    assert_eq!(shown(&h.shell, cx), Some(DialogKind::ForumManage));
    cx.update_window(chat_window, |_, window, cx| {
        h.app.update(cx, |app, cx| {
            app.windows.last_press = Some(OTHER);
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
}

/// Finding 47, the other half: a Create Bot box whose chat window closes
/// does not move to the main window.
#[gpui_kit::test]
fn a_create_bot_box_closes_with_its_chat_window(cx: &mut TestAppContext) {
    let h = new_app(cx);
    let chat_window = h.open_in_new_window(OTHER, cx);
    cx.update_window(chat_window, |_, window, cx| {
        h.app.update(cx, |app, cx| {
            app.windows.last_press = Some(OTHER);
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

/// Round 16 (area B, finding 4): a flow that carried a chat window which
/// has closed since (a mini app opened from it, a file picker's answer)
/// asks as the main window. Its toast shows there, at once, instead of
/// queuing for a window that never draws again and holding up the toasts
/// behind it.
#[gpui_kit::test]
fn a_closed_windows_flow_asks_as_the_main_window(cx: &mut TestAppContext) {
    use crate::ui::status_note::StatusNote;
    let h = new_app(cx);
    let chat_window = h.open_in_new_window(OTHER, cx);
    h.app.update(cx, |app, cx| {
        app.windows.last_press = Some(OTHER);
        app.in_window_scope(Some(OTHER), cx, |app, cx| app.ensure_mini_app_poll(cx));
    });
    cx.update_window(chat_window, |_, window, _| window.remove_window())
        .unwrap();
    cx.run_until_parked();
    h.app.update(cx, |app, _| {
        assert_eq!(
            app.as_asked_by(Some(OTHER), |app| app.asking_window()),
            Some(None),
            "the closed window asks for nothing"
        );
        assert_eq!(app.mini_app_window(), None, "the mini app forgot it");
        app.as_asked_by(Some(OTHER), |app| {
            app.set_note(StatusNote::info("Added to the attachment menu"))
        });
        assert_eq!(
            app.connection.toasts.visible_host(),
            Some(None),
            "the toast shows in the main window"
        );
    });
}

/// Round 16 (area B, finding 6): "Start video chat" pressed in a chat
/// window's header goes on in that window's frames. The "join as" answer
/// moves the box past its hidden waiting step with only the chat window
/// drawing (the main window minimized), and the main window's frames do
/// not take it.
#[gpui_kit::test]
fn a_chat_windows_video_chat_start_goes_on_in_its_own_frames(cx: &mut TestAppContext) {
    use crate::ui::dialogs::group_call_create::{CreateStep, GroupCallCreate};
    use quill::calls::group_call_create::{CreateJoinAs, CreateKind};
    use quill::ids::RequestId;
    let h = new_app(cx);
    let chat_window = h.open_in_new_window(OTHER, cx);
    let request = RequestId(77);
    let waiting = |app: &crate::ui::app::QuillApp| {
        matches!(
            app.group_call_boxes
                .start_dialog
                .as_ref()
                .map(|flow| &flow.step),
            Some(CreateStep::Waiting(_))
        )
    };
    h.app.update(cx, |app, _| {
        app.group_call_boxes.start_dialog = Some(GroupCallCreate {
            chat_id: OTHER,
            kind: CreateKind::Now,
            step: CreateStep::Waiting(request),
            join_as: None,
        });
        app.windows
            .open_box_in(DialogKind::GroupCallStart, Some(OTHER));
        let session = app.demo_session.as_mut().expect("a demo session");
        session.calls.create_join_as = Some(CreateJoinAs {
            chat_id: OTHER.0,
            request,
            answer: Some(Ok(Vec::new())),
        });
    });
    draw(h.main, cx);
    assert!(
        h.app.read_with(cx, |app, _| waiting(app)),
        "the main window's frame leaves it"
    );
    draw(chat_window, cx);
    h.app.read_with(cx, |app, _| {
        assert!(!waiting(app), "the chat window's frame took the answer");
    });
}

/// Round 16 (area B, finding 11): a Premium limit's toast (a full GIF
/// list) shows in the window whose action hit the limit, from its own
/// frame, not behind it in the main window.
#[gpui_kit::test]
fn a_premium_limit_toast_shows_in_the_window_that_hit_it(cx: &mut TestAppContext) {
    use quill::premium_limits::PremiumLimitKind;
    let h = new_app(cx);
    let chat_window = h.open_in_new_window(OTHER, cx);
    let toasts = |window: AnyWindowHandle, cx: &mut TestAppContext| {
        cx.update_window(window, |_, window, cx| {
            gpui_kit::component::WindowExt::notifications(window, cx).len()
        })
        .unwrap()
    };
    let (main_before, chat_before) = (toasts(h.main, cx), toasts(chat_window, cx));
    h.app.update(cx, |app, _| {
        app.windows.last_press = Some(OTHER);
        let session = app.demo_session.as_mut().expect("a demo session");
        session.open_premium_limit(PremiumLimitKind::SavedGifs, 0);
        assert!(session.payments.hub.limit_toast.is_some(), "a toast kind");
    });
    draw(h.main, cx);
    assert!(
        h.app.read_with(cx, |app, _| app.session().is_some_and(|s| s
            .payments
            .hub
            .limit_toast
            .is_some())),
        "the main window's frame leaves it"
    );
    draw(chat_window, cx);
    draw(chat_window, cx);
    assert_eq!(toasts(h.main, cx), main_before);
    assert_eq!(toasts(chat_window, cx), chat_before + 1);
}

/// The queued account notices are about the account, not the window that
/// showed one: closing that window frees the notice for the next window to
/// show instead of handing it to the main window's sync to dismiss unseen,
/// while an ordinary box of the window is still handed over to close.
#[gpui_kit::test]
fn an_account_notice_is_freed_by_its_windows_close_not_dismissed(cx: &mut TestAppContext) {
    let h = new_app(cx);
    h.app.update(cx, |app, _| {
        app.windows
            .dialog_opened_in(DialogKind::AccountNotice, Some(OTHER));
        app.windows
            .dialog_opened_in(DialogKind::MuteMenu, Some(OTHER));
        app.windows.dialogs_window_closed(OTHER);
        let left = app.windows.take_left_dialogs();
        assert!(
            !left.contains(&DialogKind::AccountNotice),
            "never closed unseen"
        );
        assert!(
            left.contains(&DialogKind::MuteMenu),
            "an ordinary box closes"
        );
        assert_eq!(
            app.windows.window_of_box(DialogKind::AccountNotice),
            app.windows.last_press,
            "free: it shows in the last-press window"
        );
    });
}
