//! A notification waiting for its photo: the deadline task belongs to the
//! wait that made it.
use super::PreviewWait;
use crate::ui::app::QuillApp;
use crate::ui::notifications::viewer_progress_tests::open_app;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use quill::ids::{AccountKey, ChatId, FileId};
use std::time::Duration;

fn wait_for(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, key: &(AccountKey, ChatId)) {
    app.update(vcx, |app, cx| {
        app.arm_preview_wait(
            key.clone(),
            |deadline| PreviewWait {
                account: key.0.clone(),
                file: FileId(1),
                locked: false,
                notification: quill::notify::OsNotification::about_app("t".into(), "b".into()),
                photo: None,
                _deadline: deadline,
            },
            cx,
        );
    });
}

fn waiting(app: &Entity<QuillApp>, vcx: &VisualTestContext, key: &(AccountKey, ChatId)) -> bool {
    app.read_with(vcx, |app, _| app.delivery.preview_waits.contains_key(key))
}

#[gpui_kit::test]
fn a_wait_settled_early_leaves_no_deadline_to_end_the_next_wait(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_app(cx, "ready-voice-playing");
    let key = (AccountKey::new("account-1").expect("a key"), ChatId(5));
    wait_for(&app, &mut vcx, &key);
    // The photo lands at once: the poll shows it and the entry goes.
    app.update(&mut vcx, |app, cx| app.settle_preview_wait(&key, cx));
    assert!(!waiting(&app, &vcx, &key));
    vcx.executor().advance_clock(Duration::from_secs(1));
    wait_for(&app, &mut vcx, &key);
    // The first wait's deadline passes: the second still waits.
    vcx.executor().advance_clock(Duration::from_millis(2_500));
    vcx.run_until_parked();
    assert!(waiting(&app, &vcx, &key), "settled by the old deadline");
    vcx.executor().advance_clock(Duration::from_secs(1));
    vcx.run_until_parked();
    assert!(!waiting(&app, &vcx, &key), "its own deadline settles it");
}

/// A wait names its account once, when it is queued: the account on screen
/// reads as `None` and any other as its own key, whichever account is on
/// screen when the wait settles (a switch no longer drops the notification).
#[gpui_kit::test]
fn a_wait_keeps_its_account_through_a_switch(cx: &mut TestAppContext) {
    let (app, vcx) = open_app(cx, "ready-voice-playing");
    let other = AccountKey::new("account-2").expect("a key");
    app.read_with(&vcx, |app, _| {
        let on_screen = app.notification_account(None);
        assert_eq!(
            app.account_arg(&on_screen),
            None,
            "on screen: the app's own"
        );
        assert_eq!(
            app.account_arg(&other),
            Some(other.clone()),
            "another account: its background client"
        );
    });
}

/// The trigger of the above: a wait queued for the account on screen, an
/// account switch, then the wait settles. The notification shows under the
/// account it was queued for, not the one on screen now and not dropped.
#[gpui_kit::test]
fn a_wait_settled_after_a_switch_shows_under_its_own_account(cx: &mut TestAppContext) {
    use quill::diagnostics::{DiagnosticSink, MemorySink};
    use quill::notify::custom::NotifyPlatform;
    use quill::state::Session;
    let (app, mut vcx) = open_app(cx, "ready-voice-playing");
    let other = AccountKey::new("account-2").expect("a key");
    let queued_for = app.update(&mut vcx, |app, _| {
        app.delivery.windows.platform = NotifyPlatform::Windows;
        app.notification_account(None)
    });
    let key = (queued_for.clone(), ChatId(5));
    app.update(&mut vcx, |app, cx| {
        app.arm_preview_wait(
            key.clone(),
            |deadline| PreviewWait {
                account: queued_for.clone(),
                file: FileId(1),
                locked: false,
                notification: quill::notify::OsNotification {
                    chat_id: ChatId(5),
                    message_id: None,
                    topic_id: None,
                    title: "Ada".into(),
                    body: "Hello".into(),
                },
                photo: None,
                _deadline: deadline,
            },
            cx,
        );
        // The other account comes on screen while the photo is awaited.
        let sink: std::sync::Arc<dyn DiagnosticSink> = std::sync::Arc::new(MemorySink::new());
        app.demo_session = Some(Box::new(Session::new(other.clone(), sink)));
        assert_ne!(app.notification_account(None), queued_for);
        app.settle_preview_wait(&key, cx);
    });
    assert!(!waiting(&app, &vcx, &key), "the wait settled");
    let shown = app.read_with(&vcx, |app, _| {
        app.delivery
            .windows
            .shown_keys()
            .into_iter()
            .collect::<Vec<_>>()
    });
    assert_eq!(shown, vec![key], "shown under its own account, not dropped");
}
