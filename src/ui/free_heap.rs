//! Free heap pages go back to the system once Quill goes idle.

/// Hand the allocator's free pages back to the system once Quill goes
/// idle (decoded images and their decode buffers are large, short-lived
/// blocks; without this the freed space stays resident as allocator free
/// lists).
/// - macOS: `malloc_zone_pressure_relief` on every zone. Blocks of 64 KiB
///   and up already went back when freed (`quill::large_alloc`). Measured
///   as a no-op on macOS 27, which appears to run xzone malloc
///   (docs/decisions/idle-heap-return.md).
/// - Linux (glibc): `malloc_trim`.
/// - Windows and other libcs: nothing; the Windows heap decommits large
///   free ranges on its own.
pub(super) fn release() {
    #[cfg(target_os = "macos")]
    {
        unsafe extern "C" {
            fn malloc_zone_pressure_relief(zone: *mut std::ffi::c_void, goal: usize) -> usize;
        }
        // SAFETY: a null zone means "all zones"; goal 0 means "as much as
        // possible". The call only releases free pages.
        unsafe {
            malloc_zone_pressure_relief(std::ptr::null_mut(), 0);
        }
    }
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    {
        // SAFETY: glibc's `malloc_trim` only returns free heap memory.
        unsafe {
            libc::malloc_trim(0);
        }
    }
}
