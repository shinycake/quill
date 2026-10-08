//! `tg:` URL scheme registration for platforms that need it at run time.
//!
//! * macOS: declared statically in the bundle's `Info.plist`
//!   (`CFBundleURLTypes`, see `scripts/macos-package-smoke.sh`).
//! * Linux: `MimeType=x-scheme-handler/tg;` in `quill.desktop`, made the
//!   default by `scripts/linux-install.sh` (`xdg-mime`).
//! * Windows: per-user registry keys under `HKCU\Software\Classes\tg`,
//!   written on startup like tdesktop's `psRegisterCustomScheme`
//!   (`platform/win/specific_win.cpp`). No admin rights needed.
//!
//! Only the Windows path runs code here; the entry table and the
//! "may we take over the current handler" rule are pure so every platform
//! tests them.

/// One registry value: `(subkey under HKCU\Software\Classes, value name,
/// data)`. An empty name is the key's default value.
pub type RegistryEntry = (String, &'static str, String);

/// The keys that make Windows launch `exe` for `tg:` links, with the link
/// passed after `--` so it can never be parsed as a flag.
pub fn windows_scheme_entries(exe: &str) -> Vec<RegistryEntry> {
    vec![
        ("tg".into(), "", "URL:Telegram Link".into()),
        ("tg".into(), "URL Protocol", String::new()),
        ("tg\\DefaultIcon".into(), "", format!("\"{exe}\",0")),
        (
            "tg\\shell\\open\\command".into(),
            "",
            format!("\"{exe}\" -- \"%1\""),
        ),
    ]
}

/// Whether Quill may (re)write the `tg` handler. A missing handler or one
/// that already points at a Quill build (an old install path, a moved
/// folder) is ours to update; another client's registration (Telegram
/// Desktop) is left alone so installing Quill never steals links silently.
pub fn may_register(existing_command: Option<&str>) -> bool {
    match existing_command {
        None => true,
        Some(command) => {
            let command = command.trim();
            command.is_empty() || command.to_ascii_lowercase().contains("quill")
        }
    }
}

#[cfg(windows)]
mod windows_impl {
    use super::{RegistryEntry, may_register, windows_scheme_entries};
    use windows_sys::Win32::Foundation::ERROR_SUCCESS;
    use windows_sys::Win32::System::Registry::{
        HKEY, HKEY_CURRENT_USER, KEY_SET_VALUE, REG_OPTION_NON_VOLATILE, REG_SZ, RRF_RT_REG_SZ,
        RegCloseKey, RegCreateKeyExW, RegGetValueW, RegSetValueExW,
    };

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn classes_key(subkey: &str) -> Vec<u16> {
        wide(&format!("Software\\Classes\\{subkey}"))
    }

    fn read_open_command() -> Option<String> {
        let subkey = classes_key("tg\\shell\\open\\command");
        let mut size: u32 = 0;
        // SAFETY: first call only asks for the required byte size.
        let status = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                subkey.as_ptr(),
                std::ptr::null(),
                RRF_RT_REG_SZ,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &mut size,
            )
        };
        if status != ERROR_SUCCESS || size == 0 {
            return None;
        }
        let mut buf = vec![0u16; size as usize / 2 + 1];
        // SAFETY: `buf` holds at least `size` bytes; `size` is in/out.
        let status = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                subkey.as_ptr(),
                std::ptr::null(),
                RRF_RT_REG_SZ,
                std::ptr::null_mut(),
                buf.as_mut_ptr().cast(),
                &mut size,
            )
        };
        if status != ERROR_SUCCESS {
            return None;
        }
        let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        Some(String::from_utf16_lossy(&buf[..len]))
    }

    fn write_entry((subkey, name, data): &RegistryEntry) -> bool {
        let subkey = classes_key(subkey);
        let mut key: HKEY = std::ptr::null_mut();
        // SAFETY: valid NUL-terminated key path; `key` receives the handle.
        let status = unsafe {
            RegCreateKeyExW(
                HKEY_CURRENT_USER,
                subkey.as_ptr(),
                0,
                std::ptr::null(),
                REG_OPTION_NON_VOLATILE,
                KEY_SET_VALUE,
                std::ptr::null(),
                &mut key,
                std::ptr::null_mut(),
            )
        };
        if status != ERROR_SUCCESS {
            return false;
        }
        let name = wide(name);
        let data = wide(data);
        // SAFETY: `key` is open; `data` is a NUL-terminated UTF-16 buffer
        // whose byte length is passed.
        let status = unsafe {
            RegSetValueExW(
                key,
                name.as_ptr(),
                0,
                REG_SZ,
                data.as_ptr().cast(),
                (data.len() * 2) as u32,
            )
        };
        // SAFETY: closes the handle opened above.
        unsafe { RegCloseKey(key) };
        status == ERROR_SUCCESS
    }

    /// Points `tg:` at the running executable unless another client owns it.
    pub fn register_current_exe() {
        let Ok(exe) = std::env::current_exe() else {
            return;
        };
        let exe = exe.to_string_lossy().into_owned();
        let entries = windows_scheme_entries(&exe);
        let wanted = &entries[3].2;
        let existing = read_open_command();
        if existing.as_deref() == Some(wanted.as_str()) || !may_register(existing.as_deref()) {
            return;
        }
        for entry in &entries {
            if !write_entry(entry) {
                return;
            }
        }
    }
}

/// Registers the `tg:` scheme for the current user where the platform needs
/// a run-time step (Windows). Best effort: failures are silent, links just
/// keep opening in the previous handler.
pub fn register_url_scheme() {
    #[cfg(windows)]
    windows_impl::register_current_exe();
}

#[cfg(test)]
mod tests {
    use super::{may_register, windows_scheme_entries};

    #[test]
    fn entries_launch_the_exe_with_the_link_after_a_separator() {
        let entries = windows_scheme_entries(r"C:\Apps\Quill\quill.exe");
        let command = entries
            .iter()
            .find(|(key, _, _)| key == "tg\\shell\\open\\command")
            .unwrap();
        assert_eq!(command.2, r#""C:\Apps\Quill\quill.exe" -- "%1""#);
        assert!(
            entries
                .iter()
                .any(|(k, n, _)| k == "tg" && *n == "URL Protocol")
        );
    }

    #[test]
    fn does_not_steal_another_clients_handler() {
        assert!(may_register(None));
        assert!(may_register(Some("")));
        assert!(may_register(Some(r#""C:\old\Quill\quill.exe" -- "%1""#)));
        assert!(!may_register(Some(
            r#""C:\Telegram Desktop\Telegram.exe" -- "%1""#
        )));
    }
}
