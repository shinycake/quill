//! Saving viewer media into the downloads folder.

/// Copy `src` (a downloaded media file) into the user's downloads folder
/// (`XDG_DOWNLOAD_DIR`, else `~/Downloads`), de-duplicating the file name
/// with a ` (n)` suffix like the desktop clients. Returns the final path.
/// Pure filesystem work — the UI resolves `src` through the existing
/// `usable_path` machinery first.
pub fn save_media_to_downloads(src: &std::path::Path) -> std::io::Result<std::path::PathBuf> {
    let dir = downloads_dir()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "no downloads folder"))?;
    save_media_to_downloads_in_dir(src, &dir)
}

/// The toast after "Save": "Saved to Downloads" when the file went to the
/// downloads folder (tdesktop `lng_mediaview_saved`), else the folder's
/// name, so a custom location is never mislabeled.
pub fn saved_note(dest: &std::path::Path, downloads: Option<&std::path::Path>) -> String {
    let parent = dest.parent();
    if parent.is_some() && parent == downloads {
        return "Saved to Downloads".to_string();
    }
    match parent
        .and_then(std::path::Path::file_name)
        .and_then(|name| name.to_str())
    {
        Some(folder) => format!("Saved to {folder}"),
        None => "Saved".to_string(),
    }
}

/// The user's downloads folder (`XDG_DOWNLOAD_DIR`, else the platform's,
/// else `~/Downloads`).
pub fn downloads_dir() -> Option<std::path::PathBuf> {
    crate::file_prefs::configured_download_dir().or_else(os_downloads_dir)
}

/// The operating system's downloads folder (`XDG_DOWNLOAD_DIR`, else the
/// platform's, else `~/Downloads`), ignoring the in-app choice.
pub fn os_downloads_dir() -> Option<std::path::PathBuf> {
    std::env::var("XDG_DOWNLOAD_DIR")
        .map(std::path::PathBuf::from)
        .ok()
        .filter(|p| p.is_absolute())
        .or_else(|| {
            directories::UserDirs::new().and_then(|u| u.download_dir().map(|p| p.to_path_buf()))
        })
        .or_else(|| {
            std::env::var("HOME")
                .ok()
                .map(|h| std::path::PathBuf::from(format!("{h}/Downloads")))
        })
}

/// Copy `src` into `dir`, de-duplicating the file name with a ` (n)`
/// suffix like the desktop clients. Split out so tests can pass a temp
/// dir without mutating process-global env vars.
pub fn save_media_to_downloads_in_dir(
    src: &std::path::Path,
    dir: &std::path::Path,
) -> std::io::Result<std::path::PathBuf> {
    std::fs::create_dir_all(dir)?;
    let stem = src.file_stem().and_then(|s| s.to_str()).unwrap_or("media");
    let ext = src.extension().and_then(|e| e.to_str()).unwrap_or("");
    let mut input = std::fs::File::open(src)?;
    let mut n = 1u32;
    loop {
        let name = match (n, ext.is_empty()) {
            (1, true) => stem.to_string(),
            (1, false) => format!("{stem}.{ext}"),
            (_, true) => format!("{stem} ({n})"),
            (_, false) => format!("{stem} ({n}).{ext}"),
        };
        let candidate = dir.join(name);
        // `create_new` claims the name atomically, so a file that appeared
        // since the last check (or a concurrent save) is never overwritten.
        let mut output = match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(file) => file,
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
                n = n.saturating_add(1);
                continue;
            }
            Err(err) => return Err(err),
        };
        if let Err(err) = std::io::copy(&mut input, &mut output) {
            drop(output);
            let _ = std::fs::remove_file(&candidate);
            return Err(err);
        }
        return Ok(candidate);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saved_note_names_the_downloads_folder() {
        use std::path::Path;
        let downloads = Path::new("/home/me/Downloads");
        assert_eq!(
            saved_note(Path::new("/home/me/Downloads/cat.jpg"), Some(downloads)),
            "Saved to Downloads"
        );
        assert_eq!(
            saved_note(Path::new("/home/me/Pictures/cat.jpg"), Some(downloads)),
            "Saved to Pictures"
        );
        assert_eq!(
            saved_note(Path::new("/home/me/Downloads/cat.jpg"), None),
            "Saved to Downloads"
        );
        assert_eq!(saved_note(Path::new("cat.jpg"), None), "Saved");
    }

    #[test]
    fn save_copies_into_downloads_with_dedup_suffix() {
        let tmp = std::env::temp_dir().join(format!("quill-med1-{}", std::process::id()));
        let src_dir = tmp.join("src");
        std::fs::create_dir_all(&src_dir).unwrap();
        let src = src_dir.join("photo.jpg");
        std::fs::write(&src, b"fake-jpeg").unwrap();
        let dir = tmp.join("dl");
        let first = save_media_to_downloads_in_dir(&src, &dir).unwrap();
        assert_eq!(first.file_name().unwrap(), "photo.jpg");
        assert_eq!(std::fs::read(&first).unwrap(), b"fake-jpeg");
        let second = save_media_to_downloads_in_dir(&src, &dir).unwrap();
        assert_eq!(second.file_name().unwrap(), "photo (2).jpg");
        std::fs::remove_dir_all(&tmp).unwrap();
    }

    #[test]
    fn save_never_overwrites_existing_files() {
        let tmp = std::env::temp_dir().join(format!("quill-med2-{}", std::process::id()));
        let dir = tmp.join("dl");
        std::fs::create_dir_all(&dir).unwrap();
        let src = tmp.join("photo.jpg");
        std::fs::write(&src, b"new").unwrap();
        std::fs::write(dir.join("photo.jpg"), b"mine").unwrap();
        std::fs::write(dir.join("photo (2).jpg"), b"mine too").unwrap();
        let saved = save_media_to_downloads_in_dir(&src, &dir).unwrap();
        assert_eq!(saved.file_name().unwrap(), "photo (3).jpg");
        assert_eq!(std::fs::read(dir.join("photo.jpg")).unwrap(), b"mine");
        assert_eq!(
            std::fs::read(dir.join("photo (2).jpg")).unwrap(),
            b"mine too"
        );
        assert_eq!(std::fs::read(&saved).unwrap(), b"new");
        std::fs::remove_dir_all(&tmp).unwrap();
    }
}
