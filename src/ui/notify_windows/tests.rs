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
    app.update(vcx, |app, cx| {
        app.notify.windows.platform = NotifyPlatform::Windows;
        for chat in 1..=4 {
            app.show_custom_notification(None, message(chat), None, None, false, false, cx);
        }
    });
    vcx.run_until_parked();
    app.read_with(vcx, |app, _| {
        let windows = &app.notify.windows;
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
        app.clear_custom_notifications(None, ChatId(1), cx)
    });
    app.read_with(vcx, |app, _| {
        let windows = &app.notify.windows;
        assert_eq!(windows.live(), 3);
        assert!(windows.queue.is_empty());
        assert!(
            windows
                .shown
                .iter()
                .any(|s| s.key == (None, ChatId(4)) && !s.unlinked)
        );
    });

    // A newer message of a shown chat replaces its window.
    app.update(vcx, |app, cx| {
        app.show_custom_notification(None, message(2), None, None, false, false, cx)
    });
    app.read_with(vcx, |app, _| {
        let live: Vec<_> = app
            .notify
            .windows
            .shown
            .iter()
            .filter(|s| !s.unlinked && s.key == (None, ChatId(2)))
            .collect();
        assert_eq!(live.len(), 1);
    });
}

#[gpui_kit::test]
fn a_click_opens_the_chat_and_hide_all_clears(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    app.update(vcx, |app, cx| {
        app.notify.windows.platform = NotifyPlatform::Windows;
        app.show_custom_notification(None, message(7), None, None, false, true, cx);
        app.show_custom_notification(None, message(8), None, None, false, true, cx);
    });
    app.update(vcx, |app, cx| {
        let id = app.notify.windows.shown.first().map(|s| s.id).unwrap_or(0);
        app.notify_window_event(id, ViewEvent::Activate, cx);
        // Read before the next frame hands the click on.
        let clicks = quill::locks::lock_or_recover(&app.notify.notify_clicks).clone();
        assert_eq!(clicks, vec![(ChatId(7), NotificationAction::Open)]);
        assert_eq!(app.notify.windows.live(), 1);
    });
    app.update(vcx, |app, cx| {
        app.notify_window_event(0, ViewEvent::HideAll, cx);
    });
    app.read_with(vcx, |app, _| {
        assert_eq!(app.notify.windows.live(), 0);
        assert!(app.notify.windows.queue.is_empty());
    });
    app.update(vcx, |app, cx| app.close_all_notify_windows(cx));
    app.read_with(vcx, |app, _| {
        assert!(app.notify.windows.shown.is_empty());
        assert!(app.notify.windows.hide_all.is_none());
    });
}

/// A poll hands out what it queued itself: a hidden or minimized main
/// window does not draw, so nothing may wait for its render.
#[gpui_kit::test]
fn a_queued_notification_shows_before_any_window_draws(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    app.update(vcx, |app, cx| {
        app.notify.windows.platform = NotifyPlatform::Windows;
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
        assert_eq!(app.notify.windows.live(), 1);
        assert!(
            app.notify
                .windows
                .shown
                .iter()
                .any(|s| s.key == (None, ChatId(5)))
        );
    });
}
