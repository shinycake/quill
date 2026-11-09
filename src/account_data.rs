//! What Quill keeps for one Telegram account outside TDLib's database, and
//! the one place that drops it when the account logs out or is removed.
//!
//! tdesktop's `Storage::Account::reset` (run by `Main::Account::loggedOut`)
//! wipes every per-account key: drafts, recent hashtags, trusted peers,
//! settings, web view storage. Quill's stores are spread over the account
//! folder (`accounts/<key>/`) and app-wide files keyed by the account, and
//! account keys are reused: a log-out leaves the slot under the same key,
//! and `add_account` hands out the lowest free `account-<n>` again. So
//! [`forget`] runs on every way an account goes (log-out, background
//! sign-out, removal, the lock-screen wipe, and the next start's sweep of a
//! removal that did not finish), and [`claim`] runs when a client learns
//! whose account it is: a different Telegram user in the slot drops what
//! the last one left, even when the app quit before the log-out finished.
//!
//! The account folder itself is deleted only on removal. Device
//! preferences in it (notification view, call and media options, storage
//! limits) stay on a log-out, as tdesktop keeps its app-wide settings.
//!
//! The test `every_store_is_classified` lists every file name production
//! code reads or writes, so a new per-account store can't be added without
//! deciding whether a log-out drops it.

use crate::ids::AccountKey;
use crate::settings::AccountPaths;
use std::path::{Path, PathBuf};

/// The Telegram user an account folder was last signed in as.
const OWNER_FILE: &str = "owner.json";

#[derive(serde::Serialize, serde::Deserialize)]
struct Owner {
    user_id: i64,
}

/// Where one account's mini apps keep cookies and local storage (one
/// folder per bot inside it).
pub fn webview_dir(app_root: &Path, account: &str) -> PathBuf {
    app_root.join("webview").join(account)
}

/// The salt in [`webview_dir`] that [`webview_store_id`] mixes in.
const WEBVIEW_SALT: &str = "store-salt.key";

/// 16 bytes (as hex) naming the WebKit data store of one bot of one
/// account. WKWebView keeps that store under `~/Library/WebKit`, outside
/// Quill's folder, so deleting [`webview_dir`] cannot drop it. The id
/// mixes in a random salt kept in that folder instead: [`forget`] deletes
/// the salt with the folder, so whoever signs in next under the same key
/// gets new stores. When the salt can't be saved, a fresh one per call
/// keeps the store private at the cost of keeping nothing.
pub fn webview_store_id(app_root: Option<&Path>, account: &str, bot_id: i64) -> String {
    use sha2::{Digest, Sha256};
    let salt = app_root
        .and_then(|root| webview_salt(&webview_dir(root, account)))
        .unwrap_or_else(new_salt);
    let digest = Sha256::digest(format!("quill-webview/{salt}/{account}/{bot_id}").as_bytes());
    hex(digest.iter().take(16))
}

fn hex<'a>(bytes: impl Iterator<Item = &'a u8>) -> String {
    bytes.map(|byte| format!("{byte:02x}")).collect()
}

fn new_salt() -> String {
    use rand::Rng;
    let mut bytes = [0u8; 16];
    rand::rng().fill_bytes(&mut bytes);
    hex(bytes.iter())
}

/// The salt saved in `dir`, made and saved first when there is none.
fn webview_salt(dir: &Path) -> Option<String> {
    let file = dir.join(WEBVIEW_SALT);
    if let Ok(saved) = std::fs::read_to_string(&file) {
        let saved = saved.trim();
        if saved.len() == 32 && saved.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Some(saved.to_owned());
        }
    }
    let salt = new_salt();
    std::fs::create_dir_all(dir).ok()?;
    crate::settings::write_bytes_atomic(&file, salt.as_bytes()).ok()?;
    Some(salt)
}

/// Where one account's message widgets keep their storage and last height
/// (one file per message, [`crate::message_widget::storage`]).
pub fn widget_dir(app_root: &Path, account: &str) -> PathBuf {
    app_root.join("widgets").join(account)
}

/// `key`'s account logged out or was removed: drop everything Quill kept
/// for it outside TDLib's database, so nobody who signs in next under the
/// same key inherits it. Safe to run more than once.
pub fn forget(app_root: &Path, key: &AccountKey) {
    forget_after_helper(app_root, key, None);
}

/// [`forget`], with the account's web view data removed only once the mini
/// app helper that has it open is gone (`helper_gone` hears, or
/// disconnects, when it exited or was killed). While it runs, WebView2
/// keeps its files locked (the delete fails partway) and WebKitGTK writes
/// them back, so the next sign-in under this key would find them.
pub fn forget_after_helper(
    app_root: &Path,
    key: &AccountKey,
    helper_gone: Option<std::sync::mpsc::Receiver<()>>,
) {
    let account = key.0.as_str();
    // The app-wide stores are this process's, kept under its own app root
    // (a test may name another root, with stores of its own).
    if crate::settings::safe_app_root().as_deref() == Some(app_root) {
        crate::agent::grants::forget_account(key);
        crate::agent::activity::forget_account(key);
        crate::agent::prompts::deny_account(key);
        crate::agent::pacing::forget_account(key);
        crate::web_app::trust::forget_account(account);
        crate::notify_volume::forget_account(account);
        crate::settings::forget_chat_windows_of(account);
    }
    let paths = AccountPaths::for_root(app_root, key);
    forget_folder_data(&paths);
    let _ = std::fs::remove_dir_all(widget_dir(app_root, account));
    let dir = webview_dir(app_root, account);
    let Some(helper_gone) = helper_gone else {
        let _ = std::fs::remove_dir_all(dir);
        return;
    };
    let later = dir.clone();
    let spawned = std::thread::Builder::new()
        .name("quill-webview-forget".into())
        .spawn(move || {
            let _ = helper_gone.recv();
            let _ = std::fs::remove_dir_all(later);
        });
    if spawned.is_err() {
        let _ = std::fs::remove_dir_all(dir);
    }
}

/// The account's own data inside its folder. Its device preferences stay.
fn forget_folder_data(paths: &AccountPaths) {
    if !paths.root.exists() {
        return;
    }
    // Written through the deferred queue: an empty list queued behind any
    // write still waiting, so nothing older lands after it.
    if crate::settings::read_with_pending(&paths.root.join(RECENT_HASHTAGS)).is_ok() {
        crate::settings::save_recent_hashtags(paths, &Default::default());
    }
    let _ = std::fs::remove_file(paths.root.join(crate::connect::PAID_TRUST_FILE));
    crate::launch_lines::forget(paths);
}

const RECENT_HASHTAGS: &str = "recent_hashtags.json";

/// A client of `key` learned it is signed in as `user_id`. When the folder
/// was last signed in as someone else, what they left is dropped first.
/// Returns whether it was.
pub fn claim(app_root: &Path, key: &AccountKey, user_id: i64) -> bool {
    let paths = AccountPaths::for_root(app_root, key);
    let file = paths.root.join(OWNER_FILE);
    let previous = std::fs::read(&file)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Owner>(&bytes).ok())
        .map(|owner| owner.user_id);
    if previous == Some(user_id) {
        return false;
    }
    let someone_else = previous.is_some();
    if someone_else {
        forget(app_root, key);
    }
    if std::fs::create_dir_all(&paths.root).is_ok()
        && let Err(err) = crate::settings::write_json_atomic(&file, &Owner { user_id })
    {
        eprintln!("quill: could not save {OWNER_FILE}: {err}");
    }
    someone_else
}

#[cfg(test)]
#[path = "account_data_tests.rs"]
mod tests;
