//! "Hardware accelerated video decoding" (tdesktop Settings > Advanced >
//! Performance, `Core::Settings::hardwareAcceleratedVideo`).
//!
//! An installation setting, on by default like tdesktop's. The FFmpeg video
//! player reads it when it opens a clip, so a change applies to the next
//! clip opened; players already open keep their decoder.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::RwLock;

#[derive(Debug, Serialize, Deserialize)]
#[serde(default)]
struct Stored {
    enabled: bool,
}

impl Default for Stored {
    fn default() -> Self {
        Self { enabled: true }
    }
}

static STORED: RwLock<Option<bool>> = RwLock::new(None);

fn path() -> Option<PathBuf> {
    crate::settings::safe_app_root().map(|root| root.join("hardware_video.json"))
}

fn load(path: &Path) -> bool {
    std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Stored>(&bytes).ok())
        .unwrap_or_default()
        .enabled
}

fn store(path: &Path, on: bool) {
    let _ = crate::settings::write_json_atomic(path, &Stored { enabled: on });
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
    fn hardware_decoding_is_on_until_turned_off_and_the_choice_persists() {
        let dir = std::env::temp_dir().join(format!("quill-hardware-video-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let file = dir.join("hardware_video.json");
        let _ = std::fs::remove_file(&file);
        assert!(load(&file), "on when nothing was saved");
        store(&file, false);
        assert!(!load(&file), "off after turning it off");
        store(&file, true);
        assert!(load(&file));
        std::fs::write(&file, b"not json").unwrap();
        assert!(load(&file), "on when the file is unreadable");
        std::fs::write(&file, b"{}").unwrap();
        assert!(load(&file));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
