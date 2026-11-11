//! Round 17 (area B, finding 3): the text typed into a notification is
//! taken by whichever path runs its Reply click. A click on a toast of an
//! account that was not on screen switches accounts first and then runs
//! `run_notification_action`; the text used to stay parked there, so a
//! later Reply click for the chat sent it unasked.
use crate::ui::app::QuillApp;
use crate::ui::notifications::native_extras::{take_reply_text, test_texts};
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::ids::ChatId;
use quill::notify::NotificationAction;

const CHAT: ChatId = ChatId(11);

fn demo_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(800.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(window, cx, None, Some(ScreenshotDemo::named("ready-chats")))
        });
        *slot_in.borrow_mut() = Some(view.clone());
        let shell = cx.new(|_| QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let app = slot.borrow().clone();
    let Some(app): Option<Entity<QuillApp>> = app else {
        panic!("the window builder did not run");
    };
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    vcx.update(|window, cx| window.render_frame(cx));
    vcx.run_until_parked();
    (app, vcx)
}

#[gpui_kit::test]
fn a_reply_run_after_the_switch_takes_the_typed_text(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(800.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(window, cx, None, Some(ScreenshotDemo::named("ready-chats")))
        });
        *slot_in.borrow_mut() = Some(view.clone());
        let shell = cx.new(|_| QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let app = slot.borrow().clone();
    let Some(app): Option<Entity<QuillApp>> = app else {
        panic!("the window builder did not run");
    };
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    vcx.update(|window, cx| window.render_frame(cx));
    vcx.run_until_parked();
    app.update_in(&mut vcx, |app, window, cx| {
        let account = app.current_account().expect("the demo account");
        test_texts::park(&account, CHAT, "on my way");
        // What `open_pending_account_chat` runs once the switch landed.
        app.run_notification_action(CHAT, NotificationAction::Reply, window, cx);
        let tag = quill::notify::notification_tag(&account.0, CHAT);
        assert_eq!(
            take_reply_text(&tag, &account, CHAT),
            None,
            "the text was taken, not left for a later click"
        );
        // With nothing sent (the demo has no client), the chat opens.
        assert_eq!(app.open_chat_id(), Some(CHAT));
        // The typed words are not lost with the send: they are in the
        // composer of the chat that opened.
        assert_eq!(app.composer.read(cx).value().to_string(), "on my way");
        assert!(app.delivery.unsent_replies.is_empty());
    });
}

/// A chat with a window of its own takes the click, and the text typed
/// into its toast (which could not be sent) lands in that window's
/// composer, not in the main window's and not nowhere.
#[gpui_kit::test]
fn an_unsent_reply_goes_to_the_chat_windows_composer(cx: &mut TestAppContext) {
    let (app, mut vcx) = demo_app(cx);
    app.update_in(&mut vcx, |app, window, cx| {
        let bounds =
            gpui_kit::Bounds::new(gpui_kit::point(px(0.), px(0.)), size(px(600.), px(500.)));
        app.open_chat_window_at(CHAT, Some(bounds), window, cx);
    });
    vcx.run_until_parked();
    app.update_in(&mut vcx, |app, window, cx| {
        let account = app.current_account().expect("the demo account");
        test_texts::park(&account, CHAT, "from the toast");
        app.run_notification_action(CHAT, NotificationAction::Reply, window, cx);
        assert!(
            app.delivery.unsent_replies.is_empty(),
            "the text was handed over"
        );
        assert_ne!(
            app.composer.read(cx).value().to_string(),
            "from the toast",
            "not the main window's composer"
        );
        let waiting = app
            .in_window_scope(Some(CHAT), cx, |app, _| {
                app.chat_ui.compose_after_open.clone()
            })
            .expect("the chat window's state");
        assert_eq!(
            waiting.as_deref(),
            Some("from the toast"),
            "waiting for the chat window's next frame"
        );
    });
}

/// Behind the passcode nothing opens and the text is kept; once unlocked,
/// the next click on the chat puts it into the composer.
#[gpui_kit::test]
fn an_unsent_reply_waits_out_the_lock(cx: &mut TestAppContext) {
    let (app, mut vcx) = demo_app(cx);
    app.update_in(&mut vcx, |app, window, cx| {
        let account = app.current_account().expect("the demo account");
        app.account.passcode.force_locked(true);
        test_texts::park(&account, CHAT, "kept");
        app.run_notification_action(CHAT, NotificationAction::Reply, window, cx);
        assert_eq!(
            app.delivery
                .unsent_replies
                .get(&(account.clone(), CHAT))
                .map(String::as_str),
            Some("kept"),
            "kept while locked"
        );
        app.account.passcode.force_locked(false);
        app.run_notification_action(CHAT, NotificationAction::Open, window, cx);
        assert_eq!(app.open_chat_id(), Some(CHAT));
        assert_eq!(app.composer.read(cx).value().to_string(), "kept");
        assert!(app.delivery.unsent_replies.is_empty());
    });
}
