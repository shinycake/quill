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

/// The salt saved in `dir`, if there is a valid one.
fn saved_salt(dir: &Path) -> Option<String> {
    let saved = std::fs::read_to_string(dir.join(WEBVIEW_SALT)).ok()?;
    let saved = saved.trim();
    (saved.len() == 32 && saved.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .then(|| saved.to_owned())
}

/// The salt saved in `dir`, made and saved first when there is none.
fn webview_salt(dir: &Path) -> Option<String> {
    let file = dir.join(WEBVIEW_SALT);
    if let Some(saved) = saved_salt(dir) {
        return Some(saved);
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
    // Whose folder this is: its salt now. The helper can outlive a sign-in
    // under the same key, whose folder carries another salt.
    let owner = saved_salt(&dir);
    let spawned = std::thread::Builder::new()
        .name("quill-webview-forget".into())
        .spawn(move || {
            let _ = helper_gone.recv();
            remove_webview_dir_of(&later, owner.as_deref());
        });
    if spawned.is_err() {
        let _ = std::fs::remove_dir_all(dir);
    }
}

/// Delete `dir` unless a later sign-in made it its own (it then holds a
/// different salt than `owner`, the one read when the old account left).
fn remove_webview_dir_of(dir: &Path, owner: Option<&str>) {
    if saved_salt(dir).as_deref() == owner {
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

/// Who an account folder was last signed in as.
enum Previous {
    /// No readable `owner.json`: never written, deleted, or corrupt.
    Unknown,
    Known(i64),
}

fn read_owner(file: &Path) -> Previous {
    std::fs::read(file)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Owner>(&bytes).ok())
        .map_or(Previous::Unknown, |owner| Previous::Known(owner.user_id))
}

/// Whether something the last user could have left is on disk for `key`
/// (what [`forget`] drops from the folder and the app root).
fn has_leftovers(app_root: &Path, paths: &AccountPaths, account: &str) -> bool {
    has_recent_hashtags(&paths.root.join(RECENT_HASHTAGS))
        || paths.root.join(crate::connect::PAID_TRUST_FILE).exists()
        || widget_dir(app_root, account).exists()
        || webview_dir(app_root, account).exists()
}

/// Whether the recent hashtags file holds a tag. The empty list that
/// [`forget`] writes is not a leftover, so a wipe whose owner write fails
/// does not find "leftovers" again at the next start.
fn has_recent_hashtags(file: &Path) -> bool {
    std::fs::read(file)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<crate::suggest::RecentHashtags>(&bytes).ok())
        .is_some_and(|list| !list.tags.is_empty())
}

/// How this run's client got to be signed in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SignInOrigin {
    /// TDLib resumed a session it already had: whose it is was settled
    /// when it signed in, possibly by a release that kept no `owner.json`.
    #[default]
    Resumed,
    /// This process took the user through a phone, code, QR or password
    /// step: a person just signed in, and may not be who the folder held.
    Fresh,
}

/// What [`claim`] did.
pub struct Claimed {
    /// The folder was last signed in as someone else (or as someone who
    /// cannot be told), and what they left was dropped first.
    pub dropped: bool,
    /// Why the new owner could not be saved; until it is, the next start
    /// cannot tell whose the folder is.
    pub owner_error: Option<std::io::Error>,
}

/// A client of `key` learned it is signed in as `user_id`. When the folder
/// was last signed in as someone else, what they left is dropped first. An
/// owner that cannot be read (no file, a corrupt one) with leftovers in the
/// folder counts as someone else only after a [`SignInOrigin::Fresh`]
/// sign-in; a resumed session is the folder's own account (an older release
/// kept no owner file), so its data stays and the owner is written.
pub fn claim(app_root: &Path, key: &AccountKey, user_id: i64, origin: SignInOrigin) -> Claimed {
    let paths = AccountPaths::for_root(app_root, key);
    let file = paths.root.join(OWNER_FILE);
    let someone_else = match read_owner(&file) {
        Previous::Known(previous) if previous == user_id => {
            return Claimed {
                dropped: false,
                owner_error: None,
            };
        }
        Previous::Known(_) => true,
        Previous::Unknown => {
            origin == SignInOrigin::Fresh && has_leftovers(app_root, &paths, key.0.as_str())
        }
    };
    if someone_else {
        forget(app_root, key);
    }
    let owner_error = std::fs::create_dir_all(&paths.root)
        .and_then(|()| crate::settings::write_json_atomic(&file, &Owner { user_id }))
        .err();
    Claimed {
        dropped: someone_else,
        owner_error,
    }
}

#[cfg(test)]
#[path = "account_data_tests.rs"]
mod tests;
