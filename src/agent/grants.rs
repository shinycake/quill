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
        let default = match class {
            ChatClass::Saved => return self.saved,
            ChatClass::Secret => return None,
            ChatClass::Bot => self.bots,
            ChatClass::OwnChannel => self.own_channels,
            ChatClass::Other => None,
        };
        default.max(self.all_chats)
    }

    /// The grants as agents read them (`quill_status`, `quill://grants`).
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
            "bots": word(self.bots),
            "own_channels": word(self.own_channels),
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
            format!("Bots: {}", word(self.bots)),
            format!("Channels you own: {}", word(self.own_channels)),
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

/// Change the grants of `account` and save them. Refused while the UI
/// driver injects an agent's input: an agent never widens its own grants.
pub fn update(account: &AccountKey, change: impl FnOnce(&mut AccountGrants)) {
    if super::ui_driver::guard::refuse_if_injecting("change which chats agents may use") {
        return;
    }
    GENERATION.fetch_add(1, Ordering::SeqCst);
    let mut all = match CACHE.read() {
        Ok(guard) => guard.clone().unwrap_or_else(load),
        Err(_) => load(),
    };
    change(all.entry(account.0.clone()).or_default());
    if crate::fast_buttons::persistence_enabled()
        && let Some(path) = path()
        && let Err(err) = crate::settings::write_json_atomic(
            &path,
            &Stored {
                accounts: all.clone(),
            },
        )
    {
        eprintln!("quill: could not save {GRANTS_FILE}: {err}");
    }
    if let Ok(mut guard) = CACHE.write() {
        *guard = Some(all);
    }
    crate::wake::AGENT.poke();
}

#[cfg(test)]
mod tests {
    use super::{AccountGrants, ChatClass, ChatGrant, Scope};

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
