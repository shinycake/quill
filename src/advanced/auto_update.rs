//! "Install updates automatically" (tdesktop Settings > Advanced,
//! `cAutoUpdate`, `lng_settings_update_automatically`).
//!
//! An installation setting, not an account one. tdesktop defaults it on;
//! Quill defaults it off, so nothing is downloaded until the user opts in.
//! With it on, a release found by the regular update check is downloaded,
//! verified and staged in the background and applied the next time Quill
//! starts. Quill never restarts itself.

use crate::lang::{Key, keys};
use crate::updater::{ReleaseInfo, UpdateState};
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
    /// Download and stage it now; it is applied at the next start.
    Stage,
    /// Only show the update card; the user clicks install.
    ShowCard,
}

/// Stage only when the user opted in, this installation can replace itself
/// (`update_install::self_update_available`) and the release passes the same
/// check as the download (`update_install::valid_release`).
pub fn decide(opted_in: bool, self_update_supported: bool, release: &ReleaseInfo) -> Decision {
    if opted_in && self_update_supported && crate::update_install::valid_release(release) {
        Decision::Stage
    } else {
        Decision::ShowCard
    }
}

/// The release to stage after an update check, if any: the check found one
/// (`Available`), "Check for updates on launch" is on (the switch is only
/// shown then), and [`decide`] says stage.
pub fn staging_candidate(
    state: &UpdateState,
    opted_in: bool,
    launch_check_on: bool,
    self_update_supported: bool,
) -> Option<ReleaseInfo> {
    let UpdateState::Available(release) = state else {
        return None;
    };
    (launch_check_on && decide(opted_in, self_update_supported, release) == Decision::Stage)
        .then(|| release.clone())
}

/// How the switch row looks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RowState {
    pub checked: bool,
    pub enabled: bool,
    pub hint: Key,
}

/// The row for a stored choice on an install that can or cannot update
/// itself. Where it cannot, the switch is off and disabled, and the hint says
/// why (macOS: the app updates through the release package).
pub fn row_state(stored: bool, self_update_supported: bool) -> RowState {
    if self_update_supported {
        RowState {
            checked: stored,
            enabled: true,
            hint: keys::Q_OPS_INSTALL_UPDATES_AUTOMATICALLY_HINT,
        }
    } else {
        RowState {
            checked: false,
            enabled: false,
            hint: if cfg!(target_os = "macos") {
                keys::Q_OPS_INSTALL_UPDATES_AUTOMATICALLY_MACOS
            } else {
                keys::Q_OPS_INSTALL_UPDATES_AUTOMATICALLY_UNAVAILABLE
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::updater::{RELEASES_URL, ReleaseAsset, update_asset_name};

    fn release(with_asset: bool) -> ReleaseInfo {
        ReleaseInfo {
            version: "999.0.0".into(),
            notes: String::new(),
            url: format!("{RELEASES_URL}/tag/v999.0.0"),
            asset: with_asset.then(|| ReleaseAsset {
                url: format!("{RELEASES_URL}/download/v999.0.0/{}", update_asset_name()),
                sha256: "a".repeat(64),
                size: 100,
            }),
        }
    }

    #[test]
    fn it_is_off_by_default() {
        assert!(!Stored::default().install_automatically);
        let dir = std::env::temp_dir().join(format!("quill-auto-update-{}", std::process::id()));
        assert!(!load_from(&dir.join("missing.json")));
        let stored: Stored = serde_json::from_str("{}").unwrap();
        assert!(!stored.install_automatically);
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
    fn a_release_the_download_would_refuse_never_stages() {
        assert_eq!(decide(true, true, &release(false)), Decision::ShowCard);
        let mut old = release(true);
        old.version = "0.0.1".into();
        old.url = format!("{RELEASES_URL}/tag/v0.0.1");
        assert_eq!(decide(true, true, &old), Decision::ShowCard);
    }

    #[test]
    fn a_check_result_is_staged_only_when_every_condition_holds() {
        let found = UpdateState::Available(release(true));
        assert_eq!(
            staging_candidate(&found, true, true, true),
            Some(release(true))
        );
        assert_eq!(staging_candidate(&found, false, true, true), None);
        assert_eq!(staging_candidate(&found, true, false, true), None);
        assert_eq!(staging_candidate(&found, true, true, false), None);
        for other in [
            UpdateState::UpToDate,
            UpdateState::Checking,
            UpdateState::Staged(release(true), "plan.json".into()),
            UpdateState::Downloading(release(true)),
            UpdateState::Failed("x"),
        ] {
            assert_eq!(staging_candidate(&other, true, true, true), None);
        }
    }

    #[test]
    fn the_row_is_disabled_and_off_where_the_install_cannot_update_itself() {
        let row = row_state(true, false);
        assert!(!row.enabled && !row.checked);
        let row = row_state(true, true);
        assert!(row.enabled && row.checked);
        assert_ne!(row_state(false, true).hint, row_state(false, false).hint);
    }
}
