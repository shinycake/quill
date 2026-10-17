//! A staged update found at the next start (tdesktop `Sandbox` calls
//! `checkReadyUpdate()` before the app runs: a Ready update installs at the
//! next launch, whatever the "update automatically" switch says by then).
//!
//! Quill stages the download with the install plan beside the install
//! directory. On launch, once this process owns the single-instance lock and
//! before the UI or any account opens, a valid newer staged update is handed
//! to the existing helper and this process exits; the helper installs it and
//! starts the new version. Anything stale (not newer than the running
//! version, for another install, files gone) is deleted.
#![deny(clippy::indexing_slicing)]

use super::swap::{self, TEMP_PREFIX};
use super::{BUNDLE_UPDATES, InstallPlan, installable, read_plan, valid_release};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, PartialEq, Eq)]
enum Verdict {
    /// Verified, newer than the running version and for this install.
    Install,
    /// Stale or foreign: delete it.
    Discard,
}

fn verdict(plan: &InstallPlan, exe: &Path) -> Verdict {
    let present = fs::symlink_metadata(&plan.staged).is_ok();
    if plan.target == exe
        && plan.bundle == BUNDLE_UPDATES
        && present
        && valid_release(&plan.release)
    {
        Verdict::Install
    } else {
        Verdict::Discard
    }
}

fn plan_files(install: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(install) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            name.starts_with(TEMP_PREFIX) && name.ends_with(".json")
        })
        .map(|entry| entry.path())
        .collect()
}

fn discard(plan_path: &Path, plan: Option<&InstallPlan>) {
    if let Some(plan) = plan {
        let _ = fs::remove_file(&plan.staged);
        let _ = fs::remove_dir_all(&plan.staged);
    }
    let _ = fs::remove_file(plan_path);
}

/// The newest installable staged update in `install`; every other plan and
/// its files are deleted. Leftover temp entries (partial downloads, archives)
/// go too once nothing is kept.
fn pick(install: &Path, exe: &Path) -> Option<(PathBuf, InstallPlan)> {
    let mut best: Option<(PathBuf, InstallPlan, semver::Version)> = None;
    for path in plan_files(install) {
        let Ok(plan) = read_plan(&path) else {
            discard(&path, None);
            continue;
        };
        let version = semver::Version::parse(&plan.release.version).ok();
        match (verdict(&plan, exe), version) {
            (Verdict::Install, Some(version)) => match best.take() {
                Some((old_path, old_plan, old_version))
                    if version.cmp_precedence(&old_version).is_le() =>
                {
                    discard(&path, Some(&plan));
                    best = Some((old_path, old_plan, old_version));
                }
                Some((old_path, old_plan, _)) => {
                    discard(&old_path, Some(&old_plan));
                    best = Some((path, plan, version));
                }
                None => best = Some((path, plan, version)),
            },
            _ => discard(&path, Some(&plan)),
        }
    }
    if best.is_none() {
        swap::clear_temp_entries(install);
        if let Some(parent) = install.parent() {
            swap::clear_temp_entries(parent);
        }
    }
    best.map(|(path, plan, _)| (path, plan))
}

/// Rewrites the plan so the helper waits for `parent` (this process).
fn rearm(plan_path: &Path, mut plan: InstallPlan, parent: u32) -> std::io::Result<()> {
    plan.parent = parent;
    let tmp = plan_path.with_extension("arming");
    let _ = fs::remove_file(&tmp);
    super::write_private(&tmp, &serde_json::to_vec(&plan)?)?;
    fs::rename(&tmp, plan_path)
}

/// Called once at startup, after the single-instance lock and before the UI:
/// `true` means a staged update was handed to the helper and this process
/// must exit now (the helper installs it and starts the new version).
pub fn apply_on_start() -> bool {
    let Some(exe) = std::env::current_exe()
        .and_then(fs::canonicalize)
        .ok()
        .filter(|exe| installable(exe))
    else {
        return false;
    };
    let Some(install) = exe.parent() else {
        return false;
    };
    let Some((plan_path, plan)) = pick(install, &exe) else {
        return false;
    };
    rearm(&plan_path, plan, std::process::id())
        .and_then(|()| super::launch_helper(&plan_path))
        .is_ok()
}

#[cfg(test)]
mod tests {
    use super::{Verdict, pick, verdict};
    use crate::update_install::swap::TEMP_PREFIX;
    use crate::update_install::{BUNDLE_UPDATES, InstallPlan, read_plan};
    use crate::updater::{RELEASES_URL, ReleaseAsset, ReleaseInfo, update_asset_name};
    use std::fs;
    use std::path::{Path, PathBuf};

    fn release(version: &str) -> ReleaseInfo {
        ReleaseInfo {
            version: version.into(),
            notes: String::new(),
            url: format!("{RELEASES_URL}/tag/v{version}"),
            asset: Some(ReleaseAsset {
                url: format!("{RELEASES_URL}/download/v{version}/{}", update_asset_name()),
                size: 100,
                sha256: "a".repeat(64),
            }),
        }
    }

    fn stage(dir: &Path, exe: &Path, version: &str, name: &str) -> PathBuf {
        let staged = dir.join(format!("{TEMP_PREFIX}{name}.new"));
        fs::write(&staged, b"x").unwrap();
        let plan = InstallPlan {
            target: exe.to_path_buf(),
            staged,
            parent: 1,
            release: release(version),
            bundle: BUNDLE_UPDATES,
        };
        let path = dir.join(format!("{TEMP_PREFIX}{name}.json"));
        fs::write(&path, serde_json::to_vec(&plan).unwrap()).unwrap();
        path
    }

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("quill-staged-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_newer_staged_update_for_this_install_is_applied() {
        let dir = temp("newer");
        let exe = dir.join("quill");
        let plan = stage(&dir, &exe, "999.0.0", "a");
        let (path, found) = pick(&dir, &exe).unwrap();
        assert_eq!(path, plan);
        assert_eq!(found.release.version, "999.0.0");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_staged_update_that_is_not_newer_is_deleted() {
        let dir = temp("stale");
        let exe = dir.join("quill");
        let plan = stage(&dir, &exe, "0.0.1", "a");
        assert!(pick(&dir, &exe).is_none());
        assert!(!plan.exists());
        assert!(!dir.join(format!("{TEMP_PREFIX}a.new")).exists());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn an_update_staged_for_another_install_is_deleted() {
        let dir = temp("foreign");
        let exe = dir.join("quill");
        stage(&dir, &dir.join("elsewhere"), "999.0.0", "a");
        assert!(pick(&dir, &exe).is_none());
        assert!(!dir.join(format!("{TEMP_PREFIX}a.new")).exists());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn the_newest_of_several_staged_updates_wins_and_the_rest_are_deleted() {
        let dir = temp("several");
        let exe = dir.join("quill");
        stage(&dir, &exe, "998.0.0", "old");
        let newest = stage(&dir, &exe, "999.0.0", "new");
        let (path, _) = pick(&dir, &exe).unwrap();
        assert_eq!(path, newest);
        assert!(!dir.join(format!("{TEMP_PREFIX}old.json")).exists());
        assert!(!dir.join(format!("{TEMP_PREFIX}old.new")).exists());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_staged_update_whose_files_are_gone_is_not_installed() {
        let dir = temp("gone");
        let exe = dir.join("quill");
        let plan = stage(&dir, &exe, "999.0.0", "a");
        fs::remove_file(dir.join(format!("{TEMP_PREFIX}a.new"))).unwrap();
        let read = read_plan(&plan).unwrap();
        assert_eq!(verdict(&read, &exe), Verdict::Discard);
        fs::remove_dir_all(dir).unwrap();
    }
}
