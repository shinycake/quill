//! Account folders waiting to be deleted.
//!
//! A TDLib client must have closed (`authorizationStateClosed`) before its
//! database folder is deleted, and that can take a moment. Removing an
//! account or logging out on the lock screen therefore records the folders
//! first (`pending_deletes` in `accounts.json`), deletes them once the
//! clients have closed, and only then clears the record. If the app quits
//! (or crashes) in between, the next start finishes the job before any
//! client opens: the user asked for the data to go, so it goes.

use super::*;

/// Take `key` out of the account list at once and record its folder for
/// deletion. `Ok(false)`: it was not registered (already removed).
/// Refuses to remove the last remaining account.
pub fn begin_remove_account(app_root: &Path, key: &AccountKey) -> Result<bool, RemoveAccountError> {
    let mut registry = load_registry(app_root);
    if !registry.accounts.iter().any(|r| &r.key == key) {
        return Ok(false);
    }
    if registry.accounts.len() <= 1 {
        return Err(RemoveAccountError::LastAccount);
    }
    registry.accounts.retain(|r| &r.key != key);
    if registry.active.as_ref() == Some(key) {
        registry.active = registry.accounts.first().map(|r| r.key.clone());
    }
    if !registry.pending_deletes.contains(key) {
        registry.pending_deletes.push(key.clone());
    }
    save_registry(app_root, &registry)?;
    Ok(true)
}

/// Record `keys`' folders for deletion; their accounts stay in the list
/// (the lock-screen log out: every account signs in again).
pub fn mark_for_deletion(app_root: &Path, keys: &[AccountKey]) -> std::io::Result<()> {
    let mut registry = load_registry(app_root);
    for key in keys {
        if !registry.pending_deletes.contains(key) {
            registry.pending_deletes.push(key.clone());
        }
    }
    save_registry(app_root, &registry)
}

/// Folders recorded for deletion and not deleted yet.
pub fn pending_deletes(app_root: &Path) -> Vec<AccountKey> {
    load_registry(app_root).pending_deletes
}

/// Delete `key`'s folder (`accounts/<key>`); a missing folder is fine.
/// Never call it while a TDLib client of `key` may still be open.
pub fn delete_account_folder(app_root: &Path, key: &AccountKey) -> std::io::Result<()> {
    // A queued settings write would recreate the folder after it is gone.
    crate::settings::flush_deferred_writes();
    let removed = std::fs::remove_dir_all(AccountPaths::for_root(app_root, key).root);
    // The folder held the test-environment marker.
    crate::advanced::test_environment::invalidate_active_test(app_root);
    match removed {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

/// `key`'s folder is gone: forget the record.
pub fn finish_pending_delete(app_root: &Path, key: &AccountKey) -> std::io::Result<()> {
    let mut registry = load_registry(app_root);
    if !registry.pending_deletes.contains(key) {
        return Ok(());
    }
    registry.pending_deletes.retain(|pending| pending != key);
    save_registry(app_root, &registry)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_root(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "quill-deletion-{tag}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn database(root: &Path, key: &AccountKey) -> PathBuf {
        let db = AccountPaths::for_root(root, key).tdlib_database;
        std::fs::create_dir_all(&db).unwrap();
        std::fs::write(db.join("td.binlog"), b"x").unwrap();
        db
    }

    #[test]
    fn a_removed_account_leaves_the_list_at_once_but_its_folder_waits() {
        let root = tmp_root("begin");
        let work = add_account(&root, "Work").unwrap();
        let db = database(&root, &work);
        assert!(begin_remove_account(&root, &work).unwrap());
        assert!(!list_accounts(&root).iter().any(|r| r.key == work));
        assert!(db.exists(), "deleted only once the client has closed");
        assert_eq!(pending_deletes(&root), vec![work.clone()]);
        // A second Remove is a no-op.
        assert!(!begin_remove_account(&root, &work).unwrap());
        assert_eq!(pending_deletes(&root), vec![work.clone()]);
        delete_account_folder(&root, &work).unwrap();
        finish_pending_delete(&root, &work).unwrap();
        assert!(!db.exists());
        assert!(pending_deletes(&root).is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_key_whose_folder_is_still_to_be_deleted_is_not_reused() {
        let root = tmp_root("reuse");
        let work = add_account(&root, "Work").unwrap();
        database(&root, &work);
        begin_remove_account(&root, &work).unwrap();
        let next = add_account(&root, "Next").unwrap();
        assert_ne!(next, work, "the old database must never be handed on");
        finish_pending_delete(&root, &work).unwrap();
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_log_out_keeps_the_accounts_and_survives_a_restart() {
        let root = tmp_root("wipe");
        let work = add_account(&root, "Work").unwrap();
        let keys = vec![AccountKey::primary(), work.clone()];
        mark_for_deletion(&root, &keys).unwrap();
        mark_for_deletion(&root, &keys).unwrap();
        assert_eq!(list_accounts(&root).len(), 2);
        // Read back from disk, as the next start does.
        assert_eq!(pending_deletes(&root), keys);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_last_account_cannot_be_removed() {
        let root = tmp_root("last");
        assert!(matches!(
            begin_remove_account(&root, &AccountKey::primary()),
            Err(RemoveAccountError::LastAccount)
        ));
        assert!(pending_deletes(&root).is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_missing_folder_is_already_deleted() {
        let root = tmp_root("missing");
        let key = AccountKey::new("account-9").unwrap();
        delete_account_folder(&root, &key).unwrap();
        finish_pending_delete(&root, &key).unwrap();
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_setting_still_queued_does_not_bring_the_folder_back() {
        let root = tmp_root("queued");
        let work = add_account(&root, "Work").unwrap();
        let paths = AccountPaths::for_root(&root, &work);
        let prefs = paths.root.join("hashtags.json");
        crate::settings::write_json_deferred(&prefs, &["#queued"]).unwrap();
        delete_account_folder(&root, &work).unwrap();
        assert!(!paths.root.exists());
        assert!(
            crate::settings::read_with_pending(&prefs).is_err(),
            "nothing is left to write into the deleted folder"
        );
        let _ = std::fs::remove_dir_all(&root);
    }
}
