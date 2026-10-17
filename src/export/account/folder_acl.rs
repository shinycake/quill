//! Windows: a user-only access list for the export folder.
//!
//! A new folder inherits the parent's ACL, so exporting to a shared drive or
//! `C:\Users\Public` would expose the whole account. The protected DACL
//! below drops inheritance and grants full control to the folder's owner and
//! to SYSTEM only. Files created inside inherit it (`OICI`).
#![cfg(windows)]

use std::io;
use std::path::Path;

/// Protected DACL: no inheritance from the parent; owner and SYSTEM only.
const USER_ONLY_SDDL: &str = "D:P(A;OICI;FA;;;OW)(A;OICI;FA;;;SY)";

pub fn make_private(folder: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::{ERROR_SUCCESS, LocalFree};
    use windows_sys::Win32::Security::Authorization::{
        ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1, SE_FILE_OBJECT,
        SetNamedSecurityInfoW,
    };
    use windows_sys::Win32::Security::{
        DACL_SECURITY_INFORMATION, GetSecurityDescriptorDacl, PROTECTED_DACL_SECURITY_INFORMATION,
    };

    let sddl: Vec<u16> = USER_ONLY_SDDL.encode_utf16().chain(Some(0)).collect();
    let path: Vec<u16> = folder.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut descriptor = std::ptr::null_mut();
    // SAFETY: `sddl` is NUL-terminated and outlives the call; on success the
    // system allocates `descriptor`, which is freed with LocalFree below.
    let converted = unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl.as_ptr(),
            SDDL_REVISION_1,
            &mut descriptor,
            std::ptr::null_mut(),
        )
    };
    if converted == 0 {
        return Err(io::Error::last_os_error());
    }
    let (mut present, mut defaulted) = (0, 0);
    let mut dacl = std::ptr::null_mut();
    // SAFETY: `descriptor` is the valid descriptor made above; the out
    // pointers are live locals. `dacl` points into `descriptor`.
    let got =
        unsafe { GetSecurityDescriptorDacl(descriptor, &mut present, &mut dacl, &mut defaulted) };
    let result = if got == 0 || present == 0 {
        Err(io::Error::last_os_error())
    } else {
        // SAFETY: `path` is NUL-terminated; `dacl` stays valid until the
        // descriptor is freed after this call.
        let status = unsafe {
            SetNamedSecurityInfoW(
                path.as_ptr(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                dacl,
                std::ptr::null(),
            )
        };
        if status == ERROR_SUCCESS {
            Ok(())
        } else {
            Err(io::Error::from_raw_os_error(status as i32))
        }
    };
    // SAFETY: frees the descriptor allocated by the conversion call, once.
    unsafe { LocalFree(descriptor) };
    result
}
