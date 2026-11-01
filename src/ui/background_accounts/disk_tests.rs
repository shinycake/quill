use super::closing_tests::{Fake, account};
use super::deleting_tests::{credentials, isolate, temp_root};
use super::tests::new_app;
use crate::ui::app::QuillApp;
use crate::ui::connect_ui::ConnectUiStatus;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use quill::connect::ConnectBlocker;
use quill::disk_guard::FLOORS;
use quill::settings::add_account;
use std::path::PathBuf;

const FULL: Option<u64> = Some(0);
const NEARLY: Option<u64> = Some(FLOORS.open - 1);
const ROOM: Option<u64> = Some(FLOORS.open);

fn app_with_unsigned_accounts(
    cx: &mut TestAppContext,
) -> (Entity<QuillApp>, VisualTestContext, PathBuf) {
    let (app, mut vcx) = new_app(cx);
    vcx.run_until_parked();
    let root = temp_root();
    isolate(&app, &mut vcx, &root, &[]);
    assert_eq!(add_account(&root, "Third").unwrap(), account(2));
    app.update_in(&mut vcx, |app, _, _| {
        app.credentials = Some(credentials());
    });
    (app, vcx, root)
}

#[gpui_kit::test]
fn an_account_still_closing_when_space_returns_starts_again(cx: &mut TestAppContext) {
    let (app, mut vcx, root) = app_with_unsigned_accounts(cx);
    let fake = Fake::default();
    app.update_in(&mut vcx, |app, _, cx| {
        app.background.closing.push(account(2), fake.boxed(), false);
        app.apply_disk_reading(FULL, cx);
        app.apply_disk_reading(ROOM, cx);
        app.ensure_background_accounts(cx);
        assert!(
            app.background.client(&account(2)).is_none(),
            "its database is still closing"
        );
    });
    fake.close();
    app.update_in(&mut vcx, |app, _, cx| {
        app.reap_closing(cx);
        assert!(
            app.background.client(&account(2)).is_some(),
            "it started again once its close finished"
        );
    });
    let _ = std::fs::remove_dir_all(&root);
}

#[gpui_kit::test]
fn nothing_starts_while_the_disk_is_full(cx: &mut TestAppContext) {
    let (app, mut vcx, root) = app_with_unsigned_accounts(cx);
    let fake = Fake::default();
    app.update_in(&mut vcx, |app, _, cx| {
        app.background.closing.push(account(2), fake.boxed(), true);
        app.apply_disk_reading(FULL, cx);
    });
    fake.close();
    app.update_in(&mut vcx, |app, _, cx| {
        app.reap_closing(cx);
        app.ensure_background_accounts(cx);
        assert!(app.background.client(&account(2)).is_none());
        assert!(!app.background.starting.contains(&account(1)));
        app.apply_disk_reading(ROOM, cx);
        app.ensure_background_accounts(cx);
        assert!(app.background.client(&account(2)).is_some());
        assert!(app.background.starting.contains(&account(1)));
    });
    let _ = std::fs::remove_dir_all(&root);
}

#[gpui_kit::test]
fn a_refused_background_open_waits_for_a_reading_with_room(cx: &mut TestAppContext) {
    let (app, mut vcx, root) = app_with_unsigned_accounts(cx);
    app.update_in(&mut vcx, |app, _, cx| {
        app.apply_disk_reading(NEARLY, cx);
        app.finish_background_start(account(1), Err(ConnectBlocker::DiskFull), cx);
        assert!(
            app.background.client(&account(1)).is_none(),
            "no failed entry to skip it for good"
        );
        for free in [NEARLY, None] {
            app.apply_disk_reading(free, cx);
            assert!(!app.background.starting.contains(&account(1)), "{free:?}");
        }
        app.apply_disk_reading(ROOM, cx);
        assert!(app.background.starting.contains(&account(1)));
    });
    let _ = std::fs::remove_dir_all(&root);
}

#[gpui_kit::test]
fn a_background_open_that_fails_for_another_reason_keeps_its_status(cx: &mut TestAppContext) {
    let (app, mut vcx, root) = app_with_unsigned_accounts(cx);
    app.update_in(&mut vcx, |app, _, cx| {
        app.finish_background_start(account(1), Err(ConnectBlocker::StoreError), cx);
        assert!(app.background.client(&account(1)).is_some());
    });
    let _ = std::fs::remove_dir_all(&root);
}

#[gpui_kit::test]
fn a_full_reading_ends_a_start_in_flight(cx: &mut TestAppContext) {
    let (app, mut vcx, root) = app_with_unsigned_accounts(cx);
    app.update_in(&mut vcx, |app, _, cx| {
        app.ensure_background_accounts(cx);
        assert!(app.background.starting.contains(&account(1)));
        app.apply_disk_reading(FULL, cx);
        assert!(!app.background.starting.contains(&account(1)));
    });
    vcx.run_until_parked();
    app.read_with(&vcx, |app, _| {
        assert!(app.background.client(&account(1)).is_none());
    });
    let _ = std::fs::remove_dir_all(&root);
}

#[gpui_kit::test]
fn a_start_that_lands_on_a_refused_screen_opens_nothing(cx: &mut TestAppContext) {
    let (app, mut vcx, root) = app_with_unsigned_accounts(cx);
    app.update_in(&mut vcx, |app, _, cx| {
        app.ensure_background_accounts(cx);
        app.open_client(Err(ConnectBlocker::DiskFull), cx);
        assert!(app.background.starting.contains(&account(1)));
    });
    vcx.run_until_parked();
    app.read_with(&vcx, |app, _| {
        assert!(app.background.client(&account(1)).is_none());
        assert!(!app.background.starting.contains(&account(1)));
    });
    let _ = std::fs::remove_dir_all(&root);
}

#[gpui_kit::test]
fn a_connection_prepared_before_the_disk_filled_opens_nothing(cx: &mut TestAppContext) {
    let (app, mut vcx, root) = app_with_unsigned_accounts(cx);
    app.update_in(&mut vcx, |app, _, cx| {
        app.start_connection(cx);
        app.apply_disk_reading(FULL, cx);
    });
    vcx.run_until_parked();
    app.read_with(&vcx, |app, _| {
        assert!(app.connection.status == ConnectUiStatus::Live);
        assert!(app.live.is_none());
        assert!(app.disk.is_full());
    });
    let _ = std::fs::remove_dir_all(&root);
}
