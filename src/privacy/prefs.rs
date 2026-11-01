//! The one-time last seen warning (tdesktop `lastSeenWarningSeen`): before
//! a free account first hides its last seen, a box says it will stop seeing
//! others' exact times. Whether it was shown lives in `privacy_hints.json`
//! under the app data root, for every account, like tdesktop's app setting.

use super::PrivacyRuleDetail;
use crate::telegram::requests::PrivacyWho;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::RwLock;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
struct Stored {
    last_seen_warning_seen: bool,
}

static PREFS: RwLock<Option<Stored>> = RwLock::new(None);
static PERSIST: AtomicBool = AtomicBool::new(!cfg!(test));

/// Turn disk writes off (tests keep the flag in memory only).
pub fn set_privacy_hints_persistence(enabled: bool) {
    PERSIST.store(enabled, Ordering::SeqCst);
}

fn path() -> Option<PathBuf> {
    crate::settings::safe_app_root().map(|root| root.join("privacy_hints.json"))
}

fn current() -> Stored {
    if let Ok(guard) = PREFS.read()
        && let Some(stored) = *guard
    {
        return stored;
    }
    let loaded = if PERSIST.load(Ordering::SeqCst) {
        path()
            .and_then(|path| crate::settings::read_with_pending(&path).ok())
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

/// Whether the last seen warning was already confirmed once.
pub fn last_seen_warning_seen() -> bool {
    current().last_seen_warning_seen
}

/// Remember (or forget) that the warning was confirmed.
pub fn set_last_seen_warning_seen(seen: bool) {
    let mut stored = current();
    if stored.last_seen_warning_seen == seen {
        return;
    }
    stored.last_seen_warning_seen = seen;
    if PERSIST.load(Ordering::SeqCst)
        && let Some(path) = path()
    {
        let _ = crate::settings::write_json_deferred(&path, &stored);
    }
    if let Ok(mut guard) = PREFS.write() {
        *guard = Some(stored);
    }
}

/// Saving `detail` as the last seen rule needs the warning first
/// (tdesktop `LastSeenPrivacyController::confirmSave`): it hides last seen
/// from someone, the account has no Premium and the warning was never
/// confirmed.
pub fn last_seen_warning_due(detail: &PrivacyRuleDetail, premium: bool, seen: bool) -> bool {
    let hides = detail.who != Some(PrivacyWho::Everybody) || detail.exception_counts().1 > 0;
    hides && !premium && !seen
}

#[cfg(test)]
mod tests {
    use super::{last_seen_warning_due, last_seen_warning_seen, set_last_seen_warning_seen};
    use crate::privacy::PrivacyRuleDetail;
    use crate::telegram::requests::PrivacyWho;

    #[test]
    fn hiding_last_seen_warns_a_free_account_once() {
        let everybody = PrivacyRuleDetail {
            who: Some(PrivacyWho::Everybody),
            ..Default::default()
        };
        let contacts = everybody.with_base(PrivacyWho::Contacts);
        let never_one = PrivacyRuleDetail {
            never: vec![7],
            ..everybody.clone()
        };
        assert!(!last_seen_warning_due(&everybody, false, false));
        assert!(last_seen_warning_due(&contacts, false, false));
        assert!(last_seen_warning_due(&never_one, false, false));
        assert!(!last_seen_warning_due(&contacts, true, false), "Premium");
        assert!(!last_seen_warning_due(&contacts, false, true), "seen");
    }

    #[test]
    fn the_seen_flag_sticks() {
        set_last_seen_warning_seen(true);
        assert!(last_seen_warning_seen());
        set_last_seen_warning_seen(false);
        assert!(!last_seen_warning_seen());
    }
}
