//! Account paths and the multi-account registry.
use super::*;

mod deletion;
pub use deletion::*;

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
    /// tdesktop `notifyFromAll` (Settings > Notifications > "Show
    /// notifications from: All accounts"), app-wide like tdesktop's.
    #[serde(default = "default_true")]
    notify_from_all: bool,
    /// Account folders to delete before any client opens them again
    /// (`deletion`): a removal or log-out whose TDLib client was still
    /// closing when the app quit finishes on the next start.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pending_deletes: Vec<AccountKey>,
}

impl Default for AccountRegistry {
    fn default() -> Self {
        Self {
            accounts: vec![AccountRecord {
                key: AccountKey::primary(),
                display_name: "Primary".to_string(),
            }],
            active: Some(AccountKey::primary()),
            notify_from_all: true,
            pending_deletes: Vec::new(),
        }
    }
}

fn registry_path(app_root: &Path) -> PathBuf {
    app_root.join("accounts.json")
}

/// The registry as last read from a readable file, or the default when
/// there was none, and how the file looked then. The UI asks for the active
/// account on every frame (the agent request check), and reading and
/// parsing the file each time showed in an idle profile at 60 frames a
/// second; so did trying to open a missing one, as a single-account setup
/// has no file. A write in this process, the file appearing, or any change
/// to its size or modification time, reads it again. A file
/// changed within `SETTLED` isn't kept: on a file system with coarse times
/// (1 s on HFS+, 2 s on FAT) an edit of the same length made in the same
/// tick would otherwise look unchanged.
static LOADED: std::sync::Mutex<Option<Loaded>> = std::sync::Mutex::new(None);

const SETTLED: std::time::Duration = std::time::Duration::from_secs(3);

struct Loaded {
    path: PathBuf,
    writes: u64,
    /// `None`: there was no file.
    stamp: Option<(std::time::SystemTime, u64)>,
    registry: AccountRegistry,
}

fn file_stamp(path: &Path) -> Option<(std::time::SystemTime, u64)> {
    let meta = std::fs::metadata(path).ok()?;
    Some((meta.modified().ok()?, meta.len()))
}

/// Load the registry; missing or corrupt ⇒ `[primary]` (never a hard error).
fn load_registry(app_root: &Path) -> AccountRegistry {
    let path = registry_path(app_root);
    let writes = registry_writes();
    let stamp = file_stamp(&path);
    let mut loaded = crate::locks::lock_or_recover(&LOADED);
    if let Some(last) = loaded.as_ref()
        && last.path == path
        && last.writes == writes
        && last.stamp == stamp
    {
        return last.registry.clone();
    }
    let parsed = std::fs::read(&path)
        .ok()
        .map(|bytes| serde_json::from_slice::<AccountRegistry>(&bytes));
    let (registry, keep) = match parsed {
        Some(Ok(registry)) => {
            let settled = stamp.is_some_and(|(modified, _)| {
                modified.elapsed().is_ok_and(|elapsed| elapsed >= SETTLED)
            });
            (tidy_registry(registry), settled)
        }
        Some(Err(_)) => {
            quarantine_registry(&path);
            return AccountRegistry::default();
        }
        // No file, before and at the read: the default is kept until one
        // appears. (A file that appeared in between was read, and isn't
        // kept under the missing file's stamp.)
        None if stamp.is_none() => (AccountRegistry::default(), true),
        None => return AccountRegistry::default(),
    };
    if keep {
        *loaded = Some(Loaded {
            path,
            writes,
            stamp,
            registry: registry.clone(),
        });
    }
    registry
}

fn tidy_registry(mut registry: AccountRegistry) -> AccountRegistry {
    // Drop hand-edited garbage and duplicate keys.
    registry
        .accounts
        .retain(|r| AccountKey::new(&r.key.0).is_some());
    let mut seen = std::collections::HashSet::new();
    registry.accounts.retain(|r| seen.insert(r.key.clone()));
    let mut seen = std::collections::HashSet::new();
    registry
        .pending_deletes
        .retain(|key| AccountKey::new(&key.0).is_some() && seen.insert(key.clone()));
    if registry.accounts.is_empty() {
        return AccountRegistry::default();
    }
    if registry
        .active
        .as_ref()
        .is_none_or(|k| !registry.accounts.iter().any(|r| &r.key == k))
    {
        registry.active = registry.accounts.first().map(|r| r.key.clone());
    }
    registry
}

/// Counts registry writes in this process. A reader that caches the registry
/// (`load_registry`, the Dock and Window menus) compares it to know a write
/// happened since.
static REGISTRY_WRITES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// How many times this process has written `accounts.json`; it moves on
/// every sign-in, removal, active-account switch or setting change.
pub fn registry_writes() -> u64 {
    REGISTRY_WRITES.load(std::sync::atomic::Ordering::Acquire)
}

fn save_registry(app_root: &Path, registry: &AccountRegistry) -> std::io::Result<()> {
    let written = write_json_atomic(&registry_path(app_root), registry);
    // After the write: a reader that saw the old count can only have read
    // the file before this write or after it, and the next one reads again.
    REGISTRY_WRITES.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
    // The active account may have changed; the in-memory test flag follows.
    crate::advanced::test_environment::invalidate_active_test(app_root);
    written
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

/// Whether accounts that are not on screen show notifications (default on).
pub fn notify_from_all(app_root: &Path) -> bool {
    load_registry(app_root).notify_from_all
}

pub fn set_notify_from_all(app_root: &Path, on: bool) -> std::io::Result<()> {
    let mut registry = load_registry(app_root);
    registry.notify_from_all = on;
    save_registry(app_root, &registry)
}

/// Add an account with a collision-free key (`account-<n>`); returns the key.
pub fn add_account(app_root: &Path, display_name: &str) -> std::io::Result<AccountKey> {
    let mut registry = load_registry(app_root);
    let mut n = 1u32;
    loop {
        // `account-<n>` is always a valid key; the error is unreachable.
        let candidate = AccountKey::new(&format!("account-{n}")).ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "invalid account key")
        })?;
        // A key whose old folder is still to be deleted is not free yet.
        if !registry.accounts.iter().any(|r| r.key == candidate)
            && !registry.pending_deletes.contains(&candidate)
        {
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

/// Remove an account and delete its directory tree (`accounts/<key>`) at
/// once. Refuses to remove the last remaining account. The app uses
/// [`begin_remove_account`] instead, which waits for the account's TDLib
/// client to close before the folder goes.
pub fn remove_account(app_root: &Path, key: &AccountKey) -> Result<(), RemoveAccountError> {
    begin_remove_account(app_root, key)?;
    delete_account_folder(app_root, key)?;
    finish_pending_delete(app_root, key)?;
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
    // Test installs of the self-updater run beside the user's own Quill with
    // their own data root. Release builds have no such switch.
    #[cfg(feature = "demo-capture")]
    if let Some(root) = std::env::var_os("QUILL_DATA_ROOT").filter(|root| !root.is_empty()) {
        return Some(PathBuf::from(root));
    }
    // Test processes (the lib's tests and, through the self dev-dependency,
    // the binary's) never see the real folder, even before any window or
    // explicit isolation: the first read or write already lands in a
    // per-process temp folder.
    #[cfg(any(test, feature = "test-support"))]
    {
        Some(crate::scratch::process_dir("quill-test-root"))
    }
    #[cfg(not(any(test, feature = "test-support")))]
    {
        real_app_root()
    }
}

/// The platform data directory. Never reached in a test process.
#[cfg_attr(any(test, feature = "test-support"), allow(dead_code))]
fn real_app_root() -> Option<PathBuf> {
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

    /// A test process resolves the app root to a temp folder before anything
    /// else runs, with no window and no explicit isolation: the owner's data
    /// directory is never the answer.
    #[test]
    fn the_app_root_is_a_temp_folder_in_tests() {
        let root = safe_app_root().expect("an app root");
        assert!(root.starts_with(std::env::temp_dir()), "{root:?}");
        assert_ne!(Some(root), real_app_root());
    }

    #[test]
    #[cfg(debug_assertions)]
    fn a_debug_build_never_reaches_the_release_data_folder() {
        let release = directories::ProjectDirs::from("org", "shinycake", "Quill")
            .map(|dirs| dirs.data_dir().to_path_buf());
        assert!(release.is_some());
        assert_ne!(real_app_root(), release);
    }

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
        assert_eq!(std::fs::read(&path).unwrap(), old);
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

    /// The registry is kept between reads; an edit made outside Quill
    /// still shows on the next one.
    #[test]
    fn a_registry_edited_on_disk_is_read_again() {
        let root = tmp_root("edited");
        let work = add_account(&root, "Work").expect("add works");
        set_active_account(&root, &work).expect("set works");
        // A file changed within `SETTLED` is never kept; age it so this
        // read keeps it.
        std::fs::File::options()
            .write(true)
            .open(root.join("accounts.json"))
            .and_then(|file| file.set_modified(std::time::SystemTime::now() - 2 * SETTLED))
            .unwrap();
        assert_eq!(active_account(&root), work);
        let edited =
            r#"{"accounts":[{"key":"primary","display_name":"Primary"}],"active":"primary"}"#;
        std::fs::write(root.join("accounts.json"), edited).unwrap();
        assert_eq!(active_account(&root), AccountKey::primary());
        assert_eq!(list_accounts(&root).len(), 1);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// With no file the default is kept between reads; a file written
    /// outside Quill afterwards shows on the next one.
    #[test]
    fn a_registry_that_appears_on_disk_is_read() {
        let root = tmp_root("appears");
        std::fs::create_dir_all(&root).unwrap();
        assert_eq!(active_account(&root), AccountKey::primary());
        assert_eq!(active_account(&root), AccountKey::primary());
        let written = r#"{"accounts":[{"key":"primary","display_name":"Primary"},{"key":"account-1","display_name":"Work"}],"active":"account-1"}"#;
        std::fs::write(root.join("accounts.json"), written).unwrap();
        assert_eq!(active_account(&root), AccountKey::new("account-1").unwrap());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn every_registry_writer_moves_the_write_count() {
        // The count is process-wide and other tests write too, so each step
        // only asserts that it moved.
        let root = tmp_root("write-count");
        let mut last = registry_writes();
        let mut moved = |what: &str| {
            let now = registry_writes();
            assert!(now > last, "{what} did not move the write count");
            last = now;
        };
        let key = add_account(&root, "Work").expect("add works");
        moved("add_account");
        set_active_account(&root, &key).expect("set works");
        moved("set_active_account");
        set_notify_from_all(&root, false).expect("set works");
        moved("set_notify_from_all");
        mark_for_deletion(&root, std::slice::from_ref(&key)).expect("mark works");
        moved("mark_for_deletion");
        remove_account(&root, &key).expect("remove works");
        moved("remove_account");
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

    #[test]
    fn notify_from_all_defaults_on_and_persists_with_the_accounts() {
        let root = tmp_root("notify-all");
        assert!(notify_from_all(&root), "tdesktop default: on");
        let k1 = add_account(&root, "Work").unwrap();
        set_notify_from_all(&root, false).unwrap();
        assert!(!notify_from_all(&root));
        // Other registry edits keep the choice.
        set_active_account(&root, &k1).unwrap();
        assert!(!notify_from_all(&root));
        // A registry written before the setting existed reads as on.
        std::fs::write(
            root.join("accounts.json"),
            r#"{"accounts":[{"key":"primary","display_name":"Primary"}],"active":"primary"}"#,
        )
        .unwrap();
        assert!(notify_from_all(&root));
        let _ = std::fs::remove_dir_all(&root);
    }
}
