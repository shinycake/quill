//! "Install updates automatically" (tdesktop Settings > Advanced,
//! `cAutoUpdate`, `lng_settings_update_automatically`).
//!
//! An installation setting, not an account one. tdesktop defaults it on;
//! Quill defaults it off, so nothing is downloaded until the user opts in.
//! With it on, a release found by the regular update check is downloaded,
//! verified and staged in the background and applied the next time Quill
//! starts. Quill never restarts itself.

use crate::updater::ReleaseInfo;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::RwLock;

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct Stored {
    install_automatically: bool,
}

static STORED: RwLock<Option<bool>> = RwLock::new(None);

fn path() -> Option<PathBuf> {
    crate::settings::safe_app_root().map(|root| root.join("auto_update.json"))
}

fn load_from(path: &Path) -> bool {
    std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Stored>(&bytes).ok())
        .unwrap_or_default()
        .install_automatically
}

fn save_to(path: &Path, on: bool) -> std::io::Result<()> {
    crate::settings::write_json_atomic(
        path,
        &Stored {
            install_automatically: on,
        },
    )
}

/// Whether new releases are staged without a click.
pub fn install_automatically() -> bool {
    if let Ok(guard) = STORED.read()
        && let Some(on) = *guard
    {
        return on;
    }
    let loaded = path().is_some_and(|path| load_from(&path));
    if let Ok(mut guard) = STORED.write() {
        return *guard.get_or_insert(loaded);
    }
    loaded
}

/// Remember the choice for the next launch.
pub fn set_install_automatically(on: bool) {
    if crate::fast_buttons::persistence_enabled()
        && let Some(path) = path()
    {
        let _ = save_to(&path, on);
    }
    if let Ok(mut guard) = STORED.write() {
        *guard = Some(on);
    }
}

/// What to do with a release the update check just found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// Download and stage it now; it is applied on the next start.
    Stage,
    /// Only show the update card; the user clicks install.
    ShowCard,
}

/// Stage only when the user opted in, this installation can replace itself
/// (`update_install::can_install_here`) and the release has a verified
/// binary for this platform.
pub fn decide(opted_in: bool, self_update_supported: bool, release: &ReleaseInfo) -> Decision {
    if opted_in && self_update_supported && release.asset.is_some() {
        Decision::Stage
    } else {
        Decision::ShowCard
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::updater::ReleaseAsset;

    fn release(with_asset: bool) -> ReleaseInfo {
        ReleaseInfo {
            version: "9.9.9".into(),
            notes: String::new(),
            url: "https://github.com/shinycake/quill/releases/tag/v9.9.9".into(),
            asset: with_asset.then(|| ReleaseAsset {
                url: "https://github.com/shinycake/quill/releases/download/v9.9.9/quill".into(),
                sha256: "0".repeat(64),
                size: 1,
            }),
        }
    }

    #[test]
    fn it_is_off_without_a_stored_file() {
        let stored: Stored = serde_json::from_str("{}").unwrap();
        assert!(!stored.install_automatically);
        let dir = std::env::temp_dir().join(format!("quill-auto-update-{}", std::process::id()));
        assert!(!load_from(&dir.join("missing.json")));
    }

    #[test]
    fn the_choice_survives_a_restart() {
        let dir = std::env::temp_dir().join(format!("quill-auto-update-rt-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("auto_update.json");
        save_to(&file, true).unwrap();
        assert!(load_from(&file));
        save_to(&file, false).unwrap();
        assert!(!load_from(&file));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn opted_in_with_a_newer_release_stages_it() {
        assert_eq!(decide(true, true, &release(true)), Decision::Stage);
    }

    #[test]
    fn off_only_shows_the_card() {
        assert_eq!(decide(false, true, &release(true)), Decision::ShowCard);
    }

    #[test]
    fn an_installation_that_cannot_update_itself_never_stages() {
        assert_eq!(decide(true, false, &release(true)), Decision::ShowCard);
    }

    #[test]
    fn a_release_without_a_binary_for_this_platform_never_stages() {
        assert_eq!(decide(true, true, &release(false)), Decision::ShowCard);
    }
}
