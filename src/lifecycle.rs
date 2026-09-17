//! Session restore, logout, and shutdown — no live Telegram in this crate.

use crate::auth::credentials_ready;
use crate::ids::AccountKey;
use crate::platform::{KeyDecision, SecretStore, load_or_create_key};
use crate::settings::AccountPaths;
use crate::state::{RequestPurpose, ShutdownPhase};

/// Why restore must not open a live TDLib client.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RestoreBlocker {
    MissingCredentials,
    MissingKeyAgainstExistingDb,
    LockedStore,
    StoreError,
}

/// Planned local layout for the next process start.
#[derive(Debug, Clone)]
pub struct RestorePlan {
    pub account: AccountKey,
    pub paths: AccountPaths,
    pub database_exists: bool,
}

/// Explicit quit vs logout. Close is not logout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShutdownKind {
    /// `close` then wait for `authorizationStateClosed`. Databases stay.
    CloseOnly,
    /// `logOut` then wait for closed. Pending requests are invalidated first.
    LogOut,
}

pub fn plan_restore<S: SecretStore>(
    app_root: &std::path::Path,
    account: AccountKey,
    store: &S,
    api_id: Option<i32>,
    api_hash: Option<&str>,
) -> Result<RestorePlan, RestoreBlocker> {
    if !credentials_ready(api_id, api_hash) {
        return Err(RestoreBlocker::MissingCredentials);
    }
    let paths = AccountPaths::for_root(app_root, &account);
    let database_exists = paths.database_exists();
    match load_or_create_key(store, &account, database_exists) {
        Ok(_) => Ok(RestorePlan {
            account,
            paths,
            database_exists,
        }),
        Err(KeyDecision::MissingAgainstExistingDb) => {
            Err(RestoreBlocker::MissingKeyAgainstExistingDb)
        }
        Err(KeyDecision::Locked) => Err(RestoreBlocker::LockedStore),
        Err(KeyDecision::Store(_)) => Err(RestoreBlocker::StoreError),
    }
}

/// First TDLib request for a shutdown kind. Callers must wait for Closed.
pub fn shutdown_request(kind: ShutdownKind) -> RequestPurpose {
    match kind {
        ShutdownKind::CloseOnly => RequestPurpose::Close,
        ShutdownKind::LogOut => RequestPurpose::LogOut,
    }
}

pub fn shutdown_is_complete(phase: ShutdownPhase) -> bool {
    matches!(phase, ShutdownPhase::Closed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::MemorySecretStore;
    use std::fs;

    #[test]
    fn restore_stops_without_owner_credentials() {
        let dir = std::env::temp_dir().join(format!("quill-restore-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let store = MemorySecretStore::new();
        let err = plan_restore(&dir, AccountKey::primary(), &store, None, None).unwrap_err();
        assert_eq!(err, RestoreBlocker::MissingCredentials);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn restore_does_not_mint_key_against_existing_db() {
        let dir = std::env::temp_dir().join(format!("quill-restore-db-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let paths = AccountPaths::for_root(&dir, &AccountKey::primary());
        fs::create_dir_all(&paths.tdlib_database).unwrap();
        fs::write(paths.tdlib_database.join("td.binlog"), b"x").unwrap();
        let store = MemorySecretStore::new();
        let err = plan_restore(
            &dir,
            AccountKey::primary(),
            &store,
            Some(1),
            Some("not-a-sample"),
        )
        .unwrap_err();
        assert_eq!(err, RestoreBlocker::MissingKeyAgainstExistingDb);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn close_is_not_logout() {
        assert_eq!(
            shutdown_request(ShutdownKind::CloseOnly),
            RequestPurpose::Close
        );
        assert_eq!(
            shutdown_request(ShutdownKind::LogOut),
            RequestPurpose::LogOut
        );
        assert!(!shutdown_is_complete(ShutdownPhase::CloseRequested));
        assert!(shutdown_is_complete(ShutdownPhase::Closed));
    }
}
