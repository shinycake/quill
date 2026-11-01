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
//!
//! On Windows and Linux the decoder is asked for NV12 instead
//! ([`surface::NV12_SURFACES`]): pictures the stream allows (8-bit 4:2:0,
//! BT.601/709, even sides) skip the decoder's BGRA conversion and become
//! surface frames the DirectX or wgpu renderer converts on the GPU; the
//! rest stay images. When the window's renderer refuses a surface (its NV12
//! textures or pipeline failed), the player asks [`Backend::replace_refused`]
//! for the same picture as an image, and this backend shows images for
//! [`NV12_RETRY`]: the decoder goes back to BGRA, NV12 pictures still
//! queued are converted on the CPU. Then it tries NV12 again.
#![deny(clippy::indexing_slicing)]

use super::surface;
use super::transport::{BackendStatus, Correction, Engine, RATE_TOLERANCE, Want, reconcile};
use super::{Backend, ErrorKind, Picture, PictureStats, PlayerError, Present, Pulled, Purpose};
use quill::video_decode::{OpenOptions, PixelOutput, Pixels, Player, VideoFrame};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

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
    // tdesktop decodes in hardware only in the media viewer and its picture
    // in picture, and reads the setting when a clip opens.
    let viewer = || OpenOptions {
        hardware: quill::advanced::hardware_video::enabled(),
        ..OpenOptions::viewer()
    };
    let options = match purpose {
        Purpose::Viewer => viewer(),
        Purpose::ViewerLoop => OpenOptions {
            want_audio: false,
            ..viewer()
        },
        Purpose::Inline { max_edge } => OpenOptions::inline(max_edge.max(16)),
        // Live players never open files.
        Purpose::Story | Purpose::Live { .. } => OpenOptions::viewer(),
    };
    OpenOptions {
        output: output_for(surface::NV12_SURFACES),
        ..options
    }
}

/// How long a backend shows images after its window refused a surface,
/// before it tries NV12 again (the renderer retries textures on about the
/// same schedule).
pub(super) const NV12_RETRY: Duration = Duration::from_secs(2);

/// NV12 again once the pause after a refused surface is over.
pub(super) fn nv12_allowed(paused_until: Option<Instant>, now: Instant) -> bool {
    paused_until.is_none_or(|until| now >= until)
}

fn output_for(nv12: bool) -> PixelOutput {
    if nv12 {
        PixelOutput::Nv12
    } else {
        PixelOutput::Bgra
    }
}

pub(super) struct FfmpegBackend {
    path: PathBuf,
    options: OpenOptions,
    player: Player,
    sound: Option<StreamSound>,
    volume: f32,
    /// The transport's intent as last synced (its rate is the decoder's).
    want: Want,
    image: Option<Arc<RenderImage>>,
    /// The surface picture on screen (NV12), to show as an image
    /// instead if the renderer refuses it.
    surface: Option<Picture>,
    /// The layout last asked of the decoder.
    output: PixelOutput,
    /// Images only until then: the window refused a surface.
    nv12_paused_until: Option<Instant>,
    /// A picture stepped to while paused, shown on the next pull.
    pending_step: Option<VideoFrame>,
}

impl FfmpegBackend {
    /// Returns at once: the file opens on the decode thread.
    pub(super) fn open(path: &Path, purpose: Purpose) -> Result<Self, PlayerError> {
        let options = options(purpose);
        let player = Player::open(path, options.clone())
            .map_err(|err| PlayerError::new(ErrorKind::Unsupported, err))?;
        Ok(Self::with_player(path, options, player))
    }

    fn with_player(path: &Path, options: OpenOptions, player: Player) -> Self {
        Self {
            path: path.to_path_buf(),
            output: options.output,
            options,
            player,
            sound: None,
            volume: 1.0,
            want: Want::default(),
            image: None,
            surface: None,
            nv12_paused_until: None,
            pending_step: None,
        }
    }

    /// Whether to hand over NV12 surfaces now: off macOS, unless the
    /// window refused one less than [`NV12_RETRY`] ago.
    fn nv12_now(&self, now: Instant) -> bool {
        surface::NV12_SURFACES && nv12_allowed(self.nv12_paused_until, now)
    }

    /// Connect the soundtrack once the decoder found one. One whose device
    /// failed to open is let go, as with no device at all: the clip then
    /// runs on its wall clock instead of holding its picture.
    fn attach_sound(&mut self) {
        if self.sound.as_ref().is_some_and(StreamSound::lost) {
            self.sound = None;
        }
        if self.sound.is_some() {
            return;
        }
        if let Some(tap) = self.player.take_audio_tap() {
            let running = self.player.is_playing();
            self.sound = StreamSound::start(tap, self.volume, self.want.rate, running);
        }
    }

    /// Ask the decoder for NV12 or BGRA.
    fn sync_output(&mut self, nv12: bool) {
        let wanted = output_for(nv12);
        if wanted != self.output {
            self.player.set_output(wanted);
            self.output = wanted;
        }
    }

    fn show(&mut self, frame: VideoFrame, now: Instant) -> Option<(Picture, PictureStats)> {
        let nv12 = self.nv12_now(now);
        self.sync_output(nv12);
        let started = Instant::now();
        let (width, height, decoded_us) = (frame.width, frame.height, frame.convert_us);
        let frame = match self.surface_picture(frame, nv12) {
            Ok(shown) => return Some(shown),
            Err(frame) => frame,
        };
        let image = render_image(frame)?;
        self.show_image(image.clone());
        let stats = PictureStats {
            format: "bgra",
            width,
            height,
            bytes: (width as usize)
                .saturating_mul(height as usize)
                .saturating_mul(4),
            // A CPU conversion from NV12 (renderer fallback) counts too.
            convert_us: decoded_us.saturating_add(elapsed_us(started)),
        };
        Some((Picture::Image(image), stats))
    }

    fn show_image(&mut self, image: Arc<RenderImage>) {
        if let Some(old) = self.image.replace(image) {
            super::super::image_budget::retire_all([old]);
        }
        self.surface = None;
    }

    /// An NV12 picture as a surface when `nv12` ([`Self::nv12_now`]); the
    /// picture back otherwise (it becomes an image).
    #[cfg(not(target_os = "macos"))]
    fn surface_picture(
        &mut self,
        frame: VideoFrame,
        nv12: bool,
    ) -> Result<(Picture, PictureStats), VideoFrame> {
        if !nv12 {
            return Err(frame);
        }
        let (width, height, convert_us) = (frame.width, frame.height, frame.convert_us);
        let surface = surface::surface_frame(frame)?;
        let stats = PictureStats {
            format: "nv12",
            width,
            height,
            bytes: surface.bytes().len(),
            convert_us,
        };
        if let Some(old) = self.image.take() {
            super::super::image_budget::retire_all([old]);
        }
        let picture = Picture::Surface(surface);
        self.surface = Some(picture.clone());
        Ok((picture, stats))
    }

    /// On macOS every FFmpeg picture is an image.
    #[cfg(target_os = "macos")]
    fn surface_picture(
        &mut self,
        frame: VideoFrame,
        _nv12: bool,
    ) -> Result<(Picture, PictureStats), VideoFrame> {
        Err(frame)
    }
}

fn elapsed_us(since: Instant) -> u32 {
    u32::try_from(since.elapsed().as_micros()).unwrap_or(u32::MAX)
}

impl Backend for FfmpegBackend {
    fn status(&mut self) -> BackendStatus {
        self.attach_sound();
        let info = self.player.info();
        let duration = self.player.duration();
        BackendStatus {
            ready: info.is_some(),
            failed: self.player.error().is_some(),
            position: self.player.position(),
            duration,
            stalled: self.player.starved(),
            at_end: self.want.playing && self.player.ended(),
            // A local file: everything is there once it is open.
            buffered: duration.map(|end| vec![0.0..end]).unwrap_or_default(),
            has_sound: self.sound.is_some(),
        }
    }

    fn sync(&mut self, want: Want) {
        if (want.rate - self.want.rate).abs() > RATE_TOLERANCE {
            self.player.set_rate(want.rate);
            if let Some(sound) = &self.sound {
                sound.set_speed(want.rate);
            }
        }
        self.want = want;
        let engine = Engine {
            ready: self.player.info().is_some(),
            running: self.player.is_playing(),
            rate: want.rate,
            at_end: self.player.ended(),
            // A starved decoder keeps its clock playing: nothing to restart.
            keeps_up: true,
        };
        match reconcile(want, engine) {
            Some(Correction::Start) => {
                self.player.play();
                self.attach_sound();
                if let Some(sound) = &self.sound {
                    sound.play();
                }
            }
            Some(Correction::Pause) => {
                self.player.pause();
                if let Some(sound) = &self.sound {
                    sound.pause();
                }
            }
            Some(Correction::Rate) | None => {}
        }
        // An ended clip's soundtrack plays silence for ever, which would
        // keep the audio output open while the clip sits on screen.
        if !self.player.is_playing()
            && let Some(sound) = &self.sound
        {
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
            self.options.output = self.output;
            let position = self.player.position();
            let running = self.player.is_playing();
            if let Ok(mut player) = Player::open(&self.path, self.options.clone()) {
                player.set_rate(self.want.rate);
                player.seek(position);
                if running {
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
        let now = Instant::now();
        let (picture, stats) = match frame.and_then(|frame| self.show(frame, now)) {
            Some((picture, stats)) => (Some(picture), Some(stats)),
            None => (None, None),
        };
        Pulled {
            picture,
            item_secs,
            display_secs,
            stats,
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

    /// The window refused the surface on screen: images for [`NV12_RETRY`],
    /// starting with this picture converted on the CPU.
    fn replace_refused(&mut self, now: Instant) -> Option<Picture> {
        self.nv12_paused_until = now.checked_add(NV12_RETRY);
        self.sync_output(false);
        let image = self.surface.take()?.snapshot()?;
        self.show_image(image.clone());
        Some(Picture::Image(image))
    }
}

impl Drop for FfmpegBackend {
    fn drop(&mut self) {
        if let Some(old) = self.image.take() {
            super::super::image_budget::retire_all([old]);
        }
    }
}

/// A decoded picture as a GPUI image (no copy for BGRA: the buffer moves
/// in; NV12 is converted on the CPU first).
pub(super) fn render_image(frame: VideoFrame) -> Option<Arc<RenderImage>> {
    let frame = frame.into_bgra()?;
    let Pixels::Bgra(bgra) = frame.pixels else {
        return None;
    };
    let buffer = image::RgbaImage::from_raw(frame.width, frame.height, bgra)?;
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

#[cfg(test)]
mod tests {
    use super::super::Backend;
    use super::{FfmpegBackend, NV12_RETRY, nv12_allowed, output_for};
    use crate::ui::audio::{SharedOutput, share_output_with_video};
    use quill::video_decode::{
        AudioChunk, Demuxer, Item, MediaInfo, OpenOptions, PixelOutput, Player,
    };
    use std::path::Path;
    use std::sync::mpsc;
    use std::time::Instant;

    /// A clip that is all sound; says when the decoder starts on it (its
    /// sound end, the tap, is made by then).
    struct Tone {
        started: Option<mpsc::Sender<()>>,
        pts: f64,
    }

    impl Demuxer for Tone {
        fn info(&self) -> MediaInfo {
            MediaInfo {
                frame_rate: 25.0,
                has_audio: true,
                sample_rate: 1000,
                channels: 1,
                ..MediaInfo::default()
            }
        }

        fn next(&mut self) -> Result<Option<Item>, String> {
            if let Some(started) = self.started.take() {
                let _ = started.send(());
            }
            let pts = self.pts;
            self.pts += 0.02;
            Ok(Some(Item::Audio(AudioChunk {
                pts,
                samples: vec![0.25; 20],
            })))
        }

        fn seek(&mut self, secs: f64) -> Result<(), String> {
            self.pts = secs;
            Ok(())
        }
    }

    /// While the soundtrack is attached it is the clip's clock. A playing
    /// clip whose device fails to open lets go of it, so the clip runs on
    /// its wall clock instead of holding its first picture.
    #[test]
    fn a_clip_whose_device_fails_to_open_runs_on_its_wall_clock() {
        let (output, gate) = SharedOutput::gated();
        share_output_with_video(&output);
        let (started, decoding) = mpsc::channel();
        let options = OpenOptions {
            want_audio: true,
            ..OpenOptions::viewer()
        };
        let tone = Tone {
            started: Some(started),
            pts: 0.0,
        };
        let player = Player::with_demuxer(Box::new(tone), options.clone()).unwrap();
        let mut backend = FfmpegBackend::with_player(Path::new("tone"), options, player);
        backend.player.play();
        decoding.recv().unwrap();
        assert!(backend.status().has_sound);
        assert!(backend.player.has_sound(), "the sound is the clock");
        gate.answer(false);
        output.settle_device();
        assert!(!backend.status().has_sound);
        assert!(!backend.player.has_sound(), "the wall clock is");
    }

    #[test]
    fn a_refused_surface_pauses_nv12_for_a_while() {
        let now = Instant::now();
        assert!(nv12_allowed(None, now));
        let until = now + NV12_RETRY;
        assert!(!nv12_allowed(Some(until), now));
        assert!(!nv12_allowed(Some(until), now + NV12_RETRY / 2));
        assert!(nv12_allowed(Some(until), until));
        assert_eq!(output_for(true), PixelOutput::Nv12);
        assert_eq!(output_for(false), PixelOutput::Bgra);
    }
}
