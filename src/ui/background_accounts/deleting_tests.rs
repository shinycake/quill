//! Deleting account folders in the window (`deleting`): a removed or
//! logged-out account's folder goes only once its TDLib client confirmed
//! Closed and no preparation for it still runs; a removed account never
//! starts again; a quit before then leaves the folders recorded and the
//! next start deletes them. Every test works in a temporary app root with
//! an in-memory keychain.
use super::closing_tests::{Fake, account, run_for};
use super::tests::new_app;
use crate::ui::app::QuillApp;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use quill::credentials::TelegramCredentials;
use quill::ids::AccountKey;
use quill::platform::{DatabaseKey, MemorySecretStore, SecretStore};
use quill::settings::{AccountPaths, add_account, list_accounts, pending_deletes};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// A temporary app root with the primary account and `account-1`, each
/// with a local database.
pub(super) fn temp_root() -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let root = std::env::temp_dir().join(format!(
        "quill-deleting-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    assert_eq!(add_account(&root, "Work").unwrap(), account(1));
    database(&root, &AccountKey::primary());
    database(&root, &account(1));
    root
}

fn database(root: &Path, key: &AccountKey) {
    let db = AccountPaths::for_root(root, key).tdlib_database;
    std::fs::create_dir_all(&db).unwrap();
    std::fs::write(db.join("td.binlog"), b"x").unwrap();
}

fn folder(root: &Path, key: &AccountKey) -> bool {
    AccountPaths::for_root(root, key).root.exists()
}

fn registered(root: &Path, key: &AccountKey) -> bool {
    list_accounts(root).iter().any(|record| &record.key == key)
}

/// Point `app` at `root` and an in-memory keychain holding `keys`.
pub(super) fn isolate(
    app: &Entity<QuillApp>,
    vcx: &mut VisualTestContext,
    root: &Path,
    keys: &[AccountKey],
) -> MemorySecretStore {
    let store = MemorySecretStore::new();
    for key in keys {
        store.put(key, &DatabaseKey::generate()).unwrap();
    }
    let shared = store.clone();
    let root = root.to_path_buf();
    app.update_in(vcx, |app, _, _| {
        app.background.test_root = Some(root);
        app.background.secrets = Arc::new(move || Box::new(shared.clone()));
    });
    store
}

pub(super) fn credentials() -> TelegramCredentials {
    TelegramCredentials {
        api_id: 1,
        api_hash: "unit-test-hash-not-for-network".into(),
    }
}

#[gpui_kit::test]
fn a_removed_account_waits_for_its_client_to_close_before_its_folder_goes(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let root = temp_root();
    let store = isolate(&app, &mut vcx, &root, &[account(1)]);
    let fake = Fake::default();
    app.update_in(&mut vcx, |app, _, cx| {
        // Its background client was handed to the closing set; it would
        // restart (closed for a call), but a removal cancels that.
        app.background.closing.push(account(1), fake.boxed(), true);
        app.remove_account_in(root.clone(), &account(1), cx);
        // A second click while it closes changes nothing.
        app.remove_account_in(root.clone(), &account(1), cx);
        assert!(app.account.accounts.error.is_none());
    });
    assert!(
        !registered(&root, &account(1)),
        "gone from the list at once"
    );
    assert_eq!(pending_deletes(&root), vec![account(1)]);
    run_for(&mut vcx, 400);
    assert!(fake.was_drained(), "the background poll drained the fake");
    assert!(fake.longest_drain() < Duration::from_millis(500));
    assert!(folder(&root, &account(1)), "TDLib may still be writing");
    assert!(store.get(&account(1)).unwrap().is_some());
    fake.close();
    run_for(&mut vcx, 100);
    assert!(!folder(&root, &account(1)));
    assert!(pending_deletes(&root).is_empty());
    assert!(
        store.get(&account(1)).unwrap().is_none(),
        "keychain item gone"
    );
    assert!(folder(&root, &AccountKey::primary()), "other accounts kept");
    app.read_with(&vcx, |app, _| {
        assert!(app.background.deleting.is_empty());
        assert!(
            app.background.client(&account(1)).is_none(),
            "never restarted"
        );
    });
    let _ = std::fs::remove_dir_all(&root);
}

#[gpui_kit::test]
fn a_removal_never_deletes_while_a_start_is_in_flight_and_ends_the_start(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let root = temp_root();
    // No keychain item: the preparation fails, so no native client could
    // open even if the start were not ended.
    isolate(&app, &mut vcx, &root, &[]);
    app.update_in(&mut vcx, |app, _, cx| {
        app.credentials = Some(credentials());
        app.ensure_background_accounts(cx);
        assert!(app.background.starting.contains(&account(1)));
        assert!(app.background.preparing(&account(1)));
        app.remove_account_in(root.clone(), &account(1), cx);
        assert!(!app.background.starting.contains(&account(1)));
        assert!(
            app.background.preparing(&account(1)),
            "its preparation still runs on a worker"
        );
    });
    assert!(folder(&root, &account(1)), "not while it is being prepared");
    run_for(&mut vcx, 200);
    app.read_with(&vcx, |app, _| {
        assert!(!app.background.preparing(&account(1)));
        assert!(
            app.background.client(&account(1)).is_none(),
            "the ended start added nothing"
        );
    });
    assert!(!folder(&root, &account(1)));
    assert!(pending_deletes(&root).is_empty());
    let _ = std::fs::remove_dir_all(&root);
}

#[gpui_kit::test]
fn an_account_closed_for_a_call_restarts_once_closed_unless_removed(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let root = temp_root();
    isolate(&app, &mut vcx, &root, &[]);
    // `account-2`: registered, never signed in here (starting it opens no
    // native client, it only shows its status).
    assert_eq!(add_account(&root, "Third").unwrap(), account(2));
    let call = Fake::default();
    let removed = Fake::default();
    app.update_in(&mut vcx, |app, _, cx| {
        app.credentials = Some(credentials());
        app.background.closing.push(account(2), call.boxed(), true);
        app.background
            .closing
            .push(account(1), removed.boxed(), true);
        app.remove_account_in(root.clone(), &account(1), cx);
        // While it closes it is not started again.
        app.ensure_background_accounts(cx);
        assert!(app.background.client(&account(2)).is_none());
    });
    call.close();
    removed.close();
    run_for(&mut vcx, 600);
    app.read_with(&vcx, |app, _| {
        assert!(
            app.background.client(&account(2)).is_some(),
            "restarted after its close"
        );
        assert!(app.background.client(&account(1)).is_none());
        assert!(!app.background.starting.contains(&account(1)));
    });
    assert!(!folder(&root, &account(1)));
    let _ = std::fs::remove_dir_all(&root);
}

#[gpui_kit::test]
fn the_lock_screen_log_out_wipes_only_after_every_client_closed(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let root = temp_root();
    let store = isolate(&app, &mut vcx, &root, &[AccountKey::primary(), account(1)]);
    let config = quill::passcode::config_path(&root);
    std::fs::write(&config, b"{}").unwrap();
    let screen = Fake::default();
    let background = Fake::default();
    let generation = app.update_in(&mut vcx, |app, _, cx| {
        app.background
            .closing
            .push(AccountKey::primary(), screen.boxed(), false);
        app.background
            .closing
            .push(account(1), background.boxed(), true);
        app.begin_local_wipe(root.clone(), cx);
        assert!(app.background.wiping);
        // Nothing connects while the data is being wiped.
        let generation = app.connection.generation;
        app.start_connection(cx);
        assert_eq!(app.connection.generation, generation);
        generation
    });
    assert!(!config.exists(), "the passcode goes at once");
    assert_eq!(
        pending_deletes(&root),
        vec![AccountKey::primary(), account(1)]
    );
    run_for(&mut vcx, 300);
    screen.close();
    run_for(&mut vcx, 100);
    assert!(
        folder(&root, &AccountKey::primary()),
        "account-1 still closes"
    );
    assert!(folder(&root, &account(1)));
    background.close();
    run_for(&mut vcx, 100);
    assert!(!folder(&root, &AccountKey::primary()));
    assert!(!folder(&root, &account(1)));
    assert!(pending_deletes(&root).is_empty());
    assert!(store.get(&account(1)).unwrap().is_none());
    assert_eq!(
        list_accounts(&root).len(),
        2,
        "the accounts stay, signed out"
    );
    app.read_with(&vcx, |app, _| {
        assert!(!app.background.wiping);
        assert!(app.connection.generation > generation, "connects afresh");
        assert!(
            app.background.clients.is_empty(),
            "the restart was cancelled"
        );
    });
    let _ = std::fs::remove_dir_all(&root);
}

#[gpui_kit::test]
fn a_quit_before_the_clients_closed_still_removes_and_logs_out_on_the_next_start(
    cx: &mut TestAppContext,
) {
    let (app, mut vcx) = new_app(cx);
    let root = temp_root();
    isolate(&app, &mut vcx, &root, &[]);
    let removed = Fake::default();
    let screen = Fake::default();
    app.update_in(&mut vcx, |app, _, cx| {
        app.background
            .closing
            .push(account(1), removed.boxed(), false);
        app.remove_account_in(root.clone(), &account(1), cx);
        app.background
            .closing
            .push(AccountKey::primary(), screen.boxed(), false);
        app.begin_local_wipe(root.clone(), cx);
        // Quit: neither answers within the shared deadline.
        let started = Instant::now();
        app.close_all_for_quit(started + Duration::from_millis(100));
        assert!(started.elapsed() < Duration::from_secs(2));
    });
    run_for(&mut vcx, 100);
    // Cut off by the deadline: nothing was deleted under them...
    assert!(folder(&root, &account(1)));
    assert!(folder(&root, &AccountKey::primary()));
    // ...and the work is still recorded.
    let mut pending = pending_deletes(&root);
    pending.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(pending, vec![account(1), AccountKey::primary()]);
    app.read_with(&vcx, |app, _| {
        assert!(app.background.wiping, "nothing opens in this run");
    });
    // The next start deletes them before any client opens.
    let (next, mut next_vcx) = new_app(cx);
    isolate(&next, &mut next_vcx, &root, &[]);
    next.update_in(&mut next_vcx, |next, _, _| {
        let Some(sweep) = next.startup_sweep() else {
            panic!("the first connection sweeps");
        };
        let (swept_root, deleted) = sweep.run();
        next.finish_sweep(&swept_root, &deleted);
        assert!(next.startup_sweep().is_none(), "once per run");
    });
    assert!(!folder(&root, &account(1)));
    assert!(!folder(&root, &AccountKey::primary()));
    assert!(pending_deletes(&root).is_empty());
    let _ = std::fs::remove_dir_all(&root);
}

#[gpui_kit::test]
fn the_startup_sweep_skips_accounts_this_run_still_has_open(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let root = temp_root();
    isolate(&app, &mut vcx, &root, &[]);
    quill::settings::mark_for_deletion(&root, &[account(1)]).unwrap();
    let fake = Fake::default();
    app.update_in(&mut vcx, |app, _, _| {
        app.background.closing.push(account(1), fake.boxed(), false);
        let Some(sweep) = app.startup_sweep() else {
            panic!("the first connection sweeps");
        };
        let (_, deleted) = sweep.run();
        assert!(deleted.is_empty());
    });
    assert!(folder(&root, &account(1)));
    assert_eq!(pending_deletes(&root), vec![account(1)]);
    let _ = std::fs::remove_dir_all(&root);
}

#[gpui_kit::test]
fn the_startup_sweep_deletes_only_what_was_recorded_when_it_began(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let root = temp_root();
    isolate(&app, &mut vcx, &root, &[]);
    let fake = Fake::default();
    app.update_in(&mut vcx, |app, _, cx| {
        let Some(sweep) = app.startup_sweep() else {
            panic!("the first connection sweeps");
        };
        // Removed after the sweep began, while its client still closes:
        // the worker must not take the new record.
        app.background.closing.push(account(1), fake.boxed(), false);
        app.remove_account_in(root.clone(), &account(1), cx);
        assert_eq!(pending_deletes(&root), vec![account(1)]);
        let (_, deleted) = sweep.run();
        assert!(deleted.is_empty());
    });
    assert!(folder(&root, &account(1)), "TDLib may still have it open");
    assert_eq!(pending_deletes(&root), vec![account(1)]);
    let _ = std::fs::remove_dir_all(&root);
}

#[cfg(unix)]
#[gpui_kit::test]
fn a_log_out_that_cannot_be_recorded_keeps_the_passcode_and_says_why(cx: &mut TestAppContext) {
    use std::os::unix::fs::PermissionsExt;
    let (app, mut vcx) = new_app(cx);
    let root = temp_root();
    isolate(&app, &mut vcx, &root, &[]);
    let config = quill::passcode::config_path(&root);
    std::fs::write(&config, b"{}").unwrap();
    // A read-only root: the record can't be written.
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o555)).unwrap();
    app.update_in(&mut vcx, |app, _, cx| {
        app.begin_local_wipe(root.clone(), cx);
    });
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(config.exists(), "the passcode stays");
    assert!(folder(&root, &AccountKey::primary()));
    app.read_with(&vcx, |app, _| {
        assert!(!app.background.wiping, "nothing started");
        assert!(
            app.connection
                .status_note()
                .to_string()
                .starts_with("Could not log out"),
            "{}",
            app.connection.status_note()
        );
    });
    let _ = std::fs::remove_dir_all(&root);
}

#[gpui_kit::test]
fn an_unconfirmed_log_out_tells_the_user_to_restart_quill(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let root = temp_root();
    isolate(&app, &mut vcx, &root, &[]);
    app.update_in(&mut vcx, |app, _, cx| {
        let started = Instant::now();
        app.background
            .closing
            .push(AccountKey::primary(), Fake::default().boxed(), false);
        app.begin_local_wipe(root.clone(), cx);
        app.close_all_for_quit(started + Duration::from_millis(100));
    });
    run_for(&mut vcx, 100);
    app.read_with(&vcx, |app, _| {
        assert!(app.background.wiping);
        assert_eq!(
            app.connection.status_note().to_string(),
            "Restart Quill to finish logging out. Nothing connects until then."
        );
    });
    let _ = std::fs::remove_dir_all(&root);
}

#[gpui_kit::test]
fn a_removed_account_takes_its_composer_choices_with_it(cx: &mut TestAppContext) {
    use crate::ui::composer_sends::SendSlot;
    let (app, mut vcx) = new_app(cx);
    let root = temp_root();
    let _store = isolate(&app, &mut vcx, &root, &[account(1)]);
    let slot = |key: AccountKey| SendSlot {
        account: key,
        draft: quill::composer::DraftTopic::Chat.key(11),
    };
    app.update_in(&mut vcx, |app, _, cx| {
        for key in [AccountKey::primary(), account(1)] {
            app.composer_sends
                .update(&slot(key), |send| send.preview.disabled = true);
        }
        app.remove_account_in(root.clone(), &account(1), cx);
        assert!(!app.composer_sends.get(&slot(account(1))).preview.disabled);
        assert!(
            app.composer_sends
                .get(&slot(AccountKey::primary()))
                .preview
                .disabled,
            "other accounts keep theirs"
        );
    });
    let _ = std::fs::remove_dir_all(&root);
}
