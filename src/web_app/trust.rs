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
use std::path::{Path, PathBuf};
use std::sync::RwLock;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
struct Stored {
    accounts: BTreeMap<String, BTreeSet<i64>>,
}

/// The set and whether the file under it was understood. An `Unreadable`
/// file (an I/O error other than "no such file") is never written over:
/// answers taken meanwhile stay in memory (`kept`), the accounts forgotten
/// meanwhile are listed (`forgotten`) so that reading the file again does
/// not bring their answers back, and the next change reads the file again.
/// A file with text that is not this format is moved aside and read as
/// empty (`load_from`), so it never stays `Unreadable` for good.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Trust {
    Read(Stored),
    Unreadable {
        kept: Stored,
        forgotten: BTreeSet<String>,
    },
}

impl Trust {
    fn unreadable() -> Self {
        Self::Unreadable {
            kept: Stored::default(),
            forgotten: BTreeSet::new(),
        }
    }

    fn stored(&self) -> &Stored {
        match self {
            Self::Read(stored) | Self::Unreadable { kept: stored, .. } => stored,
        }
    }
}

/// One change to the set.
#[derive(Clone, Copy)]
enum Edit<'a> {
    Trust(&'a str, i64),
    Forget(&'a str),
}

impl Edit<'_> {
    /// Whether it changed `stored`.
    fn run(self, stored: &mut Stored) -> bool {
        match self {
            Self::Trust(account, bot) => stored
                .accounts
                .entry(account.to_string())
                .or_default()
                .insert(bot),
            Self::Forget(account) => stored.accounts.remove(account).is_some(),
        }
    }
}

static BOTS: RwLock<Option<Trust>> = RwLock::new(None);
static PERSIST: AtomicBool = AtomicBool::new(true);

/// Turn disk writes off (tests and demos keep the set in memory only).
pub fn set_persistence(enabled: bool) {
    PERSIST.store(enabled, Ordering::SeqCst);
}

fn path() -> Option<PathBuf> {
    crate::settings::safe_app_root().map(|root| root.join("web_app_trust.json"))
}

fn load_from(path: Option<&Path>) -> Trust {
    let Some(path) = path else {
        return Trust::Read(Stored::default());
    };
    match crate::settings::read_with_pending(path) {
        Ok(bytes) => match serde_json::from_slice::<Stored>(&bytes) {
            Ok(stored) => Trust::Read(stored),
            // Not this format: keep the bytes beside it, and start empty so
            // the next answer is saved again.
            Err(_) => match std::fs::rename(path, path.with_extension("json.corrupt")) {
                Ok(()) => Trust::Read(Stored::default()),
                Err(_) => Trust::unreadable(),
            },
        },
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Trust::Read(Stored::default()),
        Err(_) => Trust::unreadable(),
    }
}

fn load() -> Trust {
    load_from(path().as_deref())
}

fn current() -> Stored {
    if let Ok(guard) = BOTS.read()
        && let Some(trust) = guard.as_ref()
    {
        return trust.stored().clone();
    }
    crate::locks::write_or_recover(&BOTS)
        .get_or_insert_with(load)
        .stored()
        .clone()
}

/// Change the set under its lock and queue the save there too, so two
/// changes at once (a log-out forgetting while another account trusts a
/// bot) can't put back what the other removed.
fn change(edit: Edit) {
    let mut guard = crate::locks::write_or_recover(&BOTS);
    let trust = guard.get_or_insert_with(load);
    let save = apply(trust, load, edit);
    if save
        && let Trust::Read(stored) = trust
        && PERSIST.load(Ordering::SeqCst)
        && let Some(path) = path()
    {
        crate::settings::write_json_deferred(&path, &*stored);
    }
}

/// Apply `edit`; an unreadable file is read again first: the accounts
/// forgotten meanwhile are dropped from what it holds, and what was
/// answered meanwhile joins it. Whether there is something to save: the
/// edit changed something, or the file was read again, and the file is
/// understood (never over an unreadable one).
fn apply(trust: &mut Trust, reload: impl FnOnce() -> Trust, edit: Edit) -> bool {
    let mut save = false;
    if let Trust::Unreadable { kept, forgotten } = trust
        && let Trust::Read(mut disk) = reload()
    {
        for account in std::mem::take(forgotten) {
            disk.accounts.remove(&account);
        }
        for (account, ids) in std::mem::take(&mut kept.accounts) {
            disk.accounts.entry(account).or_default().extend(ids);
        }
        *trust = Trust::Read(disk);
        save = true;
    }
    let changed = match trust {
        Trust::Read(stored) => edit.run(stored),
        Trust::Unreadable { kept, forgotten } => {
            let changed = edit.run(kept);
            if let Edit::Forget(account) = edit {
                forgotten.insert(account.to_string());
            }
            changed
        }
    };
    (changed || save) && matches!(trust, Trust::Read(_))
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
    change(Edit::Trust(account, bot_id));
}

/// A logged-out or removed account's answers go with it.
pub fn forget_account(account: &str) {
    change(Edit::Forget(account));
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

    fn set(account: &str, id: i64) -> Stored {
        let mut stored = Stored::default();
        stored
            .accounts
            .entry(account.into())
            .or_default()
            .insert(id);
        stored
    }

    /// A trust file that cannot be understood is not written over: the
    /// answer stays in memory, and the file is read again by the next change.
    #[test]
    fn an_unreadable_file_is_never_saved_over() {
        let mut trust = Trust::unreadable();
        let save = apply(&mut trust, Trust::unreadable, Edit::Trust("a", 1));
        assert!(!save, "nothing may be written while the file is unreadable");
        assert!(
            matches!(&trust, Trust::Unreadable { kept, .. } if kept.accounts.contains_key("a"))
        );
        let save = apply(
            &mut trust,
            || Trust::Read(set("b", 2)),
            Edit::Forget("none"),
        );
        assert!(save, "the file reads again: the merged set is saved");
        assert!(matches!(&trust, Trust::Read(s)
            if s.accounts.contains_key("a") && s.accounts.contains_key("b")));
    }

    /// A log-out while the file cannot be read: the account's answers on
    /// disk do not come back when the file reads again.
    #[test]
    fn a_forgotten_account_stays_forgotten_when_the_file_reads_again() {
        let mut trust = Trust::unreadable();
        assert!(!apply(&mut trust, Trust::unreadable, Edit::Forget("gone")));
        let mut disk = set("gone", 1);
        disk.accounts.entry("kept".into()).or_default().insert(2);
        assert!(apply(
            &mut trust,
            || Trust::Read(disk),
            Edit::Trust("new", 3)
        ));
        assert!(matches!(&trust, Trust::Read(stored)
            if !stored.accounts.contains_key("gone")
                && stored.accounts.contains_key("kept")
                && stored.accounts.contains_key("new")));
    }

    #[test]
    fn load_tells_a_missing_file_from_a_corrupt_one() {
        let dir = std::env::temp_dir().join(format!("quill-trust-{}", std::process::id()));
        assert!(std::fs::create_dir_all(&dir).is_ok());
        let file = dir.join("web_app_trust.json");
        assert_eq!(load_from(Some(&file)), Trust::Read(Stored::default()));
        assert!(std::fs::write(&file, b"{not json").is_ok());
        // A corrupt file is moved aside and read as empty, so the next
        // answer is saved.
        assert_eq!(load_from(Some(&file)), Trust::Read(Stored::default()));
        assert!(!file.exists() && dir.join("web_app_trust.json.corrupt").exists());
        assert!(std::fs::write(&file, br#"{"accounts":{"a":[5]}}"#).is_ok());
        assert_eq!(load_from(Some(&file)), Trust::Read(set("a", 5)));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
