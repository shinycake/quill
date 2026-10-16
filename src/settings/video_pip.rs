//! Where Picture-in-Picture was last placed, so the next one opens there
//! (tdesktop `Settings::videoPipGeometry`, saved by
//! `PipDelegate::pipSaveGeometry` whenever a move or resize settles).

use super::write_json_atomic;
use crate::media_viewer::pip::panel::PipPosition;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const FILE_NAME: &str = "video_pip.json";

/// The saved position and the screen it was on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedPipPosition {
    pub position: PipPosition,
    /// The screen's stable id, when the platform has one (GPUI's display
    /// uuid); otherwise the screen is matched by its bounds.
    #[serde(default)]
    pub screen_id: Option<String>,
}

fn path() -> Option<PathBuf> {
    super::safe_app_root().map(|root| root.join(FILE_NAME))
}

/// The saved position; `None` when there is none or the file is broken.
pub fn load_video_pip_position() -> Option<SavedPipPosition> {
    load_video_pip_position_from(&path()?)
}

pub fn load_video_pip_position_from(path: &Path) -> Option<SavedPipPosition> {
    let bytes = std::fs::read(path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

pub fn save_video_pip_position(saved: &SavedPipPosition) -> std::io::Result<()> {
    let path = path().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::NotFound, "no app data directory")
    })?;
    write_json_atomic(&path, saved)
}

#[cfg(test)]
mod tests {
    use super::{SavedPipPosition, load_video_pip_position_from};
    use crate::media_viewer::pip::panel::{PipPosition, PipRect, Sides};
    use crate::settings::write_json_atomic;

    #[test]
    fn a_saved_position_reads_back_and_a_broken_file_is_none() {
        let dir = std::env::temp_dir().join(format!("quill-video-pip-{}", std::process::id()));
        let file = dir.join("video_pip.json");
        let saved = SavedPipPosition {
            position: PipPosition {
                attached: Sides::RIGHT,
                snapped: Sides::BOTTOM,
                geometry: PipRect::new(1100, 700, 320, 180),
                screen: PipRect::new(0, 0, 1440, 900),
            },
            screen_id: Some("display".into()),
        };
        write_json_atomic(&file, &saved).unwrap();
        assert_eq!(load_video_pip_position_from(&file), Some(saved));
        std::fs::write(&file, b"{").unwrap();
        assert_eq!(load_video_pip_position_from(&file), None);
        assert_eq!(load_video_pip_position_from(&dir.join("none")), None);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
