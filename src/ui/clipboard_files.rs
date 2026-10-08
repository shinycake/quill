//! Files copied in a file manager.
//!
//! - macOS: the pasteboard holds `public.file-url` items, which GPUI's
//!   clipboard API doesn't expose, so they are read from `NSPasteboard`.
//! - Windows: GPUI already reads `CF_HDROP` into
//!   `ClipboardEntry::ExternalPaths` (gpui-pre-windows `clipboard.rs`),
//!   which the composer attaches; nothing extra is needed here.
//! - Linux: GPUI's Wayland and X11 clipboards expose only text and images,
//!   never `text/uri-list` or `x-special/gnome-copied-files`. File managers
//!   also offer the files as plain text, so [`paths_from_text`] recognises a
//!   clipboard whose text is a list of `file://` URIs (Dolphin, the
//!   `x-special/gnome-copied-files` payload) or, on Linux, of absolute paths
//!   of existing files (GNOME Files).

use std::path::PathBuf;

/// Local paths of the files on the clipboard, if it holds copied files.
#[cfg(target_os = "macos")]
pub(super) fn copied_file_paths() -> Vec<PathBuf> {
    use objc2_app_kit::{NSPasteboard, NSPasteboardTypeFileURL};
    use objc2_foundation::NSURL;
    let pasteboard = NSPasteboard::generalPasteboard();
    let Some(items) = pasteboard.pasteboardItems() else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| {
            // SAFETY: reading a framework-provided pasteboard type constant.
            let kind = unsafe { NSPasteboardTypeFileURL };
            let url = item.stringForType(kind)?;
            let url = NSURL::URLWithString(&url)?;
            if !url.isFileURL() {
                return None;
            }
            url.path().map(|path| PathBuf::from(path.to_string()))
        })
        .collect()
}

#[cfg(not(target_os = "macos"))]
pub(super) fn copied_file_paths() -> Vec<PathBuf> {
    Vec::new()
}

/// Existing files named by clipboard text, or empty when the text is not
/// purely a file list. Accepts the `x-special/gnome-copied-files` layout (a
/// leading `copy`/`cut` line), `text/uri-list` (`#` comments, CRLF), and
/// `file://` URIs; on Linux also bare absolute paths. Every non-empty line
/// must resolve to an existing regular file, so ordinary text that merely
/// mentions a path still pastes as text.
pub(super) fn paths_from_text(text: &str) -> Vec<PathBuf> {
    paths_from_text_with(text, |path| path.is_file())
}

fn paths_from_text_with(text: &str, exists: impl Fn(&std::path::Path) -> bool) -> Vec<PathBuf> {
    let mut lines = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .peekable();
    if matches!(lines.peek(), Some(&"copy" | &"cut")) {
        lines.next();
    }
    let mut paths = Vec::new();
    for line in lines {
        let Some(path) = path_from_line(line) else {
            return Vec::new();
        };
        if !exists(&path) {
            return Vec::new();
        }
        paths.push(path);
    }
    paths
}

fn path_from_line(line: &str) -> Option<PathBuf> {
    if let Some(rest) = line.strip_prefix("file://") {
        // `file:///home/a` and `file://localhost/home/a`.
        let rest = rest.strip_prefix("localhost").unwrap_or(rest);
        let decoded = percent_decode(rest)?;
        #[cfg(windows)]
        let decoded = {
            // `/C:/Users/a` -> `C:/Users/a`.
            let bytes = decoded.as_bytes();
            if bytes.len() > 2 && bytes[0] == b'/' && bytes[2] == b':' {
                decoded[1..].to_string()
            } else {
                decoded
            }
        };
        #[cfg(not(windows))]
        if !decoded.starts_with('/') {
            return None;
        }
        return Some(PathBuf::from(decoded));
    }
    #[cfg(target_os = "linux")]
    if line.starts_with('/') {
        return Some(PathBuf::from(line));
    }
    None
}

/// `%XX` decoding to a UTF-8 string; `None` on malformed escapes.
fn percent_decode(input: &str) -> Option<String> {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = input.get(i + 1..i + 3)?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

#[cfg(test)]
mod tests {
    use super::{paths_from_text_with, percent_decode};
    use std::path::{Path, PathBuf};

    fn always(_: &Path) -> bool {
        true
    }

    #[cfg(not(windows))]
    #[test]
    fn decodes_uri_list_with_comments_and_crlf() {
        let text = "# files\r\nfile:///home/u/a%20b.txt\r\nfile://localhost/tmp/%C3%A9.png\r\n";
        assert_eq!(
            paths_from_text_with(text, always),
            vec![
                PathBuf::from("/home/u/a b.txt"),
                PathBuf::from("/tmp/\u{e9}.png")
            ]
        );
    }

    #[cfg(not(windows))]
    #[test]
    fn accepts_gnome_copied_files_header() {
        let text = "copy\nfile:///a/b.txt\nfile:///a/c.txt";
        assert_eq!(paths_from_text_with(text, always).len(), 2);
        assert_eq!(
            paths_from_text_with("cut\nfile:///a/b.txt", always).len(),
            1
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn accepts_bare_absolute_paths_on_linux() {
        assert_eq!(
            paths_from_text_with("/home/u/a.txt\n/home/u/b.txt", always),
            vec![
                PathBuf::from("/home/u/a.txt"),
                PathBuf::from("/home/u/b.txt")
            ]
        );
    }

    #[test]
    fn rejects_mixed_prose_and_missing_files() {
        assert!(paths_from_text_with("hello world", always).is_empty());
        assert!(paths_from_text_with("see file:///a/b.txt\nthanks", always).is_empty());
        assert!(paths_from_text_with("https://example.com/a", always).is_empty());
        assert!(
            paths_from_text_with("file:///a/b.txt\nfile:///a/c.txt", |p| p.ends_with("b.txt"))
                .is_empty()
        );
        assert!(paths_from_text_with("file:///a/%zz", always).is_empty());
        assert!(paths_from_text_with("", always).is_empty());
        assert!(paths_from_text_with("copy", always).is_empty());
    }

    #[test]
    fn percent_decode_handles_edges() {
        assert_eq!(percent_decode("a%2Fb").as_deref(), Some("a/b"));
        assert_eq!(percent_decode("100%"), None);
        assert_eq!(percent_decode("%FF"), None);
    }

    #[cfg(windows)]
    #[test]
    fn windows_drive_uri_drops_leading_slash() {
        assert_eq!(
            paths_from_text_with("file:///C:/Users/a%20b/x.txt", always),
            vec![PathBuf::from("C:/Users/a b/x.txt")]
        );
    }
}
