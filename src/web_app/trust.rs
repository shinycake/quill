//! Which bots the person already agreed to open a mini app for. Telegram
//! Desktop asks once per bot ("By launching this mini app, you agree to
//! the Terms of Service for Mini Apps") and remembers the answer
//! (`Main::Session::local().markPeerTrustedOpenWebView`); verified bots
//! skip the box. The ids live in `web_app_trust.json` under the app data
//! root (isolated in screenshot demos), the same way `fast_buttons.json`
//! does.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::RwLock;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
struct Stored {
    ids: BTreeSet<i64>,
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
        .and_then(|path| std::fs::read(path).ok())
        .and_then(|bytes| serde_json::from_slice::<Stored>(&bytes).ok())
        .unwrap_or_default();
    if let Ok(mut guard) = BOTS.write() {
        return guard.get_or_insert(loaded).clone();
    }
    loaded
}

/// Whether the first-open box was already accepted for this bot.
pub fn is_trusted(bot_id: i64) -> bool {
    current().ids.contains(&bot_id)
}

/// Remember the accepted box.
pub fn mark_trusted(bot_id: i64) {
    let mut stored = current();
    if !stored.ids.insert(bot_id) {
        return;
    }
    if PERSIST.load(Ordering::SeqCst)
        && let Some(path) = path()
    {
        let _ = crate::settings::write_json_atomic(&path, &stored);
    }
    if let Ok(mut guard) = BOTS.write() {
        *guard = Some(stored);
    }
}

/// Whether opening `bot_id`'s app needs the box first: not for verified
/// bots, not after it was accepted once, always when the caller insists
/// (tdesktop `forceConfirmation`: an inactive app, or a link with
/// `confirmType == Always`).
pub fn needs_confirmation(bot_id: i64, verified: bool, force: bool) -> bool {
    force || !(verified || is_trusted(bot_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trust_is_remembered_in_memory_without_persistence() {
        set_persistence(false);
        let bot = 9_000_000_001;
        assert!(!is_trusted(bot));
        assert!(needs_confirmation(bot, false, false));
        assert!(
            !needs_confirmation(bot, true, false),
            "verified bots skip the box"
        );
        mark_trusted(bot);
        assert!(is_trusted(bot));
        assert!(!needs_confirmation(bot, false, false));
        assert!(needs_confirmation(bot, true, true), "force always asks");
    }
}
