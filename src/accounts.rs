//! Multi-account, part 1: the account registry.
//!
//! `AccountKey` already namespaces every account's data on disk
//! (`<app_root>/accounts/<key>/`); what was missing is the list of *known*
//! accounts. This registry (`<app_root>/accounts.json`) records them plus
//! which one is current, so later slices can add accounts and switch between
//! them. No UI, no switching yet — parts 2 and 3.

use crate::ids::AccountKey;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// One known account identity. `phone`/`display_name` may be empty when the
/// account was registered before that metadata was available.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountEntry {
    pub key: AccountKey,
    pub phone: String,
    pub display_name: String,
}

impl AccountEntry {
    pub fn new(key: AccountKey, phone: impl Into<String>, display_name: impl Into<String>) -> Self {
        Self {
            key,
            phone: phone.into(),
            display_name: display_name.into(),
        }
    }
}

/// The known accounts and which one is current. Persisted as
/// `<app_root>/accounts.json` (next to, not inside, the per-account dirs).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountRegistry {
    pub accounts: Vec<AccountEntry>,
    pub current: AccountKey,
}

impl Default for AccountRegistry {
    fn default() -> Self {
        let primary = AccountKey::primary();
        Self {
            accounts: vec![AccountEntry::new(primary.clone(), "", "")],
            current: primary,
        }
    }
}

fn registry_path(app_root: &Path) -> std::path::PathBuf {
    app_root.join("accounts.json")
}

impl AccountRegistry {
    /// Load the registry; a missing or corrupt file yields the default
    /// (single primary account) — prefs must never block startup.
    /// An empty `accounts` list or a `current` that isn't listed is repaired
    /// to the default rather than trusted.
    pub fn load(app_root: &Path) -> Self {
        let path = registry_path(app_root);
        let parsed: Option<Self> = std::fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok());
        match parsed {
            Some(r) if !r.accounts.is_empty() && r.accounts.iter().any(|a| a.key == r.current) => r,
            _ => Self::default(),
        }
    }

    /// Persist the registry. Failures are returned to the caller.
    pub fn save(&self, app_root: &Path) -> std::io::Result<()> {
        let json = serde_json::to_vec_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        std::fs::write(registry_path(app_root), json)
    }

    /// Look up an account by key.
    pub fn get(&self, key: &AccountKey) -> Option<&AccountEntry> {
        self.accounts.iter().find(|a| a.key == *key)
    }

    /// Register a new account. Returns `false` (no change) when the key is
    /// already known.
    pub fn add(&mut self, entry: AccountEntry) -> bool {
        if self.get(&entry.key).is_some() {
            return false;
        }
        self.accounts.push(entry);
        true
    }

    /// Forget an account. Returns `false` (no change) when the key is unknown
    /// or is the current account — the active account cannot be removed.
    pub fn remove(&mut self, key: &AccountKey) -> bool {
        if *key == self.current {
            return false;
        }
        let before = self.accounts.len();
        self.accounts.retain(|a| a.key != *key);
        self.accounts.len() != before
    }

    /// Make a known account current. Returns `false` (no change) when the key
    /// is unknown.
    pub fn set_current(&mut self, key: &AccountKey) -> bool {
        if self.get(key).is_none() {
            return false;
        }
        self.current = key.clone();
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_root() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "quill-accounts-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn missing_file_loads_default_primary() {
        let dir = tmp_root();
        let reg = AccountRegistry::load(&dir);
        assert_eq!(reg, AccountRegistry::default());
        assert_eq!(reg.current, AccountKey::primary());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn corrupt_file_loads_default_primary() {
        let dir = tmp_root();
        std::fs::write(dir.join("accounts.json"), b"{nope").unwrap();
        assert_eq!(AccountRegistry::load(&dir), AccountRegistry::default());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn registry_roundtrips() {
        let dir = tmp_root();
        let mut reg = AccountRegistry::default();
        let second = AccountKey("second".to_string());
        assert!(reg.add(AccountEntry::new(second.clone(), "+1555", "Two")));
        assert!(reg.set_current(&second));
        reg.save(&dir).unwrap();
        assert_eq!(AccountRegistry::load(&dir), reg);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn add_duplicate_key_is_noop() {
        let mut reg = AccountRegistry::default();
        assert!(!reg.add(AccountEntry::new(AccountKey::primary(), "", "")));
        assert_eq!(reg.accounts.len(), 1);
    }

    #[test]
    fn remove_current_or_unknown_is_noop() {
        let mut reg = AccountRegistry::default();
        let second = AccountKey("second".to_string());
        reg.add(AccountEntry::new(second.clone(), "", ""));
        assert!(!reg.remove(&AccountKey::primary()));
        assert!(!reg.remove(&AccountKey("ghost".to_string())));
        assert!(reg.remove(&second));
        assert_eq!(reg.accounts.len(), 1);
    }

    #[test]
    fn set_current_unknown_is_noop() {
        let mut reg = AccountRegistry::default();
        assert!(!reg.set_current(&AccountKey("ghost".to_string())));
        assert_eq!(reg.current, AccountKey::primary());
    }
}
