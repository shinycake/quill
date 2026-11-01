//! Screenshot captures (`QUILL_DEMO_CAPTURE`): a still picture decoded
//! synchronously, so the captured frame is the same on every run. The
//! FFmpeg shim decodes it when it is installed (open, seek, first picture,
//! at most two seconds); macOS without the shim uses AVFoundation's image
//! generator. The transport behaves as usual, but the picture only changes
//! on a seek.
#![deny(clippy::indexing_slicing)]

use super::transport::{BackendStatus, Want};
use super::{Backend, ErrorKind, Picture, PlayerError, Present, Pulled};
use gpui_kit::RenderImage;
use quill::lang::{keys, tr};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

pub(super) struct StillBackend {
    path: PathBuf,
    picture: Option<Arc<RenderImage>>,
    duration: Option<f64>,
    position: f64,
    /// A seek to decode on the next pull.
    pending: Option<f64>,
    /// The picture changed since the last pull.
    fresh: bool,
}

impl StillBackend {
    pub(super) fn open(path: &Path) -> Result<Self, PlayerError> {
        let (picture, duration) = still_picture(path, 0.0).ok_or_else(|| {
            PlayerError::new(ErrorKind::Open, tr(keys::Q_AV_THIS_VIDEO_CANT_BE_PLAYED))
        })?;
        Ok(Self {
            path: path.to_path_buf(),
            picture: Some(picture),
            duration,
            position: 0.0,
            pending: None,
            fresh: true,
        })
    }
}

/// The picture at `at` seconds and the clip's duration.
fn still_picture(path: &Path, at: f64) -> Option<(Arc<RenderImage>, Option<f64>)> {
    if quill::video_decode::available() {
        let (frame, duration) = super::backend_ffmpeg::first_picture(path, at)?;
        return Some((
            super::backend_ffmpeg::render_image(frame, &Default::default())?,
            duration,
        ));
    }
    #[cfg(target_os = "macos")]
    {
        super::backend_avplayer::still_picture(path, at)
    }
    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}

impl Drop for StillBackend {
    fn drop(&mut self) {
        if let Some(old) = self.picture.take() {
            super::super::image_budget::retire_all([old]);
        }
    }
}

impl Backend for StillBackend {
    fn status(&mut self) -> BackendStatus {
        BackendStatus {
            ready: true,
            position: self.position,
            duration: self.duration,
            buffered: self.duration.map(|end| vec![0.0..end]).unwrap_or_default(),
            ..BackendStatus::default()
        }
    }

    fn sync(&mut self, _want: Want) {}

    fn seek(&mut self, secs: f64) {
        self.position = secs.max(0.0);
        self.pending = Some(self.position);
    }

    fn set_volume(&mut self, _volume: f32) {}

    fn step_frame(&mut self, _forward: bool) {}

    fn take_picture(&mut self, _present: &Present) -> Pulled {
        if let Some(at) = self.pending.take()
            && let Some((picture, _)) = still_picture(&self.path, at)
        {
            if let Some(old) = self.picture.replace(picture) {
                super::super::image_budget::retire_all([old]);
            }
            self.fresh = true;
        }
        let picture = if std::mem::take(&mut self.fresh) {
            self.picture.clone().map(Picture::Image)
        } else {
            None
        };
        Pulled {
            display_secs: picture.as_ref().map(|_| self.position),
            picture,
            item_secs: self.position,
            stats: None,
        }
    }

    fn next_due(&self, _now: Instant, _interval: std::time::Duration) -> Option<Instant> {
        None
    }

    fn error(&self) -> Option<PlayerError> {
        None
    }
}
