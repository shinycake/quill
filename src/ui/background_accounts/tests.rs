//! Background accounts in the window: the account list badge, and what a
//! click on another account's notification does once it reaches the
//! render (needs the `demo-capture` feature for gpui-kit's test support).
use super::{AccountUnread, BackgroundClient, BackgroundStatus, PendingOpen};
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::AppContext;
use gpui_kit::component::Root;
use gpui_kit::{Entity, TestAppContext, VisualTestContext, px, size};
use quill::ids::{AccountKey, ChatId};
use quill::notify::NotificationAction;
use std::time::{Duration, Instant};

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(800.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(window, cx, None, Some(ScreenshotDemo::named("ready-chats")))
        });
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some(view);
        Root::new(shell, window, cx)
    });
    let app = slot.borrow().clone().unwrap();
    let vcx = VisualTestContext::from_window(handle.into(), cx);
    (app, vcx)
}

/// A demo chat that is not the open one.
fn closed_chat(session: &quill::state::Session) -> ChatId {
    *session
        .main_order
        .iter()
        .find(|chat| Some(**chat) != session.open_chat)
        .expect("demo chats")
}

fn other_account() -> AccountKey {
    AccountKey::new("account-1").unwrap()
}

fn client(status: BackgroundStatus, count: u32) -> BackgroundClient {
    BackgroundClient {
        key: other_account(),
        live: None,
        status,
        unread: AccountUnread {
            count,
            muted: false,
        },
    }
}

#[gpui_kit::test]
fn an_account_row_shows_the_unread_count_or_the_status(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    app.update_in(&mut vcx, |app, _, cx| {
        let key = other_account();
        assert!(app.background_account_badge(&key, cx).is_none(), "unknown");
        app.background
            .clients
            .push(client(BackgroundStatus::Ready, 0));
        assert!(app.background_account_badge(&key, cx).is_none(), "all read");
        app.background.clients[0].unread.count = 3;
        assert!(app.background_account_badge(&key, cx).is_some());
        app.background.clients[0].status = BackgroundStatus::NeedsSignIn;
        assert!(app.background_account_badge(&key, cx).is_some());
        assert_eq!(app.background.total_unread().count, 3);
    });
}

#[gpui_kit::test]
fn a_pending_open_opens_the_chat_once_its_account_is_on_screen(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let (account, chat_id) = app.read_with(&vcx, |app, _| {
        let session = app.session().expect("demo session");
        (session.account.clone(), closed_chat(session))
    });
    app.update_in(&mut vcx, |app, _, cx| {
        app.background.pending_open = Some(PendingOpen {
            account: account.clone(),
            chat_id,
            action: NotificationAction::Open,
            until: Instant::now() + Duration::from_secs(30),
        });
        cx.notify();
    });
    vcx.run_until_parked();
    app.read_with(&vcx, |app, _| {
        assert!(app.background.pending_open.is_none());
        assert_eq!(app.session().and_then(|s| s.open_chat), Some(chat_id));
    });
}

#[gpui_kit::test]
fn a_pending_open_for_another_account_or_too_late_is_dropped(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let chat_id = app.read_with(&vcx, |app, _| closed_chat(app.session().unwrap()));
    for (account, until) in [
        (other_account(), Instant::now() + Duration::from_secs(30)),
        (
            app.read_with(&vcx, |app, _| app.session().unwrap().account.clone()),
            Instant::now() - Duration::from_secs(1),
        ),
    ] {
        app.update_in(&mut vcx, |app, _, cx| {
            app.background.pending_open = Some(PendingOpen {
                account,
                chat_id,
                action: NotificationAction::Open,
                until,
            });
            cx.notify();
        });
        vcx.run_until_parked();
        app.read_with(&vcx, |app, _| {
            assert!(app.background.pending_open.is_none());
            assert_ne!(app.session().and_then(|s| s.open_chat), Some(chat_id));
        });
    }
}

#[gpui_kit::test]
fn mark_as_read_on_another_account_never_switches(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let before = app.read_with(&vcx, |app, _| app.session().unwrap().account.clone());
    app.update_in(&mut vcx, |app, _, cx| {
        app.background
            .clients
            .push(client(BackgroundStatus::Ready, 1));
        app.background.clicks.lock().unwrap().push((
            other_account(),
            ChatId(7),
            NotificationAction::MarkRead,
        ));
        app.drain_account_clicks(cx);
        assert!(app.background.clicks.lock().unwrap().is_empty());
        assert!(app.background.pending_open.is_none());
    });
    vcx.run_until_parked();
    app.read_with(&vcx, |app, _| {
        assert_eq!(app.session().unwrap().account, before);
    });
}

#[gpui_kit::test]
fn a_click_on_the_account_on_screen_takes_the_usual_path(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let (account, chat_id) = app.read_with(&vcx, |app, _| {
        let session = app.session().unwrap();
        (session.account.clone(), closed_chat(session))
    });
    app.update_in(&mut vcx, |app, _, cx| {
        app.background
            .clicks
            .lock()
            .unwrap()
            .push((account, chat_id, NotificationAction::Open));
        app.drain_account_clicks(cx);
        assert!(app.background.clicks.lock().unwrap().is_empty());
        cx.notify();
    });
    vcx.run_until_parked();
    app.read_with(&vcx, |app, _| {
        assert_eq!(app.session().and_then(|s| s.open_chat), Some(chat_id));
    });
}
