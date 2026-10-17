//! A staged update found at the next start (tdesktop `Sandbox` calls
//! `checkReadyUpdate()` before the app runs: a Ready update installs at the
//! next launch, whatever the "update automatically" switch says by then).
//!
//! Quill stages the download with the install plan beside the install
//! directory. On launch, once this process owns the single-instance lock and
//! before the UI or any account opens, a valid newer staged update is claimed
//! and handed to the existing helper, and this process exits; the helper
//! installs it and starts the new version with this launch's arguments.
//! Anything stale (not newer than the running version, for another install,
//! files gone, a claim whose helper died) is deleted.
//!
//! A start with nothing staged only lists the executable's directory.
#![deny(clippy::indexing_slicing)]

use super::claim::{self, Kind, Liveness};
use super::{BUNDLE_UPDATES, InstallPlan, installable, read_plan, valid_release};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

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
        && (!plan.bundle || !plan.files.is_empty())
        && present
        && valid_release(&plan.release)
    {
        Verdict::Install
    } else {
        Verdict::Discard
    }
}

/// The update files beside the executable.
fn update_files(install: &Path) -> Vec<(PathBuf, Kind)> {
    let Ok(entries) = fs::read_dir(install) else {
        return Vec::new();
    };
    entries
        .flatten()
        .map(|entry| entry.path())
        .filter_map(|path| claim::kind_of(&path).map(|kind| (path, kind)))
        .collect()
}

fn discard(plan_path: &Path, plan: Option<&InstallPlan>) {
    if let Some(plan) = plan {
        let _ = fs::remove_file(&plan.staged);
        let _ = fs::remove_dir_all(&plan.staged);
    }
    let _ = fs::remove_file(plan_path);
}

/// The newest installable staged update among `ready`; every other plan and
/// its files are deleted.
fn pick(ready: Vec<PathBuf>, exe: &Path) -> Option<(PathBuf, InstallPlan)> {
    let mut best: Option<(PathBuf, InstallPlan, semver::Version)> = None;
    for path in ready {
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
    best.map(|(path, plan, _)| (path, plan))
}

/// A claim whose launch and helper are gone: the attempt failed. It is
/// recorded like any failed install, so this start opens normally and says
/// so, and the update check does not stage it again on its own.
fn discard_stale(mut stale: claim::Claim, exe: &Path) {
    if let Ok(plan) = stale.read() {
        // The helper died after the swap: this start is the new version.
        let landed = plan.release.version == crate::version::APP;
        if plan.target == exe && !landed {
            let _ = super::write_result(super::receipt_for(exe), &plan.release, true);
        }
        discard(stale.path(), Some(&plan));
    }
    stale.release();
}

/// Whether any claim among `files` is live.
fn any_live(files: &[(PathBuf, Kind)], now: SystemTime) -> (bool, Vec<claim::Claim>) {
    let mut stale = Vec::new();
    for (path, kind) in files {
        if *kind == Kind::Ready {
            continue;
        }
        match claim::liveness(path, *kind, now) {
            Liveness::Live => return (true, Vec::new()),
            Liveness::Stale(claim) => stale.push(claim),
            Liveness::Unknown => {}
        }
    }
    (false, stale)
}

/// What a start should do about updates staged beside `exe`.
#[derive(Debug, PartialEq, Eq)]
enum Start {
    /// Open Quill.
    Run,
    /// Exit: a helper is installing an update and starts Quill when done.
    Exit,
}

fn on_start(
    exe: &Path,
    relaunch: &[String],
    can_install: impl FnOnce(&Path) -> bool,
    launch: impl FnOnce(&Path) -> std::io::Result<()>,
) -> Start {
    let Some(install) = exe.parent() else {
        return Start::Run;
    };
    let files = update_files(install);
    if files.is_empty() {
        return Start::Run;
    }
    let (live, stale) = any_live(&files, SystemTime::now());
    if live {
        return Start::Exit;
    }
    for claim in stale {
        discard_stale(claim, exe);
    }
    let ready = files
        .into_iter()
        .filter(|(path, kind)| *kind == Kind::Ready && path.exists())
        .map(|(path, _)| path)
        .collect::<Vec<_>>();
    if ready.is_empty() || !can_install(exe) {
        return Start::Run;
    }
    let Some((plan_path, mut plan)) = pick(ready, exe) else {
        return Start::Run;
    };
    plan.parent = std::process::id();
    plan.args = relaunch.to_vec();
    let handed = claim::claim(&plan_path, &plan)
        .and_then(|claimed| claim::launch(claimed, &plan_path, launch));
    if handed.is_ok() {
        Start::Exit
    } else {
        Start::Run
    }
}

fn current_exe() -> Option<PathBuf> {
    std::env::current_exe().and_then(fs::canonicalize).ok()
}

/// Called before the single-instance check: `true` while a helper installs
/// an update beside this executable. That helper starts Quill when it is
/// done, so this launch must not open it too (or hold the single-instance
/// lock while the new version starts).
pub fn update_in_progress() -> bool {
    current_exe()
        .and_then(|exe| exe.parent().map(update_files))
        .is_some_and(|files| any_live(&files, SystemTime::now()).0)
}

/// Called once at startup, after the single-instance lock and before the UI:
/// `true` means this process must exit now, because a staged update was
/// handed to the helper (it installs it and starts the new version with
/// `args`' links, files and flags) or one is being installed already.
pub fn apply_on_start(args: &[String]) -> bool {
    let Some(exe) = current_exe() else {
        return false;
    };
    let cwd = std::env::current_dir().unwrap_or_default();
    let relaunch = crate::start_files::relaunch_args(args.get(1..).unwrap_or_default(), &cwd);
    on_start(&exe, &relaunch, installable, super::launch_helper) == Start::Exit
}

#[cfg(test)]
mod tests {
    use super::{Start, Verdict, on_start, pick, verdict};
    use crate::update_install::swap::TEMP_PREFIX;
    use crate::update_install::{BUNDLE_UPDATES, InstallPlan, InstallResult, read_plan};
    use crate::updater::{RELEASES_URL, ReleaseAsset, ReleaseInfo, update_asset_name};
    use std::cell::Cell;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{Duration, SystemTime};

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
            staged: staged.clone(),
            parent: 1,
            release: release(version),
            bundle: BUNDLE_UPDATES,
            args: Vec::new(),
            files: crate::update_install::manifest::of(dir)
                .unwrap()
                .into_iter()
                .take(1)
                .collect(),
        };
        let path = dir.join(format!("{TEMP_PREFIX}{name}.json"));
        fs::write(&path, serde_json::to_vec(&plan).unwrap()).unwrap();
        path
    }

    fn ready(dir: &Path) -> Vec<PathBuf> {
        super::update_files(dir)
            .into_iter()
            .map(|(path, _)| path)
            .collect()
    }

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("quill-staged-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::canonicalize(dir).unwrap()
    }

    fn receipt(exe: &Path) -> Option<InstallResult> {
        let bytes = fs::read(crate::update_install::receipt_for(exe)).ok()?;
        serde_json::from_slice(&bytes).ok()
    }

    #[test]
    fn a_newer_staged_update_for_this_install_is_applied() {
        let dir = temp("newer");
        let exe = dir.join("quill");
        let plan = stage(&dir, &exe, "999.0.0", "a");
        let (path, found) = pick(ready(&dir), &exe).unwrap();
        assert_eq!(path, plan);
        assert_eq!(found.release.version, "999.0.0");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_staged_update_that_is_not_newer_is_deleted() {
        let dir = temp("stale");
        let exe = dir.join("quill");
        let plan = stage(&dir, &exe, "0.0.1", "a");
        assert!(pick(ready(&dir), &exe).is_none());
        assert!(!plan.exists());
        assert!(!dir.join(format!("{TEMP_PREFIX}a.new")).exists());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn an_update_staged_for_another_install_is_deleted() {
        let dir = temp("foreign");
        let exe = dir.join("quill");
        stage(&dir, &dir.join("elsewhere"), "999.0.0", "a");
        assert!(pick(ready(&dir), &exe).is_none());
        assert!(!dir.join(format!("{TEMP_PREFIX}a.new")).exists());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn the_newest_of_several_staged_updates_wins_and_the_rest_are_deleted() {
        let dir = temp("several");
        let exe = dir.join("quill");
        stage(&dir, &exe, "998.0.0", "old");
        let newest = stage(&dir, &exe, "999.0.0", "new");
        let (path, _) = pick(ready(&dir), &exe).unwrap();
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

    #[test]
    fn a_package_staged_without_a_manifest_is_not_installed() {
        let dir = temp("no-manifest");
        let exe = dir.join("quill");
        let plan = stage(&dir, &exe, "999.0.0", "a");
        let mut read = read_plan(&plan).unwrap();
        read.bundle = true;
        read.files.clear();
        assert_eq!(verdict(&read, &exe), Verdict::Discard);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_start_with_nothing_staged_touches_nothing() {
        let dir = temp("nothing");
        let exe = dir.join("quill");
        fs::write(dir.join(format!("{TEMP_PREFIX}9-1.0.0-1.archive")), b"x").unwrap();
        let start = on_start(
            &exe,
            &[],
            |_| panic!("no write probe without a staged update"),
            |_| panic!("no helper"),
        );
        assert_eq!(start, Start::Run);
        assert!(dir.join(format!("{TEMP_PREFIX}9-1.0.0-1.archive")).exists());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_start_hands_a_staged_update_to_one_helper_and_a_second_start_waits() {
        let dir = temp("hand-off");
        let exe = dir.join("quill");
        let plan_path = stage(&dir, &exe, "999.0.0", "a");
        let relaunch = vec!["--start-minimized".to_string()];
        let mut handed = None;
        let start = on_start(
            &exe,
            &relaunch,
            |_| true,
            |claim| {
                handed = Some(claim.to_path_buf());
                Ok(())
            },
        );
        assert_eq!(start, Start::Exit);
        let claim = handed.unwrap();
        assert!(!plan_path.exists(), "the ready plan is consumed");
        let claimed = read_plan(&claim).unwrap();
        assert_eq!(claimed.parent, std::process::id());
        assert_eq!(claimed.args, relaunch);

        // A second start while the first still holds the claim (its helper
        // is installing) starts no helper and does not open Quill.
        let helpers = Cell::new(0);
        let second = on_start(
            &exe,
            &[],
            |_| true,
            |_| {
                helpers.set(helpers.get() + 1);
                Ok(())
            },
        );
        assert_eq!(second, Start::Exit);
        assert_eq!(helpers.get(), 0);
        assert!(claim.exists());
        assert!(dir.join(format!("{TEMP_PREFIX}a.new")).exists());
        crate::update_install::claim::drop_held(&claim);
        fs::remove_dir_all(dir).unwrap();
    }

    /// A claim abandoned by a launch or helper that died, as the next start
    /// finds it.
    fn abandoned(tag: &str, kind_ext: &str, age: Duration) -> (PathBuf, PathBuf) {
        let dir = temp(tag);
        let exe = dir.join("quill");
        let plan = stage(&dir, &exe, "999.0.0", "a");
        let claim = plan.with_extension(kind_ext);
        fs::rename(&plan, &claim).unwrap();
        let file = fs::File::options().write(true).open(&claim).unwrap();
        file.set_modified(SystemTime::now() - age).unwrap();
        (dir, claim)
    }

    #[test]
    fn a_claim_whose_helper_died_is_discarded_and_quill_opens() {
        for (tag, ext, age) in [
            ("dead-claim", "claim", Duration::from_secs(120)),
            ("dead-helper", "applying", Duration::ZERO),
        ] {
            let (dir, claim) = abandoned(tag, ext, age);
            let exe = dir.join("quill");
            let start = on_start(
                &exe,
                &[],
                |_| true,
                |_| panic!("no helper for a dead claim"),
            );
            assert_eq!(start, Start::Run, "{tag}");
            assert!(!claim.exists(), "{tag}");
            assert!(!dir.join(format!("{TEMP_PREFIX}a.new")).exists(), "{tag}");
            let failed = receipt(&exe).unwrap();
            assert!(failed.failed, "{tag}: the failed attempt is reported");
            assert_eq!(failed.release.version, "999.0.0");
            // The next start opens normally too.
            assert_eq!(
                on_start(&exe, &[], |_| true, |_| panic!("nothing left")),
                Start::Run
            );
            fs::remove_dir_all(dir).unwrap();
        }
    }

    #[test]
    fn a_claim_just_handed_off_is_left_to_its_helper() {
        let (dir, claim) = abandoned("fresh-claim", "claim", Duration::ZERO);
        let exe = dir.join("quill");
        let start = on_start(&exe, &[], |_| true, |_| panic!("no second helper"));
        assert_eq!(start, Start::Exit);
        assert!(claim.exists());
        assert!(receipt(&exe).is_none());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_helper_that_cannot_start_lets_quill_open_and_keeps_the_update() {
        let dir = temp("no-helper");
        let exe = dir.join("quill");
        let plan_path = stage(&dir, &exe, "999.0.0", "a");
        let start = on_start(
            &exe,
            &[],
            |_| true,
            |_| Err(std::io::Error::other("spawn failed")),
        );
        assert_eq!(start, Start::Run);
        assert!(plan_path.exists());
        fs::remove_dir_all(dir).unwrap();
    }
}
