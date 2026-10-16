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
mod swap;

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

pub fn startup_state() -> UpdateState {
    // Windows renames the previous files aside to `*.old` while they run;
    // this is the first start after that, so they can go now.
    if cfg!(windows)
        && let Some(dir) = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(Path::to_path_buf))
    {
        swap::remove_leftovers(&dir);
    }
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

fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(bytes)?;
    file.sync_all()
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

fn valid_release(release: &ReleaseInfo) -> bool {
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
    if !BUNDLE_UPDATES {
        // macOS: the bare executable only, never an app bundle.
        return cfg!(unix)
            && !exe
                .components()
                .any(|c| c.as_os_str().to_string_lossy().ends_with(".app"));
    }
    let Some(install) = exe.parent() else {
        return false;
    };
    let packaged = exe.file_name().is_some_and(|n| n == PACKAGE_EXE)
        && install.join(PACKAGE_SENTINEL).is_file();
    if !packaged || swap::distro_managed(install) {
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

#[cfg(feature = "ui")]
pub fn download_and_stage(release: &ReleaseInfo) -> Result<PathBuf, &'static str> {
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
    let stamp = format!(
        ".quill-update-{}-{}-{}",
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
        let plan = InstallPlan {
            target,
            staged: staged.clone(),
            parent: std::process::id(),
            release: release.clone(),
            bundle: BUNDLE_UPDATES,
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

pub fn launch_helper(plan: &Path) -> std::io::Result<()> {
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

/// Blocks until process `pid` has exited (30 s at most).
fn wait_for_parent(pid: u32) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        let deadline = Instant::now() + Duration::from_secs(30);
        while Command::new("/bin/kill")
            .args(["-0", &pid.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()?
            .success()
        {
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
        use windows_sys::Win32::Foundation::{CloseHandle, WAIT_OBJECT_0};
        use windows_sys::Win32::System::Threading::{
            OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject,
        };
        // SAFETY: OpenProcess takes plain integers and returns an owned handle
        // (null when the process is already gone); the handle is waited on and
        // closed exactly once below.
        let handle = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) };
        if handle.is_null() {
            return Ok(());
        }
        // SAFETY: `handle` is a valid process handle with SYNCHRONIZE access.
        let status = unsafe { WaitForSingleObject(handle, 30_000) };
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

fn spawn_app(path: &Path) -> std::io::Result<()> {
    Command::new(path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
}

/// Installs a staged package: Linux swaps the install directory, Windows
/// renames each file aside. The receipt for the new version travels inside the
/// staged tree on Linux (the old directory is deleted on success).
fn install_bundle(plan: &InstallPlan, target: &Path, plan_path: &Path) -> std::io::Result<()> {
    let install = target
        .parent()
        .ok_or_else(|| std::io::Error::other("Invalid executable location"))?;
    let name = plan
        .staged
        .file_name()
        .unwrap_or_default()
        .to_string_lossy();
    if staging_parent(install).as_deref() != plan.staged.parent()
        || !name.starts_with(".quill-update-")
        || !name.ends_with(".new")
        || !fs::symlink_metadata(&plan.staged)?.file_type().is_dir()
    {
        return Err(std::io::Error::other("Invalid update handoff"));
    }
    let staged_exe = plan.staged.join(PACKAGE_EXE);
    probe_binary(&staged_exe, &plan.release.version)?;
    // The plan lives inside the install directory; drop it before the swap.
    let _ = fs::remove_file(plan_path);
    let launch = || spawn_app(target);
    if cfg!(windows) {
        write_result(receipt_for(target), &plan.release, false)?;
        swap::swap_files(install, &plan.staged, launch)
    } else {
        write_result(receipt_for(&staged_exe), &plan.release, false)?;
        let backup = install.with_file_name(format!(
            ".{}.update-backup",
            install.file_name().unwrap_or_default().to_string_lossy()
        ));
        swap::swap_dir(install, &plan.staged, &backup, launch)
    }
}

/// Entry point runs before GPUI, credentials, or any account is opened.
pub fn apply_update(plan_path: &Path) -> std::io::Result<()> {
    let mut bytes = Vec::new();
    fs::File::open(plan_path)?
        .take(2_097_153)
        .read_to_end(&mut bytes)?;
    if bytes.len() > 2_097_152 {
        return Err(std::io::Error::other("Update plan too large"));
    }
    let plan: InstallPlan = serde_json::from_slice(&bytes)?;
    let target = fs::canonicalize(std::env::current_exe()?)?;
    if plan.target != target
        || plan_path.parent() != target.parent()
        || (!plan.bundle && plan.staged.parent() != target.parent())
        || (cfg!(windows) && !plan.bundle)
        || plan.parent == std::process::id()
        || plan.parent == 0
        || plan.staged == target
    {
        return Err(std::io::Error::other("Invalid update handoff"));
    }
    wait_for_parent(plan.parent)?;
    let result = (|| {
        if !valid_release(&plan.release) {
            return Err(std::io::Error::other("Invalid update release"));
        }
        if plan.bundle {
            return install_bundle(&plan, &target, plan_path);
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
        write_result(receipt_for(&target), &plan.release, false)?;
        replace_and_launch(&target, &plan.staged)
    })();
    if result.is_err() {
        let _ = write_result(receipt_for(&target), &plan.release, true);
        let _ = spawn_app(&target);
        if plan.bundle {
            let _ = fs::remove_dir_all(&plan.staged);
        }
    }
    let _ = fs::remove_file(plan_path);
    result
}

fn replace_and_launch(target: &Path, staged: &Path) -> std::io::Result<()> {
    let backup = target.with_extension("update-backup");
    if backup.exists() {
        return Err(std::io::Error::other("Previous update backup still exists"));
    }
    fs::rename(target, &backup)?;
    let result = fs::rename(staged, target).and_then(|_| {
        Command::new(target)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map(|_| ())
    });
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
        assert!(replace_and_launch(&target, &staged).is_err());
        assert_eq!(fs::read(&target).unwrap(), old);
        fs::write(&staged, b"#!/bin/sh\nexit 0\n").unwrap();
        fs::set_permissions(&staged, fs::Permissions::from_mode(0o700)).unwrap();
        replace_and_launch(&target, &staged).unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"#!/bin/sh\nexit 0\n");
        fs::remove_dir_all(root).unwrap();
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
