//! OS credential store for the TDLib database encryption key.
//!
//! macOS uses Keychain. Linux uses a 0600 file under the account-scoped app data
//! directory (`FileSecretStore`). `MemorySecretStore` remains for unit tests only
//! and must not be the live-connect path on Linux.
//! A missing or locked item must never be replaced silently against an existing DB.

use crate::ids::AccountKey;
use crate::settings::AccountPaths;
use rand::RngCore;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use thiserror::Error;
use zeroize::{Zeroize, ZeroizeOnDrop};

pub const KEYCHAIN_SERVICE: &str = "org.shinycake.quill";

#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct DatabaseKey(Vec<u8>);

impl DatabaseKey {
    pub fn generate() -> Self {
        let mut bytes = vec![0u8; 32];
        rand::thread_rng().fill_bytes(&mut bytes);
        Self(bytes)
    }

    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self, SecretStoreError> {
        if bytes.len() != 32 {
            return Err(SecretStoreError::InvalidKey);
        }
        Ok(Self(bytes))
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    pub fn tdlib_base64(&self) -> String {
        use base64::Engine;
        base64::engine::general_purpose::STANDARD.encode(&self.0)
    }
}

impl std::fmt::Debug for DatabaseKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("DatabaseKey(<redacted>)")
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum SecretStoreError {
    #[error("secret store is locked or unavailable")]
    Locked,
    #[error("secret is missing")]
    Missing,
    #[error("stored key is invalid")]
    InvalidKey,
    #[error("platform secret store error")]
    Platform,
}

pub trait SecretStore: Send + Sync {
    fn get(&self, account: &AccountKey) -> Result<Option<DatabaseKey>, SecretStoreError>;
    fn put(&self, account: &AccountKey, key: &DatabaseKey) -> Result<(), SecretStoreError>;
    fn delete(&self, account: &AccountKey) -> Result<(), SecretStoreError>;
}

/// In-memory store for unit tests. Not used for live connect on Linux or macOS.
#[derive(Clone, Default)]
pub struct MemorySecretStore {
    inner: Arc<Mutex<HashMap<String, Vec<u8>>>>,
    pub locked: Arc<Mutex<bool>>,
}

/// Secret store that always fails: used when no safe platform data directory
/// exists, so live startup refuses instead of writing keys under the working
/// directory.
pub struct UnavailableSecretStore;

impl SecretStore for UnavailableSecretStore {
    fn get(&self, _account: &AccountKey) -> Result<Option<DatabaseKey>, SecretStoreError> {
        Err(SecretStoreError::Locked)
    }
    fn put(&self, _account: &AccountKey, _key: &DatabaseKey) -> Result<(), SecretStoreError> {
        Err(SecretStoreError::Locked)
    }
    fn delete(&self, _account: &AccountKey) -> Result<(), SecretStoreError> {
        Err(SecretStoreError::Locked)
    }
}

impl MemorySecretStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn lock(&self) {
        *self.locked.lock().expect("lock flag") = true;
    }

    pub fn unlock(&self) {
        *self.locked.lock().expect("lock flag") = false;
    }
}

impl SecretStore for MemorySecretStore {
    fn get(&self, account: &AccountKey) -> Result<Option<DatabaseKey>, SecretStoreError> {
        if *self.locked.lock().expect("lock flag") {
            return Err(SecretStoreError::Locked);
        }
        let map = self.inner.lock().expect("store");
        match map.get(&account.0) {
            None => Ok(None),
            Some(bytes) => Ok(Some(DatabaseKey::from_bytes(bytes.clone())?)),
        }
    }

    fn put(&self, account: &AccountKey, key: &DatabaseKey) -> Result<(), SecretStoreError> {
        if *self.locked.lock().expect("lock flag") {
            return Err(SecretStoreError::Locked);
        }
        self.inner
            .lock()
            .expect("store")
            .insert(account.0.clone(), key.as_bytes().to_vec());
        Ok(())
    }

    fn delete(&self, account: &AccountKey) -> Result<(), SecretStoreError> {
        if *self.locked.lock().expect("lock flag") {
            return Err(SecretStoreError::Locked);
        }
        self.inner.lock().expect("store").remove(&account.0);
        Ok(())
    }
}

/// Persistent file-backed store for Linux (and other non-Keychain hosts in tests).
///
/// Key path: `{app_root}/accounts/{account}/db-encryption.key`, mode `0600`.
/// Bytes read from disk are zeroized after copying into `DatabaseKey`.
#[derive(Clone, Debug)]
pub struct FileSecretStore {
    app_root: PathBuf,
}

impl FileSecretStore {
    pub fn new(app_root: impl Into<PathBuf>) -> Self {
        Self {
            app_root: app_root.into(),
        }
    }

    pub fn app_root(&self) -> &Path {
        &self.app_root
    }

    pub(crate) fn key_path(&self, account: &AccountKey) -> PathBuf {
        AccountPaths::for_root(&self.app_root, account)
            .root
            .join("db-encryption.key")
    }
}

impl SecretStore for FileSecretStore {
    fn get(&self, account: &AccountKey) -> Result<Option<DatabaseKey>, SecretStoreError> {
        let path = self.key_path(account);
        if !path.exists() {
            return Ok(None);
        }
        let stored = std::fs::read(&path).map_err(|_| SecretStoreError::Platform)?;
        let mut bytes = unseal_key_bytes(stored)?;
        let key = DatabaseKey::from_bytes(bytes.clone());
        bytes.zeroize();
        Ok(Some(key?))
    }

    fn put(&self, account: &AccountKey, key: &DatabaseKey) -> Result<(), SecretStoreError> {
        let path = self.key_path(account);
        let parent = path.parent().ok_or(SecretStoreError::Platform)?;
        std::fs::create_dir_all(parent).map_err(|_| SecretStoreError::Platform)?;
        let tmp = path.with_extension("key.tmp");
        let mut sealed = seal_key_bytes(key.as_bytes())?;
        let written = write_key_file_0600(&tmp, &sealed);
        sealed.zeroize();
        written?;
        std::fs::rename(&tmp, &path).map_err(|_| SecretStoreError::Platform)?;
        set_mode_0600(&path)?;
        Ok(())
    }

    fn delete(&self, account: &AccountKey) -> Result<(), SecretStoreError> {
        let path = self.key_path(account);
        match std::fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err(SecretStoreError::Platform),
        }
    }
}

/// On-disk form of the key. Unix keeps the raw bytes in a 0600 file. Windows
/// has no 0600, so the bytes are sealed with DPAPI (`CryptProtectData`,
/// current-user scope): another user or machine can't decrypt the file.
#[cfg(not(windows))]
fn seal_key_bytes(bytes: &[u8]) -> Result<Vec<u8>, SecretStoreError> {
    Ok(bytes.to_vec())
}

#[cfg(not(windows))]
fn unseal_key_bytes(stored: Vec<u8>) -> Result<Vec<u8>, SecretStoreError> {
    Ok(stored)
}

#[cfg(windows)]
fn seal_key_bytes(bytes: &[u8]) -> Result<Vec<u8>, SecretStoreError> {
    dpapi::protect(bytes).ok_or(SecretStoreError::Platform)
}

#[cfg(windows)]
fn unseal_key_bytes(mut stored: Vec<u8>) -> Result<Vec<u8>, SecretStoreError> {
    let plain = dpapi::unprotect(&stored).ok_or(SecretStoreError::Platform);
    stored.zeroize();
    plain
}

#[cfg(windows)]
mod dpapi {
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{
        CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData, CryptUnprotectData,
    };

    pub(super) fn protect(plain: &[u8]) -> Option<Vec<u8>> {
        transform(plain, true)
    }

    pub(super) fn unprotect(sealed: &[u8]) -> Option<Vec<u8>> {
        transform(sealed, false)
    }

    fn transform(input: &[u8], seal: bool) -> Option<Vec<u8>> {
        let blob_in = CRYPT_INTEGER_BLOB {
            cbData: u32::try_from(input.len()).ok()?,
            pbData: input.as_ptr().cast_mut(),
        };
        let mut blob_out = CRYPT_INTEGER_BLOB {
            cbData: 0,
            pbData: std::ptr::null_mut(),
        };
        // SAFETY: `blob_in` points at `input`, which outlives the call; DPAPI
        // only reads it. On success DPAPI allocates `blob_out.pbData` with
        // LocalAlloc; it is copied out and freed with LocalFree below.
        let ok = unsafe {
            if seal {
                CryptProtectData(
                    &blob_in,
                    std::ptr::null(),
                    std::ptr::null(),
                    std::ptr::null(),
                    std::ptr::null(),
                    CRYPTPROTECT_UI_FORBIDDEN,
                    &mut blob_out,
                )
            } else {
                CryptUnprotectData(
                    &blob_in,
                    std::ptr::null_mut(),
                    std::ptr::null(),
                    std::ptr::null(),
                    std::ptr::null(),
                    CRYPTPROTECT_UI_FORBIDDEN,
                    &mut blob_out,
                )
            }
        };
        if ok == 0 || blob_out.pbData.is_null() {
            return None;
        }
        // SAFETY: DPAPI returned `cbData` valid bytes at `pbData`.
        let out = unsafe {
            std::slice::from_raw_parts(blob_out.pbData, blob_out.cbData as usize).to_vec()
        };
        // SAFETY: wipe and release DPAPI's LocalAlloc buffer.
        unsafe {
            std::ptr::write_bytes(blob_out.pbData, 0, blob_out.cbData as usize);
            LocalFree(blob_out.pbData.cast());
        }
        Some(out)
    }
}

fn write_key_file_0600(path: &Path, bytes: &[u8]) -> Result<(), SecretStoreError> {
    use std::io::Write;
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)
            .map_err(|_| SecretStoreError::Platform)?;
        file.write_all(bytes)
            .map_err(|_| SecretStoreError::Platform)?;
        file.sync_all().map_err(|_| SecretStoreError::Platform)?;
    }
    #[cfg(not(unix))]
    {
        let mut file = std::fs::File::create(path).map_err(|_| SecretStoreError::Platform)?;
        file.write_all(bytes)
            .map_err(|_| SecretStoreError::Platform)?;
        file.sync_all().map_err(|_| SecretStoreError::Platform)?;
    }
    set_mode_0600(path)
}

fn set_mode_0600(path: &Path) -> Result<(), SecretStoreError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o600);
        std::fs::set_permissions(path, perms).map_err(|_| SecretStoreError::Platform)?;
    }
    let _ = path;
    Ok(())
}

pub fn account_item_name(account: &AccountKey) -> String {
    format!("db-key:{account}")
}

/// Classify a Security.framework OSStatus from `SecItemCopyMatching`.
///
/// Missing (`errSecItemNotFound`) is `Ok(None)`. User cancel / lock /
/// interaction-not-allowed is `Locked`. Other failures stay `Platform`.
pub fn classify_keychain_status(code: i32) -> Result<(), SecretStoreError> {
    match code {
        // errSecItemNotFound — caller maps this to Ok(None), not an error.
        ERR_SEC_ITEM_NOT_FOUND => Err(SecretStoreError::Missing),
        ERR_SEC_USER_CANCELED
        | ERR_SEC_AUTH_FAILED
        | ERR_SEC_INTERACTION_NOT_ALLOWED
        | ERR_SEC_NOT_AVAILABLE => Err(SecretStoreError::Locked),
        _ => Err(SecretStoreError::Platform),
    }
}

/// Map a Keychain get failure: missing vs locked vs platform.
pub fn map_keychain_get_error(code: i32) -> Result<Option<DatabaseKey>, SecretStoreError> {
    match classify_keychain_status(code) {
        Err(SecretStoreError::Missing) => Ok(None),
        Err(other) => Err(other),
        Ok(()) => Err(SecretStoreError::Platform),
    }
}

// Apple OSStatus values (Security.framework). Kept as integers so Linux tests
// can prove Locked vs Missing without linking the macOS SDK.
const ERR_SEC_ITEM_NOT_FOUND: i32 = -25300;
const ERR_SEC_USER_CANCELED: i32 = -128;
const ERR_SEC_AUTH_FAILED: i32 = -25293;
const ERR_SEC_INTERACTION_NOT_ALLOWED: i32 = -25308;
const ERR_SEC_NOT_AVAILABLE: i32 = -25291;

/// Secret store used by live connect (GPUI and `--connect-smoke`).
/// macOS Keychain; Linux file store under the app data dir; Windows the same
/// file store with the key sealed by DPAPI for the current user. Memory is
/// only for any other host.
pub fn live_secret_store() -> Box<dyn SecretStore> {
    #[cfg(target_os = "macos")]
    {
        Box::new(keychain::KeychainSecretStore)
    }
    #[cfg(any(target_os = "linux", windows))]
    {
        // Fail closed: with no platform data directory there is no safe
        // home for the database key — every operation errors instead of
        // falling back to a committable ./quill-data.
        match crate::settings::safe_app_root() {
            Some(root) => Box::new(FileSecretStore::new(root)),
            None => Box::new(UnavailableSecretStore),
        }
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
    {
        Box::new(MemorySecretStore::new())
    }
}

/// Load an existing key, or create one only when no database directory exists.
pub fn load_or_create_key<S: SecretStore + ?Sized>(
    store: &S,
    account: &AccountKey,
    database_exists: bool,
) -> Result<DatabaseKey, KeyDecision> {
    match store.get(account) {
        Ok(Some(key)) => Ok(key),
        Ok(None) if database_exists => Err(KeyDecision::MissingAgainstExistingDb),
        Ok(None) => {
            let key = DatabaseKey::generate();
            store.put(account, &key).map_err(KeyDecision::Store)?;
            Ok(key)
        }
        Err(SecretStoreError::Locked) => Err(KeyDecision::Locked),
        Err(err) => Err(KeyDecision::Store(err)),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyDecision {
    Locked,
    MissingAgainstExistingDb,
    Store(SecretStoreError),
}

#[cfg(target_os = "macos")]
pub mod keychain {
    use super::*;
    use security_framework::passwords::{
        delete_generic_password, get_generic_password, set_generic_password,
    };

    pub struct KeychainSecretStore;

    impl SecretStore for KeychainSecretStore {
        fn get(&self, account: &AccountKey) -> Result<Option<DatabaseKey>, SecretStoreError> {
            let item = account_item_name(account);
            match get_generic_password(KEYCHAIN_SERVICE, &item) {
                Ok(bytes) => Ok(Some(DatabaseKey::from_bytes(bytes)?)),
                Err(err) => map_keychain_get_error(err.code()),
            }
        }

        fn put(&self, account: &AccountKey, key: &DatabaseKey) -> Result<(), SecretStoreError> {
            let item = account_item_name(account);
            set_generic_password(KEYCHAIN_SERVICE, &item, key.as_bytes())
                .map_err(|_| SecretStoreError::Platform)
        }

        fn delete(&self, account: &AccountKey) -> Result<(), SecretStoreError> {
            let item = account_item_name(account);
            match delete_generic_password(KEYCHAIN_SERVICE, &item) {
                Ok(()) => Ok(()),
                Err(_) => Ok(()),
            }
        }
    }
}

/// Open an `http`/`https` URL with the OS (`xdg-open` / `open`). No WebView.
///
/// The argument is passed without a shell. Non-HTTP schemes are refused so a
/// message entity cannot launch a local file or a command.
pub fn open_external_url(url: &str) -> bool {
    if !crate::text::openable_http_url(url) {
        return false;
    }
    os_open(std::ffi::OsStr::new(url.trim()))
}

/// The `mailto:` URL for an email-address entity, or `None` when the text
/// is not a plain address (whitespace, control characters, a second `@`,
/// or characters that would add mail headers or change the URL).
pub fn mailto_url(address: &str) -> Option<String> {
    let (local, domain) = address.split_once('@')?;
    let plain = |part: &str, extra: &str| {
        !part.is_empty()
            && part
                .chars()
                .all(|c| c.is_alphanumeric() || extra.contains(c))
    };
    (plain(local, "._%+-") && plain(domain, ".-") && domain.contains('.'))
        .then(|| format!("mailto:{address}"))
}

/// Open the mail client on a new message to `address`.
pub fn open_mailto(address: &str) -> bool {
    mailto_url(address).is_some_and(|url| os_open(std::ffi::OsStr::new(&url)))
}

/// Hand a URL or path to the OS default handler. macOS `open`, Linux
/// `xdg-open` (argv, no shell). Windows calls `ShellExecuteW` directly:
/// `cmd /C start` would re-parse `&`, `|` and `^` inside a link or file
/// name and run whatever follows as a second command.
#[cfg(not(windows))]
fn os_open(target: &std::ffi::OsStr) -> bool {
    let program = if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    std::process::Command::new(program)
        .arg(target)
        .spawn()
        .is_ok()
}

#[cfg(windows)]
fn os_open(target: &std::ffi::OsStr) -> bool {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    let wide = |s: &std::ffi::OsStr| s.encode_wide().chain(Some(0)).collect::<Vec<u16>>();
    let file = wide(target);
    let verb = wide(std::ffi::OsStr::new("open"));
    // SAFETY: both strings are NUL-terminated UTF-16 buffers that outlive
    // the call; null HWND, parameters and directory are allowed.
    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            file.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };
    // ShellExecuteW reports success with a value greater than 32.
    result as isize > 32
}

/// MED3: open a downloaded file with the system viewer (`xdg-open` /
/// `open`). Reuses the same OS-open mechanism as URLs, without the
/// http-scheme gate (a local path is the point here). No shell involved.
pub fn open_local_file(path: &std::path::Path) -> bool {
    os_open(path.as_os_str())
}

/// MED3: reveal a file in the file manager — macOS `open -R` (selects the
/// file in Finder), Windows `explorer /select,`, Linux opens the parent
/// directory with `xdg-open` (no portable select-file equivalent).
pub fn reveal_in_file_manager(path: &std::path::Path) -> bool {
    if cfg!(target_os = "macos") {
        let mut command = std::process::Command::new("open");
        return command.args(["-R"]).arg(path).spawn().is_ok();
    }
    #[cfg(windows)]
    {
        // explorer parses `/select,"<path>"` itself (no cmd involved);
        // Windows paths can't contain `"`, so quoting the path is enough.
        use std::os::windows::process::CommandExt;
        let mut select = std::ffi::OsString::from("/select,\"");
        select.push(path.as_os_str());
        select.push("\"");
        std::process::Command::new("explorer")
            .raw_arg(select)
            .spawn()
            .is_ok()
    }
    #[cfg(not(windows))]
    match path.parent() {
        Some(dir) if !dir.as_os_str().is_empty() => std::process::Command::new("xdg-open")
            .arg(dir)
            .spawn()
            .is_ok(),
        _ => open_local_file(path),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mailto_accepts_plain_addresses_only() {
        assert_eq!(
            mailto_url("ann@example.com").as_deref(),
            Some("mailto:ann@example.com")
        );
        assert_eq!(
            mailto_url("a.b+c@sub.example.co").as_deref(),
            Some("mailto:a.b+c@sub.example.co")
        );
        for bad in [
            "ann@example",
            "@example.com",
            "ann@@example.com",
            "ann@example.com?bcc=x@y.z",
            "ann@exam ple.com",
            "ann@example.com\n",
            "ann@example.com#frag",
            "javascript:alert(1)",
        ] {
            assert_eq!(mailto_url(bad), None, "{bad}");
        }
    }

    #[test]
    fn does_not_mint_key_when_database_already_exists() {
        let store = MemorySecretStore::new();
        let account = AccountKey::primary();
        let err = load_or_create_key(&store, &account, true).unwrap_err();
        assert_eq!(err, KeyDecision::MissingAgainstExistingDb);
        assert!(store.get(&account).unwrap().is_none());
    }

    #[test]
    fn unavailable_store_fails_closed() {
        // With no safe platform data dir, every secret-store operation must
        // error — never fall back to writing keys under the working dir.
        let store = UnavailableSecretStore;
        let account = AccountKey::primary();
        assert!(store.get(&account).is_err());
        assert!(store.delete(&account).is_err());
        assert!(load_or_create_key(&store, &account, false).is_err());
    }

    #[test]
    fn creates_key_for_fresh_account() {
        let store = MemorySecretStore::new();
        let account = AccountKey::primary();
        let key = load_or_create_key(&store, &account, false).unwrap();
        assert_eq!(key.as_bytes().len(), 32);
        assert_eq!(
            store.get(&account).unwrap().unwrap().as_bytes(),
            key.as_bytes()
        );
    }

    #[test]
    fn locked_store_is_not_overwritten() {
        let store = MemorySecretStore::new();
        store.lock();
        let err = load_or_create_key(&store, &AccountKey::primary(), false).unwrap_err();
        assert_eq!(err, KeyDecision::Locked);
    }

    #[test]
    fn debug_redacts_key_bytes() {
        let key = DatabaseKey::generate();
        assert_eq!(format!("{key:?}"), "DatabaseKey(<redacted>)");
    }

    #[test]
    fn keychain_missing_is_not_locked() {
        assert!(matches!(map_keychain_get_error(-25300), Ok(None)));
        assert_eq!(
            classify_keychain_status(-25300),
            Err(SecretStoreError::Missing)
        );
    }

    #[test]
    fn keychain_user_denial_is_locked() {
        for code in [-128, -25293, -25308, -25291] {
            assert!(
                matches!(map_keychain_get_error(code), Err(SecretStoreError::Locked)),
                "status {code}"
            );
        }
    }

    #[test]
    fn keychain_other_errors_are_platform() {
        assert!(matches!(
            map_keychain_get_error(-50),
            Err(SecretStoreError::Platform)
        ));
    }

    fn tmp_app_root(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "quill-file-store-{}-{}-{}",
            tag,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn file_store_round_trip_survives_new_store_instance() {
        let root = tmp_app_root("roundtrip");
        let account = AccountKey::primary();
        let key = DatabaseKey::generate();
        {
            let store = FileSecretStore::new(&root);
            store.put(&account, &key).unwrap();
        }
        // Simulate process restart: new store, same app_root on disk.
        let store = FileSecretStore::new(&root);
        let loaded = store.get(&account).unwrap().expect("persisted key");
        assert_eq!(loaded.as_bytes(), key.as_bytes());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let path = root
                .join("accounts")
                .join(&account.0)
                .join("db-encryption.key");
            let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600, "key file must be owner-read/write only");
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn file_store_missing_against_existing_db_does_not_mint() {
        let root = tmp_app_root("missing-db");
        let account = AccountKey::primary();
        let paths = AccountPaths::for_root(&root, &account);
        std::fs::create_dir_all(&paths.tdlib_database).unwrap();
        std::fs::write(paths.tdlib_database.join("td.binlog"), b"x").unwrap();
        assert!(paths.database_exists());

        let store = FileSecretStore::new(&root);
        assert!(store.get(&account).unwrap().is_none());
        let err = load_or_create_key(&store, &account, true).unwrap_err();
        assert_eq!(err, KeyDecision::MissingAgainstExistingDb);
        assert!(store.get(&account).unwrap().is_none());
        assert!(!store.key_path(&account).exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn file_store_creates_key_for_fresh_account() {
        let root = tmp_app_root("fresh");
        let account = AccountKey::primary();
        let store = FileSecretStore::new(&root);
        let key = load_or_create_key(&store, &account, false).unwrap();
        let again = FileSecretStore::new(&root)
            .get(&account)
            .unwrap()
            .expect("key on disk");
        assert_eq!(again.as_bytes(), key.as_bytes());
        let _ = std::fs::remove_dir_all(&root);
    }
}
