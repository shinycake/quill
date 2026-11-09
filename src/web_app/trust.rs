//! Which bots the person already agreed to open a mini app for. Telegram
//! Desktop asks once per bot ("By launching this mini app, you agree to
//! the Terms of Service for Mini Apps") and remembers the answer
//! (`Main::Session::local().markPeerTrustedOpenWebView`, one list per
//! account); verified bots skip the box. The ids live in
//! `web_app_trust.json` under the app data root (isolated in screenshot
//! demos), keyed by account, so accepting on one account never skips the
//! box on another. The earlier app-wide list (`ids`) is not read: whose
//! account accepted it is unknown, so each account asks again once.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::RwLock;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
struct Stored {
    accounts: BTreeMap<String, BTreeSet<i64>>,
}

static BOTS: RwLock<Option<Stored>> = RwLock::new(None);
static PERSIST: AtomicBool = AtomicBool::new(true);

/// Turn disk writes off (tests and demos keep the set in memory only).
pub fn set_persistence(enabled: bool) {
    PERSIST.store(enabled, Ordering::SeqCst);
}

fn path() -> Option<PathBuf> {
    crate::settings::safe_app_root().map(|root| root.join("web_app_trust.json"))
}

fn current() -> Stored {
    if let Ok(guard) = BOTS.read()
        && let Some(stored) = guard.as_ref()
    {
        return stored.clone();
    }
    let loaded = path()
        .and_then(|path| crate::settings::read_with_pending(&path).ok())
        .and_then(|bytes| serde_json::from_slice::<Stored>(&bytes).ok())
        .unwrap_or_default();
    if let Ok(mut guard) = BOTS.write() {
        return guard.get_or_insert(loaded).clone();
    }
    loaded
}

/// Whether `account` already accepted the first-open box for this bot.
pub fn is_trusted(account: &str, bot_id: i64) -> bool {
    current()
        .accounts
        .get(account)
        .is_some_and(|ids| ids.contains(&bot_id))
}

/// Remember the box `account` accepted.
pub fn mark_trusted(account: &str, bot_id: i64) {
    let mut stored = current();
    if !stored
        .accounts
        .entry(account.to_string())
        .or_default()
        .insert(bot_id)
    {
        return;
    }
    store(stored);
}

/// A removed account's answers go with it.
pub fn forget_account(account: &str) {
    let mut stored = current();
    if stored.accounts.remove(account).is_none() {
        return;
    }
    store(stored);
}

fn store(stored: Stored) {
    if PERSIST.load(Ordering::SeqCst)
        && let Some(path) = path()
    {
        crate::settings::write_json_deferred(&path, &stored);
    }
    if let Ok(mut guard) = BOTS.write() {
        *guard = Some(stored);
    }
}

/// Whether opening `bot_id`'s app needs the box first: not for verified
/// bots, not after it was accepted once, always when the caller insists
/// (tdesktop `forceConfirmation`: an inactive app, or a link with
/// `confirmType == Always`).
pub fn needs_confirmation(account: &str, bot_id: i64, verified: bool, force: bool) -> bool {
    force || !(verified || is_trusted(account, bot_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trust_is_remembered_in_memory_without_persistence() {
        set_persistence(false);
        let bot = 9_000_000_001;
        assert!(!is_trusted("a", bot));
        assert!(needs_confirmation("a", bot, false, false));
        assert!(
            !needs_confirmation("a", bot, true, false),
            "verified bots skip the box"
        );
        mark_trusted("a", bot);
        assert!(is_trusted("a", bot));
        assert!(!needs_confirmation("a", bot, false, false));
        assert!(
            needs_confirmation("a", bot, true, true),
            "force always asks"
        );
    }

    /// Accepting on one account leaves the box on every other, and a
    /// removed account's answers go with it.
    #[test]
    fn trust_is_kept_per_account() {
        set_persistence(false);
        let bot = 9_000_000_002;
        mark_trusted("trust-a", bot);
        assert!(is_trusted("trust-a", bot));
        assert!(!is_trusted("trust-b", bot));
        assert!(needs_confirmation("trust-b", bot, false, false));
        forget_account("trust-a");
        assert!(!is_trusted("trust-a", bot));
    }
}
