//! The notification windows in the UI (needs the `demo-capture` feature for
//! gpui-kit's test support): the stack, the queue behind the count, "Hide
//! all", and clicks. Nothing leaves the app: no connection, and the app
//! data root is a temp directory.
use super::ViewEvent;
use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::ids::ChatId;
use quill::notify::custom::NotifyPlatform;
use quill::notify::{NotificationAction, OsNotification};

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    let root = quill::scratch::process_dir("quill-notify-windows");
    let _ = std::fs::create_dir_all(&root);
    quill::settings::use_isolated_app_root(root);
    cx.update(gpui_kit::init);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(800.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(
                window,
                cx,
                None,
                Some(ScreenshotDemo::named("ready-privacy")),
            )
        });
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some(view);
        Root::new(shell, window, cx)
    });
    let app = slot.borrow().clone();
    let Some(app) = app else {
        panic!("the window builder did not run");
    };
    let vcx = VisualTestContext::from_window(handle.into(), cx);
    (app, vcx)
}

fn message(chat: i64) -> OsNotification {
    OsNotification {
        chat_id: ChatId(chat),
        message_id: None,
        title: format!("Chat {chat}"),
        body: "Hello".into(),
    }
}

#[gpui_kit::test]
fn the_count_limits_the_stack_and_the_rest_wait(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    let me = app.read_with(vcx, |app, _| app.notification_account(None));
    app.update(vcx, |app, cx| {
        app.delivery.windows.platform = NotifyPlatform::Windows;
        for chat in 1..=4 {
            app.show_custom_notification(&me, message(chat), None, None, false, false, cx);
        }
    });
    vcx.run_until_parked();
    app.read_with(vcx, |app, _| {
        let windows = &app.delivery.windows;
        assert_eq!(windows.live(), 3, "three by default");
        assert_eq!(windows.queue.len(), 1);
        assert!(windows.hide_all.is_some(), "Hide all shows with a queue");
        let shifts: Vec<f32> = windows.shown.iter().map(|s| s.popup.shift.to).collect();
        assert!(
            shifts.windows(2).all(|pair| pair[0] > pair[1]),
            "newest nearest the corner"
        );
    });

    // Reading a shown chat makes room for the queued one.
    app.update(vcx, |app, cx| {
        app.clear_custom_notifications(&me, ChatId(1), cx)
    });
    app.read_with(vcx, |app, _| {
        let windows = &app.delivery.windows;
        assert_eq!(windows.live(), 3);
        assert!(windows.queue.is_empty());
        assert!(
            windows
                .shown
                .iter()
                .any(|s| s.key == (me.clone(), ChatId(4)) && !s.unlinked)
        );
    });

    // A newer message of a shown chat replaces its window.
    app.update(vcx, |app, cx| {
        app.show_custom_notification(&me, message(2), None, None, false, false, cx)
    });
    app.read_with(vcx, |app, _| {
        let live: Vec<_> = app
            .delivery
            .windows
            .shown
            .iter()
            .filter(|s| !s.unlinked && s.key == (me.clone(), ChatId(2)))
            .collect();
        assert_eq!(live.len(), 1);
    });
}

#[gpui_kit::test]
fn a_click_opens_the_chat_and_hide_all_clears(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    let me = app.read_with(vcx, |app, _| app.notification_account(None));
    app.update(vcx, |app, cx| {
        app.delivery.windows.platform = NotifyPlatform::Windows;
        app.show_custom_notification(&me, message(7), None, None, false, true, cx);
        app.show_custom_notification(&me, message(8), None, None, false, true, cx);
    });
    app.update(vcx, |app, cx| {
        let id = app
            .delivery
            .windows
            .shown
            .first()
            .map(|s| s.id)
            .unwrap_or(0);
        app.notify_window_event(id, ViewEvent::Activate, cx);
        // Read before the next frame hands the click on.
        let clicks = quill::locks::lock_or_recover(&app.delivery.notify_clicks).clone();
        assert_eq!(
            clicks,
            vec![quill::notify_accounts::ChatClick {
                account: me.clone(),
                chat_id: ChatId(7),
                action: NotificationAction::Open,
            }]
        );
        assert_eq!(app.delivery.windows.live(), 1);
    });
    app.update(vcx, |app, cx| {
        app.notify_window_event(0, ViewEvent::HideAll, cx);
    });
    app.read_with(vcx, |app, _| {
        assert_eq!(app.delivery.windows.live(), 0);
        assert!(app.delivery.windows.queue.is_empty());
    });
    app.update(vcx, |app, cx| app.close_all_notify_windows(cx));
    app.read_with(vcx, |app, _| {
        assert!(app.delivery.windows.shown.is_empty());
        assert!(app.delivery.windows.hide_all.is_none());
    });
}

fn shown_id(app: &QuillApp, chat: i64) -> u64 {
    app.delivery
        .windows
        .shown
        .iter()
        .find(|s| s.key.1 == ChatId(chat) && !s.unlinked)
        .map(|s| s.id)
        .unwrap_or(0)
}

fn fading(app: &QuillApp, id: u64) -> bool {
    app.delivery
        .windows
        .shown
        .iter()
        .any(|s| s.id == id && s.popup.hiding)
}

/// A new message of the chat whose window has a reply being typed does
/// not take that window away: the reply and its text stay.
#[gpui_kit::test]
fn a_new_message_keeps_the_window_being_replied_in(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    let me = app.read_with(vcx, |app, _| app.notification_account(None));
    app.update(vcx, |app, cx| {
        app.delivery.windows.platform = NotifyPlatform::Windows;
        app.show_custom_notification(&me, message(7), None, None, false, true, cx);
        let id = shown_id(app, 7);
        app.notify_window_event(id, ViewEvent::ReplyOpened, cx);
        app.show_custom_notification(&me, message(7), None, None, false, true, cx);
        let replying = app.delivery.windows.shown.iter().find(|s| s.id == id);
        assert!(
            replying.is_some_and(|s| s.replying && !s.unlinked && !s.popup.hiding),
            "the window being replied in stays up"
        );
        let live = app
            .delivery
            .windows
            .shown
            .iter()
            .filter(|s| s.key.1 == ChatId(7) && !s.unlinked)
            .count();
        assert_eq!(live, 2, "the new message shows beside it");
    });
}

/// Clicking one window while the pointer rests on it: that window goes
/// without a leave event, so the others start hiding as if the pointer
/// had left (tdesktop #28813). Before, the hover had cleared their timers
/// and they stayed up until hovered and left.
#[gpui_kit::test]
fn a_window_clicked_under_the_pointer_lets_the_others_fade(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    let me = app.read_with(vcx, |app, _| app.notification_account(None));
    app.update(vcx, |app, cx| {
        app.delivery.windows.platform = NotifyPlatform::Windows;
        app.show_custom_notification(&me, message(7), None, None, false, false, cx);
        app.show_custom_notification(&me, message(8), None, None, false, false, cx);
        let now = std::time::Instant::now();
        let later = now + std::time::Duration::from_secs(600);
        // The user was back at the computer: both are due later.
        for shown in app.delivery.windows.shown.iter_mut() {
            shown.waiting_for_input = false;
            shown.hide_at = Some(later);
        }
        let (x, y) = (shown_id(app, 7), shown_id(app, 8));
        app.notify_window_event(x, ViewEvent::Hover(true), cx);
        app.notify_settle_hiding(later);
        assert!(!fading(app, y), "nothing fades under the pointer");
        app.notify_window_event(x, ViewEvent::Activate, cx);
        assert_eq!(app.delivery.windows.hovered, None, "the hold went with it");
        app.notify_settle_hiding(later);
        assert!(fading(app, y), "the other window fades");
        // Hovering it again stops the fade; leaving starts it again.
        app.notify_window_event(y, ViewEvent::Hover(true), cx);
        assert!(!fading(app, y));
        app.notify_window_event(y, ViewEvent::Hover(false), cx);
        assert!(fading(app, y));
    });
}

/// A poll hands out what it queued itself: a hidden or minimized main
/// window does not draw, so nothing may wait for its render.
#[gpui_kit::test]
fn a_queued_notification_shows_before_any_window_draws(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    let me = app.read_with(vcx, |app, _| app.notification_account(None));
    app.update(vcx, |app, cx| {
        app.delivery.windows.platform = NotifyPlatform::Windows;
        let drain = quill::notify_accounts::NotifyDrain {
            notifications: vec![quill::notify::QueuedNotification {
                chat_id: ChatId(5),
                message_id: None,
                title: "Chat 5".into(),
                body: "Hello".into(),
                count: 1,
                sound: None,
                photo: None,
            }],
            attention: true,
            ..Default::default()
        };
        app.hand_out_notifications(drain, None, cx);
        // Still inside one update: no window has drawn since.
        assert_eq!(app.delivery.windows.live(), 1);
        assert!(
            app.delivery
                .windows
                .shown
                .iter()
                .any(|s| s.key == (me.clone(), ChatId(5)))
        );
    });
}

/// A real click on a notification window (issue #1401). It brings the
/// main window forward, never the notification window itself; and a key
/// that still lands in one (on Windows, GPUI's activation sends Alt to the
/// window it activates) finds the kit's window state there, as app-wide
/// key handlers ask every window about its dialogs.
#[gpui_kit::test]
fn clicking_a_notification_window_opens_the_chat(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    let me = app.read_with(vcx, |app, _| app.notification_account(None));
    app.update(vcx, |app, cx| {
        app.delivery.windows.platform = NotifyPlatform::Windows;
        app.show_custom_notification(&me, message(7), None, None, false, true, cx);
    });
    vcx.run_until_parked();
    let handle = app.read_with(vcx, |app, _| {
        app.delivery.windows.shown.first().map(|s| s.popup.handle)
    });
    let Some(handle) = handle else {
        panic!("no notification window opened");
    };
    let mut popup = VisualTestContext::from_window(handle.into(), cx);
    popup.run_until_parked();
    popup.simulate_click(
        gpui_kit::point(px(120.), px(40.)),
        gpui_kit::Modifiers::none(),
    );
    popup.run_until_parked();
    vcx.run_until_parked();
    app.read_with(vcx, |app, _| {
        assert_eq!(app.delivery.windows.live(), 0, "the clicked window closes");
    });
    assert!(
        !popup.update(|window, _| window.is_window_active()),
        "the click never activates the notification window"
    );

    popup.simulate_modifiers_change(gpui_kit::Modifiers::alt());
    popup.simulate_modifiers_change(gpui_kit::Modifiers::none());
    popup.simulate_keystrokes("a escape up");
    popup.run_until_parked();
}

#[gpui_kit::test]
fn the_corner_preview_ends_with_the_page_that_showed_it(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    app.update(vcx, |app, cx| {
        app.notify_corner_preview(Some(quill::notify::custom::ScreenCorner::TopLeft), cx);
    });
    app.read_with(vcx, |app, _| {
        assert!(
            app.delivery.windows.demo.to.abs() < f32::EPSILON,
            "the real notifications dim while a corner is previewed"
        );
    });
    // Leaving the Notifications page ends it; no hover-out is needed.
    app.update(vcx, |app, _| app.leave_notification_settings());
    app.read_with(vcx, |app, _| {
        assert!((app.delivery.windows.demo.to - 1.0).abs() < f32::EPSILON);
        assert!(
            app.delivery
                .windows
                .samples
                .iter()
                .all(|(_, popup)| popup.hiding),
            "the sample windows are going"
        );
    });
}
