//! The FFmpeg backend (Linux, Windows; macOS with
//! `QUILL_VIDEO_BACKEND=ffmpeg`): `quill::video_decode` decodes on its own
//! thread, pictures come out as BGRA images, and the soundtrack plays
//! through the app's audio output with the sound as master clock.
//!
//! Pictures are taken for the predicted present time of the frame being
//! drawn (`Player::take_frame_at`), and the front queued picture's due
//! time lets the frame clock sleep until just before it. Every new picture
//! becomes a new `RenderImage`; the one it replaces is retired to
//! `image_budget`, which drops it from the GPU atlas once no frame shows it.
#![deny(clippy::indexing_slicing)]

use super::transport::BackendStatus;
use super::{Backend, ErrorKind, Picture, PlayerError, Present, Pulled, Purpose};
use quill::video_decode::{OpenOptions, Player, VideoFrame};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use super::super::audio::StreamSound;
use gpui_kit::RenderImage;

/// macOS development switch: `QUILL_VIDEO_BACKEND=ffmpeg` plays through the
/// Linux/Windows decoder (built with `scripts/build-ffmpeg.sh`) instead of
/// AVPlayer.
#[cfg(target_os = "macos")]
pub(super) fn preferred_on_macos() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| {
        std::env::var("QUILL_VIDEO_BACKEND").is_ok_and(|v| v.eq_ignore_ascii_case("ffmpeg"))
            && quill::video_decode::available()
    })
}

#[cfg(not(target_os = "macos"))]
pub(super) fn preferred_on_macos() -> bool {
    false
}

fn options(purpose: Purpose) -> OpenOptions {
    match purpose {
        Purpose::Viewer => OpenOptions::viewer(),
        Purpose::ViewerLoop => OpenOptions {
            want_audio: false,
            ..OpenOptions::viewer()
        },
        Purpose::Inline { max_edge } => OpenOptions::inline(max_edge.max(16)),
        Purpose::Story => OpenOptions::viewer(),
    }
}

pub(super) struct FfmpegBackend {
    path: PathBuf,
    options: OpenOptions,
    player: Player,
    sound: Option<StreamSound>,
    volume: f32,
    rate: f64,
    playing: bool,
    image: Option<Arc<RenderImage>>,
    /// A picture stepped to while paused, shown on the next pull.
    pending_step: Option<VideoFrame>,
}

impl FfmpegBackend {
    /// Returns at once: the file opens on the decode thread.
    pub(super) fn open(path: &Path, purpose: Purpose) -> Result<Self, PlayerError> {
        let options = options(purpose);
        let player = Player::open(path, options.clone())
            .map_err(|err| PlayerError::new(ErrorKind::Unsupported, err))?;
        Ok(Self {
            path: path.to_path_buf(),
            options,
            player,
            sound: None,
            volume: 1.0,
            rate: 1.0,
            playing: false,
            image: None,
            pending_step: None,
        })
    }

    /// Connect the soundtrack once the decoder found one.
    fn attach_sound(&mut self) {
        if self.sound.is_some() {
            return;
        }
        if let Some(tap) = self.player.take_audio_tap() {
            self.sound = StreamSound::start(tap, self.volume, self.rate, self.playing);
        }
    }

    fn show(&mut self, frame: VideoFrame) -> Option<Picture> {
        let image = render_image(frame)?;
        if let Some(old) = self.image.replace(image.clone()) {
            super::super::image_budget::retire_all([old]);
        }
        Some(Picture::Image(image))
    }
}

impl Backend for FfmpegBackend {
    fn status(&mut self) -> BackendStatus {
        self.attach_sound();
        let info = self.player.info();
        let duration = self.player.duration();
        let running = self.player.is_playing();
        BackendStatus {
            ready: info.is_some(),
            failed: self.player.error().is_some(),
            position: self.player.position(),
            duration,
            stalled: self.player.starved(),
            // The decoder stopped its clock at the end while play was on.
            at_end: self.playing && !running,
            // A local file: everything is there once it is open.
            buffered: duration.map(|end| vec![0.0..end]).unwrap_or_default(),
            has_sound: self.sound.is_some(),
        }
    }

    fn play(&mut self, rate: f64) {
        self.set_rate(rate, true);
        self.player.play();
        self.playing = true;
        self.attach_sound();
        if let Some(sound) = &self.sound {
            sound.play();
        }
    }

    fn pause(&mut self) {
        self.player.pause();
        self.playing = false;
        if let Some(sound) = &self.sound {
            sound.pause();
        }
    }

    fn seek(&mut self, secs: f64) {
        self.player.seek(secs);
        if let Some(sound) = &self.sound {
            sound.flush();
        }
    }

    /// A clip opened muted reopens with its soundtrack the first time it
    /// is turned up, where it was.
    fn set_volume(&mut self, volume: f32) {
        self.volume = volume.clamp(0.0, 1.0);
        if self.volume > 0.0 && !self.options.want_audio {
            self.options.want_audio = true;
            let position = self.player.position();
            if let Ok(mut player) = Player::open(&self.path, self.options.clone()) {
                player.set_rate(self.rate);
                player.seek(position);
                if self.playing {
                    player.play();
                }
                self.player = player;
                self.sound = None;
            }
        }
        if let Some(sound) = &self.sound {
            sound.set_volume(self.volume);
        }
    }

    fn set_rate(&mut self, rate: f64, _playing: bool) {
        self.rate = rate;
        self.player.set_rate(rate);
        if let Some(sound) = &self.sound {
            sound.set_speed(rate);
        }
    }

    fn step_frame(&mut self, forward: bool) {
        if forward {
            if let Some(frame) = self.player.step_forward() {
                self.pending_step = Some(frame);
            }
        } else {
            self.player.step_back();
        }
    }

    fn take_picture(&mut self, present: &Present) -> Pulled {
        self.attach_sound();
        let item_secs = self.player.position();
        let frame = match self.pending_step.take() {
            Some(frame) => Some(frame),
            None => self.player.take_frame_at(present.at),
        };
        let display_secs = frame.as_ref().map(|frame| frame.pts);
        let picture = frame.and_then(|frame| self.show(frame));
        Pulled {
            picture,
            item_secs,
            display_secs,
        }
    }

    fn next_due(&self, now: Instant, _interval: std::time::Duration) -> Option<Instant> {
        self.player.next_due(now)
    }

    fn error(&self) -> Option<PlayerError> {
        self.player
            .error()
            .map(|err| PlayerError::new(ErrorKind::Decode, err))
    }
}

impl Drop for FfmpegBackend {
    fn drop(&mut self) {
        if let Some(old) = self.image.take() {
            super::super::image_budget::retire_all([old]);
        }
    }
}

/// A decoded BGRA picture as a GPUI image (no copy: the buffer moves in).
pub(super) fn render_image(frame: VideoFrame) -> Option<Arc<RenderImage>> {
    let buffer = image::RgbaImage::from_raw(frame.width, frame.height, frame.bgra)?;
    Some(Arc::new(RenderImage::new(smallvec::SmallVec::from_buf([
        image::Frame::new(buffer),
    ]))))
}

/// Open `path`, seek to `at`, and wait (at most two seconds) for the first
/// picture there: the still a screenshot capture shows.
#[cfg(feature = "demo-capture")]
pub(super) fn first_picture(path: &Path, at: f64) -> Option<(VideoFrame, Option<f64>)> {
    let mut player = Player::open(path, OpenOptions::viewer()).ok()?;
    let deadline = Instant::now() + std::time::Duration::from_secs(2);
    while player.info().is_none() && player.error().is_none() && Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    if at > 0.0 {
        player.seek(at);
    }
    while Instant::now() < deadline {
        if let Some(frame) = player.take_frame() {
            return Some((frame, player.duration()));
        }
        if player.error().is_some() {
            return None;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    None
}
