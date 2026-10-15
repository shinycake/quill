//! Screenshot captures (`QUILL_DEMO_CAPTURE`): a still picture decoded
//! synchronously, so the captured frame is the same on every run. The
//! FFmpeg shim decodes it when it is installed (open, seek, first picture,
//! at most two seconds); macOS without the shim uses AVFoundation's image
//! generator. The transport behaves as usual, but the picture only changes
//! on a seek.
#![deny(clippy::indexing_slicing)]

use super::transport::BackendStatus;
use super::{Backend, ErrorKind, Picture, PlayerError, Present, Pulled};
use gpui_kit::RenderImage;
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
        let (picture, duration) = still_picture(path, 0.0)
            .ok_or_else(|| PlayerError::new(ErrorKind::Open, "This video can't be played."))?;
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
        return Some((super::backend_ffmpeg::render_image(frame)?, duration));
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

    fn play(&mut self, _rate: f64) {}

    fn pause(&mut self) {}

    fn seek(&mut self, secs: f64) {
        self.position = secs.max(0.0);
        self.pending = Some(self.position);
    }

    fn set_volume(&mut self, _volume: f32) {}

    fn set_rate(&mut self, _rate: f64, _playing: bool) {}

    fn step_frame(&mut self, _forward: bool) {}

    fn take_picture(&mut self, _present: &Present) -> Pulled {
        if let Some(at) = self.pending.take()
            && let Some((picture, _)) = still_picture(&self.path, at)
        {
            self.picture = Some(picture);
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
        }
    }

    fn next_due(&self, _now: Instant, _interval: std::time::Duration) -> Option<Instant> {
        None
    }

    fn error(&self) -> Option<PlayerError> {
        None
    }
}
