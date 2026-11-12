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
use super::transport::{
    BackendStatus, Correction, Engine, EngineHealth, RATE_TOLERANCE, Want, reconcile,
};
use super::{Backend, ErrorKind, Picture, PictureStats, PlayerError, Present, Pulled, Purpose};
use quill::video_decode::{BgraPool, OpenOptions, PixelOutput, Pixels, Player, VideoFrame};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
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

/// The picture last handed out: one value, so an image and a surface are
/// never both held.
enum OnScreen {
    Nothing,
    Image(Arc<RenderImage>),
    /// An NV12 surface, to show as an image instead if the renderer
    /// refuses it. Never made on macOS.
    #[cfg_attr(target_os = "macos", allow(dead_code))]
    Surface(Picture),
}

pub(super) struct FfmpegBackend {
    path: PathBuf,
    options: OpenOptions,
    player: Player,
    sound: Option<StreamSound>,
    volume: f32,
    /// The transport's intent as last synced (its rate is the decoder's).
    want: Want,
    on_screen: OnScreen,
    /// The layout last asked of the decoder.
    output: PixelOutput,
    /// Images only until then: the window refused a surface.
    nv12_paused_until: Option<Instant>,
    /// A picture stepped to while paused, shown on the next pull.
    pending_step: Option<VideoFrame>,
    /// The wake the player asked for (`on_open`), until the open it waits
    /// for settles: every player the backend opens forwards to it, so a
    /// volume reopen neither fires it early nor loses it.
    open_wake: OpenWake,
    /// What the next reopen runs over instead of the file (tests).
    #[cfg(test)]
    reopen_with: Option<Box<dyn quill::video_decode::Demuxer>>,
}

/// The wake for the clip's open, shared by the players the backend opens.
type OpenWake = Arc<Mutex<Option<Box<dyn FnOnce() + Send>>>>;

/// Runs the wake in `slot` once, if it is still there.
fn forward_open(slot: &OpenWake) -> impl FnOnce() + Send + 'static {
    let slot = slot.clone();
    move || {
        let wake = slot.lock().ok().and_then(|mut wake| wake.take());
        if let Some(wake) = wake {
            wake();
        }
    }
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
            on_screen: OnScreen::Nothing,
            nv12_paused_until: None,
            pending_step: None,
            open_wake: OpenWake::default(),
            #[cfg(test)]
            reopen_with: None,
        }
    }

    /// Open the file again for `options` on a new player.
    fn open_player(&mut self, options: &OpenOptions) -> Result<Player, String> {
        #[cfg(test)]
        if let Some(demuxer) = self.reopen_with.take() {
            return Player::with_demuxer(demuxer, options.clone());
        }
        Player::open(&self.path, options.clone())
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
        // The first tap, or (once a device is up again) a new one for a
        // sound whose tap was let go: a device that was missing at attach
        // or lost mid-clip gives the clip its soundtrack back. Only a let-go
        // sound asks for the device, so a silent clip never opens it, and
        // only a device that is up (not one still opening, which may fail)
        // takes the reattach, which restarts the decoder.
        let tap = match self.player.take_audio_tap() {
            Some(tap) => Some(tap),
            None if self.player.sound_detached() && crate::ui::audio::video_output_ready() => {
                self.player.reattach_audio_tap()
            }
            None => None,
        };
        if let Some(tap) = tap {
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
        let image = render_image(frame, self.player.bgra_pool())?;
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
        self.retire_image(OnScreen::Image(image));
    }

    /// Show `next`; an image it replaces goes to the budget.
    fn retire_image(&mut self, next: OnScreen) {
        if let OnScreen::Image(old) = std::mem::replace(&mut self.on_screen, next) {
            super::super::image_budget::retire_all([old]);
        }
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
        let picture = Picture::Surface(surface);
        self.retire_image(OnScreen::Surface(picture.clone()));
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
    fn on_open(&mut self, wake: Box<dyn FnOnce() + Send>) -> bool {
        if let Ok(mut slot) = self.open_wake.lock() {
            *slot = Some(wake);
        }
        self.player.on_open(forward_open(&self.open_wake));
        true
    }

    fn status(&mut self) -> BackendStatus {
        self.attach_sound();
        let info = self.player.info();
        let duration = self.player.duration();
        BackendStatus {
            health: EngineHealth::read(
                info.is_some(),
                self.player.error().is_some(),
                self.player.starved(),
                self.want.playing && self.player.ended(),
            ),
            position: self.player.position(),
            duration,
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
        // A picture stepped to before the seek is not the one to show next.
        self.pending_step = None;
        self.player.seek(secs);
        if let Some(sound) = &self.sound {
            sound.flush();
        }
    }

    /// A clip opened muted reopens with its soundtrack the first time it
    /// is turned up, where it was.
    fn set_volume(&mut self, volume: f32) {
        self.volume = volume.clamp(0.0, 1.0);
        // A clip that failed to open stays failed: a reopen would start a
        // decode thread for nothing.
        if self.volume > 0.0 && !self.options.want_audio && self.player.error().is_none() {
            // Sound is wanted only once the reopen started: one that is
            // refused at once leaves the clip as it was, and the next
            // turn-up tries again. The open itself runs on the new player's
            // thread, so a failure there (the file just opened, so a rare
            // one) shows as the clip failing, not as the old player.
            let reopened = OpenOptions {
                want_audio: true,
                output: self.output,
                ..self.options.clone()
            };
            let position = self.player.position();
            let running = self.player.is_playing();
            // The wake moves to a slot of its own before the new player
            // opens: the dropped player may still be opening, and its
            // forwarder then finds nothing to fire early.
            let wake = self.open_wake.lock().ok().and_then(|mut wake| wake.take());
            let fresh: OpenWake = Arc::new(Mutex::new(wake));
            if let Ok(mut player) = self.open_player(&reopened) {
                player.set_rate(self.want.rate);
                player.seek(position);
                if running {
                    player.play();
                }
                // The new player says when it opened, for a wake still
                // waiting on the clip's open.
                self.open_wake = fresh;
                player.on_open(forward_open(&self.open_wake));
                self.options = reopened;
                self.player = player;
                self.sound = None;
                self.pending_step = None;
            } else if let Some(wake) = fresh.lock().ok().and_then(|mut wake| wake.take())
                && let Ok(mut slot) = self.open_wake.lock()
            {
                // Refused: the old player's open still settles it.
                slot.get_or_insert(wake);
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
            self.pending_step = None;
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
        let OnScreen::Surface(surface) = &self.on_screen else {
            return None;
        };
        let image = surface.snapshot()?;
        self.show_image(image.clone());
        Some(Picture::Image(image))
    }
}

impl Drop for FfmpegBackend {
    fn drop(&mut self) {
        self.retire_image(OnScreen::Nothing);
    }
}

/// A decoded picture as a GPUI image (no copy for BGRA: the buffer moves
/// in; NV12 is converted on the CPU first). The buffer goes back to `pool`
/// when the image is dropped.
pub(super) fn render_image(frame: VideoFrame, pool: &BgraPool) -> Option<Arc<RenderImage>> {
    let frame = frame.into_bgra(pool)?;
    let Pixels::Bgra(bgra) = frame.pixels else {
        return None;
    };
    let buffer = image::RgbaImage::from_raw(frame.width, frame.height, bgra)?;
    let pool = pool.clone();
    let image = RenderImage::new(smallvec::SmallVec::from_buf([image::Frame::new(buffer)]))
        .with_recycler(move |frames| {
            for frame in frames {
                pool.give(frame.into_buffer().into_raw());
            }
        });
    Some(Arc::new(image))
}

/// Open `path`, seek to `at`, and wait (at most two seconds) for the first
/// picture there: the still a screenshot capture shows.
#[cfg(feature = "demo-capture")]
pub(super) fn first_picture(path: &Path, at: f64) -> Option<(VideoFrame, Option<f64>)> {
    let deadline = Instant::now() + std::time::Duration::from_secs(2);
    let mut player = Player::open(path, OpenOptions::viewer()).ok()?;
    player.wait_open(deadline);
    if at > 0.0 {
        player.seek(at);
    }
    player.wait_frame(deadline);
    let frame = player.take_frame()?;
    Some((frame, player.duration()))
}

#[cfg(test)]
mod tests {
    use super::super::Backend;
    use super::{FfmpegBackend, NV12_RETRY, nv12_allowed, output_for, render_image};
    use crate::ui::audio::{SharedOutput, share_output_with_video};
    use quill::video_decode::{
        AudioChunk, BgraPool, Demuxer, Item, MediaInfo, OpenOptions, PixelOutput, Player,
        VideoFrame,
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

    /// A clip whose `info` waits for a signal, so its open can be held.
    struct Gated {
        opening: mpsc::Receiver<()>,
    }

    impl Demuxer for Gated {
        fn info(&self) -> MediaInfo {
            let _ = self.opening.recv();
            MediaInfo {
                frame_rate: 25.0,
                has_video: true,
                ..MediaInfo::default()
            }
        }

        fn next(&mut self) -> Result<Option<Item>, String> {
            Ok(None)
        }

        fn seek(&mut self, _secs: f64) -> Result<(), String> {
            Ok(())
        }
    }

    /// A volume reopen while the first player is still opening: the open
    /// wake is not fired by the dropped player's open, only by the new
    /// player's, which is the open that is drawn.
    #[test]
    fn a_reopen_for_volume_mid_open_fires_the_wake_for_the_new_players_open_only() {
        let options = OpenOptions {
            want_audio: false,
            ..OpenOptions::viewer()
        };
        let (release_first, first_gate) = mpsc::channel();
        let first = Player::with_demuxer(
            Box::new(Gated {
                opening: first_gate,
            }),
            options.clone(),
        )
        .unwrap();
        let mut backend = FfmpegBackend::with_player(Path::new("gated"), options, first);
        let (release_second, second_gate) = mpsc::channel();
        backend.reopen_with = Some(Box::new(Gated {
            opening: second_gate,
        }));
        let (woken, heard) = mpsc::channel();
        assert!(backend.on_open(Box::new(move || {
            let _ = woken.send(());
        })));
        // Runs right after the forwarder on the first player's open.
        let (first_opened, first_open_done) = mpsc::channel();
        backend.player.on_open(move || {
            let _ = first_opened.send(());
        });
        backend.set_volume(0.5);
        assert!(backend.options.want_audio, "the reopen started");
        let _ = release_first.send(());
        assert!(first_open_done.recv().is_ok(), "the first open settled");
        assert!(
            heard.try_recv().is_err(),
            "the dropped player's open woke it"
        );
        let _ = release_second.send(());
        assert!(heard.recv().is_ok(), "the new player's open wakes it");
        assert!(heard.try_recv().is_err(), "once");
    }

    /// A clip whose decoding failed.
    struct Broken;

    impl Demuxer for Broken {
        fn info(&self) -> MediaInfo {
            MediaInfo {
                frame_rate: 25.0,
                has_video: true,
                ..MediaInfo::default()
            }
        }

        fn next(&mut self) -> Result<Option<Item>, String> {
            Err("broken".into())
        }

        fn seek(&mut self, _secs: f64) -> Result<(), String> {
            Ok(())
        }
    }

    /// A clip that failed is not reopened for its volume.
    #[test]
    fn a_failed_clip_is_not_reopened_for_its_volume() {
        let options = OpenOptions {
            want_audio: false,
            ..OpenOptions::viewer()
        };
        let player = Player::with_demuxer(Box::new(Broken), options.clone()).unwrap();
        // Returns once the decode thread hit the error.
        player.wait_frame_settled();
        assert!(player.error().is_some());
        let mut backend = FfmpegBackend::with_player(Path::new("broken"), options, player);
        backend.set_volume(0.5);
        assert!(!backend.options.want_audio, "no reopen was started");
    }

    #[test]
    fn a_seek_drops_the_picture_stepped_to_before_it() {
        let options = OpenOptions::viewer();
        let tone = Tone {
            started: None,
            pts: 0.0,
        };
        let player = Player::with_demuxer(Box::new(tone), options.clone()).unwrap();
        let mut backend = FfmpegBackend::with_player(Path::new("tone"), options, player);
        backend.pending_step = Some(VideoFrame::bgra(0.0, 2, 2, vec![0; 16]));
        backend.seek(1.0);
        assert!(backend.pending_step.is_none());
    }

    /// A BGRA picture's buffer goes back to its player once the image
    /// showing it is gone, for the next picture to be turned into.
    #[test]
    fn a_picture_hands_its_buffer_back_once_its_image_goes() {
        let pool = BgraPool::default();
        // The spare byte tells this buffer from a new one.
        let mut bgra = Vec::with_capacity(17);
        bgra.resize(16, 0);
        let image = render_image(VideoFrame::bgra(0.0, 2, 2, bgra), &pool).expect("image");
        assert_eq!(pool.take(16).capacity(), 16, "the image still shows it");
        drop(image);
        assert_eq!(pool.take(16).capacity(), 17);
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
