//! Which chats agents may use, per account, and with what scope.
//!
//! The user sets this in Settings > Agent access. Three classes
//! of chat have a default of their own (Saved Messages, bots, channels the
//! user owns); "All chats" covers every other chat except secret chats;
//! a per-chat entry overrides all of these, and is the only way to grant a
//! secret chat. Every tool call reads the grants afresh, so a change applies
//! to the next call.

use crate::ids::AccountKey;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::RwLock;
use std::sync::atomic::{AtomicU64, Ordering};

const GRANTS_FILE: &str = "agent_grants.json";

/// What an agent may do in a chat.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    Read,
    ReadWrite,
}

impl Scope {
    /// The word agents and the guide use.
    pub fn as_str(self) -> &'static str {
        match self {
            Scope::Read => "read",
            Scope::ReadWrite => "read_write",
        }
    }

    pub fn parse(word: &str) -> Option<Self> {
        match word
            .trim()
            .to_ascii_lowercase()
            .replace(['+', ' ', '-'], "_")
            .as_str()
        {
            "read" => Some(Scope::Read),
            "read_write" | "write" | "readwrite" => Some(Scope::ReadWrite),
            _ => None,
        }
    }

    /// Whether this scope covers `needed`.
    pub fn allows(self, needed: Scope) -> bool {
        self >= needed
    }
}

/// Whether TDLib's chat id layout puts `chat_id` in the secret chats' range.
pub fn is_secret_chat_id(chat_id: i64) -> bool {
    // TDLib: secret chat ids are -2_000_000_000_000 plus the 32-bit secret
    // chat id (td/telegram/DialogId.h).
    const MIN: i64 = -2_000_000_000_000 + i32::MIN as i64 + 1;
    const MAX: i64 = -2_000_000_000_000 + i32::MAX as i64;
    (MIN..=MAX).contains(&chat_id)
}

/// The kind of chat, which picks the default that applies to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatClass {
    Saved,
    Bot,
    OwnChannel,
    Secret,
    Other,
}

/// One chat the user granted (or blocked) by name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatGrant {
    /// `None` blocks the chat even when a default or All chats covers it.
    pub scope: Option<Scope>,
    /// The chat's title when it was granted, for the settings list.
    pub title: String,
}

/// One account's grants.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AccountGrants {
    pub saved: Option<Scope>,
    pub bots: Option<Scope>,
    pub own_channels: Option<Scope>,
    /// Every chat that is not secret. Off unless the user agrees to it.
    pub all_chats: Option<Scope>,
    /// Reading the contact list (names, usernames; never phone numbers).
    pub contacts: bool,
    /// Per-chat entries by chat id.
    pub chats: BTreeMap<i64, ChatGrant>,
}

impl Default for AccountGrants {
    /// The narrow defaults: Saved Messages and bots read and write, the
    /// user's own channels read only (posting there reaches every
    /// subscriber), nothing else.
    fn default() -> Self {
        Self {
            saved: Some(Scope::ReadWrite),
            bots: Some(Scope::ReadWrite),
            own_channels: Some(Scope::Read),
            all_chats: None,
            contacts: false,
            chats: BTreeMap::new(),
        }
    }
}

impl AccountGrants {
    /// The scope agents have in chat `chat_id` of class `class`.
    pub fn scope_for(&self, chat_id: i64, class: ChatClass) -> Option<Scope> {
        if let Some(grant) = self.chats.get(&chat_id) {
            return grant.scope;
        }
        self.class_scope(class)
    }

    /// The scope agents have in a chat of class `class` that was not added
    /// by name: the class's own default, raised by All chats for every
    /// class but Saved Messages and secret chats.
    pub fn class_scope(&self, class: ChatClass) -> Option<Scope> {
        match class {
            ChatClass::Saved => self.saved,
            ChatClass::Secret => None,
            ChatClass::Bot => self.bots.max(self.all_chats),
            ChatClass::OwnChannel => self.own_channels.max(self.all_chats),
            ChatClass::Other => self.all_chats,
        }
    }

    /// The grants as agents read them (`quill_status`, `quill://grants`):
    /// each class with All chats counted, as calls are checked.
    pub fn describe(&self) -> serde_json::Value {
        let word = |scope: Option<Scope>| scope.map_or("off", Scope::as_str);
        let chats: Vec<serde_json::Value> = self
            .chats
            .iter()
            .map(|(id, grant)| {
                serde_json::json!({"chat_id": id, "title": grant.title, "scope": word(grant.scope)})
            })
            .collect();
        serde_json::json!({
            "saved_messages": word(self.saved),
            "bots": word(self.class_scope(ChatClass::Bot)),
            "own_channels": word(self.class_scope(ChatClass::OwnChannel)),
            "all_chats": word(self.all_chats),
            "secret_chats": "only those listed in chats",
            "contacts": self.contacts,
            "chats": chats,
        })
    }

    /// One line per grant, for text answers.
    pub fn summary_lines(&self) -> Vec<String> {
        let word = |scope: Option<Scope>| scope.map_or("off", Scope::as_str);
        let mut lines = vec![
            format!("Saved Messages: {}", word(self.saved)),
            format!("Bots: {}", word(self.class_scope(ChatClass::Bot))),
            format!(
                "Channels you own: {}",
                word(self.class_scope(ChatClass::OwnChannel))
            ),
            format!("All other chats: {}", word(self.all_chats)),
            format!("Contacts: {}", if self.contacts { "read" } else { "off" }),
        ];
        for (id, grant) in &self.chats {
            lines.push(format!("\"{}\" [{id}]: {}", grant.title, word(grant.scope)));
        }
        lines
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct Stored {
    accounts: BTreeMap<String, AccountGrants>,
}

static CACHE: RwLock<Option<BTreeMap<String, AccountGrants>>> = RwLock::new(None);

fn path() -> Option<PathBuf> {
    crate::settings::safe_app_root().map(|root| root.join(GRANTS_FILE))
}

fn load() -> BTreeMap<String, AccountGrants> {
    path()
        .and_then(|path| std::fs::read(path).ok())
        .and_then(|bytes| serde_json::from_slice::<Stored>(&bytes).ok())
        .map(|stored| stored.accounts)
        .unwrap_or_default()
}

/// The account Quill has open: the one agents' tools and the window driver
/// act on.
pub fn open_account() -> AccountKey {
    crate::settings::safe_app_root()
        .map(|root| crate::settings::active_account(&root))
        .unwrap_or_else(AccountKey::primary)
}

/// Whether the window driver shows agents the whole window: All chats is
/// on for the open account, whatever account Settings is editing.
pub fn window_shows_everything() -> bool {
    for_account(&open_account()).all_chats.is_some()
}

/// The grants of `account` (the defaults until the user changes them).
pub fn for_account(account: &AccountKey) -> AccountGrants {
    if let Ok(guard) = CACHE.read()
        && let Some(all) = guard.as_ref()
    {
        return all.get(&account.0).cloned().unwrap_or_default();
    }
    let all = load();
    let grants = all.get(&account.0).cloned().unwrap_or_default();
    if let Ok(mut guard) = CACHE.write() {
        guard.get_or_insert(all);
    }
    grants
}

/// Counts every change to any account's grants, so a check made before a
/// slow step can tell whether the grants it read still hold.
static GENERATION: AtomicU64 = AtomicU64::new(0);

/// The grants' generation; it moves on every [`update`].
pub fn generation() -> u64 {
    GENERATION.load(Ordering::SeqCst)
}

/// Moves the generation as a grant change would, without touching any
/// account's grants on disk.
#[cfg(test)]
pub(crate) fn bump_generation_for_test() {
    GENERATION.fetch_add(1, Ordering::SeqCst);
}

/// Change the grants of `account` and save them. Refused while the UI
/// driver injects an agent's input: an agent never widens its own grants.
/// A change that leaves the grants as they were (an Allow on a one-off
/// confirmation) saves nothing and keeps the generation, so it does not
/// refuse the writes waiting on it.
pub fn update(
    account: &AccountKey,
    change: impl FnOnce(&mut AccountGrants),
) -> Result<(), SaveError> {
    update_as(account, Direction::Widen, change)
}

/// Like [`update`] for a change that only takes access away (a revoke, the
/// contacts switch turned off). Not refused while the UI driver injects
/// input, and the cache takes it even when the file cannot be written: a
/// narrowing that failed to save must not leave agents with what the user
/// just took back. The save error is still returned.
pub fn update_narrowing(
    account: &AccountKey,
    change: impl FnOnce(&mut AccountGrants),
) -> Result<(), SaveError> {
    update_as(account, Direction::Narrow, change)
}

fn update_as(
    account: &AccountKey,
    direction: Direction,
    change: impl FnOnce(&mut AccountGrants),
) -> Result<(), SaveError> {
    if direction == Direction::Widen
        && super::ui_driver::guard::refuse_if_injecting("change which chats agents may use")
    {
        return Ok(());
    }
    change_all(direction, |all| {
        let before = all.get(&account.0).cloned().unwrap_or_default();
        let mut after = before.clone();
        change(&mut after);
        if after == before {
            return false;
        }
        all.insert(account.0.clone(), after);
        true
    })
}

/// Serializes changes: each reads, changes and saves the grants while it
/// holds this, so two at once (a log-out forgetting one account while the
/// user edits another) can't put back what the other changed, and the
/// file is written in the same order. Readers only wait for the cache.
static CHANGING: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// The grants file could not be written. The change was not made: the
/// cache keeps what is on disk, so what Settings shows is what a restart
/// reads back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveError(pub String);

/// Whether a change gives agents more access or takes it away.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Direction {
    /// The file is written before the cache takes the change, so a failed
    /// save leaves both as they were.
    Widen,
    /// The cache takes the change whatever the save says, so a revoke
    /// holds in this run even when the file cannot be written.
    Narrow,
}

/// Change every account's grants; `change` says whether it changed any.
fn change_all(
    direction: Direction,
    change: impl FnOnce(&mut BTreeMap<String, AccountGrants>) -> bool,
) -> Result<(), SaveError> {
    change_all_saving(direction, change, save_to_disk)
}

/// Writes the grants file, when this run persists anything.
fn save_to_disk(stored: &Stored) -> Result<(), String> {
    if !crate::fast_buttons::persistence_enabled() {
        return Ok(());
    }
    let Some(path) = path() else {
        return Ok(());
    };
    crate::settings::write_json_atomic(&path, stored).map_err(|err| err.to_string())
}

fn change_all_saving(
    direction: Direction,
    change: impl FnOnce(&mut BTreeMap<String, AccountGrants>) -> bool,
    save: impl FnOnce(&Stored) -> Result<(), String>,
) -> Result<(), SaveError> {
    let _changing = crate::locks::lock_or_recover(&CHANGING);
    let mut all = crate::locks::read_or_recover(&CACHE)
        .clone()
        .unwrap_or_else(load);
    if !change(&mut all) {
        return Ok(());
    }
    let saved = save(&Stored {
        accounts: all.clone(),
    });
    if let Err(err) = &saved {
        eprintln!("quill: could not save {GRANTS_FILE}: {err}");
        if direction == Direction::Widen {
            return Err(SaveError(err.clone()));
        }
    }
    GENERATION.fetch_add(1, Ordering::SeqCst);
    *crate::locks::write_or_recover(&CACHE) = Some(all);
    crate::wake::AGENT.poke();
    saved.map_err(SaveError)
}

/// `account` logged out or was removed: its grants go, so whoever signs in
/// next under the same key starts from the defaults. Not refused while the
/// UI driver injects input: it only ever narrows what agents may use. The
/// forgetting holds in memory even when the file cannot be written (the
/// failure is logged), so the next sign-in never inherits the grants.
pub fn forget_account(account: &AccountKey) {
    let _ = change_all(Direction::Narrow, |all| all.remove(&account.0).is_some());
}

#[cfg(test)]
mod tests {
    use super::{AccountGrants, ChatClass, ChatGrant, Scope};

    #[test]
    fn a_failed_save_leaves_the_cache_and_generation_as_they_were() {
        let _guard = crate::locks::lock_or_recover(&crate::agent::access::TEST_LOCK);
        let account = crate::ids::AccountKey("save-failure-test".into());
        let failed = super::change_all_saving(
            super::Direction::Widen,
            |all| {
                all.insert(account.0.clone(), AccountGrants::default());
                true
            },
            |_| Err("disk full".into()),
        );
        assert_eq!(failed, Err(super::SaveError("disk full".into())));
        let read = super::for_account(&account);
        assert_eq!(read, AccountGrants::default());
        assert!(
            crate::locks::read_or_recover(&super::CACHE)
                .as_ref()
                .is_none_or(|all| !all.contains_key(&account.0)),
            "the cache never took the change"
        );
        let saved = super::change_all_saving(
            super::Direction::Widen,
            |all| {
                all.insert(account.0.clone(), AccountGrants::default());
                true
            },
            |_| Ok(()),
        );
        assert_eq!(saved, Ok(()));
        super::forget_account(&account);
    }

    #[test]
    fn a_narrowing_that_fails_to_save_still_takes_hold_in_memory() {
        let _guard = crate::locks::lock_or_recover(&crate::agent::access::TEST_LOCK);
        let account = crate::ids::AccountKey("narrow-failure-test".into());
        let wide = AccountGrants {
            all_chats: Some(Scope::ReadWrite),
            ..AccountGrants::default()
        };
        let widened = super::change_all_saving(
            super::Direction::Widen,
            |all| {
                all.insert(account.0.clone(), wide.clone());
                true
            },
            |_| Ok(()),
        );
        assert_eq!(widened, Ok(()));
        let before = super::generation();
        let narrowed = super::change_all_saving(
            super::Direction::Narrow,
            |all| all.remove(&account.0).is_some(),
            |_| Err("disk full".into()),
        );
        assert_eq!(narrowed, Err(super::SaveError("disk full".into())));
        assert_eq!(
            super::for_account(&account),
            AccountGrants::default(),
            "the forgotten account keeps nothing, so a sign-in under the same key starts clean"
        );
        assert!(super::generation() > before, "waiting writes are refused");
    }

    #[test]
    fn defaults_are_narrow_and_entries_override() {
        let mut grants = AccountGrants::default();
        assert_eq!(
            grants.scope_for(1, ChatClass::Saved),
            Some(Scope::ReadWrite)
        );
        assert_eq!(grants.scope_for(2, ChatClass::Bot), Some(Scope::ReadWrite));
        assert_eq!(
            grants.scope_for(3, ChatClass::OwnChannel),
            Some(Scope::Read)
        );
        assert_eq!(grants.scope_for(4, ChatClass::Other), None);
        assert_eq!(grants.scope_for(5, ChatClass::Secret), None);

        grants.all_chats = Some(Scope::Read);
        assert_eq!(grants.scope_for(4, ChatClass::Other), Some(Scope::Read));
        assert_eq!(
            grants.scope_for(2, ChatClass::Bot),
            Some(Scope::ReadWrite),
            "the wider scope wins"
        );
        assert_eq!(
            grants.scope_for(5, ChatClass::Secret),
            None,
            "All chats skips secret chats"
        );

        grants.chats.insert(
            5,
            ChatGrant {
                scope: Some(Scope::Read),
                title: "s".into(),
            },
        );
        grants.chats.insert(
            4,
            ChatGrant {
                scope: None,
                title: "blocked".into(),
            },
        );
        assert_eq!(grants.scope_for(5, ChatClass::Secret), Some(Scope::Read));
        assert_eq!(
            grants.scope_for(4, ChatClass::Other),
            None,
            "a blocked chat stays blocked"
        );
    }

    #[test]
    fn scopes_parse_loosely_and_order() {
        assert_eq!(Scope::parse("read+write"), Some(Scope::ReadWrite));
        assert_eq!(Scope::parse("Read"), Some(Scope::Read));
        assert_eq!(Scope::parse("admin"), None);
        assert!(Scope::ReadWrite.allows(Scope::Read));
        assert!(!Scope::Read.allows(Scope::ReadWrite));
    }

    #[test]
    fn saved_grants_round_trip() {
        let mut grants = AccountGrants::default();
        grants.chats.insert(
            -100,
            ChatGrant {
                scope: None,
                title: "x".into(),
            },
        );
        let json = serde_json::to_string(&grants).unwrap_or_default();
        let back: AccountGrants = serde_json::from_str(&json).unwrap_or_default();
        assert_eq!(back, grants);
        let empty: AccountGrants = serde_json::from_str("{}").unwrap_or_default();
        assert_eq!(
            empty,
            AccountGrants::default(),
            "missing fields keep the defaults"
        );
    }
}
