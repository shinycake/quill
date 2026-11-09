//! Locking Quill with a chat window open: each window shows the lock
//! screen with a field of its own, and the windows go idle.
#![cfg(feature = "demo-capture")]

use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, AppContext, Entity, Focusable, TestAppContext, px, size};
use quill::ids::ChatId;

const OTHER: ChatId = ChatId(12);

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, AnyWindowHandle) {
    new_app_on(cx, "ready-chats")
}

fn new_app_on(cx: &mut TestAppContext, demo: &'static str) -> (Entity<QuillApp>, AnyWindowHandle) {
    cx.update(gpui_kit::init);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(700.)), move |window, cx| {
        let view = cx
            .new(|cx| QuillApp::new_with_demo(window, cx, None, Some(ScreenshotDemo::named(demo))));
        let focus = view.focus_handle(cx);
        window.focus(&focus, cx);
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some(view);
        Root::new(shell, window, cx)
    });
    let app = slot.borrow().clone().expect("the app view");
    let main: AnyWindowHandle = handle.into();
    cx.run_until_parked();
    (app, main)
}

fn open_chat_window(
    app: &Entity<QuillApp>,
    main: AnyWindowHandle,
    cx: &mut TestAppContext,
) -> AnyWindowHandle {
    cx.update_window(main, |_, window, cx| {
        app.update(cx, |app, cx| app.open_chat_in_new_window(OTHER, window, cx));
    })
    .expect("the main window");
    cx.run_until_parked();
    cx.windows()
        .into_iter()
        .find(|handle| handle.window_id() != main.window_id())
        .expect("the chat window opened")
}

fn lock(app: &Entity<QuillApp>, cx: &mut TestAppContext) {
    app.update(cx, |app, cx| {
        app.set_window_scope(None);
        app.account.passcode.enabled = true;
        app.lock_by_passcode(cx);
    });
}

fn typed(app: &Entity<QuillApp>, scope: Option<ChatId>, cx: &mut TestAppContext) -> String {
    app.update(cx, |app, cx| {
        app.in_window_scope(scope, cx, |app, cx| {
            app.account.passcode.lock.input.read(cx).value().to_string()
        })
    })
    .expect("the window is open")
}

/// The windows sized apart is what made a shared field's paint look like
/// a layout change to the other window, every frame.
#[gpui_kit::test]
fn locking_with_a_chat_window_open_lets_both_windows_go_idle(cx: &mut TestAppContext) {
    let (app, main) = new_app(cx);
    let chat_window = open_chat_window(&app, main, cx);
    lock(&app, cx);
    cx.run_until_parked();
    for window in [main, chat_window] {
        cx.update_window(window, |_, window, cx| {
            window.render_frame(cx);
            assert!(
                window.try_find("lock-submit").is_some(),
                "every window shows the passcode box"
            );
        })
        .expect("the window");
        cx.run_until_parked();
    }
    app.read_with(cx, |app, _| assert!(app.account.passcode.is_locked()));
}

/// tdesktop's lock widget is per window: what one window's field holds
/// is not the other's.
#[gpui_kit::test]
fn each_window_types_into_its_own_lock_field(cx: &mut TestAppContext) {
    let (app, main) = new_app(cx);
    let chat_window = open_chat_window(&app, main, cx);
    lock(&app, cx);
    cx.run_until_parked();
    cx.update_window(chat_window, |_, window, cx| {
        app.update(cx, |app, cx| {
            app.in_window_scope(Some(OTHER), cx, |app, cx| {
                let input = app.account.passcode.lock.input.clone();
                input.update(cx, |input, cx| input.set_value("1234", window, cx));
            })
        })
    })
    .expect("the chat window");
    cx.run_until_parked();
    assert_eq!(typed(&app, Some(OTHER), cx), "1234");
    assert_eq!(
        typed(&app, None, cx),
        "",
        "the main window's field is its own"
    );
}

fn type_into(
    app: &Entity<QuillApp>,
    window: AnyWindowHandle,
    scope: Option<ChatId>,
    text: &'static str,
    cx: &mut TestAppContext,
) {
    cx.update_window(window, |_, window, cx| {
        app.update(cx, |app, cx| {
            app.in_window_scope(scope, cx, |app, cx| {
                let input = app.account.passcode.lock.input.clone();
                input.update(cx, |input, cx| input.set_value(text, window, cx));
            })
        })
    })
    .expect("the window");
}

/// The passcode check runs off the UI thread and answers later, with the
/// main window's state in place by then: a wrong passcode still empties
/// the field it was typed in, not the main window's.
#[gpui_kit::test]
fn a_wrong_passcode_empties_the_field_it_was_typed_in(cx: &mut TestAppContext) {
    let (app, main) = new_app(cx);
    let chat_window = open_chat_window(&app, main, cx);
    lock(&app, cx);
    cx.run_until_parked();
    type_into(&app, chat_window, Some(OTHER), "1234", cx);
    type_into(&app, main, None, "99", cx);
    cx.update_window(chat_window, |_, window, cx| {
        app.update(cx, |app, cx| {
            app.in_window_of(window, cx, |app, window, cx| {
                app.run_passcode_job(
                    || Err(quill::passcode::PasscodeError::Wrong),
                    QuillApp::unlock_answered,
                    window,
                    cx,
                );
            })
        })
    })
    .expect("the chat window");
    app.update(cx, |app, _| app.set_window_scope(None));
    cx.run_until_parked();
    assert_eq!(typed(&app, Some(OTHER), cx), "", "the wrong passcode went");
    assert_eq!(typed(&app, None, cx), "99", "the main window's text stays");
    app.read_with(cx, |app, _| {
        assert!(!app.account.passcode.job.is_running());
        assert!(app.account.passcode.lock_error.is_some());
    });
}

/// Unlocking in one window empties every window's field: what was typed
/// in the others does not wait there for the next lock.
#[gpui_kit::test]
fn unlocking_empties_every_windows_field(cx: &mut TestAppContext) {
    let (app, main) = new_app(cx);
    let chat_window = open_chat_window(&app, main, cx);
    lock(&app, cx);
    cx.run_until_parked();
    type_into(&app, chat_window, Some(OTHER), "1234", cx);
    type_into(&app, main, None, "99", cx);
    cx.update_window(main, |_, window, cx| {
        app.update(cx, |app, cx| {
            app.set_window_scope(None);
            app.unlock_answered(Ok(()), window, cx);
        })
    })
    .expect("the main window");
    cx.update_window(chat_window, |_, window, cx| window.render_frame(cx))
        .expect("the chat window");
    cx.run_until_parked();
    assert_eq!(typed(&app, None, cx), "");
    assert_eq!(
        typed(&app, Some(OTHER), cx),
        "",
        "emptied at its next frame"
    );
}

/// The lock reaches every window's media viewer: a chat window's viewer is
/// not back, showing its media, once the lock opens.
#[gpui_kit::test]
fn locking_closes_a_chat_windows_media_viewer(cx: &mut TestAppContext) {
    let (app, main) = new_app_on(cx, "ready-video-inline");
    let _chat_window = open_chat_window(&app, main, cx);
    let opened = app.update(cx, |app, cx| {
        app.in_window_scope(Some(OTHER), cx, |app, cx| {
            app.open_media_viewer(ChatId(11), quill::ids::MessageId(204), cx);
            app.viewer.state.is_open()
        })
    });
    assert_eq!(opened, Some(true), "the chat window's viewer opened");
    lock(&app, cx);
    cx.run_until_parked();
    let open = app.update(cx, |app, cx| {
        app.in_window_scope(Some(OTHER), cx, |app, _| app.viewer.state.is_open())
    });
    assert_eq!(open, Some(false));
}

/// A Touch ID answer that comes back after the chat window that asked
/// closed still ends the wait: the lock can be opened again.
#[gpui_kit::test]
fn a_system_unlock_answered_after_its_window_closed_ends_the_wait(cx: &mut TestAppContext) {
    let (app, main) = new_app(cx);
    let chat_window = open_chat_window(&app, main, cx);
    lock(&app, cx);
    cx.run_until_parked();
    let (answer, receiver) = std::sync::mpsc::channel();
    let _ = answer.send(false);
    cx.update_window(chat_window, |_, window, cx| {
        app.update(cx, |app, cx| {
            app.in_window_scope(Some(OTHER), cx, |app, cx| {
                app.account.passcode.job = crate::ui::passcode::PasscodeJob::Running;
                let master = quill::passcode::MasterKey::from_bytes([7; 32]);
                app.spawn_system_unlock_wait(receiver, master, window, cx);
            });
        });
        window.remove_window();
    })
    .expect("the chat window");
    cx.run_until_parked();
    app.read_with(cx, |app, _| {
        assert!(
            !app.account.passcode.job.is_running(),
            "the lock takes a passcode again"
        );
        assert!(app.account.passcode.is_locked());
    });
}

/// The lock stops a chat window's inline clips too: that window runs no
/// frame upkeep while locked, so a round video there kept its sound.
#[gpui_kit::test]
fn locking_stops_a_chat_windows_round_video(cx: &mut TestAppContext) {
    let (app, main) = new_app(cx);
    let _chat_window = open_chat_window(&app, main, cx);
    let key = (OTHER.0, 77);
    app.update(cx, |app, _| {
        app.players.inline_videos.request(
            Some(OTHER),
            key,
            crate::ui::inline_players::Demand::Sound,
        );
    });
    lock(&app, cx);
    cx.run_until_parked();
    assert_eq!(
        app.read_with(cx, |app, _| app.players.inline_videos.demand(key)),
        None,
        "the chat window's clip left"
    );
}

/// An open asked under the lock runs after the unlock as the window that
/// asked, not the one the passcode was typed in, and only while its
/// account is still on screen.
#[gpui_kit::test]
fn an_unlock_intent_runs_as_its_window_and_only_for_its_account(cx: &mut TestAppContext) {
    let (app, main) = new_app(cx);
    let _chat_window = open_chat_window(&app, main, cx);
    lock(&app, cx);
    cx.run_until_parked();
    let ran = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let (first, second) = (ran.clone(), ran.clone());
    app.update(cx, |app, cx| {
        app.in_window_scope(Some(OTHER), cx, |app, _| {
            app.wait_for_unlock(move |app, _| first.borrow_mut().push(app.asking_window_or_last_press()));
        });
    });
    cx.update_window(main, |_, window, cx| {
        app.update(cx, |app, cx| {
            app.set_window_scope(None);
            app.unlock_answered(Ok(()), window, cx);
        })
    })
    .expect("the main window");
    cx.run_until_parked();
    assert_eq!(
        *ran.borrow(),
        vec![Some(OTHER)],
        "it ran as the chat window"
    );
    // Asked for one account; another came on screen before the unlock.
    lock(&app, cx);
    cx.run_until_parked();
    app.update(cx, |app, _| {
        app.wait_for_unlock(move |app, _| second.borrow_mut().push(app.asking_window_or_last_press()));
        if let Some(session) = app.demo_session.as_deref_mut() {
            session.account = quill::ids::AccountKey::new("account-2").expect("a key");
        }
    });
    cx.update_window(main, |_, window, cx| {
        app.update(cx, |app, cx| app.unlock_answered(Ok(()), window, cx))
    })
    .expect("the main window");
    cx.run_until_parked();
    assert_eq!(ran.borrow().len(), 1, "dropped for the other account");
}
