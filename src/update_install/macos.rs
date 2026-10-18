//! macOS: the install is the app bundle, replaced as a whole.
//!
//! The release asset is `quill-macos-aarch64.zip`, a `ditto` archive of
//! `Quill.app`. It is unpacked with `/usr/bin/ditto -x -k` (the zip crate
//! would drop the resource forks and extended attributes the signature
//! seals) beside the bundle, checked (`codesign.rs`), cleared of
//! `com.apple.quarantine`, and swapped in at the next start with
//! `renamex_np(RENAME_SWAP)` (`swap.rs`). The new bundle starts through
//! LaunchServices (`/usr/bin/open`), so the Dock, the running-application
//! list and the `tg:` URL handler see the replaced bundle, not a process that
//! happens to run from it.
//!
//! Plans, claims and receipts live beside the bundle, not inside it: a file
//! added under `Contents/` breaks the bundle's seal, which the next update's
//! check would then refuse.
#![deny(clippy::indexing_slicing)]

pub(super) mod codesign;

use super::{PACKAGE_EXE, Unavailable};
use std::ffi::CString;
use std::fs;
use std::io::{self, Read};
use std::os::unix::ffi::OsStrExt;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

/// `MNT_RDONLY` from `<sys/mount.h>`, the `statfs` flag of a read-only
/// volume (the libc crate does not export it for macOS).
const MNT_RDONLY: u32 = 0x0000_0001;

/// How long `ditto` may take to unpack the release (about 60 MB).
const UNPACK_LIMIT: Duration = Duration::from_secs(120);
/// How long LaunchServices may take to accept the launch.
const LAUNCH_LIMIT: Duration = Duration::from_secs(30);
/// How long `hdiutil` may take to list the mounted disk images.
const HDIUTIL_LIMIT: Duration = Duration::from_secs(5);
const MAX_HDIUTIL_BYTES: u64 = 1024 * 1024;
const MAX_DEPTH: usize = 16;

/// The bundle `exe` runs from, when `exe` is Quill's packaged executable:
/// `<bundle>.app/Contents/MacOS/quill` with an `Info.plist` beside it. A
/// plain cargo build or a renamed executable is not.
pub(super) fn bundle_of(exe: &Path) -> Option<PathBuf> {
    if exe.file_name()? != PACKAGE_EXE {
        return None;
    }
    let macos = exe.parent()?;
    let contents = macos.parent()?;
    let bundle = contents.parent()?;
    let layout = macos.file_name()? == "MacOS"
        && contents.file_name()? == "Contents"
        && bundle.extension()? == "app"
        && contents.join("Info.plist").is_file();
    layout.then(|| bundle.to_path_buf())
}

/// The executable inside `bundle`.
pub(super) fn executable(bundle: &Path) -> PathBuf {
    bundle.join("Contents").join("MacOS").join(PACKAGE_EXE)
}

/// The receipt beside the bundle (`.Quill.app.last-update.json`), where a
/// file does not break the bundle's seal.
pub(super) fn receipt_for(exe: &Path) -> Option<PathBuf> {
    let bundle = bundle_of(exe)?;
    let name = bundle.file_name()?.to_string_lossy();
    Some(bundle.parent()?.join(format!(".{name}.last-update.json")))
}

// Where the bundle sits.

/// What was found out about the bundle's location.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Location {
    /// Gatekeeper runs the app from a random read-only copy
    /// (`/private/var/folders/…/AppTranslocation/…`).
    pub translocated: bool,
    /// The volume is a mounted disk image.
    pub disk_image: bool,
    /// The volume is mounted read-only.
    pub read_only: bool,
    /// Homebrew's Caskroom owns the bundle.
    pub package_manager: bool,
}

/// The most useful reason first: a translocated app and one on a disk image
/// are both read-only, and the fix for either is to move it to Applications.
pub(super) fn judge_location(location: Location) -> Result<(), Unavailable> {
    if location.translocated {
        Err(Unavailable::Translocated)
    } else if location.disk_image {
        Err(Unavailable::DiskImage)
    } else if location.read_only {
        Err(Unavailable::ReadOnly)
    } else if location.package_manager {
        Err(Unavailable::PackageManager)
    } else {
        Ok(())
    }
}

/// Whether the bundle at `bundle`, run as `exe` (canonical), can be replaced
/// here. The generic checks (write probes) run after this one.
pub(super) fn check_location(exe: &Path, bundle: &Path) -> Result<(), Unavailable> {
    judge_location(Location {
        translocated: translocated(exe),
        disk_image: on_disk_image(bundle),
        read_only: read_only_volume(bundle),
        package_manager: homebrew_cask(exe),
    })
}

pub(super) fn translocated(path: &Path) -> bool {
    path.components()
        .any(|part| matches!(part, Component::Normal(name) if name == "AppTranslocation"))
}

/// Homebrew keeps cask apps under `Caskroom` (and links them into
/// Applications, which a canonical path resolves).
pub(super) fn homebrew_cask(path: &Path) -> bool {
    path.components()
        .any(|part| matches!(part, Component::Normal(name) if name == "Caskroom"))
        || path.starts_with("/opt/homebrew")
        || path.starts_with("/usr/local/Cellar")
}

/// The mount point of the volume holding `path`, and its `statfs` flags.
fn volume_of(path: &Path) -> Option<(PathBuf, u32)> {
    let path = CString::new(path.as_os_str().as_bytes()).ok()?;
    let mut stat = std::mem::MaybeUninit::<libc::statfs>::uninit();
    // SAFETY: `path` is a NUL-terminated string that outlives the call, and
    // `stat` is a correctly sized buffer the call fills on success.
    let rc = unsafe { libc::statfs(path.as_ptr(), stat.as_mut_ptr()) };
    if rc != 0 {
        return None;
    }
    // SAFETY: statfs returned 0, so the buffer is initialised.
    let stat = unsafe { stat.assume_init() };
    let mount = stat
        .f_mntonname
        .iter()
        .take_while(|byte| **byte != 0)
        .map(|byte| *byte as u8)
        .collect::<Vec<u8>>();
    Some((
        PathBuf::from(std::ffi::OsStr::from_bytes(&mount)),
        stat.f_flags,
    ))
}

pub(super) fn read_only_volume(path: &Path) -> bool {
    volume_of(path).is_some_and(|(_, flags)| flags & MNT_RDONLY != 0)
}

/// Whether `bundle` sits on a mounted disk image. Only volumes under
/// `/Volumes` can be one, so a bundle on the system volume costs nothing.
fn on_disk_image(bundle: &Path) -> bool {
    if !bundle.starts_with("/Volumes") {
        return false;
    }
    let Some((mount, _)) = volume_of(bundle) else {
        return false;
    };
    hdiutil_mounts().is_some_and(|plist| mounted_images(&plist).contains(&mount))
}

/// `hdiutil info -plist`: every mounted disk image with its mount points.
/// `None` when it cannot be asked within [`HDIUTIL_LIMIT`].
fn hdiutil_mounts() -> Option<String> {
    let out = std::env::temp_dir().join(format!(".quill-hdiutil-{}", std::process::id()));
    let result = (|| {
        let file = fs::File::create(&out)?;
        let mut child = Command::new("/usr/bin/hdiutil")
            .args(["info", "-plist"])
            .stdin(Stdio::null())
            .stdout(Stdio::from(file))
            .stderr(Stdio::null())
            .spawn()?;
        let status = super::wait_with_timeout(&mut child, HDIUTIL_LIMIT)?;
        if !status.success() {
            return Err(io::Error::other("hdiutil failed"));
        }
        let mut text = String::new();
        fs::File::open(&out)?
            .take(MAX_HDIUTIL_BYTES)
            .read_to_string(&mut text)?;
        Ok(text)
    })();
    let _ = fs::remove_file(&out);
    result.ok()
}

/// The `mount-point` strings of an `hdiutil info -plist` answer.
pub(super) fn mounted_images(plist: &str) -> Vec<PathBuf> {
    plist
        .split("<key>mount-point</key>")
        .skip(1)
        .filter_map(|rest| {
            let start = rest.find("<string>")? + "<string>".len();
            let end = rest.get(start..)?.find("</string>")? + start;
            let value = rest.get(start..end)?;
            Some(PathBuf::from(
                value
                    .replace("&lt;", "<")
                    .replace("&gt;", ">")
                    .replace("&quot;", "\"")
                    .replace("&apos;", "'")
                    .replace("&amp;", "&"),
            ))
        })
        .collect()
}

// Unpacking.

/// Unpacks the release zip at `archive` so that `staged` is the bundle. The
/// archive must hold exactly one top-level entry, an `.app` directory.
pub(super) fn unpack(archive: &Path, staged: &Path) -> io::Result<()> {
    let unpack = staged.with_extension("unpack");
    let result = (|| {
        fs::create_dir(&unpack)?;
        let mut child = Command::new("/usr/bin/ditto")
            .arg("-x")
            .arg("-k")
            .arg(archive)
            .arg(&unpack)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        let status = super::wait_with_timeout(&mut child, UNPACK_LIMIT)?;
        if !status.success() {
            return Err(io::Error::other("ditto could not unpack the update"));
        }
        let mut entries = fs::read_dir(&unpack)?;
        let (Some(entry), None) = (entries.next().transpose()?, entries.next()) else {
            return Err(io::Error::other(
                "Update package does not hold exactly one app bundle",
            ));
        };
        let path = entry.path();
        if !entry.file_type()?.is_dir() || path.extension().is_none_or(|ext| ext != "app") {
            return Err(io::Error::other("Update package is not an app bundle"));
        }
        fs::rename(&path, staged)
    })();
    let _ = fs::remove_dir_all(&unpack);
    result
}

// Quarantine.

/// Removes `com.apple.quarantine` from every entry under `root` (and `root`
/// itself), without following links. Called only after the bundle passed
/// verification. A quarantined bundle would make LaunchServices assess an
/// app that is not notarized, and refuse it.
pub(super) fn remove_quarantine(root: &Path) -> io::Result<()> {
    fn walk(path: &Path, depth: usize) -> io::Result<()> {
        if depth > MAX_DEPTH {
            return Err(io::Error::other("Update package is nested too deeply"));
        }
        remove_quarantine_attribute(path)?;
        if fs::symlink_metadata(path)?.is_dir() {
            for entry in fs::read_dir(path)? {
                walk(&entry?.path(), depth + 1)?;
            }
        }
        Ok(())
    }
    walk(root, 0)
}

fn remove_quarantine_attribute(path: &Path) -> io::Result<()> {
    let path = CString::new(path.as_os_str().as_bytes())
        .map_err(|_| io::Error::other("Invalid path in update package"))?;
    // SAFETY: both pointers are NUL-terminated strings that outlive the call;
    // removexattr only reads them. XATTR_NOFOLLOW keeps it off link targets.
    let rc = unsafe {
        libc::removexattr(
            path.as_ptr(),
            c"com.apple.quarantine".as_ptr(),
            libc::XATTR_NOFOLLOW,
        )
    };
    if rc == 0 {
        return Ok(());
    }
    let error = io::Error::last_os_error();
    match error.raw_os_error() {
        Some(libc::ENOATTR) => Ok(()),
        _ => Err(error),
    }
}

// Launching.

/// Starts the bundle at `bundle` with `args` through LaunchServices
/// (`open -n -a`), as tdesktop's updater does with `launchApplicationAtURL`.
/// `open` does not pass the caller's environment on, so Quill's own
/// variables (`QUILL_*`: a data root or a release fixture in test builds)
/// are forwarded the way a Linux relaunch inherits them.
pub(super) fn launch(bundle: &Path, args: &[String]) -> io::Result<()> {
    let mut command = Command::new("/usr/bin/open");
    command.arg("-n").arg("-a").arg(bundle);
    for (key, value) in std::env::vars_os() {
        if key.as_bytes().starts_with(b"QUILL_") {
            let mut pair = key;
            pair.push("=");
            pair.push(value);
            command.arg("--env").arg(pair);
        }
    }
    if !args.is_empty() {
        command.arg("--args").args(args);
    }
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    let status = super::wait_with_timeout(&mut child, LAUNCH_LIMIT)?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "LaunchServices did not start the updated app ({status})"
        )))
    }
}

#[cfg(test)]
pub(super) mod tests;
