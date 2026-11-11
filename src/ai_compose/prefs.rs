//! App-wide AI compose preferences, kept in `ai_compose_prefs.json` under
//! the app data root (isolated in tests and screenshot demos through
//! `safe_app_root`, like `file_prefs.json`).
//!
//! tdesktop keeps the same three values app-wide: the tooltips' "hidden"
//! prefs (`kAiComposeTooltipHiddenPref`, `kAiComposeStyleTooltipHiddenPref`
//! in compose_ai_box.cpp) and the style bound to the apply-in-place hotkey
//! (`ai-apply-tone-slug`, api_compose_with_ai.cpp).
use crate::ids::AccountKey;
use crate::locks::RwLockExt;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::RwLock;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AiComposePrefs {
    pub tooltip_hidden: bool,
    /// The Style tab's "pick a style" tooltip was dismissed (a style was
    /// chosen once).
    pub style_tooltip_hidden: bool,
    /// The style each account's apply-in-place shortcut rewrites with
    /// (custom styles belong to one account, so the binding does too).
    /// Cleared when the server says the style is gone.
    pub bound_style_names: BTreeMap<String, String>,
    /// The app-wide binding older builds wrote; moved to the primary
    /// account when the file loads.
    #[serde(skip_serializing)]
    bound_style_name: Option<String>,
}

impl AiComposePrefs {
    /// The style bound to `account`'s shortcut.
    pub fn bound_style(&self, account: &AccountKey) -> Option<&str> {
        self.bound_style_names.get(&account.0).map(String::as_str)
    }

    /// Bind (or with `None` unbind) `account`'s shortcut.
    pub fn bind_style(&mut self, account: &AccountKey, name: Option<&str>) {
        match name {
            Some(name) => {
                self.bound_style_names
                    .insert(account.0.clone(), name.to_string());
            }
            None => {
                self.bound_style_names.remove(&account.0);
            }
        }
    }
}

static PREFS: RwLock<Option<AiComposePrefs>> = RwLock::new(None);
static PERSIST: AtomicBool = AtomicBool::new(true);

/// Turn disk writes off (tests drive the in-memory value only).
pub fn set_persistence(enabled: bool) {
    PERSIST.store(enabled, Ordering::SeqCst);
}

fn prefs_path() -> Option<PathBuf> {
    crate::settings::safe_app_root().map(|root| root.join("ai_compose_prefs.json"))
}

fn load_from_disk() -> AiComposePrefs {
    prefs_path()
        .and_then(|path| crate::settings::read_with_pending(&path).ok())
        .and_then(|bytes| serde_json::from_slice::<AiComposePrefs>(&bytes).ok())
        .map(migrate_legacy_binding)
        .unwrap_or_default()
}

fn migrate_legacy_binding(mut prefs: AiComposePrefs) -> AiComposePrefs {
    if let Some(name) = prefs.bound_style_name.take() {
        prefs
            .bound_style_names
            .entry(AccountKey::primary().0)
            .or_insert(name);
    }
    prefs
}

/// The current preferences (loaded from disk on first use).
pub fn current() -> AiComposePrefs {
    if let Some(prefs) = PREFS.read_or_recover().as_ref() {
        return prefs.clone();
    }
    let loaded = load_from_disk();
    PREFS.write_or_recover().get_or_insert(loaded).clone()
}

/// Replace the preferences and persist them. A failed write keeps the
/// in-memory value for this run.
pub fn store(prefs: AiComposePrefs) {
    if PERSIST.load(Ordering::SeqCst)
        && let Some(path) = prefs_path()
    {
        crate::settings::write_json_deferred(&path, &prefs);
    }
    *PREFS.write_or_recover() = Some(prefs);
}

pub fn update(edit: impl FnOnce(&mut AiComposePrefs)) {
    let mut prefs = current();
    edit(&mut prefs);
    store(prefs);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_round_trips_and_tolerates_missing_fields() {
        let prefs = AiComposePrefs {
            tooltip_hidden: true,
            style_tooltip_hidden: false,
            bound_style_names: BTreeMap::from([("a".to_string(), "pirate".to_string())]),
            bound_style_name: None,
        };
        let text = serde_json::to_string(&prefs).unwrap();
        assert_eq!(
            serde_json::from_str::<AiComposePrefs>(&text).unwrap(),
            prefs
        );
        assert_eq!(
            serde_json::from_str::<AiComposePrefs>("{}").unwrap(),
            AiComposePrefs::default()
        );
    }

    #[test]
    fn update_keeps_the_value_in_memory() {
        set_persistence(false);
        let a = AccountKey("a".into());
        update(|prefs| prefs.bind_style(&a, Some("poet")));
        assert_eq!(current().bound_style(&a), Some("poet"));
        update(|prefs| prefs.bind_style(&a, None));
        assert_eq!(current().bound_style(&a), None);
    }

    #[test]
    fn a_binding_belongs_to_its_account() {
        let (a, b) = (AccountKey("a".into()), AccountKey("b".into()));
        let mut prefs = AiComposePrefs::default();
        prefs.bind_style(&a, Some("pirate"));
        assert_eq!(prefs.bound_style(&a), Some("pirate"));
        assert_eq!(prefs.bound_style(&b), None);
        prefs.bind_style(&b, None);
        assert_eq!(
            prefs.bound_style(&a),
            Some("pirate"),
            "b's clear leaves a's"
        );
    }

    #[test]
    fn the_old_app_wide_binding_moves_to_the_primary_account() {
        let old: AiComposePrefs = serde_json::from_str(r#"{"bound_style_name":"pirate"}"#).unwrap();
        let migrated = migrate_legacy_binding(old);
        assert_eq!(migrated.bound_style(&AccountKey::primary()), Some("pirate"));
        assert!(
            !serde_json::to_string(&migrated)
                .unwrap()
                .contains(r#""bound_style_name""#)
        );
    }
}
