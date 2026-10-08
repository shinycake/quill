//! Explicit binary updates: verify first, hand off after exit, retain rollback.
// The self-update handoff is Unix-only for now (Windows needs an installer
// handoff — see the cross-platform backlog), so its helpers go unused there.
#![cfg_attr(not(unix), allow(dead_code))]
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
}

#[derive(Serialize, Deserialize)]
struct InstallResult {
    release: ReleaseInfo,
    failed: bool,
}

fn result_path() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .map(|p| p.with_extension("last-update.json"))
}

pub fn startup_state() -> UpdateState {
    let result = result_path()
        .and_then(|p| fs::read(p).ok())
        .and_then(|b| serde_json::from_slice::<InstallResult>(&b).ok());
    match result {
        Some(r) if r.failed => UpdateState::InstallFailed(
            r.release,
            "Update installation failed. Your previous version was restored. Retry the update.",
        ),
        Some(r) if r.release.version == env!("CARGO_PKG_VERSION") => {
            UpdateState::Installed(r.release)
        }
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

fn write_result(release: &ReleaseInfo, failed: bool) -> std::io::Result<()> {
    let path =
        result_path().ok_or_else(|| std::io::Error::other("No application data directory"))?;
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
        digest.update(&buffer[..count]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

fn valid_release(release: &ReleaseInfo) -> bool {
    let Some(tag) = release
        .url
        .strip_prefix(&format!("{}/tag/", crate::updater::RELEASES_URL))
    else {
        return false;
    };
    let (Ok(version), Ok(tag_version), Some(asset)) = (
        semver::Version::parse(&release.version),
        semver::Version::parse(tag.strip_prefix('v').unwrap_or(tag)),
        release.asset.as_ref(),
    ) else {
        return false;
    };
    version == tag_version
        && version.pre.is_empty()
        && version
            .cmp_precedence(&semver::Version::parse(env!("CARGO_PKG_VERSION")).unwrap())
            .is_gt()
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
        probe
            .stdout
            .take()
            .unwrap()
            .take(1024)
            .read_to_string(&mut output)?;
        if !status.success() || output.trim() != expected {
            return Err(std::io::Error::other(
                "Updated binary version does not match release",
            ));
        }
    }
    Ok(())
}

/// Only our supported standalone executable can be atomically replaced.
pub fn can_install_here() -> bool {
    cfg!(unix)
        && std::env::current_exe().is_ok_and(|p| {
            !p.components()
                .any(|c| c.as_os_str().to_string_lossy().ends_with(".app"))
        })
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
            "This installation uses an app bundle. Install its release package from GitHub.",
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
    let staged = parent.join(format!(
        ".quill-update-{}-{}-{}.new",
        std::process::id(),
        release.version,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    let plan_path = staged.with_extension("json");
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
            options.mode(0o700);
        }
        let mut file = options.open(&staged).map_err(
            |_| "Could not stage update beside the executable. Check folder permissions and retry.",
        )?;
        let mut reader = response.body_mut().as_reader().take(asset.size + 1);
        let size = std::io::copy(&mut reader, &mut file)
            .map_err(|_| "Download interrupted or disk full. Retry the update.")?;
        file.sync_all()
            .map_err(|_| "Could not save the update. Check disk space and retry.")?;
        if size != asset.size
            || digest_file(&staged).map_err(|_| "Could not verify the update.")?
                != asset.sha256.to_ascii_lowercase()
        {
            return Err(
                "Update checksum or size mismatch. Nothing was installed. Retry the release check.",
            );
        }
        probe_binary(&staged, &release.version).map_err(
            |_| "The downloaded binary failed its version check. Nothing was installed.",
        )?;
        let plan = InstallPlan {
            target,
            staged: staged.clone(),
            parent: std::process::id(),
            release: release.clone(),
        };
        write_private(
            &plan_path,
            &serde_json::to_vec(&plan).map_err(|_| "Could not prepare update handoff.")?,
        )
        .map_err(|_| "Could not save update handoff. Retry the update.")?;
        Ok(plan_path.clone())
    })();
    if result.is_err() {
        let _ = fs::remove_file(staged);
        let _ = fs::remove_file(plan_path);
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
    helper.spawn().map(|_| ())
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
        || plan.staged.parent() != target.parent()
        || plan_path.parent() != target.parent()
        || plan.parent == std::process::id()
        || plan.parent == 0
        || plan.staged == target
    {
        return Err(std::io::Error::other("Invalid update handoff"));
    }
    #[cfg(not(unix))]
    {
        Err(std::io::Error::other(
            "Binary updates are unavailable on this platform",
        ))
    }
    #[cfg(unix)]
    {
        let deadline = Instant::now() + Duration::from_secs(30);
        while Command::new("/bin/kill")
            .args(["-0", &plan.parent.to_string()])
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
        let result = (|| {
            if !valid_release(&plan.release) {
                return Err(std::io::Error::other("Invalid update release"));
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
            write_result(&plan.release, false)?;
            replace_and_launch(&target, &plan.staged)
        })();
        if result.is_err() {
            let _ = write_result(&plan.release, true);
            let _ = Command::new(&target).spawn();
        }
        let _ = fs::remove_file(plan_path);
        result
    }
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

#[cfg(all(test, unix))]
mod tests {
    use super::*;
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
}
