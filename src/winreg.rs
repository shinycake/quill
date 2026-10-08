//! Minimal HKCU registry string access (Windows only), over `windows-sys`.
//!
//! Used by autostart (`Software\Microsoft\Windows\CurrentVersion\Run`, as
//! tdesktop's `platform/win/specific_win.cpp` does) and by the toast
//! identity registration (`Software\Classes\AppUserModelId\<AUMID>`).
//! Only `REG_SZ` values under `HKEY_CURRENT_USER`, which needs no elevation.

use std::io;
use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_MORE_DATA, ERROR_SUCCESS};
use windows_sys::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_SET_VALUE, REG_OPTION_NON_VOLATILE, REG_SZ, RRF_RT_REG_SZ,
    RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegGetValueW, RegSetValueExW,
};

/// NUL-terminated UTF-16 for the Win32 `W` APIs.
pub fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

fn check(code: u32) -> io::Result<()> {
    if code == ERROR_SUCCESS {
        Ok(())
    } else {
        Err(io::Error::from_raw_os_error(code as i32))
    }
}

/// Create (or open) `HKCU\<subkey>` and set the string value `name`.
pub fn set_string(subkey: &str, name: &str, value: &str) -> io::Result<()> {
    let subkey = wide(subkey);
    let name = wide(name);
    let data = wide(value);
    let mut key: HKEY = std::ptr::null_mut();
    // SAFETY: all pointers reference live, NUL-terminated buffers; `key` is
    // only used after RegCreateKeyExW reports success and is closed below.
    unsafe {
        check(RegCreateKeyExW(
            HKEY_CURRENT_USER,
            subkey.as_ptr(),
            0,
            std::ptr::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE,
            std::ptr::null(),
            &mut key,
            std::ptr::null_mut(),
        ))?;
        let result = check(RegSetValueExW(
            key,
            name.as_ptr(),
            0,
            REG_SZ,
            data.as_ptr().cast(),
            (data.len() * 2) as u32,
        ));
        RegCloseKey(key);
        result
    }
}

/// Read the string value `name` under `HKCU\<subkey>`; `Ok(None)` when the
/// key or value does not exist.
pub fn get_string(subkey: &str, name: &str) -> io::Result<Option<String>> {
    let subkey = wide(subkey);
    let name = wide(name);
    let mut buf: Vec<u16> = vec![0; 260];
    loop {
        let mut bytes = (buf.len() * 2) as u32;
        // SAFETY: `buf` provides `bytes` writable bytes; the API updates
        // `bytes` to the size it wrote (or needs).
        let code = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                subkey.as_ptr(),
                name.as_ptr(),
                RRF_RT_REG_SZ,
                std::ptr::null_mut(),
                buf.as_mut_ptr().cast(),
                &mut bytes,
            )
        };
        match code {
            ERROR_SUCCESS => {
                let units = (bytes as usize / 2).min(buf.len());
                let end = buf[..units].iter().position(|&c| c == 0).unwrap_or(units);
                return Ok(Some(String::from_utf16_lossy(&buf[..end])));
            }
            ERROR_FILE_NOT_FOUND => return Ok(None),
            ERROR_MORE_DATA => buf.resize((bytes as usize / 2) + 1, 0),
            other => return Err(io::Error::from_raw_os_error(other as i32)),
        }
    }
}

/// Delete the value `name` under `HKCU\<subkey>`; a missing key or value is
/// success (the postcondition holds).
pub fn delete_value(subkey: &str, name: &str) -> io::Result<()> {
    let subkey = wide(subkey);
    let name = wide(name);
    let mut key: HKEY = std::ptr::null_mut();
    // SAFETY: as in `set_string`.
    unsafe {
        let opened = RegCreateKeyExW(
            HKEY_CURRENT_USER,
            subkey.as_ptr(),
            0,
            std::ptr::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE,
            std::ptr::null(),
            &mut key,
            std::ptr::null_mut(),
        );
        check(opened)?;
        let code = RegDeleteValueW(key, name.as_ptr());
        RegCloseKey(key);
        if code == ERROR_FILE_NOT_FOUND {
            return Ok(());
        }
        check(code)
    }
}

/// Delete the whole `HKCU\<subkey>` tree (test cleanup).
#[cfg(test)]
pub fn delete_tree(subkey: &str) {
    use windows_sys::Win32::System::Registry::RegDeleteTreeW;
    let subkey = wide(subkey);
    // SAFETY: `subkey` is a live NUL-terminated buffer.
    unsafe {
        RegDeleteTreeW(HKEY_CURRENT_USER, subkey.as_ptr());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch_key() -> String {
        format!("Software\\QuillTest\\winreg-{}", std::process::id())
    }

    #[test]
    fn string_roundtrip_overwrite_and_delete() {
        let key = scratch_key();
        assert_eq!(get_string(&key, "Value").unwrap(), None);
        set_string(&key, "Value", "\"C:\\Program Files\\Quill\\quill.exe\"").unwrap();
        assert_eq!(
            get_string(&key, "Value").unwrap().as_deref(),
            Some("\"C:\\Program Files\\Quill\\quill.exe\"")
        );
        set_string(&key, "Value", "second").unwrap();
        assert_eq!(
            get_string(&key, "Value").unwrap().as_deref(),
            Some("second")
        );
        delete_value(&key, "Value").unwrap();
        assert_eq!(get_string(&key, "Value").unwrap(), None);
        // Deleting again is a no-op.
        delete_value(&key, "Value").unwrap();
        delete_tree(&key);
    }

    #[test]
    fn long_values_grow_the_buffer() {
        let key = scratch_key() + "-long";
        let long = "x".repeat(1000);
        set_string(&key, "Long", &long).unwrap();
        assert_eq!(
            get_string(&key, "Long").unwrap().as_deref(),
            Some(long.as_str())
        );
        delete_tree(&key);
    }
}
