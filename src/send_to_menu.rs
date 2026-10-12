//! The Windows "Send to" menu entry (`parity:platform-sendto-menu`).
//!
//! tdesktop (`psSendToMenu` in `platform/win/specific_win.cpp`, the
//! "Place Telegram in \"Send to\" menu" checkbox in Advanced settings) saves
//! a shortcut named after the app in the user's `FOLDERID_SendTo` folder,
//! pointing at the executable with the argument `--` and the app's working
//! directory; removing the shortcut turns the entry off. Explorer then runs
//! `Quill.exe -- <file> <file>...`, which `start_files` turns into the
//! "Forward to…" chooser. macOS and Linux have no counterpart in tdesktop
//! (`psSendToMenu` is empty there), and none is invented here.
//!
//! Whether the entry is on is read from the shortcut itself, the way
//! autostart reads its registry value, so the switch can never disagree with
//! what Explorer shows.

use std::io;
use std::path::{Path, PathBuf};

/// The shortcut's file name: the app's name, like tdesktop's `AppFile.lnk`.
pub const LINK_NAME: &str = "Quill.lnk";
/// The shortcut passes this before the files, so a file name that starts
/// with a dash is not taken for a flag.
pub const LINK_ARGUMENTS: &str = "--";
/// The shortcut's comment (tdesktop writes the same note).
pub const LINK_DESCRIPTION: &str =
    "Quill send to link.\nYou can disable the send to menu item in Quill settings.";

/// Whether this system has a "Send to" menu Quill can join.
pub fn supported() -> bool {
    cfg!(windows)
}

/// The shortcut inside the "Send to" folder `dir`.
pub fn link_path(dir: &Path) -> PathBuf {
    dir.join(LINK_NAME)
}

/// Whether the shortcut is in `dir`.
fn is_enabled_in(dir: &Path) -> bool {
    link_path(dir).is_file()
}

/// Deletes the shortcut from `dir`; a missing one is already off.
fn remove_in(dir: &Path) -> io::Result<()> {
    match std::fs::remove_file(link_path(dir)) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
}

/// Whether Quill is in the "Send to" menu.
pub fn is_enabled() -> bool {
    platform::send_to_dir()
        .map(|dir| is_enabled_in(&dir))
        .unwrap_or(false)
}

/// Puts Quill in the "Send to" menu or takes it out. Blocks on the shell:
/// call it off the UI thread.
pub fn set_enabled(on: bool) -> io::Result<()> {
    let dir = platform::send_to_dir()?;
    if on {
        platform::create_link(&dir, &std::env::current_exe()?)
    } else {
        remove_in(&dir)
    }
}

#[cfg(windows)]
mod platform {
    use super::{LINK_ARGUMENTS, LINK_DESCRIPTION, link_path};
    use std::io;
    use std::path::{Path, PathBuf};
    use windows::Win32::Storage::EnhancedStorage::PKEY_AppUserModel_ID;
    use windows::Win32::System::Com::StructuredStorage::PROPVARIANT;
    use windows::Win32::System::Com::{
        CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
        CoTaskMemFree, CoUninitialize, IPersistFile,
    };
    use windows::Win32::UI::Shell::PropertiesSystem::IPropertyStore;
    use windows::Win32::UI::Shell::{
        FOLDERID_SendTo, IShellLinkW, KF_FLAG_CREATE, SHGetKnownFolderPath, ShellLink,
    };
    use windows::core::{HSTRING, Interface};

    /// The app's user model id, the one the toast identity registers, so
    /// Windows groups the shortcut's windows with Quill.
    const APP_USER_MODEL_ID: &str = "org.shinycake.quill";

    fn io_error(error: windows::core::Error) -> io::Error {
        io::Error::other(error.message())
    }

    /// The user's "Send to" folder, created when missing.
    pub(super) fn send_to_dir() -> io::Result<PathBuf> {
        // SAFETY: the returned string is read once and freed with
        // `CoTaskMemFree`, as `SHGetKnownFolderPath` requires.
        unsafe {
            let raw =
                SHGetKnownFolderPath(&FOLDERID_SendTo, KF_FLAG_CREATE, None).map_err(io_error)?;
            let path = raw.to_string();
            CoTaskMemFree(Some(raw.0.cast()));
            path.map(PathBuf::from)
                .map_err(|error| io::Error::other(error.to_string()))
        }
    }

    /// Saves the shortcut: the executable with `--`, its folder as the
    /// working directory and the description (`ManageAppLink`).
    pub(super) fn create_link(dir: &Path, exe: &Path) -> io::Result<()> {
        let working_dir = exe.parent().unwrap_or(dir);
        let link = link_path(dir);
        // SAFETY: COM is initialized for this thread before any call and
        // released after the interface pointers are dropped.
        unsafe {
            let initialized = CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_ok();
            let result = (|| -> windows::core::Result<()> {
                let shell_link: IShellLinkW =
                    CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)?;
                shell_link.SetPath(&HSTRING::from(exe.as_os_str()))?;
                shell_link.SetArguments(&HSTRING::from(LINK_ARGUMENTS))?;
                shell_link.SetWorkingDirectory(&HSTRING::from(working_dir.as_os_str()))?;
                shell_link.SetDescription(&HSTRING::from(LINK_DESCRIPTION))?;
                // Like tdesktop, tag the shortcut with the app's id; a shell
                // that refuses the property still gets a working shortcut.
                if let Ok(store) = shell_link.cast::<IPropertyStore>()
                    && store
                        .SetValue(&PKEY_AppUserModel_ID, &PROPVARIANT::from(APP_USER_MODEL_ID))
                        .is_ok()
                {
                    let _ = store.Commit();
                }
                let file: IPersistFile = shell_link.cast()?;
                file.Save(&HSTRING::from(link.as_os_str()), true)
            })();
            if initialized {
                CoUninitialize();
            }
            result.map_err(io_error)
        }
    }
}

#[cfg(not(windows))]
mod platform {
    use std::io;
    use std::path::{Path, PathBuf};

    pub(super) fn send_to_dir() -> io::Result<PathBuf> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "this system has no \"Send to\" menu",
        ))
    }

    pub(super) fn create_link(_dir: &Path, _exe: &Path) -> io::Result<()> {
        Err(io::Error::from(io::ErrorKind::Unsupported))
    }
}

#[cfg(test)]
mod tests {
    use super::{LINK_NAME, is_enabled_in, link_path, remove_in, supported};

    fn scratch(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("quill-sendto-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn the_shortcut_is_named_after_the_app_inside_the_send_to_folder() {
        let dir = scratch("name");
        assert_eq!(link_path(&dir), dir.join(LINK_NAME));
        assert_eq!(LINK_NAME, "Quill.lnk");
    }

    #[test]
    fn the_entry_is_on_exactly_while_the_shortcut_exists() {
        let dir = scratch("state");
        assert!(!is_enabled_in(&dir));
        std::fs::write(link_path(&dir), b"lnk").unwrap();
        assert!(is_enabled_in(&dir));
        remove_in(&dir).unwrap();
        assert!(!is_enabled_in(&dir));
        // Already off stays off without an error.
        remove_in(&dir).unwrap();
    }

    #[test]
    fn only_windows_has_the_menu() {
        assert_eq!(supported(), cfg!(windows));
        if !supported() {
            assert!(!super::is_enabled());
            assert!(super::set_enabled(true).is_err());
        }
    }
}
