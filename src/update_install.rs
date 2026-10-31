//! Explicit updates: verify first, hand off after exit, retain rollback.
//!
//! Every platform installs the whole release package, so the bundled TDLib,
//! call, FFmpeg and OpenSSL libraries never go stale next to a new
//! executable. Linux stages the package beside the install directory and
//! swaps the directories; macOS does the same with the app bundle, after a
//! strict code-signature check (`macos.rs`); Windows renames each running
//! file aside and moves the new one in (`swap.rs`). Locations Quill cannot
//! rewrite without elevation or a package manager keep the "install the
//! release package from GitHub" path, each with its own reason.
#![deny(clippy::indexing_slicing)]
// The self-update handoff has per-platform halves; the others go unused.
#![cfg_attr(not(any(unix, windows)), allow(dead_code))]
// Downloading (and so unpacking) is part of the UI build only.
#![cfg_attr(not(feature = "ui"), allow(dead_code))]
// macOS unpacks with ditto (`macos::unpack`); the others with the in-process extractor.
#[cfg(not(target_os = "macos"))]
mod bundle;
mod claim;
#[cfg(any(feature = "ui", test))]
mod fetch;
mod layout;
#[cfg(target_os = "macos")]
mod macos;
mod manifest;
mod staged;
mod swap;

pub use staged::{apply_on_start, update_in_progress};

use crate::lang::{Key, keys};
use crate::updater::{ReleaseInfo, UpdateState};
// Staging (`ensure_private_dir`) is part of the UI build only.
#[cfg_attr(not(feature = "ui"), allow(unused_imports))]
use layout::{
    Layout, ensure_private_dir, owned_by_me, package_exe, plans_dir, receipt_for, result_path,
    staged_for, trusted_dir, trusted_file, update_dirs,
};
#[cfg(test)]
#[cfg(unix)]
use layout::{trusted_dir_as, trusted_file_as};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

#[derive(Serialize, Deserialize)]
pub struct InstallPlan {
    target: PathBuf,
    staged: PathBuf,
    parent: u32,
    release: ReleaseInfo,
    /// `staged` is an extracted package directory. Plans written before
    /// packages were the only update (a bare executable) say `false` and are
    /// discarded.
    #[serde(default)]
    bundle: bool,
    /// What the new version is started with (`start_files::relaunch_args`).
    #[serde(default)]
    args: Vec<String>,
    /// Every file of a staged package, checked again before it is installed.
    #[serde(default)]
    files: Vec<manifest::FileHash>,
}

/// Linux, macOS and Windows install the whole package; nothing else is
/// published.
#[cfg(not(target_os = "macos"))]
const BUNDLE_UPDATES: bool = cfg!(any(target_os = "linux", windows));
const PACKAGE_EXE: &str = if cfg!(windows) { "quill.exe" } else { "quill" };
#[cfg(not(target_os = "macos"))]
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

/// Why this copy cannot replace itself. Each reason has its own line in the
/// update section.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unavailable {
    /// Not a package Quill updates: a plain cargo build, a renamed
    /// executable, a platform without a release package.
    NotPackaged,
    /// macOS runs the app from a random read-only copy (App Translocation).
    Translocated,
    /// The app runs from a mounted disk image.
    DiskImage,
    /// The volume is read-only.
    ReadOnly,
    /// A package manager or sandbox owns the install (apt, Flatpak, Snap,
    /// AppImage, winget, Scoop, Homebrew).
    PackageManager,
    /// The folder cannot be written without elevation.
    NotWritable,
    /// Another user owns the install, or the update folder beside it, so
    /// this user could not clear a backup or trust the update files.
    OwnedByAnother,
    /// The startup thread has not answered yet; the switch waits.
    Checking,
}

impl Unavailable {
    pub fn hint(self) -> Key {
        match self {
            Self::NotPackaged => keys::Q_OPS_SELF_UPDATE_NOT_PACKAGED,
            Self::Translocated => keys::Q_OPS_SELF_UPDATE_TRANSLOCATED,
            Self::DiskImage => keys::Q_OPS_SELF_UPDATE_DISK_IMAGE,
            Self::ReadOnly => keys::Q_OPS_SELF_UPDATE_READ_ONLY,
            Self::PackageManager => keys::Q_OPS_SELF_UPDATE_PACKAGE_MANAGER,
            Self::NotWritable => keys::Q_OPS_SELF_UPDATE_NOT_WRITABLE,
            Self::OwnedByAnother => keys::Q_OPS_SELF_UPDATE_OWNED_BY_ANOTHER,
            Self::Checking => keys::Q_OPS_SELF_UPDATE_CHECKING,
        }
    }

    /// The `QUILL_DEMO_SELF_UPDATE` names screenshot demos use.
    #[cfg(any(test, feature = "demo-capture"))]
    pub fn from_demo_name(name: &str) -> Option<Self> {
        Some(match name {
            "0" | "not-packaged" => Self::NotPackaged,
            "translocated" => Self::Translocated,
            "disk-image" => Self::DiskImage,
            "read-only" => Self::ReadOnly,
            "package-manager" => Self::PackageManager,
            "not-writable" => Self::NotWritable,
            "owned-by-another" => Self::OwnedByAnother,
            "checking" => Self::Checking,
            _ => return None,
        })
    }
}

/// What a finished update leaves for the new version's first start: Windows
/// `*.old` files that could not go while the previous version ran, and the
/// Linux and macOS backup directory, kept until now so a failed start could
/// still be undone by hand. Runs on a background thread (the backup is a
/// whole package tree), which also answers [`can_install_here`] once, so no
/// render asks the disk first.
fn finish_previous_update() {
    let Some(exe) = std::env::current_exe()
        .ok()
        .and_then(|p| fs::canonicalize(p).ok())
    else {
        return;
    };
    let _ = crate::thread_guard::spawn_guarded("quill-update-cleanup", move || {
        // First, so the Settings rows stop saying "checking" at once; render
        // never computes it (`availability_now`).
        let _ = can_install_here();
        let Some(layout) = Layout::of(&exe) else {
            return;
        };
        if cfg!(windows) {
            swap::remove_leftovers(&layout.install);
        } else if let Some(backup) = layout.backup() {
            // This version started, so the previous one is no longer needed.
            let _ = swap::recover_or_clear_backup(&layout.install, &exe, &backup);
        }
    });
}

/// What the last update left for this start, shown once: the receipt is
/// removed as it is read, so a later start opens on `Idle` and the "Updated
/// to Quill X" banner does not come back.
pub fn startup_state() -> UpdateState {
    finish_previous_update();
    let result = result_path().and_then(|p| take_receipt(&p));
    match result {
        Some(r) if r.failed => UpdateState::InstallFailed(
            r.release,
            "Update installation failed. Your previous version was restored. Retry the update.",
        ),
        Some(r) if r.release.version == crate::version::APP => UpdateState::Installed(r.release),
        _ => UpdateState::Idle,
    }
}

fn take_receipt(path: &Path) -> Option<InstallResult> {
    if !trusted_file(path) {
        return None;
    }
    let bytes = fs::read(path).ok();
    let _ = fs::remove_file(path);
    bytes.and_then(|b| serde_json::from_slice(&b).ok())
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

/// Waits for `child` up to `limit`, then kills it. Output is read after this
/// returns, so a child that floods a pipe blocks until the deadline and dies.
fn wait_with_timeout(child: &mut Child, limit: Duration) -> std::io::Result<ExitStatus> {
    let deadline = Instant::now() + limit;
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(status);
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "Process did not finish in time",
            ));
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn probe_binary(path: &Path, version: &str) -> std::io::Result<()> {
    for (arg, expected) in [
        ("--version", format!("Quill {version}")),
        ("--build-info", "ui".to_string()),
    ] {
        let mut probe = Command::new(path)
            .arg(arg)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let status = wait_with_timeout(&mut probe, Duration::from_secs(5))
            .map_err(|_| std::io::Error::other("Updated binary did not answer version probe"))?;
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

/// Whether the install at `exe` can be rewritten by Quill itself, and why
/// not.
fn installable(exe: &Path) -> Result<(), Unavailable> {
    let layout = Layout::of(exe).ok_or(Unavailable::NotPackaged)?;
    #[cfg(target_os = "macos")]
    macos::check_location(exe, &layout.install)?;
    if swap::distro_managed(&layout.install) {
        return Err(Unavailable::PackageManager);
    }
    // Another user's install: its backup could never be cleared, and its
    // update files could not be trusted (`trusted_file`).
    if !owned_by_me(&layout.install)
        || (fs::symlink_metadata(&layout.plans).is_ok() && !trusted_dir(&layout.plans))
    {
        return Err(Unavailable::OwnedByAnother);
    }
    // Windows renames files inside the install directory; Linux also swaps
    // the directory itself, so its parent must be writable too; macOS swaps
    // the bundle (its folder must be writable) with the staged one in the
    // private update folder, which is created on first use.
    let stage_writable = if fs::symlink_metadata(&layout.stage_root).is_ok() {
        swap::dir_writable(&layout.stage_root)
    } else {
        layout.stage_root.parent().is_some_and(swap::dir_writable)
    };
    let writable = stage_writable
        && (layout.install == layout.stage_root || swap::dir_writable(&layout.install))
        && (!cfg!(target_os = "macos") || layout.install.parent().is_some_and(swap::dir_writable));
    if writable {
        Ok(())
    } else {
        Err(Unavailable::NotWritable)
    }
}

static AVAILABILITY: std::sync::OnceLock<Result<(), Unavailable>> = std::sync::OnceLock::new();

/// Whether this installation can replace itself without elevation, and why
/// not (app bundle in a read-only place, package manager, Program Files,
/// AppImage, a plain cargo build); anything else is pointed at the release
/// download instead. Probed once, on the startup thread
/// (`finish_previous_update`) or by the first caller off the render path.
fn availability() -> Result<(), Unavailable> {
    *AVAILABILITY.get_or_init(|| {
        let exe = std::env::current_exe()
            .and_then(fs::canonicalize)
            .map_err(|_| Unavailable::NotPackaged)?;
        installable(&exe)
    })
}

/// The probed answer, or `None` while the startup thread is still at it.
/// Render reads only this: it never touches the disk.
fn availability_now() -> Option<Result<(), Unavailable>> {
    AVAILABILITY.get().copied()
}

/// Whether a download may be staged here. Computes the answer when the
/// startup thread has not yet; never called from render.
pub fn can_install_here() -> bool {
    availability().is_ok()
}

/// Whether "Install updates automatically" can work here, or the reason it
/// cannot (the hint under the switch), `Unavailable::Checking` until the
/// startup thread has answered.
pub fn self_update_availability() -> Result<(), Unavailable> {
    #[cfg(any(test, feature = "demo-capture"))]
    if let Some(forced) = FORCED_AVAILABLE.lock().ok().and_then(|guard| *guard) {
        return forced;
    }
    availability_now().unwrap_or(Err(Unavailable::Checking))
}

pub fn self_update_available() -> bool {
    self_update_availability().is_ok()
}

/// Screenshot demos and UI tests pick the answer of
/// [`self_update_availability`].
#[cfg(any(test, feature = "demo-capture"))]
static FORCED_AVAILABLE: std::sync::Mutex<Option<Result<(), Unavailable>>> =
    std::sync::Mutex::new(None);

#[cfg(any(test, feature = "demo-capture"))]
pub fn force_self_update_available(answer: Option<Result<(), Unavailable>>) {
    if let Ok(mut guard) = FORCED_AVAILABLE.lock() {
        *guard = answer;
    }
}

/// Unpacks the downloaded `archive` so that `staged` is the package root.
fn unpack_package(archive: &Path, staged: &Path) -> std::io::Result<()> {
    #[cfg(target_os = "macos")]
    {
        macos::unpack(archive, staged)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let kind = if cfg!(windows) {
            bundle::ArchiveKind::Zip
        } else {
            bundle::ArchiveKind::TarGz
        };
        bundle::extract(archive, kind, staged)
    }
}

/// The checks a staged package passes before its plan is written, and again
/// in the helper before the swap: on macOS the strict code-signature chain
/// (`macos::codesign`), then the version probe of the package executable.
fn verify_staged(layout: &Layout, staged: &Path, release: &ReleaseInfo) -> Result<(), Refusal> {
    #[cfg(target_os = "macos")]
    match macos::codesign::verify(&layout.install, staged, &release.version) {
        Ok(()) => {}
        Err(macos::codesign::Rejection::Mismatch(detail)) => {
            eprintln!("quill: update refused: {detail}");
            return Err(Refusal::SignedDifferently);
        }
        Err(macos::codesign::Rejection::Invalid(detail)) => {
            eprintln!("quill: update refused: {detail}");
            return Err(Refusal::Invalid);
        }
    }
    #[cfg(not(target_os = "macos"))]
    let _ = layout;
    probe_binary(&package_exe(staged), &release.version).map_err(|_| Refusal::Probe)
}

/// Why a staged package was not accepted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Refusal {
    /// Signed by someone else than the running copy: this copy updates by
    /// hand, and the release is not downloaded again (`remember_refused`).
    SignedDifferently,
    /// A broken signature, another bundle identifier, another version
    /// (the signature check exists on macOS only).
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    Invalid,
    /// The executable did not answer the version probe.
    Probe,
}

impl Refusal {
    fn message(self) -> &'static str {
        match self {
            Self::SignedDifferently => crate::lang::tr(keys::Q_OPS_UPDATE_SIGNED_DIFFERENTLY),
            Self::Invalid => crate::lang::tr(keys::Q_OPS_UPDATE_FAILED_VERIFICATION),
            Self::Probe => "The downloaded binary failed its version check. Nothing was installed.",
        }
    }
}

impl From<Refusal> for std::io::Error {
    fn from(refusal: Refusal) -> Self {
        std::io::Error::other(refusal.message())
    }
}

/// Verification, then (macOS) the quarantine mark goes: only a bundle that
/// passed every check loses it, so a refused one stays quarantined.
fn accept_staged(layout: &Layout, staged: &Path, release: &ReleaseInfo) -> Result<(), Refusal> {
    verify_staged(layout, staged, release)?;
    #[cfg(target_os = "macos")]
    macos::remove_quarantine(staged).map_err(|_| Refusal::Invalid)?;
    Ok(())
}

#[derive(Serialize, Deserialize)]
struct Refused {
    version: String,
}

fn refused_path(layout: &Layout) -> PathBuf {
    layout.plans.join("refused-release.json")
}

/// A release refused as signed differently is remembered, so that neither
/// the launch check nor the button downloads it again.
fn remember_refused(layout: &Layout, version: &str) {
    let path = refused_path(layout);
    let _ = fs::remove_file(&path);
    if let Ok(bytes) = serde_json::to_vec(&Refused {
        version: version.to_string(),
    }) {
        let _ = write_private(&path, &bytes);
    }
}

fn refused_before(layout: &Layout, version: &str) -> bool {
    let path = refused_path(layout);
    trusted_file(&path)
        && fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Refused>(&bytes).ok())
            .is_some_and(|refused| refused.version == version)
}

/// Before staging a new package (the running install is intact): drops what
/// an interrupted or finished earlier update left, so none of it can block
/// this one or keep using disk: a stale backup directory, staged packages,
/// archives, plans and failed trees.
fn clear_interrupted_update(exe: &Path, layout: &Layout) {
    if !cfg!(windows)
        && let Some(backup) = layout.backup()
    {
        let _ = swap::recover_or_clear_backup(&layout.install, exe, &backup);
    }
    swap::clear_temp_entries(&layout.stage_root);
    if layout.plans != layout.stage_root {
        swap::clear_temp_entries(&layout.plans);
    }
}

#[cfg(feature = "ui")]
pub fn download_and_stage(
    release: &ReleaseInfo,
    progress: &crate::updater::DownloadProgress,
) -> Result<PathBuf, &'static str> {
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
    let layout = Layout::of(&target).ok_or("Invalid executable location.")?;
    if refused_before(&layout, &release.version) {
        return Err(Refusal::SignedDifferently.message());
    }
    // macOS: the private update folder, created closed to everyone else.
    ensure_private_dir(&layout.stage_root).map_err(
        |_| "Could not create the update folder beside Quill. Check folder permissions.",
    )?;
    clear_interrupted_update(&target, &layout);
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
    let staged = layout.stage_root.join(format!("{stamp}.new"));
    let archive = fetch::partial_path(&layout.stage_root, &asset.sha256);
    fetch::clear_other_partials(&layout.stage_root, &archive);
    let plan_path = layout.plans.join(format!("{stamp}.json"));
    let source = crate::updater::download_source(&asset.url);
    let result = (|| {
        let agent = fetch::agent(source.https_only, source.redirects);
        fetch::fetch(&agent, &source.url, &archive, asset.size, progress)?;
        if digest_file(&archive).map_err(|_| "Could not verify the update.")?
            != asset.sha256.to_ascii_lowercase()
        {
            let _ = fs::remove_file(&archive);
            return Err(fetch::MISMATCH);
        }
        let unpacked = unpack_package(&archive, &staged);
        let _ = fs::remove_file(&archive);
        unpacked
            .map_err(|_| "The downloaded package could not be unpacked. Nothing was installed.")?;
        accept_staged(&layout, &staged, release).map_err(|refusal| {
            if refusal == Refusal::SignedDifferently {
                remember_refused(&layout, &release.version);
            }
            refusal.message()
        })?;
        // The package may wait on disk until the next start; the helper
        // checks every file against this first.
        let files = manifest::of(&staged)
            .map_err(|_| "The downloaded package could not be unpacked. Nothing was installed.")?;
        let plan = InstallPlan {
            target,
            staged: staged.clone(),
            parent: std::process::id(),
            release: release.clone(),
            bundle: true,
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
        let _ = swap::remove_any(&staged);
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

/// How the helper starts Quill again: the executable at `target` with
/// `args`.
type Starter = fn(&Path, &[String]) -> std::io::Result<()>;

/// Starts the install `target` belongs to: the bundle through LaunchServices
/// on macOS, the executable itself elsewhere.
fn spawn_app(target: &Path, args: &[String]) -> std::io::Result<()> {
    #[cfg(target_os = "macos")]
    if let Some(bundle) = macos::bundle_of(target) {
        return macos::launch(&bundle, args);
    }
    Command::new(target)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
}

/// Installs a staged package: Linux and macOS swap the install directory
/// (the bundle), Windows renames each file aside. The receipt for the new
/// version travels inside the staged tree on Linux (the old directory is
/// deleted on success) and sits beside the bundle on macOS.
fn install_bundle(
    plan: &InstallPlan,
    target: &Path,
    layout: &Layout,
    launch: impl FnOnce() -> std::io::Result<()>,
) -> std::io::Result<()> {
    let name = plan
        .staged
        .file_name()
        .unwrap_or_default()
        .to_string_lossy();
    if plan.staged.parent() != Some(layout.stage_root.as_path())
        || !name.starts_with(swap::TEMP_PREFIX)
        || !name.ends_with(".new")
        || !fs::symlink_metadata(&plan.staged)?.file_type().is_dir()
    {
        return Err(std::io::Error::other("Invalid update handoff"));
    }
    manifest::verify(&plan.staged, &plan.files)?;
    verify_staged(layout, &plan.staged, &plan.release)?;
    if cfg!(windows) {
        write_result(receipt_for(target), &plan.release, false)?;
        return swap::swap_files(&layout.install, &plan.staged, launch);
    }
    if cfg!(target_os = "macos") {
        // Beside the bundle, like the plan: the swap carries nothing extra.
        write_result(receipt_for(target), &plan.release, false)?;
    } else {
        write_result(
            receipt_for(&package_exe(&plan.staged)),
            &plan.release,
            false,
        )?;
    }
    let backup = layout
        .backup()
        .ok_or_else(|| std::io::Error::other("Invalid install location"))?;
    // A backup the previous version never got to clear (it did not start, or
    // its startup cleanup failed) must not block this update.
    swap::recover_or_clear_backup(&layout.install, target, &backup)?;
    swap::swap_dir(&layout.install, &plan.staged, &backup, launch)
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
    apply_claimed(claim::take_over(plan_path), target, spawn_app)
}

/// [`apply_update`] once [`claim::take_over`] has answered.
fn apply_claimed(
    taken: Result<(claim::Claim, InstallPlan), claim::Refused>,
    target: PathBuf,
    start: Starter,
) -> std::io::Result<()> {
    match taken {
        Ok((claim, plan)) => apply_plan(claim, plan, target, start),
        // Another helper has it, or a start found it abandoned and opened
        // Quill itself: nothing is left to do here.
        Err(claim::Refused::Taken(error)) => Err(error),
        Err(claim::Refused::Unreadable(claim, error)) => {
            claim.release();
            let _ = start(&target, &[]);
            Err(error)
        }
        // This helper cannot install it, and nobody else will: the attempt
        // ends as a failure, like one inside `apply_plan`.
        Err(claim::Refused::Failed(claim, error)) => {
            let plan = claim.and_then(|mut claim| {
                let plan = claim.read().ok();
                claim.release();
                plan
            });
            if let Some(plan) = &plan {
                let _ = write_result(receipt_for(&target), &plan.release, true);
            }
            let _ = start(
                &target,
                plan.as_ref().map_or(&[][..], |plan| plan.args.as_slice()),
            );
            if let Some(plan) = plan
                && staged_for(&plan.staged, &target)
            {
                let _ = swap::remove_any(&plan.staged);
            }
            Err(error)
        }
    }
}

fn is_temp_entry(path: &Path) -> bool {
    path.file_name()
        .is_some_and(|name| name.to_string_lossy().starts_with(swap::TEMP_PREFIX))
}

/// [`apply_update`] for the executable at `target` (the helper itself).
fn apply_plan(
    claim: claim::Claim,
    plan: InstallPlan,
    target: PathBuf,
    start: Starter,
) -> std::io::Result<()> {
    let claim_dir = claim.path().parent().map(Path::to_path_buf);
    let mut claim = Some(claim);
    // The claim goes right before Quill starts, so that Quill never finds it.
    let mut relaunch = || {
        if let Some(claim) = claim.take() {
            claim.release();
        }
        start(&target, &plan.args)
    };
    let result = install(&plan, &target, claim_dir.as_deref(), &mut relaunch);
    if result.is_err() {
        // Written first: the restarted version reads it at once.
        let _ = write_result(receipt_for(&target), &plan.release, true);
        let _ = relaunch();
        if staged_for(&plan.staged, &target) {
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
    let layout =
        Layout::of(target).ok_or_else(|| std::io::Error::other("Invalid update handoff"))?;
    if plan.target != target
        || claim_dir != Some(layout.plans.as_path())
        || !plan.bundle
        || plan.parent == std::process::id()
        || plan.parent == 0
        || plan.staged == target
        || plan.staged == layout.install
    {
        return Err(std::io::Error::other("Invalid update handoff"));
    }
    wait_for_parent(plan.parent)?;
    if !valid_release(&plan.release) {
        return Err(std::io::Error::other("Invalid update release"));
    }
    install_bundle(plan, target, &layout, launch)
}

#[cfg(test)]
#[cfg(unix)]
mod tests;
