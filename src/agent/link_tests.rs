//! Which executable `quill agent connect`, `describe`, `--start` and
//! `quill mcp` name: `$APPIMAGE` only from inside that AppImage's mount.

use super::appimage_file;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

const MOUNT: &str = "/tmp/.mount_QuillAbc123";

fn os(text: &str) -> Option<&OsStr> {
    Some(OsStr::new(text))
}

#[test]
fn quills_own_appimage_names_the_appimage_file() {
    let exe = Path::new(MOUNT).join("usr/bin/quill");
    assert_eq!(
        appimage_file(&exe, os("/home/ann/Apps/Quill.AppImage"), os(MOUNT)),
        Some(PathBuf::from("/home/ann/Apps/Quill.AppImage"))
    );
}

#[test]
fn another_apps_appimage_variables_are_ignored() {
    // Cursor is an AppImage and passes APPIMAGE and APPDIR to what it runs.
    let exe = Path::new("/home/ann/.local/bin/quill");
    assert_eq!(
        appimage_file(
            exe,
            os("/home/ann/Apps/Cursor.AppImage"),
            os("/tmp/.mount_CursorXyz")
        ),
        None
    );
    // A mount whose name only starts the same is not the same folder.
    let exe = Path::new("/tmp/.mount_QuillAbc1234/usr/bin/quill");
    assert_eq!(
        appimage_file(exe, os("/home/ann/Apps/Other.AppImage"), os(MOUNT)),
        None
    );
}

#[test]
fn appimage_needs_both_variables() {
    let exe = Path::new(MOUNT).join("usr/bin/quill");
    assert_eq!(appimage_file(&exe, os("/a/Quill.AppImage"), None), None);
    assert_eq!(appimage_file(&exe, os("/a/Quill.AppImage"), os("")), None);
    assert_eq!(appimage_file(&exe, None, os(MOUNT)), None);
    assert_eq!(appimage_file(&exe, os(""), os(MOUNT)), None);
}

#[test]
fn outside_an_appimage_it_is_this_executable() {
    let exe = std::env::current_exe().ok();
    assert!(exe.is_some());
    // No AppImage in a test run, so executable() is current_exe().
    if std::env::var_os("APPIMAGE").is_none() {
        assert_eq!(super::executable(), exe);
    }
}
