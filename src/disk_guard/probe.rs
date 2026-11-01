//! The free bytes on a volume, as this user can write them: `f_bavail`
//! leaves out what is reserved for root, `GetDiskFreeSpaceExW` counts
//! quotas, and on Linux `super::linux` adds the user's block quota.
use std::path::Path;

/// What one read of a volume says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Volume {
    Free(u64),
    /// The volume answered without sizes: nothing is known.
    NoSizes,
    /// The read failed, or the volume is mounted read-only: TDLib cannot
    /// write there whatever the free space (a pulled drive, a remount after
    /// an I/O error).
    Unwritable,
}

impl Volume {
    pub fn free(self) -> Option<u64> {
        match self {
            Self::Free(free) => Some(free),
            Self::NoSizes | Self::Unwritable => None,
        }
    }
}

/// Free bytes on the volume holding `path`. `None` when the read fails or
/// the volume reports no sizes.
pub fn available_bytes(path: &Path) -> Option<u64> {
    read_volume(path).free()
}

/// [`Volume`] for the volume holding `path`, or its nearest existing
/// parent.
pub fn read_volume(path: &Path) -> Volume {
    read_available(nearest_existing(path))
}

pub(super) fn nearest_existing(path: &Path) -> &Path {
    path.ancestors()
        .find(|folder| folder.exists())
        .unwrap_or(path)
}

/// Some network and FUSE volumes report zero blocks: that is no reading,
/// and it must not look like a full disk.
pub(super) fn usable(blocks: u64, available: u64, block_size: u64) -> Volume {
    if blocks == 0 || block_size == 0 {
        return Volume::NoSizes;
    }
    Volume::Free(available.saturating_mul(block_size))
}

/// macOS counts purgeable space (local snapshots, caches it can evict) as
/// available for important use, which is what Finder shows and what a
/// write can get. `statfs` leaves it out, so it is the fallback only, for a
/// volume that does not answer the key.
#[cfg(target_os = "macos")]
fn read_available(path: &Path) -> Volume {
    match statfs_available(path) {
        Volume::Free(free) => Volume::Free(with_important_usage(free, important_usage(path))),
        other => other,
    }
}

/// Volumes that do not track it (exFAT, HFS+ and network shares) answer
/// the key with 0, which is no reading: statfs then decides.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn with_important_usage(statfs: u64, important: Option<u64>) -> u64 {
    important.filter(|bytes| *bytes > 0).unwrap_or(statfs)
}

#[cfg(all(target_os = "macos", feature = "ui"))]
fn important_usage(path: &Path) -> Option<u64> {
    use objc2_foundation::{
        NSNumber, NSString, NSURL, NSURLVolumeAvailableCapacityForImportantUsageKey,
    };
    let url = NSURL::fileURLWithPath(&NSString::from_str(path.to_str()?));
    let mut value = None;
    // SAFETY: the key is a Foundation constant, and `value` is a live slot
    // the call fills with an NSNumber or nil.
    unsafe {
        url.getResourceValue_forKey_error(
            &mut value,
            NSURLVolumeAvailableCapacityForImportantUsageKey,
        )
    }
    .ok()?;
    let number = value?.downcast::<NSNumber>().ok()?;
    u64::try_from(number.longLongValue()).ok()
}

#[cfg(all(target_os = "macos", not(feature = "ui")))]
fn important_usage(_path: &Path) -> Option<u64> {
    None
}

// `statfs`, not `statvfs`: Darwin's `statvfs` has 32-bit block counts.
#[cfg(target_os = "macos")]
fn statfs_available(path: &Path) -> Volume {
    use std::os::unix::ffi::OsStrExt;
    let Ok(path) = std::ffi::CString::new(path.as_os_str().as_bytes()) else {
        return Volume::NoSizes;
    };
    let mut stat = std::mem::MaybeUninit::<libc::statfs>::uninit();
    // SAFETY: `path` is a valid C string and `stat` has room for a `statfs`.
    let rc = unsafe { libc::statfs(path.as_ptr(), stat.as_mut_ptr()) };
    if rc != 0 {
        return Volume::Unwritable;
    }
    // SAFETY: statfs returned 0, so the buffer is initialised.
    let stat = unsafe { stat.assume_init() };
    if stat.f_flags & libc::MNT_RDONLY as u32 != 0 {
        return Volume::Unwritable;
    }
    usable(stat.f_blocks, stat.f_bavail, u64::from(stat.f_bsize))
}

#[cfg(target_os = "linux")]
use super::linux::read_available;

#[cfg(all(unix, not(any(target_os = "macos", target_os = "linux"))))]
fn read_available(path: &Path) -> Volume {
    use std::os::unix::ffi::OsStrExt;
    let Ok(path) = std::ffi::CString::new(path.as_os_str().as_bytes()) else {
        return Volume::NoSizes;
    };
    let mut stat = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    // SAFETY: `path` is a valid C string and `stat` has room for a `statvfs`.
    let rc = unsafe { libc::statvfs(path.as_ptr(), stat.as_mut_ptr()) };
    if rc != 0 {
        return Volume::Unwritable;
    }
    // SAFETY: statvfs returned 0, so the buffer is initialised.
    let stat = unsafe { stat.assume_init() };
    if stat.f_flag & libc::ST_RDONLY != 0 {
        return Volume::Unwritable;
    }
    usable(stat.f_blocks, stat.f_bavail, stat.f_frsize)
}

#[cfg(windows)]
fn read_available(path: &Path) -> Volume {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut available = 0u64;
    let mut total = 0u64;
    // SAFETY: `wide` is NUL-terminated and outlives the call, both out
    // pointers point at live `u64`s, and the last one is optional.
    let ok = unsafe {
        GetDiskFreeSpaceExW(
            wide.as_ptr(),
            &mut available,
            &mut total,
            std::ptr::null_mut(),
        )
    };
    if ok == 0 || read_only_volume(&wide) {
        return Volume::Unwritable;
    }
    usable(total, available, 1)
}

/// `FILE_READ_ONLY_VOLUME` in `GetVolumeInformationW`'s flags.
#[cfg(windows)]
fn read_only_volume(wide_path: &[u16]) -> bool {
    use windows_sys::Win32::Storage::FileSystem::{GetVolumeInformationW, GetVolumePathNameW};
    const FILE_READ_ONLY_VOLUME: u32 = 0x0008_0000;
    let mut volume = [0u16; 261];
    // SAFETY: `wide_path` is NUL-terminated, and `volume` has room for the
    // length passed.
    let ok =
        unsafe { GetVolumePathNameW(wide_path.as_ptr(), volume.as_mut_ptr(), volume.len() as u32) };
    if ok == 0 {
        return false;
    }
    let mut flags = 0u32;
    // SAFETY: `volume` holds a NUL-terminated root path, `flags` is a live
    // `u32`, and the other out pointers are optional.
    let ok = unsafe {
        GetVolumeInformationW(
            volume.as_ptr(),
            std::ptr::null_mut(),
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut flags,
            std::ptr::null_mut(),
            0,
        )
    };
    ok != 0 && flags & FILE_READ_ONLY_VOLUME != 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_volume_that_reports_no_sizes_is_no_reading() {
        assert_eq!(usable(0, 0, 4096), Volume::NoSizes);
        assert_eq!(usable(100, 0, 0), Volume::NoSizes);
    }

    #[test]
    fn a_full_volume_reads_as_zero() {
        assert_eq!(usable(100, 0, 4096), Volume::Free(0));
        assert_eq!(usable(100, 3, 4096), Volume::Free(12_288));
    }

    #[test]
    fn a_zero_important_usage_answer_falls_back_to_statfs() {
        assert_eq!(with_important_usage(5 << 30, Some(0)), 5 << 30);
        assert_eq!(with_important_usage(5 << 30, None), 5 << 30);
        assert_eq!(with_important_usage(5 << 30, Some(9 << 30)), 9 << 30);
    }

    #[test]
    fn a_huge_volume_saturates() {
        assert_eq!(usable(u64::MAX, u64::MAX, 4096), Volume::Free(u64::MAX));
    }

    #[test]
    fn a_deleted_folder_reads_its_nearest_parent() {
        let temp = std::env::temp_dir();
        let deleted = temp.join("quill-no-such-folder/accounts");
        assert_eq!(nearest_existing(&deleted), temp);
        assert_eq!(nearest_existing(&temp), temp);
        assert!(available_bytes(&deleted).is_some());
    }

    #[test]
    fn a_real_folder_has_a_reading_and_a_missing_one_is_unwritable() {
        assert!(read_available(&std::env::temp_dir()).free().is_some());
        assert_eq!(
            read_available(Path::new("/no/such/quill/folder/at/all")),
            Volume::Unwritable
        );
    }
}
