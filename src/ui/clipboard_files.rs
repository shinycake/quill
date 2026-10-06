//! Files copied in Finder: the pasteboard holds `public.file-url` items,
//! which GPUI's clipboard API doesn't expose on macOS.

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
