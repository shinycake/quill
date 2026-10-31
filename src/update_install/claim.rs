//! One helper per staged update, and no launch loop.
//!
//! A ready plan is `<stamp>.json` beside the executable. Handing it to the
//! helper claims it first: the plan is written again as `<stamp>.claim`
//! (exclusive create, so of two launches only one wins) with this process as
//! the parent and the arguments to relaunch with, the claim is share-locked,
//! and the ready plan is deleted. The helper share-locks the claim as well,
//! while the launch that started it still holds its lock, and renames it to
//! `<stamp>.applying` (of two helpers only one wins the rename). The helper
//! deletes the claim, still locked, right before it starts Quill, on every
//! path, so the Quill it starts never finds it.
//!
//! A lock dies with its process. A start that finds a claim nobody has
//! locked knows its launch and its helper are gone: the claim is stale, and
//! the start discards it with a failed receipt and opens normally. The one
//! exception is a `.claim` younger than [`CLAIM_GRACE`], whose launch may
//! have exited a moment before its helper got to the lock.
//!
//! tdesktop: `checkReadyUpdate()` (core/update_checker.cpp) moves the ready
//! update into place for the updater before the app quits, and every
//! failure on the way clears it (`ClearAll()`), so a ready update is tried
//! once.
#![deny(clippy::indexing_slicing)]

use super::InstallPlan;
use super::swap::TEMP_PREFIX;
use crate::locks::LockExt;
use std::fs::{self, File, TryLockError};
use std::io::{self, Seek};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

const READY_EXT: &str = "json";
const CLAIM_EXT: &str = "claim";
const APPLYING_EXT: &str = "applying";

/// How long a `.claim` nobody has locked still counts as in hand-off: the
/// launch that wrote it has exited and its helper is still starting (a
/// virus scan of a freshly started executable can take seconds).
pub(super) const CLAIM_GRACE: Duration = Duration::from_secs(30);

/// Where the file system has no locks: how long any claim counts as live by
/// its age alone. The helper is done well within it (30 s waiting for the
/// app to exit, two 5 s probes, hashing and moving one package).
pub(super) const APPLY_LIMIT: Duration = Duration::from_secs(600);

/// An update file beside the executable, by name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Kind {
    /// `<stamp>.json`: staged and waiting.
    Ready,
    /// `<stamp>.claim`: a launch is handing it to the helper.
    Claimed,
    /// `<stamp>.applying`: a helper is installing it.
    Applying,
}

pub(super) fn kind_of(path: &Path) -> Option<Kind> {
    let name = path.file_name()?.to_str()?;
    if !name.starts_with(TEMP_PREFIX) {
        return None;
    }
    match path.extension()?.to_str()? {
        READY_EXT => Some(Kind::Ready),
        CLAIM_EXT => Some(Kind::Claimed),
        APPLYING_EXT => Some(Kind::Applying),
        _ => None,
    }
}

/// A claim this process has open (and, where the file system allows,
/// locked).
pub(super) struct Claim {
    path: PathBuf,
    file: File,
}

impl Claim {
    pub(super) fn path(&self) -> &Path {
        &self.path
    }

    /// The plan, read through this handle (on Windows a lock also keeps
    /// other handles from reading).
    pub(super) fn read(&mut self) -> io::Result<InstallPlan> {
        self.file.rewind()?;
        super::read_plan_from(&mut self.file)
    }

    /// Deletes the claim, and a ready plan of the same stamp, while the lock
    /// is still held, so no start ever sees it unlocked.
    pub(super) fn release(self) {
        let _ = fs::remove_file(&self.path);
        let _ = fs::remove_file(self.path.with_extension(READY_EXT));
    }

    /// Makes the plan ready again (the helper could not be started).
    fn restore(self, ready: &Path) {
        if fs::rename(&self.path, ready).is_err() {
            let _ = fs::remove_file(&self.path);
        }
    }
}

/// Claims mean "this process is handing the update off" until it exits.
static HELD: Mutex<Vec<Claim>> = Mutex::new(Vec::new());

fn hold(claim: Claim) {
    HELD.lock_or_recover().push(claim);
}

/// Lets go of a claim [`launch`] kept, as if this process had exited.
#[cfg(test)]
pub(super) fn drop_held(path: &Path) {
    HELD.lock_or_recover().retain(|claim| claim.path != path);
}

/// Claims the ready plan at `ready` for this process: `plan` is written as
/// `<stamp>.claim`, which fails if another launch claimed it first.
pub(super) fn claim(ready: &Path, plan: &InstallPlan) -> io::Result<Claim> {
    if kind_of(ready) != Some(Kind::Ready) {
        return Err(io::Error::other("Not a ready update plan"));
    }
    let path = ready.with_extension(CLAIM_EXT);
    let file = super::create_private(&path, &serde_json::to_vec(plan)?)?;
    // A file system without locks falls back on the claim's age.
    let _ = file.lock_shared();
    let _ = fs::remove_file(ready);
    Ok(Claim { path, file })
}

/// Hands the claim to the helper started by `launch`. On success the lock is
/// held until this process exits; on failure the plan is ready again.
pub(super) fn launch(
    claim: Claim,
    ready: &Path,
    launch: impl FnOnce(&Path) -> io::Result<()>,
) -> io::Result<()> {
    match launch(claim.path()) {
        Ok(()) => {
            hold(claim);
            Ok(())
        }
        Err(error) => {
            claim.restore(ready);
            Err(error)
        }
    }
}

/// Why the helper did not take a claim over.
pub(super) enum Refused {
    /// Gone, or another helper has it: nothing to do.
    Taken(io::Error),
    /// Taken over, but the plan cannot be read.
    Unreadable(Claim, io::Error),
    /// Not taken over for another reason (a file kept busy past the retries,
    /// say by a virus scanner). The claim, when it could be opened, is still
    /// at its old path; nobody else can have it, so this helper ends the
    /// attempt and starts Quill.
    Failed(Option<Claim>, io::Error),
}

/// The helper's side: lock the claim at `path` beside its launch's lock,
/// then rename it to `<stamp>.applying`, which only one helper can do.
pub(super) fn take_over(path: &Path) -> Result<(Claim, InstallPlan), Refused> {
    take_over_with(path, |from, to| fs::rename(from, to))
}

pub(super) fn take_over_with(
    path: &Path,
    rename: impl Fn(&Path, &Path) -> io::Result<()>,
) -> Result<(Claim, InstallPlan), Refused> {
    if kind_of(path) == Some(Kind::Applying) {
        return Err(Refused::Taken(io::Error::other(
            "This update is already being applied",
        )));
    }
    // Windows: a scanner or indexer can hold a new file for a moment.
    let file = match super::swap::retry(|| File::open(path)) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Err(Refused::Taken(error));
        }
        Err(error) => {
            // A claim no helper renamed is nobody's; a fresh one would make
            // the Quill started next wait for its 30 s grace.
            let _ = super::swap::retry(|| fs::remove_file(path));
            return Err(Refused::Failed(None, error));
        }
    };
    // Only a claim this user wrote (and nobody else can change) is applied.
    if !super::trusted_file(path) {
        return Err(Refused::Taken(io::Error::other(
            "Update claim is not this user's own",
        )));
    }
    let _ = file.lock_shared();
    let applying = path.with_extension(APPLYING_EXT);
    let claim = match super::swap::retry(|| rename(path, &applying)) {
        Ok(()) => Claim {
            path: applying,
            file,
        },
        // Another helper renamed it first, or a start discarded it.
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Err(Refused::Taken(error));
        }
        Err(error) => {
            return Err(Refused::Failed(
                Some(Claim {
                    path: path.to_path_buf(),
                    file,
                }),
                error,
            ));
        }
    };
    let mut claim = claim;
    match claim.read() {
        Ok(plan) => Ok((claim, plan)),
        Err(error) => Err(Refused::Unreadable(claim, error)),
    }
}

/// Whether the claim at `path` is still being worked on.
pub(super) enum Liveness {
    Live,
    /// Nobody holds it; this process does now, to discard it.
    Stale(Claim),
    /// Could not be opened (just deleted, or delete-pending on Windows).
    Unknown,
}

pub(super) fn liveness(path: &Path, kind: Kind, now: SystemTime) -> Liveness {
    let Ok(file) = File::open(path) else {
        return Liveness::Unknown;
    };
    // A modification time in the future (the clock went back) counts as old,
    // so a claim can never look live forever.
    let young = |limit: Duration| {
        file.metadata()
            .and_then(|meta| meta.modified())
            .ok()
            .and_then(|modified| now.duration_since(modified).ok())
            .is_some_and(|age| age < limit)
    };
    let live = match file.try_lock() {
        Err(TryLockError::WouldBlock) => true,
        Err(TryLockError::Error(_)) => young(APPLY_LIMIT),
        Ok(()) => kind == Kind::Claimed && young(CLAIM_GRACE),
    };
    if live {
        // Any lock this check took ends here, not when the file closes: a
        // child another thread is starting shares the open file, and with
        // it the lock, until its exec.
        let _ = file.unlock();
        Liveness::Live
    } else {
        Liveness::Stale(Claim {
            path: path.to_path_buf(),
            file,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::update_install::InstallPlan;
    use crate::updater::ReleaseInfo;

    fn plan() -> InstallPlan {
        InstallPlan {
            target: PathBuf::from("quill"),
            staged: PathBuf::from("staged"),
            parent: 1,
            release: ReleaseInfo {
                version: "999.0.0".into(),
                notes: String::new(),
                url: String::new(),
                asset: None,
            },
            bundle: false,
            args: Vec::new(),
            files: Vec::new(),
        }
    }

    fn ready(tag: &str) -> PathBuf {
        let dir = crate::scratch::process_dir(&format!("quill-claim-{tag}"));
        let path = dir.join(format!("{TEMP_PREFIX}1-999.0.0-{tag}.json"));
        fs::write(&path, serde_json::to_vec(&plan()).unwrap()).unwrap();
        path
    }

    /// Lets go of `claim` as the exit of its process does. Unlocked, not
    /// only closed: a child another test thread is starting shares the open
    /// file, and with it the lock, until its exec.
    fn let_go(claim: Claim) {
        claim.file.unlock().unwrap();
    }

    fn age(path: &Path, by: Duration) {
        let file = fs::File::options().write(true).open(path).unwrap();
        file.set_modified(SystemTime::now() - by).unwrap();
    }

    #[test]
    fn only_one_launch_can_claim_a_plan() {
        let ready = ready("once");
        let first = claim(&ready, &plan()).unwrap();
        assert!(!ready.exists());
        assert!(claim(&ready, &plan()).is_err());
        fs::write(&ready, b"{}").unwrap();
        assert!(claim(&ready, &plan()).is_err(), "the claim file exists");
        first.release();
        assert!(!ready.exists(), "release also drops the ready plan");
    }

    #[test]
    fn only_one_helper_can_take_a_claim_over() {
        let ready = ready("helpers");
        let held = claim(&ready, &plan()).unwrap();
        let claim_path = held.path().to_path_buf();
        let (taken, read) = take_over(&claim_path).ok().unwrap();
        assert_eq!(read.release.version, "999.0.0");
        assert_eq!(kind_of(taken.path()), Some(Kind::Applying));
        assert!(matches!(take_over(&claim_path), Err(Refused::Taken(_))));
        assert!(matches!(take_over(taken.path()), Err(Refused::Taken(_))));
        drop(held);
        taken.release();
    }

    #[test]
    fn a_held_claim_is_live_and_an_abandoned_one_is_stale() {
        let ready = ready("live");
        let held = claim(&ready, &plan()).unwrap();
        let path = held.path().to_path_buf();
        let now = SystemTime::now();
        assert!(matches!(
            liveness(&path, Kind::Claimed, now),
            Liveness::Live
        ));
        let_go(held);
        // Nobody holds it, but its launch may just have exited.
        assert!(matches!(
            liveness(&path, Kind::Claimed, now),
            Liveness::Live
        ));
        age(&path, CLAIM_GRACE + Duration::from_secs(1));
        let Liveness::Stale(mut stale) = liveness(&path, Kind::Claimed, now) else {
            panic!("an old claim nobody holds is stale");
        };
        assert_eq!(stale.read().unwrap().release.version, "999.0.0");
        stale.release();
        assert!(matches!(
            liveness(&path, Kind::Claimed, now),
            Liveness::Unknown
        ));
    }

    #[test]
    fn an_applying_plan_whose_helper_is_gone_is_stale_at_once() {
        let ready = ready("applying");
        let held = claim(&ready, &plan()).unwrap();
        let (taken, _) = take_over(held.path()).ok().unwrap();
        let_go(held);
        let path = taken.path().to_path_buf();
        assert!(matches!(
            liveness(&path, Kind::Applying, SystemTime::now()),
            Liveness::Live
        ));
        let_go(taken);
        let Liveness::Stale(stale) = liveness(&path, Kind::Applying, SystemTime::now()) else {
            panic!("a fresh .applying nobody holds is stale");
        };
        stale.release();
    }

    #[test]
    fn a_rename_that_keeps_failing_is_a_failure_and_not_another_helper() {
        let ready = ready("busy");
        let held = claim(&ready, &plan()).unwrap();
        let path = held.path().to_path_buf();
        let refused = take_over_with(&path, |_, _| {
            Err(io::Error::new(io::ErrorKind::PermissionDenied, "busy"))
        });
        let Err(Refused::Failed(Some(mut stuck), _)) = refused else {
            panic!("a busy claim is not someone else's");
        };
        assert_eq!(stuck.path(), path);
        assert_eq!(stuck.read().unwrap().release.version, "999.0.0");
        drop(held);
        stuck.release();
        assert!(!path.exists());
    }

    #[test]
    fn a_claim_whose_clock_went_back_is_not_live_forever() {
        let ready = ready("future");
        let held = claim(&ready, &plan()).unwrap();
        let path = held.path().to_path_buf();
        let_go(held);
        let past = SystemTime::now() - Duration::from_secs(3600);
        assert!(matches!(
            liveness(&path, Kind::Claimed, past),
            Liveness::Stale(_)
        ));
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn a_helper_that_cannot_start_leaves_the_plan_ready() {
        let ready = ready("restore");
        let held = claim(&ready, &plan()).unwrap();
        assert!(launch(held, &ready, |_| Err(io::Error::other("no helper"))).is_err());
        assert!(ready.exists());
        assert!(!ready.with_extension(CLAIM_EXT).exists());
    }
}
