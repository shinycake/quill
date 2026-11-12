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

/// Quill's own toast hands its typed text over like the platform's reply
/// field: any outcome but a send keeps it for the chat that opens.
#[gpui_kit::test]
fn a_reply_typed_into_our_own_toast_is_kept_unless_sent(cx: &mut TestAppContext) {
    use crate::ui::notifications::inline_reply::InlineReplyOutcome;
    let (app, mut vcx) = demo_app(cx);
    app.update_in(&mut vcx, |app, _, _| {
        let account = app.current_account().expect("the demo account");
        let key = (account.clone(), CHAT);
        app.keep_unsent_reply(&account, CHAT, "sent".into(), InlineReplyOutcome::Sent);
        assert!(
            app.delivery.unsent_replies.is_empty(),
            "a sent reply is gone"
        );
        app.keep_unsent_reply(&account, CHAT, "  ".into(), InlineReplyOutcome::Refused);
        assert!(
            app.delivery.unsent_replies.is_empty(),
            "blank text is not kept"
        );
        app.keep_unsent_reply(
            &account,
            CHAT,
            "no rights".into(),
            InlineReplyOutcome::Refused,
        );
        assert_eq!(
            app.delivery.unsent_replies.get(&key).map(String::as_str),
            Some("no rights")
        );
    });
}

/// An account that logs out or is removed takes its parked reply texts,
/// reply targets and photo waits with it; another account's stay.
#[gpui_kit::test]
fn a_removed_accounts_notification_state_goes_with_it(cx: &mut TestAppContext) {
    let (app, mut vcx) = demo_app(cx);
    app.update(&mut vcx, |app, _| {
        let account = app.current_account().expect("the demo account");
        let other = quill::ids::AccountKey("someone-else".into());
        for owner in [&account, &other] {
            app.delivery
                .unsent_replies
                .insert((owner.clone(), CHAT), "typed".into());
            app.delivery
                .reply_targets
                .insert((owner.clone(), CHAT), quill::ids::MessageId(5));
        }
        app.delivery.forget_account(&account);
        assert!(
            !app.delivery
                .unsent_replies
                .contains_key(&(account.clone(), CHAT))
        );
        assert!(!app.delivery.reply_targets.contains_key(&(account, CHAT)));
        assert!(
            app.delivery
                .unsent_replies
                .contains_key(&(other.clone(), CHAT))
        );
        assert!(app.delivery.reply_targets.contains_key(&(other, CHAT)));
    });
}

/// One decode serves every notification for an avatar; it is held by the
/// delivery state, so dropping the entry ends the wait (nothing detached).
#[gpui_kit::test]
fn an_avatar_decode_is_held_and_queues_later_notifications(cx: &mut TestAppContext) {
    let (app, mut vcx) = demo_app(cx);
    let photo = std::path::PathBuf::from("/definitely/not/a/photo.jpg");
    app.update_in(&mut vcx, |app, _, cx| {
        let note = |text: &str| {
            quill::notify::OsNotification::about_app("Quill".to_string(), text.to_string())
        };
        app.show_notification_with_photo(None, note("one"), Some(photo.clone()), None, false, cx);
        app.show_notification_with_photo(None, note("two"), Some(photo.clone()), None, false, cx);
        assert_eq!(app.delivery.avatar_decodes.len(), 1, "one decode");
        assert_eq!(
            app.delivery
                .avatar_decodes
                .get(&photo)
                .map(|d| d.waiting_len()),
            Some(2),
            "both wait on it"
        );
        // The owner going away drops the task with the entry.
        app.delivery.avatar_decodes.clear();
    });
    vcx.run_until_parked();
    app.read_with(&vcx, |app, _| {
        assert!(app.delivery.avatar_decodes.is_empty())
    });
}
