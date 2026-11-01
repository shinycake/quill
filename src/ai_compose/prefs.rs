//! App-wide AI compose preferences, kept in `ai_compose_prefs.json` under
//! the app data root (isolated in tests and screenshot demos through
//! `safe_app_root`, like `file_prefs.json`).
//!
//! tdesktop keeps the same three values app-wide: the tooltips' "hidden"
//! prefs (`kAiComposeTooltipHiddenPref`, `kAiComposeStyleTooltipHiddenPref`
//! in compose_ai_box.cpp) and the style bound to the apply-in-place hotkey
//! (`ai-apply-tone-slug`, api_compose_with_ai.cpp).
use crate::locks::RwLockExt;
use serde::{Deserialize, Serialize};
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
    /// The style the apply-in-place shortcut rewrites with; `None` until the
    /// user binds one. Cleared when the server says the style is gone.
    pub bound_style_name: Option<String>,
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
        .unwrap_or_default()
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
            bound_style_name: Some("pirate".into()),
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
        update(|prefs| prefs.bound_style_name = Some("poet".into()));
        assert_eq!(current().bound_style_name.as_deref(), Some("poet"));
        update(|prefs| prefs.bound_style_name = None);
        assert_eq!(current().bound_style_name, None);
    }
}
