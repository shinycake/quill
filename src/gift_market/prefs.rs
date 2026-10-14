//! The one gift-market preference: whether the auction "About" box was
//! shown (tdesktop's `gift_auction_about_shown` pref). It lives in
//! `gift_market.json` under the app data root (isolated in screenshot
//! demos), for every account of the app, like tdesktop's local pref.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::RwLock;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
struct Stored {
    auction_about_shown: bool,
}

static PREFS: RwLock<Option<Stored>> = RwLock::new(None);
static PERSIST: AtomicBool = AtomicBool::new(!cfg!(test));

/// Turn disk writes off (tests keep the pref in memory only).
pub fn set_gift_market_persistence(enabled: bool) {
    PERSIST.store(enabled, Ordering::SeqCst);
}

fn path() -> Option<PathBuf> {
    crate::settings::safe_app_root().map(|root| root.join("gift_market.json"))
}

fn current() -> Stored {
    if let Ok(guard) = PREFS.read()
        && let Some(stored) = *guard
    {
        return stored;
    }
    let loaded = if PERSIST.load(Ordering::SeqCst) {
        path()
            .and_then(|path| std::fs::read(path).ok())
            .and_then(|bytes| serde_json::from_slice::<Stored>(&bytes).ok())
            .unwrap_or_default()
    } else {
        Stored::default()
    };
    if let Ok(mut guard) = PREFS.write() {
        return *guard.get_or_insert(loaded);
    }
    loaded
}

/// Whether the auction "About" box was already shown once.
pub fn auction_about_shown() -> bool {
    current().auction_about_shown
}

/// Remember (or forget) that the "About" box was shown.
pub fn set_auction_about_shown(shown: bool) {
    let mut stored = current();
    if stored.auction_about_shown == shown {
        return;
    }
    stored.auction_about_shown = shown;
    if PERSIST.load(Ordering::SeqCst)
        && let Some(path) = path()
    {
        let _ = crate::settings::write_json_atomic(&path, &stored);
    }
    if let Ok(mut guard) = PREFS.write() {
        *guard = Some(stored);
    }
}
