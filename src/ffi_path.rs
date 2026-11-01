//! A file path as the C string FFmpeg's `avformat_open_input` takes.

use std::ffi::CString;
use std::path::Path;

/// On Unix FFmpeg opens the raw bytes, so a name that is not UTF-8 still
/// opens. On Windows FFmpeg reads the string as UTF-8 and converts it to
/// UTF-16 itself, so only a UTF-8 path can be passed. `None` for a path
/// FFmpeg can't take (an interior NUL, or non-UTF-8 on Windows).
pub fn c_path(path: &Path) -> Option<CString> {
    #[cfg(unix)]
    let bytes = {
        use std::os::unix::ffi::OsStrExt;
        path.as_os_str().as_bytes()
    };
    #[cfg(not(unix))]
    let bytes = path.to_str()?.as_bytes();
    CString::new(bytes).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_utf8_path_passes_through() {
        let c = c_path(Path::new("/tmp/clip é.mp4")).unwrap();
        assert_eq!(c.as_bytes(), "/tmp/clip é.mp4".as_bytes());
    }

    #[test]
    fn an_interior_nul_is_refused() {
        assert!(c_path(Path::new("/tmp/a\0b.mp4")).is_none());
    }

    #[cfg(unix)]
    #[test]
    fn a_non_utf8_name_keeps_its_bytes_on_unix() {
        use std::os::unix::ffi::OsStrExt;
        let raw = b"/tmp/clip-\xff\xfe.mp4";
        let path = Path::new(std::ffi::OsStr::from_bytes(raw));
        assert!(path.to_str().is_none());
        assert_eq!(c_path(path).unwrap().as_bytes(), raw);
    }
}
