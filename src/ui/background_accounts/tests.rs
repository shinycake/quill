//! Background accounts in the window: the account list badge, and what a
//! click on another account's notification does once it reaches the
//! render (needs the `demo-capture` feature for gpui-kit's test support).
use super::calls::BackgroundRing;
use super::{AccountUnread, BackgroundClient, BackgroundStatus, PendingOpen};
use crate::ui::app::QuillApp;
use crate::ui::call_sounds::CallSound;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::DialogKind;
use crate::ui::shell::QuillShell;
use gpui_kit::AppContext;
use gpui_kit::component::Root;
use gpui_kit::{Entity, TestAppContext, VisualTestContext, px, size};
use quill::ids::{AccountKey, ChatId};
use quill::notify::NotificationAction;
use std::time::{Duration, Instant};

pub(super) fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    let (app, _, vcx) = new_app_shell(cx);
    (app, vcx)
}

fn new_app_shell(
    cx: &mut TestAppContext,
) -> (Entity<QuillApp>, Entity<QuillShell>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(800.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(window, cx, None, Some(ScreenshotDemo::named("ready-chats")))
        });
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some((view, shell.clone()));
        Root::new(shell, window, cx)
    });
    let (app, shell) = slot.borrow().clone().unwrap();
    let vcx = VisualTestContext::from_window(handle.into(), cx);
    (app, shell, vcx)
}

fn ring() -> BackgroundRing {
    BackgroundRing {
        account: other_account(),
        call_id: 77,
        caller: "Alice".into(),
        is_video: false,
        account_name: "Work".into(),
        dismissed: false,
        // No OS notification in tests.
        notified: true,
    }
}

#[gpui_kit::test]
fn a_call_ringing_on_another_account_shows_its_prompt(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = new_app_shell(cx);
    app.update_in(&mut vcx, |app, _, cx| {
        assert!(!app.background_ring_audible());
        app.background.ringing = Some(ring());
        assert!(app.background_ring_audible());
        assert_eq!(app.background_ring_account(), Some(&other_account()));
        cx.notify();
    });
    vcx.run_until_parked();
    shell.read_with(&vcx, |shell, _| {
        assert_eq!(shell.open_dialog, Some(DialogKind::BackgroundCall));
    });
    // Declining with no running client only stops the ring here.
    app.update_in(&mut vcx, |app, _, cx| {
        app.decline_background_call(cx);
        assert!(app.background_ring_account().is_none());
    });
    vcx.run_until_parked();
    shell.read_with(&vcx, |shell, _| {
        assert_ne!(shell.open_dialog, Some(DialogKind::BackgroundCall));
    });
}

#[gpui_kit::test]
fn a_dismissed_prompt_stops_ringing_in_the_app(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = new_app_shell(cx);
    app.update_in(&mut vcx, |app, _, cx| {
        let mut dismissed = ring();
        dismissed.dismissed = true;
        app.background.ringing = Some(dismissed);
        cx.notify();
    });
    vcx.run_until_parked();
    app.read_with(&vcx, |app, _| assert!(!app.background_ring_audible()));
    shell.read_with(&vcx, |shell, _| {
        assert_ne!(shell.open_dialog, Some(DialogKind::BackgroundCall));
    });
}

#[gpui_kit::test]
fn the_tick_loops_another_accounts_ringtone_with_no_poll(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    app.update_in(&mut vcx, |app, _, cx| {
        app.background.ringing = Some(ring());
        app.ensure_call_tick(cx);
        assert_eq!(app.calls.sounds.looping(), None);
    });
    vcx.executor().advance_clock(Duration::from_secs(1));
    vcx.run_until_parked();
    app.read_with(&vcx, |app, _| {
        assert_eq!(app.calls.sounds.looping(), Some(CallSound::Incoming));
        assert!(app.calls.tick_active);
    });
    app.update_in(&mut vcx, |app, _, _| {
        app.background.ringing = None;
    });
    vcx.executor().advance_clock(Duration::from_secs(1));
    vcx.run_until_parked();
    app.read_with(&vcx, |app, _| {
        assert!(!app.calls.tick_active, "no ring, no tick");
    });
}

#[gpui_kit::test]
fn the_main_menu_counts_the_other_accounts(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    app.update_in(&mut vcx, |app, _, cx| {
        assert!(app.other_accounts_unread().is_none());
        app.background
            .clients
            .push(client(BackgroundStatus::Ready, 4));
        assert_eq!(app.other_accounts_unread().map(|u| u.count), Some(4));
        // Not signed in on screen (demo): no account name in the title.
        assert!(app.title_account_name().is_none());
        cx.notify();
    });
    // The menu button with its dot draws.
    vcx.run_until_parked();
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
            Instant::now()
                .checked_sub(Duration::from_secs(1))
                .unwrap_or_else(Instant::now),
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
