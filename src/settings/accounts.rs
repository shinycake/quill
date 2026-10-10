//! Account paths and the multi-account registry.
use super::*;

#[derive(Debug, Clone)]
pub struct AccountPaths {
    pub root: PathBuf,
    pub tdlib_database: PathBuf,
    pub tdlib_files: PathBuf,
    pub app_thumbnails: PathBuf,
    pub exports: PathBuf,
}

impl AccountPaths {
    pub fn for_root(app_root: &Path, account: &AccountKey) -> Self {
        let root = app_root.join("accounts").join(&account.0);
        Self {
            tdlib_database: root.join("tdlib"),
            tdlib_files: root.join("files"),
            app_thumbnails: root.join("thumbnails"),
            exports: root.join("exports"),
            root,
        }
    }

    pub fn database_exists(&self) -> bool {
        self.tdlib_database.join("db.sqlite").exists()
            || self.tdlib_database.join("td.binlog").exists()
            || directory_nonempty(&self.tdlib_database)
    }
}

fn directory_nonempty(path: &Path) -> bool {
    std::fs::read_dir(path)
        .ok()
        .map(|mut it| it.next().is_some())
        .unwrap_or(false)
}

/// Multi-account registry (`accounts.json` at the app root, next to
/// `accounts/`). Missing or corrupt files fall back to `[primary]` — prefs
/// must not block startup.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AccountRecord {
    pub key: AccountKey,
    pub display_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AccountRegistry {
    accounts: Vec<AccountRecord>,
    #[serde(default)]
    active: Option<AccountKey>,
}

impl Default for AccountRegistry {
    fn default() -> Self {
        Self {
            accounts: vec![AccountRecord {
                key: AccountKey::primary(),
                display_name: "Primary".to_string(),
            }],
            active: Some(AccountKey::primary()),
        }
    }
}

fn registry_path(app_root: &Path) -> PathBuf {
    app_root.join("accounts.json")
}

/// Load the registry; missing or corrupt ⇒ `[primary]` (never a hard error).
fn load_registry(app_root: &Path) -> AccountRegistry {
    let path = registry_path(app_root);
    let mut registry: AccountRegistry = match std::fs::read(&path) {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_else(|_| {
            quarantine_registry(&path);
            AccountRegistry::default()
        }),
        Err(_) => AccountRegistry::default(),
    };
    // Drop hand-edited garbage and duplicate keys.
    registry
        .accounts
        .retain(|r| AccountKey::new(&r.key.0).is_some());
    let mut seen = std::collections::HashSet::new();
    registry.accounts.retain(|r| seen.insert(r.key.clone()));
    if registry.accounts.is_empty() {
        return AccountRegistry::default();
    }
    if registry
        .active
        .as_ref()
        .is_none_or(|k| !registry.accounts.iter().any(|r| &r.key == k))
    {
        registry.active = Some(registry.accounts[0].key.clone());
    }
    registry
}

fn save_registry(app_root: &Path, registry: &AccountRegistry) -> std::io::Result<()> {
    write_json_atomic(&registry_path(app_root), registry)
}

/// Move an unreadable `accounts.json` aside as `accounts.json.corrupt-<ts>`
/// (never over an existing backup) so the account list stays recoverable
/// instead of being overwritten by the next save.
fn quarantine_registry(path: &Path) {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let base = format!("{}.corrupt-{ts}", path.display());
    let mut backup = PathBuf::from(&base);
    for n in 1.. {
        if !backup.exists() {
            break;
        }
        backup = PathBuf::from(format!("{base}-{n}"));
    }
    match std::fs::rename(path, &backup) {
        Ok(()) => eprintln!(
            "quill: accounts.json is unreadable; moved it to {} and starting from defaults",
            backup.display()
        ),
        Err(err) => {
            eprintln!("quill: accounts.json is unreadable and could not be backed up: {err}")
        }
    }
}

/// All known accounts; missing or corrupt registry ⇒ `[primary]`.
pub fn list_accounts(app_root: &Path) -> Vec<AccountRecord> {
    load_registry(app_root).accounts
}

/// The account to connect at startup; persisted in the registry,
/// default `primary`.
pub fn active_account(app_root: &Path) -> AccountKey {
    load_registry(app_root)
        .active
        .unwrap_or_else(AccountKey::primary)
}

/// Persist the startup account. Errors when `key` is not in the registry.
pub fn set_active_account(app_root: &Path, key: &AccountKey) -> std::io::Result<()> {
    let mut registry = load_registry(app_root);
    if !registry.accounts.iter().any(|r| &r.key == key) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("no such account: {key}"),
        ));
    }
    registry.active = Some(key.clone());
    save_registry(app_root, &registry)
}

/// Add an account with a collision-free key (`account-<n>`); returns the key.
pub fn add_account(app_root: &Path, display_name: &str) -> std::io::Result<AccountKey> {
    let mut registry = load_registry(app_root);
    let mut n = 1u32;
    loop {
        let candidate = AccountKey::new(&format!("account-{n}")).expect("generated id is valid");
        if !registry.accounts.iter().any(|r| r.key == candidate) {
            registry.accounts.push(AccountRecord {
                key: candidate.clone(),
                display_name: display_name.to_string(),
            });
            save_registry(app_root, &registry)?;
            return Ok(candidate);
        }
        n += 1;
    }
}

/// Refusing to remove the last remaining account, or deleting an account's
/// directory tree, must not panic — typed error instead.
#[derive(Debug)]
pub enum RemoveAccountError {
    /// The registry must always keep at least one account.
    LastAccount,
    Io(std::io::Error),
}

impl std::fmt::Display for RemoveAccountError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::LastAccount => write!(f, "cannot remove the last remaining account"),
            Self::Io(e) => write!(f, "failed to remove account: {e}"),
        }
    }
}

impl std::error::Error for RemoveAccountError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for RemoveAccountError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

/// Remove an account and delete its directory tree (`accounts/<key>`).
/// Refuses to remove the last remaining account.
pub fn remove_account(app_root: &Path, key: &AccountKey) -> Result<(), RemoveAccountError> {
    let mut registry = load_registry(app_root);
    if registry.accounts.len() <= 1 {
        return Err(RemoveAccountError::LastAccount);
    }
    registry.accounts.retain(|r| &r.key != key);
    if registry.active.as_ref() == Some(key) {
        registry.active = registry.accounts.first().map(|r| r.key.clone());
    }
    // The DB tree is gone once the record is; a missing dir is not an error.
    let dir = app_root.join("accounts").join(&key.0);
    if let Err(e) = std::fs::remove_dir_all(&dir)
        && e.kind() != std::io::ErrorKind::NotFound
    {
        return Err(RemoveAccountError::Io(e));
    }
    save_registry(app_root, &registry)?;
    Ok(())
}

/// Platform data directory, or `None` when the OS provides none. There is
/// deliberately no `./quill-data` fallback: live startup must refuse rather
/// than scatter account databases and encryption keys under whatever
/// directory the process was launched from (a broad `git add` there would
/// commit them).
pub fn safe_app_root() -> Option<PathBuf> {
    if let Some(root) = ISOLATED_APP_ROOT.get() {
        return Some(root.clone());
    }
    directories::ProjectDirs::from("org", "shinycake", APP_DIR_NAME)
        .map(|dirs| dirs.data_dir().to_path_buf())
}

static ISOLATED_APP_ROOT: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

/// Route every account/settings path for the rest of the process to
/// `root` instead of the user's data directory. Screenshot demos call this
/// first so fixtures start from default settings and never read or
/// overwrite the user's real preferences. Returns `false` if a root was
/// already chosen (the first one wins).
pub fn use_isolated_app_root(root: PathBuf) -> bool {
    ISOLATED_APP_ROOT.set(root).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn account_paths_are_scoped() {
        let root = PathBuf::from("/tmp/quill-test");
        let paths = AccountPaths::for_root(&root, &AccountKey::primary());
        assert!(paths.tdlib_database.ends_with("accounts/primary/tdlib"));
        assert!(paths.tdlib_files.ends_with("accounts/primary/files"));
    }

    #[test]
    fn empty_dir_is_not_an_existing_database() {
        let dir = std::env::temp_dir().join(format!("quill-empty-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("accounts/primary/tdlib")).unwrap();
        let paths = AccountPaths::for_root(&dir, &AccountKey::primary());
        assert!(!paths.database_exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn gitignore_backstops_account_data() {
        // The .gitignore entries are the last line of defense against a
        // broad `git add` committing live account state. If someone edits
        // them away, this fails loudly.
        let ignore =
            fs::read(format!("{}/.gitignore", env!("CARGO_MANIFEST_DIR"))).expect(".gitignore");
        let ignore = String::from_utf8(ignore).expect("utf8");
        for entry in [
            "quill-data/",
            "db-encryption.key",
            "td.binlog",
            "db.sqlite",
            "db.sqlite-wal",
            "db.sqlite-shm",
        ] {
            assert!(
                ignore.lines().any(|line| line.trim() == entry),
                ".gitignore must contain exact entry: {entry}"
            );
        }
    }
}

#[cfg(test)]
mod account_registry_tests {
    use super::*;

    fn tmp_root(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("quill-accounts-test-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn missing_registry_falls_back_to_primary() {
        let root = tmp_root("missing");
        let accounts = list_accounts(&root);
        assert_eq!(accounts.len(), 1);
        assert_eq!(accounts[0].key, AccountKey::primary());
        assert_eq!(active_account(&root), AccountKey::primary());
    }

    #[test]
    fn corrupt_registry_falls_back_to_primary() {
        let root = tmp_root("corrupt");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("accounts.json"), b"not json{{").unwrap();
        assert_eq!(
            list_accounts(&root),
            list_accounts(&tmp_root("missing-never-written"))
        );
        assert_eq!(active_account(&root), AccountKey::primary());
        let backups = corrupt_backups(&root);
        assert_eq!(backups.len(), 1);
        assert_eq!(std::fs::read(&backups[0]).unwrap(), b"not json{{");
        assert!(!root.join("accounts.json").exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    fn corrupt_backups(root: &Path) -> Vec<PathBuf> {
        let mut found: Vec<PathBuf> = std::fs::read_dir(root)
            .unwrap()
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with("accounts.json.corrupt-"))
            })
            .collect();
        found.sort();
        found
    }

    #[test]
    fn truncated_registry_is_backed_up_and_never_overwrites_old_backup() {
        let root = tmp_root("truncated");
        add_account(&root, "Work").expect("add works");
        let good = std::fs::read(root.join("accounts.json")).unwrap();
        std::fs::write(root.join("accounts.json"), &good[..good.len() / 2]).unwrap();
        assert_eq!(list_accounts(&root).len(), 1);
        assert_eq!(corrupt_backups(&root).len(), 1);
        // A second corruption in the same second must keep both backups.
        std::fs::write(root.join("accounts.json"), b"{").unwrap();
        let _ = list_accounts(&root);
        let backups = corrupt_backups(&root);
        assert_eq!(backups.len(), 2);
        assert_eq!(std::fs::read(&backups[0]).unwrap(), &good[..good.len() / 2]);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn atomic_write_replaces_existing_file_and_leaves_no_temp() {
        let root = tmp_root("atomic-replace");
        let path = root.join("x.json");
        write_json_atomic(&path, &vec![1, 2]).unwrap();
        write_json_atomic(&path, &vec![3]).unwrap();
        assert_eq!(
            serde_json::from_slice::<Vec<i32>>(&std::fs::read(&path).unwrap()).unwrap(),
            vec![3]
        );
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn atomic_write_failure_keeps_old_file() {
        let root = tmp_root("atomic-fail");
        let path = root.join("x.json");
        write_json_atomic(&path, &vec![1]).unwrap();
        let old = std::fs::read(&path).unwrap();
        // Fail at the rename step: the target path is now a directory.
        let blocked = root.join("blocked.json");
        std::fs::create_dir_all(blocked.join("child")).unwrap();
        assert!(write_json_atomic(&blocked, &vec![2]).is_err());
        assert_eq!(
            std::fs::read_dir(&root).unwrap().count(),
            2,
            "temp file cleaned up"
        );
        // Fail at the create step: read-only directory (Unix permissions).
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&root).unwrap().permissions();
            perms.set_mode(0o555);
            std::fs::set_permissions(&root, perms.clone()).unwrap();
            let denied = write_json_atomic(&path, &vec![9]);
            perms.set_mode(0o755);
            std::fs::set_permissions(&root, perms).unwrap();
            // Root can write anywhere; only assert when the OS denied it.
            if denied.is_err() {
                assert_eq!(std::fs::read(&path).unwrap(), old);
            }
        }
        assert_eq!(std::fs::read(&path).unwrap(), old);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn add_list_remove_roundtrip() {
        let root = tmp_root("roundtrip");
        let k1 = add_account(&root, "Work").expect("add works");
        let k2 = add_account(&root, "Personal").expect("add works");
        // Collision-free keys.
        assert_ne!(k1, k2);
        assert_ne!(k1, AccountKey::primary());
        let keys: Vec<String> = list_accounts(&root)
            .iter()
            .map(|r| r.key.0.clone())
            .collect();
        assert_eq!(
            keys,
            vec!["primary".to_string(), k1.0.clone(), k2.0.clone()]
        );
        // display names survive the round-trip.
        let records = list_accounts(&root);
        let names: Vec<&str> = records.iter().map(|r| r.display_name.as_str()).collect();
        assert_eq!(names, vec!["Primary", "Work", "Personal"]);

        // Removing deletes the account's directory tree.
        let dir = root.join("accounts").join(&k2.0);
        std::fs::create_dir_all(dir.join("tdlib")).unwrap();
        remove_account(&root, &k2).expect("remove works");
        assert!(!root.join("accounts").join(&k2.0).exists());
        let keys: Vec<String> = list_accounts(&root)
            .iter()
            .map(|r| r.key.0.clone())
            .collect();
        assert_eq!(keys, vec!["primary".to_string(), k1.0.clone()]);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn remove_refuses_last_remaining_account() {
        let root = tmp_root("last");
        // Fresh registry holds only primary.
        assert!(matches!(
            remove_account(&root, &AccountKey::primary()),
            Err(RemoveAccountError::LastAccount)
        ));
        let k1 = add_account(&root, "Work").expect("add works");
        remove_account(&root, &AccountKey::primary())
            .expect("removing primary is fine while others remain");
        // Now k1 is the last one — removal is refused, no panic.
        assert!(matches!(
            remove_account(&root, &k1),
            Err(RemoveAccountError::LastAccount)
        ));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn active_account_persists_and_rejects_unknown() {
        let root = tmp_root("active");
        let k1 = add_account(&root, "Work").expect("add works");
        set_active_account(&root, &k1).expect("set works");
        assert_eq!(active_account(&root), k1);
        // Unknown key ⇒ NotFound, and the active account is unchanged.
        let err = set_active_account(&root, &AccountKey::new("nope").unwrap()).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
        assert_eq!(active_account(&root), k1);
        // Removing the active account falls back to a remaining one.
        remove_account(&root, &k1).expect("remove works");
        assert_eq!(active_account(&root), AccountKey::primary());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn account_paths_are_isolated_per_account() {
        let root = PathBuf::from("/tmp/quill-accounts-test-isolation");
        let p1 = AccountPaths::for_root(&root, &AccountKey::primary());
        let p2 = AccountPaths::for_root(&root, &AccountKey::new("account-1").unwrap());
        assert!(p1.tdlib_database.ends_with("accounts/primary/tdlib"));
        assert!(p2.tdlib_database.ends_with("accounts/account-1/tdlib"));
        assert_ne!(p1.tdlib_database, p2.tdlib_database);
        assert_ne!(p1.tdlib_files, p2.tdlib_files);
    }
}
