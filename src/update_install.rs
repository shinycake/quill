//! Explicit updates: verify first, hand off after exit, retain rollback.
//!
//! macOS (bare executable) swaps the one file. Linux and Windows install the
//! whole release package so the bundled TDLib, ntgcalls, FFmpeg and OpenSSL
//! libraries never go stale next to a new executable: Linux stages the package
//! beside the install directory and swaps the directories; Windows renames
//! each running file aside and moves the new one in (`swap.rs`). Locations
//! Quill cannot rewrite without elevation or a package manager keep the
//! "install the release package from GitHub" path.
#![deny(clippy::indexing_slicing)]
// The self-update handoff has per-platform halves; the others go unused.
#![cfg_attr(not(any(unix, windows)), allow(dead_code))]
// Downloading (and so unpacking) is part of the UI build only.
#![cfg_attr(not(feature = "ui"), allow(dead_code))]
mod bundle;
mod claim;
mod manifest;
mod staged;
mod swap;

pub use staged::{apply_on_start, update_in_progress};

use crate::updater::{ReleaseInfo, UpdateState};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[derive(Serialize, Deserialize)]
pub struct InstallPlan {
    target: PathBuf,
    staged: PathBuf,
    parent: u32,
    release: ReleaseInfo,
    /// `staged` is an extracted package directory, not a single executable.
    #[serde(default)]
    bundle: bool,
    /// What the new version is started with (`start_files::relaunch_args`).
    #[serde(default)]
    args: Vec<String>,
    /// Every file of a staged package, checked again before it is installed.
    #[serde(default)]
    files: Vec<manifest::FileHash>,
}

/// Linux and Windows install the whole package; other platforms swap the file.
const BUNDLE_UPDATES: bool = cfg!(any(target_os = "linux", windows));
const PACKAGE_EXE: &str = if cfg!(windows) { "quill.exe" } else { "quill" };
const PACKAGE_SENTINEL: &str = if cfg!(windows) {
    "tdjson.dll"
} else {
    "lib/libtdjson.so"
};

#[derive(Serialize, Deserialize)]
struct InstallResult {
    release: ReleaseInfo,
    failed: bool,
}

fn receipt_for(exe: &Path) -> PathBuf {
    exe.with_extension("last-update.json")
}

fn result_path() -> Option<PathBuf> {
    std::env::current_exe().ok().map(|p| receipt_for(&p))
}

/// The package executable at `exe` sits next to the package's TDLib.
fn packaged(exe: &Path) -> bool {
    exe.file_name().is_some_and(|n| n == PACKAGE_EXE)
        && exe
            .parent()
            .is_some_and(|install| install.join(PACKAGE_SENTINEL).is_file())
}

/// Where a Linux swap keeps the previous install directory:
/// `.quill.update-backup` beside it.
fn backup_for(install: &Path) -> Option<PathBuf> {
    let name = install.file_name()?.to_string_lossy();
    Some(install.with_file_name(format!(".{name}.update-backup")))
}

/// What a finished update leaves for the new version's first start: Windows
/// `*.old` files that could not go while the previous version ran, and the
/// Linux backup directory, kept until now so a failed start could still be
/// undone by hand. Runs on a background thread (the backup is a whole
/// package tree).
fn finish_previous_update() {
    let Some(exe) = std::env::current_exe()
        .ok()
        .and_then(|p| fs::canonicalize(p).ok())
    else {
        return;
    };
    let Some(install) = exe.parent().map(Path::to_path_buf) else {
        return;
    };
    let windows = cfg!(windows);
    if !windows && !(BUNDLE_UPDATES && packaged(&exe)) {
        return;
    }
    let _ = crate::thread_guard::spawn_guarded("quill-update-cleanup", move || {
        if windows {
            swap::remove_leftovers(&install);
        } else if let Some(backup) = backup_for(&install) {
            // This version started, so the previous one is no longer needed.
            let _ = swap::recover_or_clear_backup(&install, &exe, &backup);
        }
    });
}

pub fn startup_state() -> UpdateState {
    finish_previous_update();
    let result = result_path()
        .and_then(|p| fs::read(p).ok())
        .and_then(|b| serde_json::from_slice::<InstallResult>(&b).ok());
    match result {
        Some(r) if r.failed => UpdateState::InstallFailed(
            r.release,
            "Update installation failed. Your previous version was restored. Retry the update.",
        ),
        Some(r) if r.release.version == crate::version::APP => UpdateState::Installed(r.release),
        _ => UpdateState::Idle,
    }
}

pub fn acknowledge_changelog() -> std::io::Result<()> {
    if let Some(path) = result_path() {
        match fs::remove_file(path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            other => other?,
        }
    }
    Ok(())
}

/// A new file only this user can read, with `bytes` written and synced.
fn create_private(path: &Path, bytes: &[u8]) -> std::io::Result<fs::File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(file)
}

fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    create_private(path, bytes).map(drop)
}

fn write_result(path: PathBuf, release: &ReleaseInfo, failed: bool) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temp = path.with_extension(format!("{}.tmp", std::process::id()));
    write_private(
        &temp,
        &serde_json::to_vec(&InstallResult {
            release: release.clone(),
            failed,
        })?,
    )?;
    fs::rename(temp, path)
}

fn digest_file(path: &Path) -> std::io::Result<String> {
    let mut file = fs::File::open(path)?;
    let mut digest = Sha256::new();
    let mut buffer = [0; 32768];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        digest.update(buffer.get(..count).unwrap_or_default());
    }
    Ok(digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

pub(crate) fn valid_release(release: &ReleaseInfo) -> bool {
    let Some(tag) = release
        .url
        .strip_prefix(&format!("{}/tag/", crate::updater::RELEASES_URL))
    else {
        return false;
    };
    let (Ok(version), Ok(tag_version), Some(asset), Ok(current)) = (
        semver::Version::parse(&release.version),
        semver::Version::parse(tag.strip_prefix('v').unwrap_or(tag)),
        release.asset.as_ref(),
        semver::Version::parse(crate::version::APP),
    ) else {
        return false;
    };
    version == tag_version
        && version.pre.is_empty()
        && version.cmp_precedence(&current).is_gt()
        && crate::updater::valid_asset(asset, tag)
}

fn probe_binary(path: &Path, version: &str) -> std::io::Result<()> {
    for (arg, expected) in [
        ("--version", format!("Quill {version}")),
        ("--build-info", "ui".to_string()),
    ] {
        let mut probe = Command::new(path)
            .arg(arg)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let deadline = Instant::now() + Duration::from_secs(5);
        let status = loop {
            if let Some(status) = probe.try_wait()? {
                break status;
            }
            if Instant::now() >= deadline {
                let _ = probe.kill();
                let _ = probe.wait();
                return Err(std::io::Error::other(
                    "Updated binary did not answer version probe",
                ));
            }
            std::thread::sleep(Duration::from_millis(50));
        };
        let mut output = String::new();
        let Some(stdout) = probe.stdout.take() else {
            return Err(std::io::Error::other(
                "Updated binary version probe has no stdout",
            ));
        };
        stdout.take(1024).read_to_string(&mut output)?;
        if !status.success() || output.trim() != expected {
            return Err(std::io::Error::other(
                "Updated binary version does not match release",
            ));
        }
    }
    Ok(())
}

/// Whether the install at `exe` can be rewritten by Quill itself.
fn installable(exe: &Path) -> bool {
    let Some(install) = exe.parent() else {
        return false;
    };
    if !BUNDLE_UPDATES {
        // macOS: the bare executable only, never an app bundle, and never
        // a package-manager or read-only location.
        return cfg!(unix)
            && !exe
                .components()
                .any(|c| c.as_os_str().to_string_lossy().ends_with(".app"))
            && !swap::distro_managed(install)
            && swap::dir_writable(install);
    }
    if !packaged(exe) || swap::distro_managed(install) {
        return false;
    }
    // Windows renames files inside the install directory; Linux also swaps the
    // directory itself, so its parent must be writable too.
    swap::dir_writable(install)
        && (cfg!(windows) || install.parent().is_some_and(swap::dir_writable))
}

/// Only an installation Quill can replace itself, without elevation, qualifies;
/// anything else (app bundle, package manager, Program Files, AppImage, a plain
/// cargo build) is pointed at the release download instead. Probed once.
pub fn can_install_here() -> bool {
    static ANSWER: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ANSWER.get_or_init(|| {
        std::env::current_exe()
            .and_then(fs::canonicalize)
            .is_ok_and(|exe| installable(&exe))
    })
}

/// Whether "Install updates automatically" can work here: the install can
/// replace itself (`can_install_here`) and releases ship a package for it
/// (Linux and Windows; the macOS release is an app bundle, which Quill does
/// not replace yet).
pub fn self_update_available() -> bool {
    #[cfg(any(test, feature = "demo-capture"))]
    if let Some(forced) = FORCED_AVAILABLE.lock().ok().and_then(|guard| *guard) {
        return forced;
    }
    BUNDLE_UPDATES && can_install_here()
}

/// Screenshot demos and UI tests pick the answer of [`self_update_available`].
#[cfg(any(test, feature = "demo-capture"))]
static FORCED_AVAILABLE: std::sync::Mutex<Option<bool>> = std::sync::Mutex::new(None);

#[cfg(any(test, feature = "demo-capture"))]
pub fn force_self_update_available(answer: Option<bool>) {
    if let Ok(mut guard) = FORCED_AVAILABLE.lock() {
        *guard = answer;
    }
}

fn archive_kind() -> bundle::ArchiveKind {
    if cfg!(windows) {
        bundle::ArchiveKind::Zip
    } else {
        bundle::ArchiveKind::TarGz
    }
}

/// Where the extracted package waits: Linux swaps directories, so beside the
/// install directory; Windows moves files, so inside it (same volume).
fn staging_parent(install: &Path) -> Option<PathBuf> {
    if cfg!(windows) {
        Some(install.to_path_buf())
    } else {
        install.parent().map(Path::to_path_buf)
    }
}

/// Before staging a new package (the running install is intact): drops what
/// an interrupted or finished earlier update left, so none of it can block
/// this one or keep using disk. Linux: a stale backup directory; both
/// platforms: staged packages, archives, plans and failed trees.
fn clear_interrupted_update(exe: &Path, install: &Path, stage_root: &Path) {
    if !cfg!(windows)
        && let Some(backup) = backup_for(install)
    {
        let _ = swap::recover_or_clear_backup(install, exe, &backup);
    }
    swap::clear_temp_entries(stage_root);
    if stage_root != install {
        swap::clear_temp_entries(install);
    }
}

#[cfg(feature = "ui")]
pub fn download_and_stage(release: &ReleaseInfo) -> Result<PathBuf, &'static str> {
    // Screenshot demos and UI tests that force the answer never download.
    #[cfg(any(test, feature = "demo-capture"))]
    if FORCED_AVAILABLE
        .lock()
        .ok()
        .is_some_and(|guard| guard.is_some())
    {
        return Err("Download failed. Check your connection and retry.");
    }
    if !valid_release(release) {
        return Err(
            "The release has no valid newer binary for this platform. Retry the release check.",
        );
    }
    if !can_install_here() {
        return Err(
            "This installation cannot update itself. Install its release package from GitHub.",
        );
    }
    let asset = release
        .asset
        .as_ref()
        .ok_or("This release has no verified binary for your platform.")?;
    let target = std::env::current_exe()
        .and_then(fs::canonicalize)
        .map_err(|_| "Could not locate the installed executable.")?;
    let parent = target.parent().ok_or("Invalid executable location.")?;
    let stage_root = if BUNDLE_UPDATES {
        staging_parent(parent).ok_or("Invalid executable location.")?
    } else {
        parent.to_path_buf()
    };
    if BUNDLE_UPDATES {
        clear_interrupted_update(&target, parent, &stage_root);
    } else {
        swap::clear_temp_entries(&stage_root);
    }
    let stamp = format!(
        "{}{}-{}-{}",
        swap::TEMP_PREFIX,
        std::process::id(),
        release.version,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    );
    let staged = stage_root.join(format!("{stamp}.new"));
    let archive = stage_root.join(format!("{stamp}.archive"));
    let download = if BUNDLE_UPDATES { &archive } else { &staged };
    let plan_path = parent.join(format!("{stamp}.json"));
    let result = (|| {
        let agent = ureq::Agent::config_builder()
            .https_only(true)
            .max_redirects(3)
            .timeout_global(Some(Duration::from_secs(300)))
            .build()
            .new_agent();
        let mut response = agent
            .get(&asset.url)
            .header("User-Agent", "Quill-updater")
            .call()
            .map_err(|_| "Download failed. Check your connection and retry.")?;
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(if BUNDLE_UPDATES { 0o600 } else { 0o700 });
        }
        let mut file = options.open(download).map_err(
            |_| "Could not stage update beside the executable. Check folder permissions and retry.",
        )?;
        let mut reader = response.body_mut().as_reader().take(asset.size + 1);
        let size = std::io::copy(&mut reader, &mut file)
            .map_err(|_| "Download interrupted or disk full. Retry the update.")?;
        file.sync_all()
            .map_err(|_| "Could not save the update. Check disk space and retry.")?;
        if size != asset.size
            || digest_file(download).map_err(|_| "Could not verify the update.")?
                != asset.sha256.to_ascii_lowercase()
        {
            return Err(
                "Update checksum or size mismatch. Nothing was installed. Retry the release check.",
            );
        }
        let probed = if BUNDLE_UPDATES {
            bundle::extract(&archive, archive_kind(), &staged).map_err(
                |_| "The downloaded package could not be unpacked. Nothing was installed.",
            )?;
            let _ = fs::remove_file(&archive);
            staged.join(PACKAGE_EXE)
        } else {
            staged.clone()
        };
        probe_binary(&probed, &release.version).map_err(
            |_| "The downloaded binary failed its version check. Nothing was installed.",
        )?;
        // The package may wait on disk until the next start; the helper
        // checks every file against this first.
        let files = if BUNDLE_UPDATES {
            manifest::of(&staged).map_err(
                |_| "The downloaded package could not be unpacked. Nothing was installed.",
            )?
        } else {
            Vec::new()
        };
        let plan = InstallPlan {
            target,
            staged: staged.clone(),
            parent: std::process::id(),
            release: release.clone(),
            bundle: BUNDLE_UPDATES,
            args: Vec::new(),
            files,
        };
        write_private(
            &plan_path,
            &serde_json::to_vec(&plan).map_err(|_| "Could not prepare update handoff.")?,
        )
        .map_err(|_| "Could not save update handoff. Retry the update.")?;
        Ok(plan_path.clone())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&archive);
        let _ = fs::remove_file(&staged);
        let _ = fs::remove_dir_all(&staged);
        let _ = fs::remove_file(&plan_path);
    }
    result
}

/// Hands the staged update at `plan` (from [`download_and_stage`]) to the
/// helper, which installs it once this process has exited and starts the new
/// version. The plan is claimed first, so no other launch or helper can
/// apply it too. The caller quits next.
pub fn hand_off(plan: &Path) -> std::io::Result<()> {
    let mut ready = read_plan(plan)?;
    ready.parent = std::process::id();
    ready.args = Vec::new();
    claim::launch(claim::claim(plan, &ready)?, plan, launch_helper)
}

fn launch_helper(plan: &Path) -> std::io::Result<()> {
    let mut helper = Command::new(std::env::current_exe()?);
    helper
        .arg("--apply-update")
        .arg(plan)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        helper.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // DETACHED_PROCESS | CREATE_NO_WINDOW: outlive the exiting app, no console.
        helper.creation_flags(0x0000_0008 | 0x0800_0000);
    }
    helper.spawn().map(|_| ())
}

/// `ERROR_INVALID_PARAMETER`: OpenProcess says no process has that id, so
/// the parent is gone. Any other failure (access denied, an elevated or
/// other-session parent) leaves the parent running.
#[cfg(any(windows, test))]
fn open_failure_means_gone(code: u32) -> bool {
    code == 87
}

/// Whether process `pid` still exists. `EPERM` means it exists but belongs
/// to someone else.
#[cfg(unix)]
fn unix_process_alive(pid: u32) -> bool {
    let Ok(pid) = i32::try_from(pid) else {
        return false;
    };
    // SAFETY: signal 0 only checks that the process exists and can be
    // signalled; no signal is delivered.
    let result = unsafe { libc::kill(pid, 0) };
    result == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

/// Blocks until process `pid` has exited (30 s at most).
fn wait_for_parent(pid: u32) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        let deadline = Instant::now() + Duration::from_secs(30);
        while unix_process_alive(pid) {
            if Instant::now() >= deadline {
                return Err(std::io::Error::other(
                    "App did not exit; update was not installed",
                ));
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        Ok(())
    }
    #[cfg(windows)]
    {
        use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, WAIT_OBJECT_0};
        use windows_sys::Win32::System::Threading::{
            OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject,
        };
        let deadline = Instant::now() + Duration::from_secs(30);
        let handle = loop {
            // SAFETY: OpenProcess takes plain integers and returns an owned
            // handle, or null with the failure in the thread's last error.
            let handle = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) };
            if !handle.is_null() {
                break handle;
            }
            // SAFETY: reads the calling thread's last-error value.
            let code = unsafe { GetLastError() };
            if open_failure_means_gone(code) {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(std::io::Error::other(
                    "App did not exit; update was not installed",
                ));
            }
            std::thread::sleep(Duration::from_millis(200));
        };
        let remaining = deadline.saturating_duration_since(Instant::now());
        let millis = u32::try_from(remaining.as_millis()).unwrap_or(30_000);
        // SAFETY: `handle` is a valid process handle with SYNCHRONIZE access.
        let status = unsafe { WaitForSingleObject(handle, millis) };
        // SAFETY: closes the handle opened above, once.
        unsafe { CloseHandle(handle) };
        if status == WAIT_OBJECT_0 {
            Ok(())
        } else {
            Err(std::io::Error::other(
                "App did not exit; update was not installed",
            ))
        }
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = pid;
        Err(std::io::Error::other(
            "Updates are unavailable on this platform",
        ))
    }
}

fn spawn_app(path: &Path, args: &[String]) -> std::io::Result<()> {
    Command::new(path)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
}

/// Installs a staged package: Linux swaps the install directory, Windows
/// renames each file aside. The receipt for the new version travels inside the
/// staged tree on Linux (the old directory is deleted on success).
fn install_bundle(
    plan: &InstallPlan,
    target: &Path,
    launch: impl FnOnce() -> std::io::Result<()>,
) -> std::io::Result<()> {
    let install = target
        .parent()
        .ok_or_else(|| std::io::Error::other("Invalid executable location"))?;
    let name = plan
        .staged
        .file_name()
        .unwrap_or_default()
        .to_string_lossy();
    if staging_parent(install).as_deref() != plan.staged.parent()
        || !name.starts_with(swap::TEMP_PREFIX)
        || !name.ends_with(".new")
        || !fs::symlink_metadata(&plan.staged)?.file_type().is_dir()
    {
        return Err(std::io::Error::other("Invalid update handoff"));
    }
    manifest::verify(&plan.staged, &plan.files)?;
    let staged_exe = plan.staged.join(PACKAGE_EXE);
    probe_binary(&staged_exe, &plan.release.version)?;
    if cfg!(windows) {
        write_result(receipt_for(target), &plan.release, false)?;
        swap::swap_files(install, &plan.staged, launch)
    } else {
        write_result(receipt_for(&staged_exe), &plan.release, false)?;
        let backup =
            backup_for(install).ok_or_else(|| std::io::Error::other("Invalid install location"))?;
        // A backup the previous version never got to clear (it did not
        // start, or its startup cleanup failed) must not block this update.
        swap::recover_or_clear_backup(install, target, &backup)?;
        swap::swap_dir(install, &plan.staged, &backup, launch)
    }
}

fn read_plan_from(reader: &mut impl Read) -> std::io::Result<InstallPlan> {
    let mut bytes = Vec::new();
    Read::take(reader, 2_097_153).read_to_end(&mut bytes)?;
    if bytes.len() > 2_097_152 {
        return Err(std::io::Error::other("Update plan too large"));
    }
    Ok(serde_json::from_slice(&bytes)?)
}

fn read_plan(plan_path: &Path) -> std::io::Result<InstallPlan> {
    read_plan_from(&mut fs::File::open(plan_path)?)
}

/// Entry point runs before GPUI, credentials, or any account is opened.
///
/// Once this helper has taken the claim over, every way out deletes the
/// claim and starts Quill again (the new version, or the old one with a
/// failed receipt), so an update is tried once and never keeps Quill from
/// opening.
pub fn apply_update(plan_path: &Path) -> std::io::Result<()> {
    let exe = std::env::current_exe()?;
    let target = fs::canonicalize(&exe).unwrap_or(exe);
    match claim::take_over(plan_path) {
        Ok((claim, plan)) => apply_plan(claim, plan, target),
        // Another helper has it, or a start found it abandoned and opened
        // Quill itself: nothing is left to do here.
        Err(claim::Refused::Taken(error)) => Err(error),
        Err(claim::Refused::Unreadable(claim, error)) => {
            claim.release();
            let _ = spawn_app(&target, &[]);
            Err(error)
        }
    }
}

fn is_temp_entry(path: &Path) -> bool {
    path.file_name()
        .is_some_and(|name| name.to_string_lossy().starts_with(swap::TEMP_PREFIX))
}

/// [`apply_update`] for the executable at `target` (the helper itself).
fn apply_plan(claim: claim::Claim, plan: InstallPlan, target: PathBuf) -> std::io::Result<()> {
    let claim_dir = claim.path().parent().map(Path::to_path_buf);
    let mut claim = Some(claim);
    // The claim goes right before Quill starts, so that Quill never finds it.
    let mut relaunch = || {
        if let Some(claim) = claim.take() {
            claim.release();
        }
        spawn_app(&target, &plan.args)
    };
    let result = install(&plan, &target, claim_dir.as_deref(), &mut relaunch);
    if result.is_err() {
        // Written first: the restarted version reads it at once.
        let _ = write_result(receipt_for(&target), &plan.release, true);
        let _ = relaunch();
        if is_temp_entry(&plan.staged) {
            let _ = swap::remove_any(&plan.staged);
        }
    }
    drop(relaunch);
    if let Some(claim) = claim.take() {
        claim.release();
    }
    result
}

fn install(
    plan: &InstallPlan,
    target: &Path,
    claim_dir: Option<&Path>,
    launch: impl FnOnce() -> std::io::Result<()>,
) -> std::io::Result<()> {
    if plan.target != target
        || claim_dir != target.parent()
        || (!plan.bundle && plan.staged.parent() != target.parent())
        || (cfg!(windows) && !plan.bundle)
        || plan.parent == std::process::id()
        || plan.parent == 0
        || plan.staged == target
    {
        return Err(std::io::Error::other("Invalid update handoff"));
    }
    wait_for_parent(plan.parent)?;
    if !valid_release(&plan.release) {
        return Err(std::io::Error::other("Invalid update release"));
    }
    if plan.bundle {
        return install_bundle(plan, target, launch);
    }
    let asset = plan
        .release
        .asset
        .as_ref()
        .ok_or_else(|| std::io::Error::other("Missing update checksum"))?;
    if fs::symlink_metadata(&plan.staged)?.file_type().is_symlink()
        || fs::metadata(&plan.staged)?.len() != asset.size
        || digest_file(&plan.staged)? != asset.sha256
    {
        return Err(std::io::Error::other(
            "Staged update changed before installation",
        ));
    }
    probe_binary(&plan.staged, &plan.release.version)?;
    // The new process may read this immediately after spawn.
    write_result(receipt_for(target), &plan.release, false)?;
    replace_and_launch(target, &plan.staged, launch)
}

fn replace_and_launch(
    target: &Path,
    staged: &Path,
    launch: impl FnOnce() -> std::io::Result<()>,
) -> std::io::Result<()> {
    let backup = target.with_extension("update-backup");
    if backup.exists() {
        return Err(std::io::Error::other("Previous update backup still exists"));
    }
    fs::rename(target, &backup)?;
    let result = fs::rename(staged, target).and_then(|()| launch());
    if result.is_err() {
        let _ = fs::remove_file(target);
        fs::rename(&backup, target)?;
    } else {
        let _ = fs::remove_file(backup);
    }
    result
}

#[cfg(test)]
#[cfg(unix)]
mod tests {
    use super::*;

    #[test]
    fn only_invalid_parameter_means_the_parent_is_gone() {
        assert!(open_failure_means_gone(87));
        assert!(!open_failure_means_gone(5));
        assert!(!open_failure_means_gone(0));
    }

    #[cfg(unix)]
    #[test]
    fn unix_liveness_sees_self_and_not_a_missing_pid() {
        assert!(unix_process_alive(std::process::id()));
        assert!(!unix_process_alive(i32::MAX as u32));
        assert!(!unix_process_alive(u32::MAX));
    }

    #[test]
    fn digest_file_is_lowercase_hex_sha256() {
        let path = std::env::temp_dir().join(format!("quill-digest-{}", std::process::id()));
        fs::write(&path, b"abc").unwrap();
        let digest = digest_file(&path).unwrap();
        let _ = fs::remove_file(&path);
        assert_eq!(
            digest,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn update_verifies_version_and_rolls_back_when_launch_fails() {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!("quill-update-check-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let target = root.join("quill");
        let staged = root.join("new");
        let old = b"#!/bin/sh\necho old\n";
        fs::write(&target, old).unwrap();
        fs::set_permissions(&target, fs::Permissions::from_mode(0o700)).unwrap();
        fs::write(
            &staged,
            b"#!/bin/sh\nif [ \"$1\" = --build-info ]; then echo ui; else echo 'Quill 0.2.0'; fi\n",
        )
        .unwrap();
        fs::set_permissions(&staged, fs::Permissions::from_mode(0o700)).unwrap();
        assert!(probe_binary(&staged, "0.2.0").is_ok());
        assert!(probe_binary(&staged, "0.3.0").is_err());
        assert_eq!(digest_file(&staged).unwrap().len(), 64);
        fs::set_permissions(&staged, fs::Permissions::from_mode(0o600)).unwrap();
        assert!(replace_and_launch(&target, &staged, || spawn_app(&target, &[])).is_err());
        assert_eq!(fs::read(&target).unwrap(), old);
        fs::write(&staged, b"#!/bin/sh\nexit 0\n").unwrap();
        fs::set_permissions(&staged, fs::Permissions::from_mode(0o700)).unwrap();
        replace_and_launch(&target, &staged, || spawn_app(&target, &[])).unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"#!/bin/sh\nexit 0\n");
        fs::remove_dir_all(root).unwrap();
    }

    fn release() -> ReleaseInfo {
        let base = crate::updater::RELEASES_URL;
        ReleaseInfo {
            version: "999.0.0".into(),
            notes: String::new(),
            url: format!("{base}/tag/v999.0.0"),
            asset: Some(crate::updater::ReleaseAsset {
                url: format!(
                    "{base}/download/v999.0.0/{}",
                    crate::updater::update_asset_name()
                ),
                size: 100,
                sha256: "a".repeat(64),
            }),
        }
    }

    /// A pid that is certainly not running (spawned and reaped).
    fn exited_pid() -> u32 {
        let mut child = Command::new("/bin/sh")
            .args(["-c", "exit 0"])
            .spawn()
            .unwrap();
        let pid = child.id();
        child.wait().unwrap();
        pid
    }

    fn script(path: &Path, body: &str) {
        use std::os::unix::fs::PermissionsExt;
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, format!("#!/bin/sh\n{body}\n")).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }

    /// The new version: answers the probes, and records what it was started
    /// with in `started` beside it.
    const NEW_APP: &str = "case \"$1\" in --version) echo 'Quill 999.0.0';; --build-info) echo ui;; \
         *) echo \"$@\" > \"$(dirname \"$0\")/started\";; esac";

    /// The old version: records what it was started with, like `NEW_APP`.
    const OLD_APP: &str = "echo \"$@\" > \"$(dirname \"$0\")/started\"";

    struct Fixture {
        root: PathBuf,
        install: PathBuf,
        staged: PathBuf,
        plan_path: PathBuf,
        target: PathBuf,
    }

    impl Fixture {
        /// The plan as `download_and_stage` wrote it, taken over by this
        /// process as the helper.
        fn take(&self, parent: u32, args: &[&str]) -> (claim::Claim, InstallPlan) {
            let plan = InstallPlan {
                target: self.target.clone(),
                staged: self.staged.clone(),
                parent,
                release: release(),
                bundle: true,
                args: args.iter().map(|arg| arg.to_string()).collect(),
                files: manifest::of(&self.staged).unwrap(),
            };
            fs::write(&self.plan_path, serde_json::to_vec(&plan).unwrap()).unwrap();
            claim::take_over(&self.plan_path).ok().unwrap()
        }

        /// What the started version got, once it ran (spawned, so polled).
        fn started(&self) -> Option<String> {
            for _ in 0..100 {
                if let Ok(args) = fs::read_to_string(self.install.join("started")) {
                    return Some(args.trim().to_string());
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            None
        }

        /// No update file is left beside the executable.
        fn no_claim_left(&self) -> bool {
            fs::read_dir(&self.install)
                .unwrap()
                .flatten()
                .all(|entry| claim::kind_of(&entry.path()).is_none())
        }
    }

    /// An installed package `opt/quill` and a staged one beside it, as
    /// `download_and_stage` leaves them on Linux.
    fn bundle_fixture(tag: &str, old_app: &str, new_app: &str) -> Fixture {
        let root = std::env::temp_dir().join(format!("quill-apply-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("opt")).unwrap();
        let root = fs::canonicalize(root).unwrap();
        let install = root.join("opt/quill");
        script(&install.join("quill"), old_app);
        fs::create_dir_all(install.join("lib")).unwrap();
        fs::write(install.join("lib/libtdjson.so"), b"old-td").unwrap();
        let staged = root.join("opt/.quill-update-1-999.0.0-1.new");
        script(&staged.join("quill"), new_app);
        fs::create_dir_all(staged.join("lib")).unwrap();
        fs::write(staged.join("lib/libtdjson.so"), b"new-td").unwrap();
        let plan_path = install.join(".quill-update-1-999.0.0-1.json");
        let target = install.join("quill");
        Fixture {
            root,
            install,
            staged,
            plan_path,
            target,
        }
    }

    fn receipt(install: &Path) -> InstallResult {
        serde_json::from_slice(&fs::read(install.join("quill.last-update.json")).unwrap()).unwrap()
    }

    #[test]
    fn apply_plan_installs_a_bundle_and_keeps_the_old_tree_until_the_next_start() {
        let fx = bundle_fixture("bundle-ok", OLD_APP, NEW_APP);
        // A backup an earlier update never cleared must not block this one.
        let backup = fx.root.join("opt/.quill.update-backup");
        fs::create_dir_all(backup.join("stale")).unwrap();
        let (claim, plan) = fx.take(
            exited_pid(),
            &["--start-minimized", "--", "tg://resolve?domain=a"],
        );
        apply_plan(claim, plan, fx.target.clone()).unwrap();
        assert!(
            fs::read_to_string(&fx.target)
                .unwrap()
                .contains("Quill 999.0.0")
        );
        assert_eq!(
            fs::read(fx.install.join("lib/libtdjson.so")).unwrap(),
            b"new-td"
        );
        let result = receipt(&fx.install);
        assert!(!result.failed);
        assert_eq!(result.release.version, "999.0.0");
        assert!(!fx.staged.exists() && fx.no_claim_left());
        // The new version starts with the launch's flags and links.
        assert_eq!(
            fx.started().as_deref(),
            Some("--start-minimized -- tg://resolve?domain=a")
        );
        // The previous tree waits for the new version's first start.
        assert_eq!(
            fs::read(backup.join("lib/libtdjson.so")).unwrap(),
            b"old-td"
        );
        assert!(!backup.join("stale").exists());
        swap::recover_or_clear_backup(&fx.install, &fx.target, &backup).unwrap();
        assert!(!backup.exists());
        fs::remove_dir_all(fx.root).unwrap();
    }

    /// The old version is started again with the launch's arguments, the
    /// failure is on record for it, and nothing is left to try again.
    fn assert_failed_and_old_version_restarted(fx: &Fixture) {
        assert_eq!(
            fs::read_to_string(&fx.target).unwrap(),
            format!("#!/bin/sh\n{OLD_APP}\n")
        );
        assert_eq!(
            fs::read(fx.install.join("lib/libtdjson.so")).unwrap(),
            b"old-td"
        );
        assert!(receipt(&fx.install).failed);
        assert!(!fx.staged.exists() && fx.no_claim_left());
        assert!(!fx.root.join("opt/.quill.update-backup").exists());
        assert_eq!(fx.started().as_deref(), Some("--start-minimized"));
    }

    #[test]
    fn apply_plan_keeps_the_old_bundle_and_reports_when_the_new_one_fails_its_probe() {
        let fx = bundle_fixture("bundle-bad", OLD_APP, "echo 'Quill 0.0.1'");
        let (claim, plan) = fx.take(exited_pid(), &["--start-minimized"]);
        assert!(apply_plan(claim, plan, fx.target.clone()).is_err());
        assert_failed_and_old_version_restarted(&fx);
        fs::remove_dir_all(&fx.root).unwrap();
    }

    #[test]
    fn a_staged_package_with_a_missing_or_changed_file_is_not_installed() {
        for (tag, missing) in [("bundle-missing", true), ("bundle-corrupt", false)] {
            let fx = bundle_fixture(tag, OLD_APP, NEW_APP);
            let (claim, plan) = fx.take(exited_pid(), &["--start-minimized"]);
            let library = fx.staged.join("lib/libtdjson.so");
            if missing {
                fs::remove_file(&library).unwrap();
            } else {
                fs::write(&library, b"bad-td").unwrap();
            }
            assert!(apply_plan(claim, plan, fx.target.clone()).is_err(), "{tag}");
            assert_failed_and_old_version_restarted(&fx);
            fs::remove_dir_all(&fx.root).unwrap();
        }
    }

    #[test]
    fn a_handoff_refused_before_the_install_still_consumes_the_plan_and_restarts_quill() {
        // The plan names another executable: refused before any waiting.
        let fx = bundle_fixture("bundle-foreign", OLD_APP, NEW_APP);
        let (claim, mut plan) = fx.take(exited_pid(), &["--start-minimized"]);
        plan.target = fx.root.join("elsewhere/quill");
        assert!(apply_plan(claim, plan, fx.target.clone()).is_err());
        assert_failed_and_old_version_restarted(&fx);
        fs::remove_dir_all(&fx.root).unwrap();
    }

    #[test]
    fn a_second_helper_on_the_same_plan_does_nothing() {
        let fx = bundle_fixture("bundle-twice", OLD_APP, NEW_APP);
        let (claim, _plan) = fx.take(exited_pid(), &[]);
        // The ready plan is gone; the other helper's argument names it.
        assert!(matches!(
            claim::take_over(&fx.plan_path),
            Err(claim::Refused::Taken(_))
        ));
        assert!(fx.staged.exists());
        assert!(!fx.install.join("quill.last-update.json").exists());
        claim.release();
        fs::remove_dir_all(&fx.root).unwrap();
    }

    #[test]
    fn staging_first_clears_what_an_interrupted_update_left() {
        let fx = bundle_fixture("bundle-clear", OLD_APP, NEW_APP);
        fs::write(&fx.plan_path, b"{}").unwrap();
        let backup = fx.root.join("opt/.quill.update-backup");
        fs::create_dir_all(&backup).unwrap();
        fs::write(fx.root.join("opt/.quill-update-2-1.0.0-3.archive"), b"x").unwrap();
        clear_interrupted_update(&fx.target, &fx.install, &fx.root.join("opt"));
        assert!(!backup.exists() && !fx.staged.exists() && !fx.plan_path.exists());
        assert!(!fx.root.join("opt/.quill-update-2-1.0.0-3.archive").exists());
        assert!(fx.target.exists() && fx.install.join("lib/libtdjson.so").exists());
        fs::remove_dir_all(fx.root).unwrap();
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn only_a_writable_unmanaged_package_directory_can_self_update() {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!("quill-installable-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let install = root.join("opt/quill");
        fs::create_dir_all(install.join("lib")).unwrap();
        let exe = install.join("quill");
        fs::write(&exe, b"x").unwrap();
        // A bare binary without the package's library directory (cargo build).
        assert!(!installable(&exe));
        fs::write(install.join("lib/libtdjson.so"), b"td").unwrap();
        assert!(installable(&exe));
        // A renamed executable is not our package layout.
        let renamed = install.join("quill-dev");
        fs::write(&renamed, b"x").unwrap();
        assert!(!installable(&renamed));
        // Read-only install directory: fall back to the download page. (Root
        // can write anywhere, so only assert when the probe agrees.)
        fs::set_permissions(&install, fs::Permissions::from_mode(0o555)).unwrap();
        if !swap::dir_writable(&install) {
            assert!(!installable(&exe));
        }
        fs::set_permissions(&install, fs::Permissions::from_mode(0o755)).unwrap();
        fs::remove_dir_all(root).unwrap();
    }
}
