//! Local passcode (tdesktop `Settings::LocalPasscode`, `Window::PasscodeLockWidget`).
//!
//! Design (docs/decisions/codex-local-passcode.md):
//!
//! * The passcode itself is never stored. It is stretched with
//!   PBKDF2-HMAC-SHA512 (210 000 rounds, random 16-byte salt) into a
//!   key-encryption key (KEK).
//! * A random 32-byte *master key* is wrapped by the KEK
//!   (XChaCha20-Poly1305, AEAD tag = passcode check, so there is no separate
//!   verifier to attack). Changing the passcode re-wraps only the master key
//!   in one atomic file write.
//! * Each account's TDLib database key is wrapped by the master key and lives
//!   in `accounts/<id>/db-encryption.wrapped`. While a passcode is set the
//!   copy in the OS secret store (Keychain / key file / DPAPI file) is
//!   removed, so TDLib cannot open the database until the passcode is
//!   entered: the app starts locked, as in tdesktop.
//! * Failed attempts are rate limited with tdesktop's schedule (free for 3,
//!   then 5/10/15/20/25/30 s) and the counter survives a restart.

use crate::ids::AccountKey;
use crate::platform::{DatabaseKey, SecretStore, SecretStoreError};
use crate::settings::AccountPaths;
use base64::Engine;
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{Key, XChaCha20Poly1305, XNonce};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use thiserror::Error;
use zeroize::{Zeroize, ZeroizeOnDrop};

/// PBKDF2-HMAC-SHA512 rounds for new passcodes (OWASP 2023 minimum for
/// SHA-512). Stored in the config so it can be raised later.
pub const KDF_ITERATIONS: u32 = 210_000;
pub const KDF_NAME: &str = "pbkdf2-hmac-sha512";
const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 24;
const WRAP_MAGIC: &[u8; 4] = b"QPW1";
const MASTER_AAD: &[u8] = b"quill-passcode-master-v1";
pub const CONFIG_FILE: &str = "passcode.json";
pub const WRAPPED_KEY_FILE: &str = "db-encryption.wrapped";

/// tdesktop `Settings::autoLock` default and presets (`AutoLockBox`).
pub const DEFAULT_AUTOLOCK_SECS: u32 = 3600;
pub const AUTOLOCK_PRESETS: [u32; 4] = [60, 300, 3600, 18_000];

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum PasscodeError {
    #[error("Enter a passcode")]
    Empty,
    #[error("Passcodes are different")]
    Mismatch,
    #[error("Passcode was not changed")]
    Same,
    #[error("Wrong passcode")]
    Wrong,
    /// tdesktop `lng_flood_error`; `retry_in_ms` says how long to wait.
    #[error("Too many tries. Please try again later.")]
    Flood { retry_in_ms: u64 },
    #[error("A local passcode is already set")]
    AlreadyEnabled,
    #[error("No local passcode is set")]
    NotEnabled,
    #[error("Unlock Quill first")]
    Locked,
    #[error("The passcode data is damaged")]
    Corrupt,
    #[error("Could not access the secure key store")]
    Store(SecretStoreError),
    #[error("Could not write the passcode data")]
    Io,
}

impl From<SecretStoreError> for PasscodeError {
    fn from(err: SecretStoreError) -> Self {
        Self::Store(err)
    }
}

/// 32-byte key that wraps every account's database key. Wiped on drop.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct MasterKey([u8; 32]);

impl MasterKey {
    fn generate() -> Self {
        let mut bytes = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut bytes);
        Self(bytes)
    }

    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl std::fmt::Debug for MasterKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("MasterKey(<redacted>)")
    }
}

/// Whether the master key is in memory. Process-wide for the live app
/// ([`global_unlock`]); tests build their own.
#[derive(Default)]
pub struct UnlockState {
    key: Mutex<Option<MasterKey>>,
}

impl UnlockState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set(&self, key: MasterKey) {
        *self.key.lock().expect("unlock state") = Some(key);
    }

    /// Forget the master key (locking). Dropping it zeroizes the bytes.
    pub fn clear(&self) {
        *self.key.lock().expect("unlock state") = None;
    }

    pub fn is_unlocked(&self) -> bool {
        self.key.lock().expect("unlock state").is_some()
    }

    pub fn key(&self) -> Option<MasterKey> {
        self.key.lock().expect("unlock state").clone()
    }
}

pub fn global_unlock() -> Arc<UnlockState> {
    static STATE: OnceLock<Arc<UnlockState>> = OnceLock::new();
    STATE.get_or_init(|| Arc::new(UnlockState::new())).clone()
}

/// `passcode.json` at the app root. Holds no secret: the salt, the
/// AEAD-wrapped master key and the user's lock preferences.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PasscodeConfig {
    pub version: u32,
    pub kdf: String,
    pub iterations: u32,
    pub salt: String,
    pub wrapped_master: String,
    /// "Auto-Lock if away for" seconds.
    #[serde(default = "default_autolock")]
    pub autolock_secs: u32,
    /// "Unlock with Touch ID / system authentication" (macOS).
    #[serde(default)]
    pub system_unlock: bool,
    #[serde(default)]
    pub bad_tries: u32,
    #[serde(default)]
    pub last_try_unix_ms: u64,
}

fn default_autolock() -> u32 {
    DEFAULT_AUTOLOCK_SECS
}

fn b64() -> base64::engine::GeneralPurpose {
    base64::engine::general_purpose::STANDARD
}

pub fn config_path(root: &Path) -> PathBuf {
    root.join(CONFIG_FILE)
}

pub fn wrapped_key_path(root: &Path, account: &AccountKey) -> PathBuf {
    AccountPaths::for_root(root, account)
        .root
        .join(WRAPPED_KEY_FILE)
}

pub fn is_enabled(root: &Path) -> bool {
    config_path(root).exists()
}

pub fn load_config(root: &Path) -> Option<PasscodeConfig> {
    let bytes = std::fs::read(config_path(root)).ok()?;
    serde_json::from_slice(&bytes).ok()
}

pub fn save_config(root: &Path, config: &PasscodeConfig) -> Result<(), PasscodeError> {
    crate::settings::write_json_atomic(&config_path(root), config)
        .map_err(|_| PasscodeError::Io)?;
    restrict_file(&config_path(root));
    Ok(())
}

fn restrict_file(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
    let _ = path;
}

/// PBKDF2-HMAC-SHA512 of the passcode: the key-encryption key.
fn derive_kek(passcode: &str, salt: &[u8], iterations: u32) -> MasterKey {
    let mut out = [0u8; 32];
    let mut full = [0u8; 64];
    pbkdf2::pbkdf2_hmac::<sha2::Sha512>(passcode.as_bytes(), salt, iterations.max(1), &mut full);
    out.copy_from_slice(&full[..32]);
    full.zeroize();
    MasterKey(out)
}

fn seal(key: &MasterKey, aad: &[u8], plain: &[u8]) -> Result<Vec<u8>, PasscodeError> {
    let cipher = XChaCha20Poly1305::new(Key::from_slice(key.as_bytes()));
    let mut nonce = [0u8; NONCE_LEN];
    rand::thread_rng().fill_bytes(&mut nonce);
    let body = cipher
        .encrypt(XNonce::from_slice(&nonce), Payload { msg: plain, aad })
        .map_err(|_| PasscodeError::Corrupt)?;
    let mut out = Vec::with_capacity(WRAP_MAGIC.len() + NONCE_LEN + body.len());
    out.extend_from_slice(WRAP_MAGIC);
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&body);
    Ok(out)
}

fn open(key: &MasterKey, aad: &[u8], sealed: &[u8]) -> Result<Vec<u8>, PasscodeError> {
    let header = WRAP_MAGIC.len() + NONCE_LEN;
    if sealed.len() <= header || &sealed[..WRAP_MAGIC.len()] != WRAP_MAGIC {
        return Err(PasscodeError::Corrupt);
    }
    let nonce = &sealed[WRAP_MAGIC.len()..header];
    let cipher = XChaCha20Poly1305::new(Key::from_slice(key.as_bytes()));
    cipher
        .decrypt(
            XNonce::from_slice(nonce),
            Payload {
                msg: &sealed[header..],
                aad,
            },
        )
        .map_err(|_| PasscodeError::Wrong)
}

/// Wrap a database key under the master key, bound to the account id.
pub fn wrap_db_key(
    master: &MasterKey,
    account: &AccountKey,
    key: &DatabaseKey,
) -> Result<Vec<u8>, PasscodeError> {
    seal(master, account.0.as_bytes(), key.as_bytes())
}

pub fn unwrap_db_key(
    master: &MasterKey,
    account: &AccountKey,
    sealed: &[u8],
) -> Result<DatabaseKey, PasscodeError> {
    let mut plain = open(master, account.0.as_bytes(), sealed)?;
    let key = DatabaseKey::from_bytes(plain.clone()).map_err(|_| PasscodeError::Corrupt);
    plain.zeroize();
    key
}

fn write_private(path: &Path, bytes: &[u8]) -> Result<(), PasscodeError> {
    use std::io::Write;
    let parent = path.parent().ok_or(PasscodeError::Io)?;
    std::fs::create_dir_all(parent).map_err(|_| PasscodeError::Io)?;
    let tmp = path.with_extension("tmp");
    {
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&tmp).map_err(|_| PasscodeError::Io)?;
        file.write_all(bytes).map_err(|_| PasscodeError::Io)?;
        file.sync_all().map_err(|_| PasscodeError::Io)?;
    }
    std::fs::rename(&tmp, path).map_err(|_| PasscodeError::Io)?;
    restrict_file(path);
    Ok(())
}

fn read_wrapped(root: &Path, master: &MasterKey, account: &AccountKey) -> Option<DatabaseKey> {
    let sealed = std::fs::read(wrapped_key_path(root, account)).ok()?;
    unwrap_db_key(master, account, &sealed).ok()
}

fn build_config(
    passcode: &str,
    iterations: u32,
    master: &MasterKey,
    previous: Option<&PasscodeConfig>,
) -> Result<PasscodeConfig, PasscodeError> {
    let mut salt = [0u8; SALT_LEN];
    rand::thread_rng().fill_bytes(&mut salt);
    let kek = derive_kek(passcode, &salt, iterations);
    let wrapped = seal(&kek, MASTER_AAD, master.as_bytes())?;
    Ok(PasscodeConfig {
        version: 1,
        kdf: KDF_NAME.to_string(),
        iterations,
        salt: b64().encode(salt),
        wrapped_master: b64().encode(wrapped),
        autolock_secs: previous.map_or(DEFAULT_AUTOLOCK_SECS, |p| p.autolock_secs),
        system_unlock: previous.is_some_and(|p| p.system_unlock),
        bad_tries: 0,
        last_try_unix_ms: 0,
    })
}

fn unwrap_master(config: &PasscodeConfig, passcode: &str) -> Result<MasterKey, PasscodeError> {
    if config.kdf != KDF_NAME {
        return Err(PasscodeError::Corrupt);
    }
    let salt = b64()
        .decode(&config.salt)
        .map_err(|_| PasscodeError::Corrupt)?;
    let sealed = b64()
        .decode(&config.wrapped_master)
        .map_err(|_| PasscodeError::Corrupt)?;
    let kek = derive_kek(passcode, &salt, config.iterations);
    let mut plain = open(&kek, MASTER_AAD, &sealed)?;
    let bytes: Result<[u8; 32], _> = plain.as_slice().try_into();
    plain.zeroize();
    bytes.map(MasterKey).map_err(|_| PasscodeError::Corrupt)
}

/// tdesktop `passcodeCanTry`: three free tries, then 5, 10, 15, 20, 25 and
/// finally 30 seconds between tries. Returns the wait left, if any.
pub fn retry_wait_ms(bad_tries: u32, since_last_try_ms: u64) -> Option<u64> {
    let need: u64 = match bad_tries {
        0..=2 => return None,
        3 => 5_000,
        4 => 10_000,
        5 => 15_000,
        6 => 20_000,
        7 => 25_000,
        _ => 30_000,
    };
    need.checked_sub(since_last_try_ms).filter(|left| *left > 0)
}

pub fn unix_ms_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

/// The wait left before the next attempt, from the persisted counter.
pub fn lock_wait_ms(root: &Path, now_unix_ms: u64) -> Option<u64> {
    let config = load_config(root)?;
    retry_wait_ms(
        config.bad_tries,
        now_unix_ms.saturating_sub(config.last_try_unix_ms),
    )
}

/// Check `passcode`, honoring and updating the persisted rate limit. On
/// success the master key goes into `state`.
pub fn unlock(
    root: &Path,
    state: &UnlockState,
    passcode: &str,
    now_unix_ms: u64,
) -> Result<MasterKey, PasscodeError> {
    let mut config = load_config(root).ok_or(PasscodeError::NotEnabled)?;
    if passcode.is_empty() {
        return Err(PasscodeError::Empty);
    }
    if let Some(retry_in_ms) = retry_wait_ms(
        config.bad_tries,
        now_unix_ms.saturating_sub(config.last_try_unix_ms),
    ) {
        return Err(PasscodeError::Flood { retry_in_ms });
    }
    match unwrap_master(&config, passcode) {
        Ok(master) => {
            if config.bad_tries != 0 {
                config.bad_tries = 0;
                config.last_try_unix_ms = 0;
                let _ = save_config(root, &config);
            }
            state.set(master.clone());
            Ok(master)
        }
        Err(PasscodeError::Wrong) => {
            config.bad_tries = config.bad_tries.saturating_add(1);
            config.last_try_unix_ms = now_unix_ms;
            let _ = save_config(root, &config);
            Err(PasscodeError::Wrong)
        }
        Err(other) => Err(other),
    }
}

/// Unlock with a master key the platform released (Touch ID path).
pub fn unlock_with_master(
    root: &Path,
    state: &UnlockState,
    accounts: &[AccountKey],
    master: MasterKey,
) -> Result<(), PasscodeError> {
    // A stale or foreign key must not unlock: it has to open a wrapped file
    // (or, with none yet, there is nothing to prove and it is refused).
    let proves = accounts
        .iter()
        .any(|a| wrapped_key_path(root, a).exists() && read_wrapped(root, &master, a).is_some());
    if !proves {
        return Err(PasscodeError::Wrong);
    }
    state.set(master);
    Ok(())
}

/// New passcode entry rules (`lng_passcode_differ`, `lng_passcode_is_same`).
pub fn validate_new(new: &str, confirm: &str, old: Option<&str>) -> Result<(), PasscodeError> {
    if new.is_empty() {
        return Err(PasscodeError::Empty);
    }
    if new != confirm {
        return Err(PasscodeError::Mismatch);
    }
    if old == Some(new) {
        return Err(PasscodeError::Same);
    }
    Ok(())
}

/// Turn the passcode on: wrap every account's database key, then remove the
/// copies from the OS secret store. Everything is rolled back on failure, so
/// no database key is ever lost or left half-migrated.
pub fn enable(
    root: &Path,
    inner: &dyn SecretStore,
    accounts: &[AccountKey],
    passcode: &str,
    iterations: u32,
    state: &UnlockState,
) -> Result<(), PasscodeError> {
    if is_enabled(root) {
        return Err(PasscodeError::AlreadyEnabled);
    }
    if passcode.is_empty() {
        return Err(PasscodeError::Empty);
    }
    let master = MasterKey::generate();
    let config = build_config(passcode, iterations, &master, None)?;
    let mut written: Vec<AccountKey> = Vec::new();
    let rollback = |written: &[AccountKey]| {
        for account in written {
            let _ = std::fs::remove_file(wrapped_key_path(root, account));
        }
        let _ = std::fs::remove_file(config_path(root));
    };
    for account in accounts {
        let key = match inner.get(account) {
            Ok(Some(key)) => key,
            Ok(None) => continue,
            Err(err) => {
                rollback(&written);
                return Err(err.into());
            }
        };
        let step = wrap_db_key(&master, account, &key).and_then(|sealed| {
            write_private(&wrapped_key_path(root, account), &sealed)?;
            // Read back before anything is removed from the secret store.
            match read_wrapped(root, &master, account) {
                Some(round) if round.as_bytes() == key.as_bytes() => Ok(()),
                _ => Err(PasscodeError::Corrupt),
            }
        });
        if let Err(err) = step {
            rollback(&written);
            let _ = std::fs::remove_file(wrapped_key_path(root, account));
            return Err(err);
        }
        written.push(account.clone());
    }
    if let Err(err) = save_config(root, &config) {
        rollback(&written);
        return Err(err);
    }
    for account in &written {
        if let Err(err) = inner.delete(account) {
            rollback(&written);
            return Err(err.into());
        }
    }
    state.set(master);
    Ok(())
}

/// Change the passcode: re-wrap the master key only (one atomic write).
pub fn change(
    root: &Path,
    old: &str,
    new: &str,
    iterations: u32,
    state: &UnlockState,
    now_unix_ms: u64,
) -> Result<(), PasscodeError> {
    let config = load_config(root).ok_or(PasscodeError::NotEnabled)?;
    validate_new(new, new, Some(old))?;
    let master = unlock(root, state, old, now_unix_ms)?;
    let fresh = build_config(new, iterations, &master, Some(&config))?;
    save_config(root, &fresh)?;
    state.set(master);
    Ok(())
}

/// Turn the passcode off: put every database key back into the OS secret
/// store first, then remove the wrapped copies and the config.
pub fn disable(
    root: &Path,
    inner: &dyn SecretStore,
    accounts: &[AccountKey],
    passcode: &str,
    state: &UnlockState,
    now_unix_ms: u64,
) -> Result<(), PasscodeError> {
    let master = unlock(root, state, passcode, now_unix_ms)?;
    let mut restored: Vec<(AccountKey, DatabaseKey)> = Vec::new();
    for account in accounts {
        if !wrapped_key_path(root, account).exists() {
            continue;
        }
        let Some(key) = read_wrapped(root, &master, account) else {
            return Err(PasscodeError::Corrupt);
        };
        restored.push((account.clone(), key));
    }
    for (account, key) in &restored {
        inner.put(account, key)?;
    }
    for (account, _) in &restored {
        let _ = std::fs::remove_file(wrapped_key_path(root, account));
    }
    std::fs::remove_file(config_path(root)).map_err(|_| PasscodeError::Io)?;
    state.clear();
    Ok(())
}

pub fn set_autolock(root: &Path, secs: u32) -> Result<(), PasscodeError> {
    let mut config = load_config(root).ok_or(PasscodeError::NotEnabled)?;
    config.autolock_secs = secs.max(1);
    save_config(root, &config)
}

pub fn set_system_unlock(root: &Path, on: bool) -> Result<(), PasscodeError> {
    let mut config = load_config(root).ok_or(PasscodeError::NotEnabled)?;
    config.system_unlock = on;
    save_config(root, &config)
}

/// "Log out" on the lock screen: the master key is gone, so the databases
/// cannot be opened any more. Delete every account's local data and the
/// passcode (tdesktop does the same when the passcode is forgotten).
pub fn wipe_local_data(root: &Path, accounts: &[AccountKey], state: &UnlockState) {
    state.clear();
    for account in accounts {
        let _ = std::fs::remove_dir_all(AccountPaths::for_root(root, account).root);
    }
    let _ = std::fs::remove_file(config_path(root));
}

/// [`SecretStore`] that keeps database keys wrapped while a passcode is
/// set, and defers to `inner` otherwise.
pub struct PasscodeStore<S> {
    inner: S,
    root: PathBuf,
    state: Arc<UnlockState>,
}

impl<S> PasscodeStore<S> {
    pub fn new(inner: S, root: impl Into<PathBuf>, state: Arc<UnlockState>) -> Self {
        Self {
            inner,
            root: root.into(),
            state,
        }
    }
}

impl<S: SecretStore> SecretStore for PasscodeStore<S> {
    fn get(&self, account: &AccountKey) -> Result<Option<DatabaseKey>, SecretStoreError> {
        if !is_enabled(&self.root) {
            return self.inner.get(account);
        }
        let path = wrapped_key_path(&self.root, account);
        if !path.exists() {
            // Not migrated (a new account on a crash-interrupted change):
            // the OS store still holds it.
            return self.inner.get(account);
        }
        let master = self.state.key().ok_or(SecretStoreError::Locked)?;
        let sealed = std::fs::read(&path).map_err(|_| SecretStoreError::Platform)?;
        unwrap_db_key(&master, account, &sealed)
            .map(Some)
            .map_err(|_| SecretStoreError::InvalidKey)
    }

    fn put(&self, account: &AccountKey, key: &DatabaseKey) -> Result<(), SecretStoreError> {
        if !is_enabled(&self.root) {
            return self.inner.put(account, key);
        }
        let master = self.state.key().ok_or(SecretStoreError::Locked)?;
        let sealed = wrap_db_key(&master, account, key).map_err(|_| SecretStoreError::Platform)?;
        write_private(&wrapped_key_path(&self.root, account), &sealed)
            .map_err(|_| SecretStoreError::Platform)
    }

    fn delete(&self, account: &AccountKey) -> Result<(), SecretStoreError> {
        let _ = std::fs::remove_file(wrapped_key_path(&self.root, account));
        self.inner.delete(account)
    }
}

/// Auto-lock decision (tdesktop `Application::checkAutoLock`): lock once the
/// user has been away for `timeout_secs`.
pub fn autolock_due(idle_ms: u64, timeout_secs: u32) -> bool {
    idle_ms >= u64::from(timeout_secs) * 1000
}

/// Milliseconds until [`autolock_due`] turns true (0 when it already is).
pub fn autolock_remaining_ms(idle_ms: u64, timeout_secs: u32) -> u64 {
    (u64::from(timeout_secs) * 1000).saturating_sub(idle_ms)
}

/// Idle time that also counts a suspend: if the poll loop was not run for
/// much longer than its period (the laptop slept), the missed time is idle
/// time too, like tdesktop's `kAutoLockTimeoutLateMs` check.
#[derive(Debug, Default)]
pub struct IdleTracker {
    last_tick_ms: Option<u64>,
    last_idle_ms: u64,
    suspended_ms: u64,
}

const SUSPEND_GAP_MS: u64 = 5_000;

impl IdleTracker {
    /// `now_ms` is a wall clock (it keeps running through sleep); `idle_ms`
    /// is the input idle time. Returns the effective idle time.
    pub fn effective_idle(&mut self, now_ms: u64, idle_ms: u64) -> u64 {
        if idle_ms < self.last_idle_ms {
            // Input happened since the last tick.
            self.suspended_ms = 0;
        }
        if let Some(last) = self.last_tick_ms {
            let gap = now_ms.saturating_sub(last);
            if gap > SUSPEND_GAP_MS {
                self.suspended_ms = self.suspended_ms.saturating_add(gap);
            }
        }
        self.last_tick_ms = Some(now_ms);
        self.last_idle_ms = idle_ms;
        idle_ms.saturating_add(self.suspended_ms)
    }
}

/// Parse the custom "HH:MM" auto-lock time of tdesktop's box.
pub fn parse_hhmm(text: &str) -> Option<u32> {
    let (h, m) = text.trim().split_once(':')?;
    let hours: u32 = h.trim().parse().ok()?;
    let minutes: u32 = m.trim().parse().ok()?;
    if minutes >= 60 || hours > 99 {
        return None;
    }
    let secs = hours * 3600 + minutes * 60;
    (secs > 0).then_some(secs)
}

/// "1 min", "5 min", "1 h", "5 h", else "Hh Mm" (tdesktop `lng_minutes` /
/// `lng_hours` / `lng_passcode_autolock_hours_minutes`).
pub fn autolock_label(secs: u32) -> String {
    let hours = secs / 3600;
    let minutes = (secs % 3600) / 60;
    match (hours, minutes) {
        (0, m) => format!("{m} min"),
        (h, 0) => format!("{h} h"),
        (h, m) => format!("{h}h {m}m"),
    }
}

/// OS-wide idle time (time since the last keyboard or mouse input anywhere
/// on the desktop) where the platform exposes it without extra
/// dependencies: macOS (CoreGraphics) and Windows (`GetLastInputInfo`).
/// On Linux it asks GNOME's idle monitor over D-Bus ([`linux_idle_ms`]);
/// `None` on other desktops, where idle time needs a compositor-specific
/// protocol, so callers fall back to in-window input ("inactive" instead of
/// "away").
pub fn os_idle_ms() -> Option<u64> {
    #[cfg(target_os = "macos")]
    {
        #[link(name = "CoreGraphics", kind = "framework")]
        unsafe extern "C" {
            fn CGEventSourceSecondsSinceLastEventType(state_id: i32, event_type: u32) -> f64;
        }
        // kCGEventSourceStateCombinedSessionState, kCGAnyInputEventType.
        // SAFETY: plain C call with value arguments and no pointers.
        let secs = unsafe { CGEventSourceSecondsSinceLastEventType(0, u32::MAX) };
        if secs.is_finite() && secs >= 0.0 {
            return Some((secs * 1000.0) as u64);
        }
        None
    }
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::SystemInformation::GetTickCount;
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
        let mut info = LASTINPUTINFO {
            cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32,
            dwTime: 0,
        };
        // SAFETY: `info` is a properly sized, writable LASTINPUTINFO.
        let ok = unsafe { GetLastInputInfo(&mut info) };
        if ok == 0 {
            return None;
        }
        // SAFETY: no arguments.
        let now = unsafe { GetTickCount() };
        Some(u64::from(now.wrapping_sub(info.dwTime)))
    }
    #[cfg(target_os = "linux")]
    {
        linux_idle_ms()
    }
    #[cfg(not(any(target_os = "macos", windows, target_os = "linux")))]
    {
        None
    }
}

/// Sampled idle state shared between the poller thread and readers.
#[cfg(any(target_os = "linux", test))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IdleCache {
    /// Poller not started or no sample yet.
    Unknown,
    /// `ms` of idle time measured `age_ms` ago relative to the read.
    Sample { ms: u64, taken_ms: u64 },
    /// The query failed once; never retried.
    Failed,
}

#[cfg(any(target_os = "linux", test))]
impl IdleCache {
    /// Idle time at `now_ms` (same clock as `taken_ms`), extrapolated from
    /// the last sample. `None` until a sample exists and forever after a
    /// failure.
    fn read(self, now_ms: u64) -> Option<u64> {
        match self {
            Self::Sample { ms, taken_ms } => Some(ms + now_ms.saturating_sub(taken_ms)),
            Self::Unknown | Self::Failed => None,
        }
    }
}

/// Reads the cache a background thread refreshes every two seconds from
/// GNOME Mutter's `org.gnome.Mutter.IdleMonitor.GetIdletime` over zbus.
/// A read starts the thread; it pauses once reads stop (auto-lock off or
/// locked) and stops for good after the first failure (not GNOME, no bus).
/// Never blocks on D-Bus.
#[cfg(target_os = "linux")]
fn linux_idle_ms() -> Option<u64> {
    IDLE_LAST_READ.store(linux_now_ms(), std::sync::atomic::Ordering::Relaxed);
    linux_idle_start();
    linux_idle_cache().read(linux_now_ms())
}

/// True once a sample succeeded; starts nothing (safe from render).
#[cfg(target_os = "linux")]
fn linux_idle_known() -> bool {
    linux_idle_cache().read(0).is_some()
}

#[cfg(target_os = "linux")]
static IDLE: std::sync::Mutex<IdleCache> = std::sync::Mutex::new(IdleCache::Unknown);

#[cfg(target_os = "linux")]
fn linux_idle_cache() -> IdleCache {
    *IDLE.lock().unwrap_or_else(|e| e.into_inner())
}

#[cfg(target_os = "linux")]
fn linux_now_ms() -> u64 {
    use std::sync::OnceLock;
    use std::time::Instant;
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_millis() as u64
}

/// How long the sampler keeps running after the last [`os_idle_ms`] read.
/// The auto-lock tick reads every second while armed, so a gap this long
/// means auto-lock is off, locked, or the app is gone.
#[cfg(any(target_os = "linux", test))]
const IDLE_ARMED_GRACE_MS: u64 = 5_000;

/// Whether the sampler should take another sample: auto-lock asked for the
/// idle time recently enough.
#[cfg(any(target_os = "linux", test))]
fn idle_sampling_wanted(last_read_ms: Option<u64>, now_ms: u64) -> bool {
    last_read_ms.is_some_and(|t| now_ms.saturating_sub(t) <= IDLE_ARMED_GRACE_MS)
}

/// Time of the latest [`linux_idle_ms`] read (`u64::MAX` = never).
#[cfg(target_os = "linux")]
static IDLE_LAST_READ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(u64::MAX);

/// True while a sampler thread is alive.
#[cfg(target_os = "linux")]
static IDLE_RUNNING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[cfg(target_os = "linux")]
fn idle_last_read() -> Option<u64> {
    use std::sync::atomic::Ordering;
    match IDLE_LAST_READ.load(Ordering::Relaxed) {
        u64::MAX => None,
        t => Some(t),
    }
}

/// One `GetIdletime` call on the session bus (no subprocess).
#[cfg(all(target_os = "linux", feature = "ui"))]
fn linux_idle_sample(conn: &zbus::blocking::Connection) -> Option<u64> {
    let reply = conn
        .call_method(
            Some("org.gnome.Mutter.IdleMonitor"),
            "/org/gnome/Mutter/IdleMonitor/Core",
            Some("org.gnome.Mutter.IdleMonitor"),
            "GetIdletime",
            &(),
        )
        .ok()?;
    reply.body().deserialize::<u64>().ok()
}

/// Starts the sampler thread unless one is running or sampling already
/// failed for good. The thread exits when auto-lock stops reading, and the
/// next read restarts it.
#[cfg(target_os = "linux")]
fn linux_idle_start() {
    use std::sync::atomic::Ordering;
    use std::time::Duration;
    if linux_idle_cache() == IdleCache::Failed || IDLE_RUNNING.swap(true, Ordering::AcqRel) {
        return;
    }
    linux_now_ms(); // pin the clock origin before the first sample
    // A sample from before a pause would extrapolate to a bogus huge idle
    // time (and lock spuriously); drop it until the fresh one lands.
    *IDLE.lock().unwrap_or_else(|e| e.into_inner()) = IdleCache::Unknown;
    let spawned = std::thread::Builder::new()
        .name("quill-idle".into())
        .spawn(|| {
            #[cfg(feature = "ui")]
            let conn = zbus::blocking::Connection::session().ok();
            loop {
                if !idle_sampling_wanted(idle_last_read(), linux_now_ms()) {
                    IDLE_RUNNING.store(false, Ordering::Release);
                    // A read may have raced the exit; pick it back up.
                    if idle_sampling_wanted(idle_last_read(), linux_now_ms())
                        && !IDLE_RUNNING.swap(true, Ordering::AcqRel)
                    {
                        continue;
                    }
                    return;
                }
                #[cfg(feature = "ui")]
                let ms = conn.as_ref().and_then(linux_idle_sample);
                #[cfg(not(feature = "ui"))]
                let ms: Option<u64> = None;
                let state = match ms {
                    Some(ms) => IdleCache::Sample {
                        ms,
                        taken_ms: linux_now_ms(),
                    },
                    None => IdleCache::Failed,
                };
                *IDLE.lock().unwrap_or_else(|e| e.into_inner()) = state;
                if state == IdleCache::Failed {
                    IDLE_RUNNING.store(false, Ordering::Release);
                    return;
                }
                std::thread::sleep(Duration::from_secs(2));
            }
        });
    if spawned.is_err() {
        *IDLE.lock().unwrap_or_else(|e| e.into_inner()) = IdleCache::Failed;
        IDLE_RUNNING.store(false, Ordering::Release);
    }
}

/// Whether OS idle time is currently known, without starting any sampling
/// (for render paths). Equivalent to `os_idle_ms().is_some()` elsewhere.
pub fn os_idle_known() -> bool {
    #[cfg(target_os = "linux")]
    {
        linux_idle_known()
    }
    #[cfg(not(target_os = "linux"))]
    {
        os_idle_ms().is_some()
    }
}

#[cfg(test)]
mod tests {
    use crate::passcode::*;
    use crate::platform::MemorySecretStore;

    #[test]
    fn idle_cache_extrapolates_and_fails_closed() {
        assert_eq!(IdleCache::Unknown.read(10), None);
        assert_eq!(IdleCache::Failed.read(10), None);
        let sample = IdleCache::Sample {
            ms: 500,
            taken_ms: 1000,
        };
        assert_eq!(sample.read(1000), Some(500));
        assert_eq!(sample.read(2500), Some(2000));
        assert_eq!(sample.read(900), Some(500));
    }

    #[test]
    fn idle_sampling_runs_only_while_reads_are_recent() {
        assert!(!idle_sampling_wanted(None, 10_000));
        assert!(idle_sampling_wanted(Some(1_000), 1_000));
        assert!(idle_sampling_wanted(
            Some(1_000),
            1_000 + IDLE_ARMED_GRACE_MS
        ));
        assert!(!idle_sampling_wanted(
            Some(1_000),
            1_001 + IDLE_ARMED_GRACE_MS
        ));
        // A clock that reads earlier than the stamp still counts as recent.
        assert!(idle_sampling_wanted(Some(2_000), 1_000));
    }

    const FAST: u32 = 8;

    fn temp_root(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "quill-passcode-{tag}-{}-{}",
            std::process::id(),
            unix_ms_now()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn acct(name: &str) -> AccountKey {
        AccountKey::new(name).unwrap()
    }

    #[test]
    fn kdf_is_deterministic_per_salt_and_differs_by_input() {
        let a = derive_kek("hunter2", b"salt-salt-salt!!", FAST);
        let b = derive_kek("hunter2", b"salt-salt-salt!!", FAST);
        let c = derive_kek("hunter3", b"salt-salt-salt!!", FAST);
        let d = derive_kek("hunter2", b"other-salt-here!", FAST);
        assert_eq!(a.as_bytes(), b.as_bytes());
        assert_ne!(a.as_bytes(), c.as_bytes());
        assert_ne!(a.as_bytes(), d.as_bytes());
    }

    #[test]
    fn pbkdf2_matches_the_rfc_vector_shape() {
        // PBKDF2-HMAC-SHA512, "password"/"salt", 1 round (published vector).
        let mut out = [0u8; 64];
        pbkdf2::pbkdf2_hmac::<sha2::Sha512>(b"password", b"salt", 1, &mut out);
        assert_eq!(&out[..8], &[0x86, 0x7f, 0x70, 0xcf, 0x1a, 0xde, 0x02, 0xcf]);
    }

    #[test]
    fn wrap_unwrap_round_trip_and_tamper_detection() {
        let master = MasterKey::generate();
        let key = DatabaseKey::generate();
        let sealed = wrap_db_key(&master, &acct("primary"), &key).unwrap();
        let back = unwrap_db_key(&master, &acct("primary"), &sealed).unwrap();
        assert_eq!(back.as_bytes(), key.as_bytes());
        // Bound to the account id.
        assert_eq!(
            unwrap_db_key(&master, &acct("other"), &sealed).unwrap_err(),
            PasscodeError::Wrong
        );
        // Wrong master key.
        let other = MasterKey::generate();
        assert!(unwrap_db_key(&other, &acct("primary"), &sealed).is_err());
        // Flipped bit.
        let mut bad = sealed.clone();
        let last = bad.len() - 1;
        bad[last] ^= 1;
        assert!(unwrap_db_key(&master, &acct("primary"), &bad).is_err());
        // Wrapping is salted: same input, different bytes.
        assert_ne!(
            sealed,
            wrap_db_key(&master, &acct("primary"), &key).unwrap()
        );
    }

    #[test]
    fn retry_schedule_matches_tdesktop() {
        assert_eq!(retry_wait_ms(0, 0), None);
        assert_eq!(retry_wait_ms(2, 0), None);
        assert_eq!(retry_wait_ms(3, 0), Some(5_000));
        assert_eq!(retry_wait_ms(3, 4_000), Some(1_000));
        assert_eq!(retry_wait_ms(3, 5_000), None);
        assert_eq!(retry_wait_ms(4, 9_999), Some(1));
        assert_eq!(retry_wait_ms(5, 0), Some(15_000));
        assert_eq!(retry_wait_ms(6, 0), Some(20_000));
        assert_eq!(retry_wait_ms(7, 0), Some(25_000));
        assert_eq!(retry_wait_ms(8, 0), Some(30_000));
        assert_eq!(retry_wait_ms(40, 29_999), Some(1));
    }

    #[test]
    fn enable_unlock_and_rate_limit_persist() {
        let root = temp_root("limit");
        let inner = MemorySecretStore::new();
        let a = acct("primary");
        inner.put(&a, &DatabaseKey::generate()).unwrap();
        let state = UnlockState::new();
        enable(
            &root,
            &inner,
            std::slice::from_ref(&a),
            "correct horse",
            FAST,
            &state,
        )
        .unwrap();
        state.clear();
        let t0 = 1_000_000;
        for _ in 0..3 {
            assert_eq!(
                unlock(&root, &state, "nope", t0).unwrap_err(),
                PasscodeError::Wrong
            );
        }
        // The right passcode is refused while flooded...
        assert_eq!(
            unlock(&root, &state, "correct horse", t0 + 1_000).unwrap_err(),
            PasscodeError::Flood { retry_in_ms: 4_000 }
        );
        assert!(!state.is_unlocked());
        // ...the counter survived (it is on disk, not in memory)...
        assert_eq!(lock_wait_ms(&root, t0 + 1_000), Some(4_000));
        // ...and after the wait it works and resets the counter.
        unlock(&root, &state, "correct horse", t0 + 5_000).unwrap();
        assert!(state.is_unlocked());
        assert_eq!(load_config(&root).unwrap().bad_tries, 0);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn enable_moves_the_key_out_of_the_os_store_and_disable_restores_it() {
        let root = temp_root("migrate");
        let inner = MemorySecretStore::new();
        let a = acct("primary");
        let b = acct("account-1");
        let key_a = DatabaseKey::generate();
        let key_b = DatabaseKey::generate();
        inner.put(&a, &key_a).unwrap();
        inner.put(&b, &key_b).unwrap();
        let state = Arc::new(UnlockState::new());
        enable(&root, &inner, &[a.clone(), b.clone()], "pass", FAST, &state).unwrap();
        // The OS store no longer has the keys.
        assert!(inner.get(&a).unwrap().is_none());
        assert!(inner.get(&b).unwrap().is_none());
        // The passcode store serves them while unlocked...
        let store = PasscodeStore::new(inner.clone(), &root, state.clone());
        assert_eq!(store.get(&a).unwrap().unwrap().as_bytes(), key_a.as_bytes());
        // ...and refuses with Locked once the master key is forgotten.
        state.clear();
        assert_eq!(store.get(&a).unwrap_err(), SecretStoreError::Locked);
        assert_eq!(store.put(&a, &key_a).unwrap_err(), SecretStoreError::Locked);
        // Wrong passcode cannot disable; right one restores both keys.
        assert_eq!(
            disable(&root, &inner, &[a.clone(), b.clone()], "bad", &state, 1).unwrap_err(),
            PasscodeError::Wrong
        );
        disable(&root, &inner, &[a.clone(), b.clone()], "pass", &state, 2).unwrap();
        assert_eq!(inner.get(&a).unwrap().unwrap().as_bytes(), key_a.as_bytes());
        assert_eq!(inner.get(&b).unwrap().unwrap().as_bytes(), key_b.as_bytes());
        assert!(!is_enabled(&root));
        assert!(!wrapped_key_path(&root, &a).exists());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn change_rewraps_only_the_master_key() {
        let root = temp_root("change");
        let inner = MemorySecretStore::new();
        let a = acct("primary");
        let key = DatabaseKey::generate();
        inner.put(&a, &key).unwrap();
        let state = Arc::new(UnlockState::new());
        enable(&root, &inner, std::slice::from_ref(&a), "old", FAST, &state).unwrap();
        let before = std::fs::read(wrapped_key_path(&root, &a)).unwrap();
        assert_eq!(
            change(&root, "old", "old", FAST, &state, 1).unwrap_err(),
            PasscodeError::Same
        );
        assert_eq!(
            change(&root, "wrong", "new", FAST, &state, 1).unwrap_err(),
            PasscodeError::Wrong
        );
        change(&root, "old", "new", FAST, &state, 2).unwrap();
        assert_eq!(before, std::fs::read(wrapped_key_path(&root, &a)).unwrap());
        state.clear();
        assert_eq!(
            unlock(&root, &state, "old", 10).unwrap_err(),
            PasscodeError::Wrong
        );
        unlock(&root, &state, "new", 20).unwrap();
        let store = PasscodeStore::new(inner, &root, state);
        assert_eq!(store.get(&a).unwrap().unwrap().as_bytes(), key.as_bytes());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn failed_enable_rolls_back_and_loses_nothing() {
        let root = temp_root("rollback");
        let inner = MemorySecretStore::new();
        let a = acct("primary");
        let key = DatabaseKey::generate();
        inner.put(&a, &key).unwrap();
        inner.lock();
        let state = UnlockState::new();
        let err = enable(
            &root,
            &inner,
            std::slice::from_ref(&a),
            "pass",
            FAST,
            &state,
        )
        .unwrap_err();
        assert_eq!(err, PasscodeError::Store(SecretStoreError::Locked));
        assert!(!is_enabled(&root));
        assert!(!state.is_unlocked());
        inner.unlock();
        assert_eq!(inner.get(&a).unwrap().unwrap().as_bytes(), key.as_bytes());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn passcode_is_never_written_to_disk() {
        let root = temp_root("plain");
        let inner = MemorySecretStore::new();
        let a = acct("primary");
        inner.put(&a, &DatabaseKey::generate()).unwrap();
        let state = UnlockState::new();
        enable(
            &root,
            &inner,
            std::slice::from_ref(&a),
            "s3cret-passcode",
            FAST,
            &state,
        )
        .unwrap();
        let config = std::fs::read_to_string(config_path(&root)).unwrap();
        assert!(!config.contains("s3cret-passcode"));
        let wrapped = std::fs::read(wrapped_key_path(&root, &a)).unwrap();
        assert!(!wrapped.windows(8).any(|w| w == b"s3cret-p"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn new_passcode_validation() {
        assert_eq!(validate_new("", "", None), Err(PasscodeError::Empty));
        assert_eq!(validate_new("a", "b", None), Err(PasscodeError::Mismatch));
        assert_eq!(validate_new("a", "a", Some("a")), Err(PasscodeError::Same));
        assert_eq!(validate_new("a", "a", Some("b")), Ok(()));
    }

    #[test]
    fn autolock_timer_logic() {
        assert!(!autolock_due(59_999, 60));
        assert!(autolock_due(60_000, 60));
        assert_eq!(autolock_remaining_ms(10_000, 60), 50_000);
        assert_eq!(autolock_remaining_ms(90_000, 60), 0);
        assert_eq!(parse_hhmm("10:00"), Some(36_000));
        assert_eq!(parse_hhmm("0:05"), Some(300));
        assert_eq!(parse_hhmm("0:00"), None);
        assert_eq!(parse_hhmm("1:75"), None);
        assert_eq!(parse_hhmm("abc"), None);
        assert_eq!(autolock_label(60), "1 min");
        assert_eq!(autolock_label(18_000), "5 h");
        assert_eq!(autolock_label(5_400), "1h 30m");
    }

    #[test]
    fn a_suspend_counts_as_idle_and_input_clears_it() {
        let mut tracker = IdleTracker::default();
        assert_eq!(tracker.effective_idle(1_000, 0), 0);
        assert_eq!(tracker.effective_idle(1_040, 40), 40);
        // The machine slept for ten minutes: the monotonic idle clock paused.
        assert_eq!(tracker.effective_idle(601_040, 80), 80 + 600_000);
        assert!(autolock_due(tracker.effective_idle(601_080, 120), 300));
        // Input resets idle (the in-window clock restarts from zero).
        assert_eq!(tracker.effective_idle(601_120, 0), 0);
    }
}
