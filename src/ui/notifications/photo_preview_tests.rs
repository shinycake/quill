//! A notification waiting for its photo: the deadline task belongs to the
//! wait that made it.
use super::{PHOTO_WAIT, PreviewWait};
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
                account: Some(key.0.clone()),
                file: FileId(1),
                on_screen: None,
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
    vcx.executor()
        .advance_clock(PHOTO_WAIT - Duration::from_millis(500));
    vcx.run_until_parked();
    assert!(waiting(&app, &vcx, &key), "settled by the old deadline");
    vcx.executor().advance_clock(Duration::from_secs(1));
    vcx.run_until_parked();
    assert!(!waiting(&app, &vcx, &key), "its own deadline settles it");
}
