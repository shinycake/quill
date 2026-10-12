//! "Install beta versions" (tdesktop Settings > Advanced, `cInstallBetaVersion`
//! and `Launcher::writeInstallBetaVersionsSetting`).
//!
//! The switch is an installation setting, not an account one, and it is off
//! by default. With it on, the update check also considers GitHub releases
//! flagged as pre-release (`updater::check_release`); with it off only the
//! latest stable release counts.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::RwLock;

pub const LABEL: &str = "Install beta versions";

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct Stored {
    install_beta: bool,
}

static STORED: RwLock<Option<bool>> = RwLock::new(None);

fn path() -> Option<PathBuf> {
    crate::settings::safe_app_root().map(|root| root.join("update_channel.json"))
}

/// Whether beta versions are wanted.
pub fn install_beta() -> bool {
    if let Ok(guard) = STORED.read()
        && let Some(on) = *guard
    {
        return on;
    }
    let loaded = path()
        .and_then(|path| std::fs::read(path).ok())
        .and_then(|bytes| serde_json::from_slice::<Stored>(&bytes).ok())
        .unwrap_or_default()
        .install_beta;
    if let Ok(mut guard) = STORED.write() {
        return *guard.get_or_insert(loaded);
    }
    loaded
}

/// Remember the choice for the next launch.
pub fn set_install_beta(on: bool) {
    if crate::fast_buttons::persistence_enabled()
        && let Some(path) = path()
    {
        let _ = crate::settings::write_json_atomic(&path, &Stored { install_beta: on });
    }
    if let Ok(mut guard) = STORED.write() {
        *guard = Some(on);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_stored_file_defaults_to_stable() {
        let stored: Stored = serde_json::from_str("{}").unwrap();
        assert!(!stored.install_beta);
        let stored: Stored = serde_json::from_str(r#"{"install_beta":true}"#).unwrap();
        assert!(stored.install_beta);
    }
}
