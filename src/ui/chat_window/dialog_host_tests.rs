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
    shell.read_with(cx, |shell, _| shell.open_dialog.kind())
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

/// A closer that holds only the app moves the value the window's shell
/// reads, with no mailbox for the shell to take on its next render.
#[gpui_kit::test]
fn an_app_side_close_moves_the_shells_own_value(cx: &mut TestAppContext) {
    let h = new_app(cx);
    let chat_window = h.open_in_new_window(OTHER, cx);
    open_mute_in_chat_window(&h, chat_window, cx);
    let in_chat_window = chat_window_shell(&h, cx);
    assert_eq!(shown(&in_chat_window, cx), Some(DialogKind::MuteMenu));
    h.app.read_with(cx, |app, _| {
        app.main_window.shown_dialogs.close(chat_window.window_id());
    });
    assert_eq!(shown(&in_chat_window, cx), None, "closed before any render");
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
        assert!(app.notify.mute_box.is_none(), "its own close ran");
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

/// A date lookup that failed says so in the window that asked, not in the
/// main window's toast host.
#[gpui_kit::test]
fn a_failed_date_lookup_toasts_in_the_window_that_asked(cx: &mut TestAppContext) {
    let h = new_app(cx);
    let _chat_window = h.open_in_new_window(OTHER, cx);
    h.app.update(cx, |app, _| {
        assert!(app.say_date_jumps_failed(vec![Some(OTHER)]));
        assert_eq!(
            app.connection.toasts.visible_host(),
            Some(Some(OTHER)),
            "the chat window's toast"
        );
        app.connection.toasts.clear_note();
        assert!(app.say_date_jumps_failed(vec![None]));
        assert_eq!(app.connection.toasts.visible_host(), Some(None));
        assert!(!app.say_date_jumps_failed(Vec::new()));
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
        });
        app.windows.open_box_in(
            DialogKind::GroupCallStart,
            Some(OTHER),
            crate::ui::chat_window::AskWaits::Demo,
        );
        let session = app.demo_session.as_mut().expect("a demo session");
        let mut asked = CreateJoinAs::asking(OTHER.0, request);
        asked.accept(Some(request), Vec::new());
        session.calls.create_join_as = Some(asked);
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
        assert!(session.payments.hub.limit_toast().is_some(), "a toast kind");
    });
    draw(h.main, cx);
    assert!(
        h.app.read_with(cx, |app, _| app.session().is_some_and(|s| s
            .payments
            .hub
            .limit_toast()
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

/// Sweep 13-009: an ask ends when the requests it was made with settle, not
/// when no request at all is in flight; a demo's ask never settles by itself.
#[test]
fn an_ask_settles_with_its_own_requests_only() {
    use super::AskWaits;
    use quill::ids::RequestId;
    let (first, second, later) = (RequestId(1), RequestId(2), RequestId(9));
    let waits = AskWaits::Requests(vec![first, second]);
    assert!(!waits.settled(|id| id == first), "one of its own is out");
    assert!(
        waits.settled(|id| id == later),
        "a request made after the ask does not hold it"
    );
    assert!(AskWaits::Requests(Vec::new()).settled(|_| true));
    assert!(!AskWaits::Demo.settled(|_| false));
    assert!(
        !AskWaits::Unread.settled(|_| false),
        "an ask whose requests are not read yet does not end"
    );
}

/// Sweep 13-009: a box is asked for before the request whose answer opens
/// it goes out. The ask reads its requests at the next dialog sync, so that
/// request is one of them and the ask is not over before the answer.
#[gpui_kit::test]
fn an_ask_made_before_its_request_waits_for_that_request(cx: &mut TestAppContext) {
    use super::AskWaits;
    use quill::ids::RequestId;
    let h = new_app(cx);
    let sent_after = RequestId(41);
    h.app.update(cx, |app, _| {
        let waits_of = |app: &crate::ui::app::QuillApp| {
            app.windows
                .asked_dialogs()
                .into_iter()
                .find(|(kind, _)| *kind == DialogKind::AccountNotice)
                .map(|(_, waits)| waits)
        };
        app.windows
            .open_box_in(DialogKind::AccountNotice, None, AskWaits::Unread);
        assert!(app.windows.has_unread_asks());
        // The asking code sends its request after the ask; the sync that
        // follows reads it.
        app.windows.read_ask_waits(&[sent_after]);
        assert!(!app.windows.has_unread_asks());
        let waits = waits_of(app).expect("the ask waits");
        assert_eq!(waits, AskWaits::Requests(vec![sent_after]));
        assert!(!waits.settled(|id| id == sent_after), "its request is out");
        assert!(waits.settled(|_| false), "its request settled");
        // An ask already read keeps its own requests.
        app.windows.read_ask_waits(&[RequestId(99)]);
        assert_eq!(waits_of(app), Some(AskWaits::Requests(vec![sent_after])));
    });
}

/// Sweep r4-07 round 4 (item 10): a closed chat window's value goes through
/// the one writer (`SlotFarewell for ChatUi`), so the acks its bar work
/// waits on are forgotten with it, and a late answer is kept for nobody.
#[gpui_kit::test]
fn closing_a_chat_window_forgets_the_acks_of_its_bar_work(cx: &mut TestAppContext) {
    use crate::ui::bar_report::{BarKind, BarWork};
    use quill::ids::RequestId;
    use quill::state::Answer;
    let h = new_app(cx);
    let chat_window = h.open_in_new_window(OTHER, cx);
    let request = RequestId(4_000_001);
    h.app.update(cx, |app, cx| {
        app.in_window_scope(Some(OTHER), cx, |app, _| {
            app.chat_ui.bar_work = BarWork::Out {
                kind: BarKind::Report,
                waiting: vec![request],
            };
            if let Some(session) = app.session_mut() {
                session.acks.expect(request);
            }
        })
    });
    cx.update_window(chat_window, |_, window, _| window.remove_window())
        .unwrap();
    cx.run_until_parked();
    h.app.update(cx, |app, _| {
        let session = app.session_mut().expect("a session");
        session.acks.settle(request, Answer::Done);
        assert_eq!(
            session.acks.take(request),
            None,
            "nothing waits for it any more"
        );
    });
}
