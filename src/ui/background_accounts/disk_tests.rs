use super::closing_tests::{Fake, account};
use super::deleting_tests::{credentials, isolate, temp_root};
use super::tests::new_app;
use crate::ui::app::QuillApp;
use crate::ui::connect_ui::ConnectStage;
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
        app.apply_disk_reading(FULL, cx);
    });
    fake.close();
    app.update_in(&mut vcx, |app, _, cx| {
        app.reap_closing(cx);
        app.ensure_background_accounts(cx);
        assert!(app.background.client(&account(2)).is_none());
        assert!(!app.background.prepare.wanted(&account(1)));
        app.apply_disk_reading(ROOM, cx);
        app.ensure_background_accounts(cx);
        assert!(app.background.client(&account(2)).is_some());
        assert!(app.background.prepare.wanted(&account(1)));
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
            assert!(!app.background.prepare.wanted(&account(1)), "{free:?}");
        }
        app.apply_disk_reading(ROOM, cx);
        assert!(app.background.prepare.wanted(&account(1)));
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
        assert!(app.background.prepare.wanted(&account(1)));
        app.apply_disk_reading(FULL, cx);
        app.apply_disk_reading(FULL, cx);
        assert!(!app.background.prepare.wanted(&account(1)));
    });
    vcx.run_until_parked();
    app.read_with(&vcx, |app, _| {
        assert!(app.background.client(&account(1)).is_none());
    });
    let _ = std::fs::remove_dir_all(&root);
}

#[gpui_kit::test]
fn a_refused_screen_leaves_other_accounts_running_above_close(cx: &mut TestAppContext) {
    let (app, mut vcx, root) = app_with_unsigned_accounts(cx);
    app.update_in(&mut vcx, |app, _, cx| {
        app.open_client(Err(ConnectBlocker::DiskFull), cx);
        app.apply_disk_reading(NEARLY, cx);
        app.apply_disk_reading(NEARLY, cx);
        assert!(!app.disk.is_full(), "only the screen's client waits");
        assert!(app.disk_full_view().is_some());
        app.ensure_background_accounts(cx);
        assert!(app.background.client(&account(2)).is_some());
    });
    let _ = std::fs::remove_dir_all(&root);
}

#[gpui_kit::test]
fn a_connection_prepared_before_the_disk_filled_opens_nothing(cx: &mut TestAppContext) {
    let (app, mut vcx, root) = app_with_unsigned_accounts(cx);
    app.update_in(&mut vcx, |app, _, cx| {
        app.start_connection(cx);
        app.apply_disk_reading(FULL, cx);
        app.apply_disk_reading(FULL, cx);
    });
    vcx.run_until_parked();
    app.read_with(&vcx, |app, _| {
        // The prepared client was refused: the connection stage says so.
        assert!(matches!(app.auth_demo.stage, ConnectStage::DiskRefused));
        assert!(app.live.is_none());
        assert!(app.disk.is_full());
    });
    let _ = std::fs::remove_dir_all(&root);
}

/// A start that failed leaves its status, and the next pass starts the
/// account again instead of skipping it for the rest of the run.
#[gpui_kit::test]
fn an_account_that_could_not_start_is_started_again_by_the_next_pass(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    vcx.run_until_parked();
    let root = temp_root();
    isolate(&app, &mut vcx, &root, &[account(1)]);
    app.update_in(&mut vcx, |app, _, cx| {
        app.credentials = Some(credentials());
        app.finish_background_start(account(1), Err(ConnectBlocker::StoreError), cx);
        assert!(
            app.background
                .client(&account(1))
                .is_some_and(|client| client.retryable()),
            "a failed start keeps its status"
        );
        app.ensure_background_accounts(cx);
        assert!(
            app.background.prepare.wanted(&account(1)),
            "the next pass tried again"
        );
        assert!(
            app.background.client(&account(1)).is_none(),
            "the dead entry no longer blocks it"
        );
    });
    let _ = std::fs::remove_dir_all(&root);
}

/// A failed client is started again by the poll once its delay is over,
/// without a connect, switch or unlock.
#[gpui_kit::test]
fn a_failed_background_start_waits_for_its_delay_then_is_due(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    vcx.run_until_parked();
    let root = temp_root();
    isolate(&app, &mut vcx, &root, &[account(1)]);
    app.update_in(&mut vcx, |app, _, cx| {
        app.credentials = Some(credentials());
        app.finish_background_start(account(1), Err(ConnectBlocker::StoreError), cx);
        let now = std::time::Instant::now();
        let wait = app
            .background
            .next_restart(now)
            .expect("a failed client waits for a restart");
        assert!(wait > std::time::Duration::ZERO && wait <= std::time::Duration::from_secs(5));
        assert_eq!(
            app.background
                .next_restart(now + std::time::Duration::from_secs(5)),
            Some(std::time::Duration::ZERO),
            "due once the delay is over"
        );
    });
    let _ = std::fs::remove_dir_all(&root);
}

/// A client that is due while nothing may start it (here its old client is
/// still closing) does not stay due: the poll looks again after the first
/// delay instead of running the start pass every 40 ms.
#[gpui_kit::test]
fn a_due_client_that_cannot_start_waits_again_instead_of_staying_due(cx: &mut TestAppContext) {
    use std::time::{Duration, Instant};
    let (app, mut vcx) = new_app(cx);
    vcx.run_until_parked();
    let root = temp_root();
    isolate(&app, &mut vcx, &root, &[account(1)]);
    let fake = Fake::default();
    app.update_in(&mut vcx, |app, _, cx| {
        app.credentials = Some(credentials());
        app.finish_background_start(account(1), Err(ConnectBlocker::StoreError), cx);
        app.background.closing.push(account(1), fake.boxed(), false);
        let due = Instant::now() + Duration::from_secs(5);
        assert_eq!(app.background.next_restart(due), Some(Duration::ZERO));
        app.restart_due(due, cx);
        assert!(
            app.background
                .client(&account(1))
                .is_some_and(|client| client.retryable()),
            "nothing starts while its database is still closing"
        );
        assert!(!app.background.prepare.wanted(&account(1)));
        assert_eq!(
            app.background.next_restart(due),
            Some(Duration::from_secs(5)),
            "it waits again"
        );
        app.restart_due(due + Duration::from_secs(1), cx);
        assert_eq!(
            app.background.next_restart(due + Duration::from_secs(1)),
            Some(Duration::from_secs(4)),
            "a pass before the delay is over changes nothing"
        );
    });
    let _ = std::fs::remove_dir_all(&root);
}
