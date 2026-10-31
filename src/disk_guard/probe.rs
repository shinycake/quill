//! The free bytes on a volume, as this user can write them: `f_bavail`
//! leaves out what is reserved for root, and `GetDiskFreeSpaceExW` counts
//! quotas.
use std::path::Path;

/// Free bytes on the volume holding `path`. `None` when the read fails or
/// the volume reports no sizes.
pub fn available_bytes(path: &Path) -> Option<u64> {
    #[cfg(feature = "demo-capture")]
    if let Some(free) = demo_available_bytes() {
        return Some(free);
    }
    read_available(path)
}

#[cfg(feature = "demo-capture")]
fn demo_available_bytes() -> Option<u64> {
    std::env::var("QUILL_DEMO_DISK_FREE_MB")
        .ok()?
        .parse::<u64>()
        .ok()?
        .checked_mul(super::MIB)
}

/// Some network and FUSE volumes report zero blocks: that is no reading,
/// and it must not look like a full disk.
fn usable(blocks: u64, available: u64, block_size: u64) -> Option<u64> {
    (blocks != 0 && block_size != 0).then(|| available.saturating_mul(block_size))
}

// `statfs`, not `statvfs`: Darwin's `statvfs` has 32-bit block counts.
#[cfg(target_os = "macos")]
fn read_available(path: &Path) -> Option<u64> {
    use std::os::unix::ffi::OsStrExt;
    let path = std::ffi::CString::new(path.as_os_str().as_bytes()).ok()?;
    let mut stat = std::mem::MaybeUninit::<libc::statfs>::uninit();
    // SAFETY: `path` is a valid C string and `stat` has room for a `statfs`.
    let rc = unsafe { libc::statfs(path.as_ptr(), stat.as_mut_ptr()) };
    if rc != 0 {
        return None;
    }
    // SAFETY: statfs returned 0, so the buffer is initialised.
    let stat = unsafe { stat.assume_init() };
    usable(stat.f_blocks, stat.f_bavail, u64::from(stat.f_bsize))
}

#[cfg(all(unix, not(target_os = "macos")))]
fn read_available(path: &Path) -> Option<u64> {
    use std::os::unix::ffi::OsStrExt;
    let path = std::ffi::CString::new(path.as_os_str().as_bytes()).ok()?;
    let mut stat = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    // SAFETY: `path` is a valid C string and `stat` has room for a `statvfs`.
    let rc = unsafe { libc::statvfs(path.as_ptr(), stat.as_mut_ptr()) };
    if rc != 0 {
        return None;
    }
    // SAFETY: statvfs returned 0, so the buffer is initialised.
    let stat = unsafe { stat.assume_init() };
    usable(stat.f_blocks, stat.f_bavail, stat.f_frsize)
}

#[cfg(windows)]
fn read_available(path: &Path) -> Option<u64> {
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
    if ok == 0 {
        return None;
    }
    usable(total, available, 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_volume_that_reports_no_sizes_is_no_reading() {
        assert_eq!(usable(0, 0, 4096), None);
        assert_eq!(usable(100, 0, 0), None);
    }

    #[test]
    fn a_full_volume_reads_as_zero() {
        assert_eq!(usable(100, 0, 4096), Some(0));
        assert_eq!(usable(100, 3, 4096), Some(12_288));
    }

    #[test]
    fn a_huge_volume_saturates() {
        assert_eq!(usable(u64::MAX, u64::MAX, 4096), Some(u64::MAX));
    }

    #[test]
    fn a_real_folder_has_a_reading_and_a_missing_one_has_none() {
        assert!(read_available(&std::env::temp_dir()).is_some());
        assert_eq!(
            read_available(Path::new("/no/such/quill/folder/at/all")),
            None
        );
    }
}
