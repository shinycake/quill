//! What `statvfs` misses on Linux. A user quota (`EDQUOT`) can stop writes
//! long before the volume fills, so the room left under it counts as free.
//! A zero reading is not always a full disk: btrfs reports 0 when its
//! metadata runs out while data may still fit, and as root `f_bavail`
//! leaves out the reserved blocks root may still use. There a small write
//! decides.
#![deny(clippy::indexing_slicing)]

use super::probe::Volume;
use std::io::Write as _;
use std::path::Path;

const KIB: u64 = 1024;
/// Big enough to need a fresh block on every file system.
const PROBE_BYTES: usize = 64 * 1024;
const PROBE_FILE: &str = ".quill-disk-probe";

pub(super) struct Statvfs {
    pub(super) available: u64,
    pub(super) free_to_root: u64,
    pub(super) btrfs: bool,
}

pub(super) fn read_available(path: &Path) -> Volume {
    let stat = match statvfs(path) {
        Ok(Some(stat)) => stat,
        Ok(None) => return Volume::NoSizes,
        Err(()) => return Volume::Unwritable,
    };
    let free = match quota_room(path) {
        Some(room) => stat.available.min(room),
        None => stat.available,
    };
    if free == 0 && zero_needs_a_write(&stat, is_root()) && !small_write_fails(path) {
        return Volume::NoSizes;
    }
    Volume::Free(free)
}

/// A 0 that a write may still prove wrong.
pub(super) fn zero_needs_a_write(stat: &Statvfs, root: bool) -> bool {
    stat.available == 0 && (stat.btrfs || (root && stat.free_to_root > 0))
}

pub(super) fn small_write_fails(folder: &Path) -> bool {
    let path = folder.join(PROBE_FILE);
    let written = std::fs::File::create(&path).and_then(|mut file| {
        file.write_all(&[0; PROBE_BYTES])?;
        file.sync_all()
    });
    let _ = std::fs::remove_file(&path);
    written.is_err_and(|err| no_room(&err))
}

fn no_room(err: &std::io::Error) -> bool {
    matches!(err.raw_os_error(), Some(libc::ENOSPC | libc::EDQUOT))
}

fn is_root() -> bool {
    // SAFETY: geteuid has no preconditions.
    unsafe { libc::geteuid() == 0 }
}

fn c_path(path: &Path) -> Option<std::ffi::CString> {
    use std::os::unix::ffi::OsStrExt;
    std::ffi::CString::new(path.as_os_str().as_bytes()).ok()
}

/// `Err` when the read fails or the mount is read-only, `Ok(None)` when the
/// volume reports no sizes.
fn statvfs(path: &Path) -> Result<Option<Statvfs>, ()> {
    let Some(c) = c_path(path) else {
        return Ok(None);
    };
    let mut vfs = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    // SAFETY: `c` is a valid C string and `vfs` has room for a `statvfs`.
    if unsafe { libc::statvfs(c.as_ptr(), vfs.as_mut_ptr()) } != 0 {
        return Err(());
    }
    // SAFETY: statvfs returned 0, so the buffer is initialised.
    let vfs = unsafe { vfs.assume_init() };
    if vfs.f_flag & libc::ST_RDONLY != 0 {
        return Err(());
    }
    let Volume::Free(available) = super::probe::usable(vfs.f_blocks, vfs.f_bavail, vfs.f_frsize)
    else {
        return Ok(None);
    };
    let mut fs = std::mem::MaybeUninit::<libc::statfs>::uninit();
    // SAFETY: as above, for a `statfs`.
    let btrfs = unsafe { libc::statfs(c.as_ptr(), fs.as_mut_ptr()) } == 0 && {
        // SAFETY: statfs returned 0, so the buffer is initialised.
        let fs = unsafe { fs.assume_init() };
        // The field's and the constant's types differ between libcs.
        #[allow(clippy::unnecessary_cast)]
        let btrfs = fs.f_type as i64 == libc::BTRFS_SUPER_MAGIC as i64;
        btrfs
    };
    Ok(Some(Statvfs {
        available,
        free_to_root: vfs.f_bfree.saturating_mul(vfs.f_frsize),
        btrfs,
    }))
}

/// The bytes this user may still write under its block quota on the volume
/// holding `path`, when it has one.
fn quota_room(path: &Path) -> Option<u64> {
    // SAFETY: getuid has no preconditions.
    let uid = unsafe { libc::getuid() };
    let id = libc::c_int::try_from(uid).ok()?;
    let cmd = libc::QCMD(libc::Q_GETQUOTA, USRQUOTA);
    let mut quota = std::mem::MaybeUninit::<libc::dqblk>::zeroed();
    let file = std::fs::File::open(path).ok()?;
    let fd = std::os::fd::AsRawFd::as_raw_fd(&file);
    // SAFETY: `fd` is open for the call, and `quota` has room for a dqblk.
    let rc = unsafe { libc::syscall(libc::SYS_quotactl_fd, fd, cmd, id, quota.as_mut_ptr()) };
    if rc != 0 {
        let device = c_path(&mount_source(path)?)?;
        // SAFETY: `device` is a valid C string and `quota` has room for a
        // dqblk.
        let rc = unsafe { libc::quotactl(cmd, device.as_ptr(), id, quota.as_mut_ptr().cast()) };
        if rc != 0 {
            return None;
        }
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_secs());
    // SAFETY: zeroed is a valid dqblk, and a call that returned 0 filled it.
    room_under(&unsafe { quota.assume_init() }, now)
}

const USRQUOTA: libc::c_int = 0;

/// The kernel (`check_bdq` in fs/quota/dquot.c) refuses a write only past
/// the hard limit, or past the soft limit once its grace time is over: in
/// the grace time, writes still reach the hard limit. `now` is in seconds
/// since the epoch, as `dqb_btime` is.
pub(super) fn room_under(quota: &libc::dqblk, now: u64) -> Option<u64> {
    if quota.dqb_valid & libc::QIF_BLIMITS == 0 || quota.dqb_valid & libc::QIF_SPACE == 0 {
        return None;
    }
    let grace_over =
        quota.dqb_valid & libc::QIF_BTIME != 0 && quota.dqb_btime != 0 && now >= quota.dqb_btime;
    let limit = if quota.dqb_bsoftlimit != 0 && grace_over {
        quota.dqb_bsoftlimit
    } else if quota.dqb_bhardlimit != 0 {
        quota.dqb_bhardlimit
    } else {
        return None;
    };
    Some(limit.saturating_mul(KIB).saturating_sub(quota.dqb_curspace))
}

/// The device `quotactl` wants: the source of the mount holding `path`,
/// from `/proc/self/mountinfo`.
fn mount_source(path: &Path) -> Option<std::path::PathBuf> {
    let real = std::fs::canonicalize(path).ok()?;
    let mountinfo = std::fs::read_to_string("/proc/self/mountinfo").ok()?;
    source_in(&mountinfo, &real)
}

pub(super) fn source_in(mountinfo: &str, path: &Path) -> Option<std::path::PathBuf> {
    mountinfo
        .lines()
        .filter_map(|line| {
            let (mount, tail) = line.split_once(" - ")?;
            let point = mount.split(' ').nth(4)?;
            let source = tail.split(' ').nth(1)?;
            let point = unescape(point);
            path.starts_with(&point).then_some((point, source))
        })
        .max_by_key(|(point, _)| point.len())
        .map(|(_, source)| std::path::PathBuf::from(unescape(source)))
}

/// mountinfo writes a space as `\040`, and a tab, newline or backslash the
/// same way.
fn unescape(field: &str) -> String {
    field
        .replace("\\040", " ")
        .replace("\\011", "\t")
        .replace("\\012", "\n")
        .replace("\\134", "\\")
}
