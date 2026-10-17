//! "Hardware accelerated video decoding" (tdesktop Settings > Advanced >
//! Performance, `Core::Settings::hardwareAcceleratedVideo`).
//!
//! An installation setting, on by default on macOS and off elsewhere, like
//! tdesktop's. Windows D3D11VA/DXVA2 is unverified on a real GPU, so it stays
//! opt-in until it is. The FFmpeg video player reads it when it opens a clip,
//! so a change applies to the next clip opened; players already open keep
//! their decoder.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::RwLock;

#[derive(Debug, Serialize, Deserialize)]
#[serde(default)]
struct Stored {
    enabled: bool,
}

const DEFAULT: bool = cfg!(target_os = "macos");

impl Default for Stored {
    fn default() -> Self {
        Self { enabled: DEFAULT }
    }
}

static STORED: RwLock<Option<bool>> = RwLock::new(None);

fn path() -> Option<PathBuf> {
    crate::settings::safe_app_root().map(|root| root.join("hardware_video.json"))
}

fn load(path: &Path) -> bool {
    let Ok(bytes) = std::fs::read(path) else {
        return DEFAULT;
    };
    match serde_json::from_slice::<Stored>(&bytes) {
        Ok(stored) => stored.enabled,
        Err(err) => {
            eprintln!(
                "quill: {} is unreadable ({err}); using the default",
                path.display()
            );
            DEFAULT
        }
    }
}

fn store(path: &Path, on: bool) {
    if let Err(err) = crate::settings::write_json_atomic(path, &Stored { enabled: on }) {
        eprintln!("quill: could not save {}: {err}", path.display());
    }
}

/// Whether video may decode on the GPU.
pub fn enabled() -> bool {
    if let Ok(guard) = STORED.read()
        && let Some(on) = *guard
    {
        return on;
    }
    let loaded = path().is_none_or(|path| load(&path));
    if let Ok(mut guard) = STORED.write() {
        return *guard.get_or_insert(loaded);
    }
    loaded
}

/// Remember the choice for the next clip and the next launch.
pub fn set_enabled(on: bool) {
    if crate::fast_buttons::persistence_enabled()
        && let Some(path) = path()
    {
        store(&path, on);
    }
    if let Ok(mut guard) = STORED.write() {
        *guard = Some(on);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hardware_decoding_starts_at_the_platform_default_and_the_choice_persists() {
        let dir = std::env::temp_dir().join(format!("quill-hardware-video-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let file = dir.join("hardware_video.json");
        let _ = std::fs::remove_file(&file);
        let default = cfg!(target_os = "macos");
        assert_eq!(
            load(&file),
            default,
            "on only on macOS when nothing was saved"
        );
        store(&file, !default);
        assert_eq!(load(&file), !default, "the choice persists");
        store(&file, default);
        assert_eq!(load(&file), default);
        std::fs::write(&file, b"not json").unwrap();
        assert_eq!(
            load(&file),
            default,
            "the default when the file is unreadable"
        );
        std::fs::write(&file, b"{}").unwrap();
        assert_eq!(load(&file), default);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
