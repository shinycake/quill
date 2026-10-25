//! A security descriptor that admits only the current user, for Quill's
//! named pipe (the default one lets every local user connect).
#![cfg(windows)]

use interprocess::os::windows::security_descriptor::SecurityDescriptor;
use std::io;

/// Full access for this process's user, nobody else (SDDL `D:P(A;;GA;;;<sid>)`).
pub fn current_user_only() -> io::Result<SecurityDescriptor> {
    let sddl = format!("D:P(A;;GA;;;{})", current_user_sid()?);
    let wide = widestring::U16CString::from_str(&sddl).map_err(io::Error::other)?;
    SecurityDescriptor::deserialize(&wide)
}

/// This process's user SID in string form (`S-1-5-21-…`).
fn current_user_sid() -> io::Result<String> {
    use windows_sys::Win32::System::Threading::GetCurrentProcess;
    // SAFETY: GetCurrentProcess returns a pseudo handle that needs no closing.
    process_user_sid(unsafe { GetCurrentProcess() })
}

/// Whether process `pid` runs as this process's user. Agents check the
/// other end of the pipe with it: the pipe name is predictable, so another
/// local user could create it first. Fails closed.
pub fn is_current_user_process(pid: u32) -> bool {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};
    // SAFETY: OpenProcess takes plain values and returns a handle or null.
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if process.is_null() {
        return false;
    }
    let theirs = process_user_sid(process);
    // SAFETY: `process` was opened above and is closed once.
    unsafe { CloseHandle(process) };
    match (theirs, current_user_sid()) {
        (Ok(theirs), Ok(mine)) => theirs == mine,
        _ => false,
    }
}

/// The user SID of `process`, in string form.
fn process_user_sid(process: windows_sys::Win32::Foundation::HANDLE) -> io::Result<String> {
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, LocalFree};
    use windows_sys::Win32::Security::Authorization::ConvertSidToStringSidW;
    use windows_sys::Win32::Security::{GetTokenInformation, TOKEN_QUERY, TOKEN_USER, TokenUser};
    use windows_sys::Win32::System::Threading::OpenProcessToken;

    let mut token: HANDLE = std::ptr::null_mut();
    // SAFETY: `process` is a live process handle; `token` is a valid out pointer.
    if unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut token) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let result = (|| {
        let mut needed = 0u32;
        // SAFETY: a size query with a null buffer; `needed` is a valid out pointer.
        unsafe { GetTokenInformation(token, TokenUser, std::ptr::null_mut(), 0, &mut needed) };
        // u64 words keep the buffer aligned for TOKEN_USER.
        let words = (needed as usize).div_ceil(8).max(1);
        let mut buffer = vec![0u64; words];
        let size = u32::try_from(words * 8).map_err(io::Error::other)?;
        // SAFETY: `buffer` is writable for `size` bytes and outlives the call.
        if unsafe {
            GetTokenInformation(
                token,
                TokenUser,
                buffer.as_mut_ptr().cast(),
                size,
                &mut needed,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: GetTokenInformation(TokenUser) filled the aligned buffer with a TOKEN_USER.
        let sid = unsafe { (*buffer.as_ptr().cast::<TOKEN_USER>()).User.Sid };
        let mut text: windows_sys::core::PWSTR = std::ptr::null_mut();
        // SAFETY: `sid` points into `buffer`, alive here; `text` is a valid out pointer.
        if unsafe { ConvertSidToStringSidW(sid, &mut text) } == 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: on success `text` is a NUL-terminated wide string from LocalAlloc.
        let sid = unsafe { widestring::U16CStr::from_ptr_str(text) }.to_string_lossy();
        // SAFETY: frees the LocalAlloc'd string exactly once.
        unsafe { LocalFree(text.cast()) };
        Ok(sid)
    })();
    // SAFETY: `token` was opened above and is closed once.
    unsafe { CloseHandle(token) };
    result
}
