//! Video playback for the inline history tiles and the media viewer, one
//! API over two backends:
//!
//! - macOS: `AVPlayer` decodes (hardware, with audio and A/V sync), an
//!   `AVPlayerItemVideoOutput` hands back the current frame as a
//!   `CVPixelBuffer`, and GPUI's `surface` element draws it straight from
//!   the GPU ([`VideoPicture::Surface`]).
//! - Linux and Windows: the bundled FFmpeg through `quill::video_decode`
//!   (`ffmpeg_video`), pictures as BGRA images ([`VideoPicture::Image`]),
//!   sound through the app's audio output with the sound as master clock.
//!   macOS development builds can try it with `QUILL_VIDEO_BACKEND=ffmpeg`.
//!
//! The UI pulls a frame each animation frame while playing
//! ([`NativeVideo::frame`]).

use std::path::Path;
use std::sync::Arc;

use gpui_kit::*;

pub(super) use super::ffmpeg_video::Purpose;

/// Whether this platform can play clips in-process right now: always on
/// macOS; elsewhere once the bundled FFmpeg (`quillvideo`) loads. Without
/// it the viewer keeps the ffmpeg-binary frame path and tiles stay still.
pub(super) fn supported() -> bool {
    cfg!(target_os = "macos") || quill::video_decode::available()
}

/// macOS development switch: `QUILL_VIDEO_BACKEND=ffmpeg` plays through the
/// Linux/Windows decoder (built with `scripts/build-ffmpeg.sh`) instead of
/// AVPlayer, to check that path on a Mac.
#[cfg(target_os = "macos")]
fn ffmpeg_on_macos() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| {
        std::env::var("QUILL_VIDEO_BACKEND").is_ok_and(|v| v.eq_ignore_ascii_case("ffmpeg"))
            && quill::video_decode::available()
    })
}

/// A frame to draw.
#[derive(Clone)]
pub(super) enum VideoPicture {
    /// A GPU pixel buffer (AVPlayer), drawn by GPUI's `surface`.
    #[cfg(target_os = "macos")]
    Surface(core_video::pixel_buffer::CVPixelBuffer),
    /// A decoded BGRA image (FFmpeg), drawn by `img`.
    Image(Arc<RenderImage>),
}

impl VideoPicture {
    /// Whether the picture can be clipped by its element (rounded corners,
    /// a circle) — images can; a native surface needs a mask over it.
    pub(super) fn clips(&self) -> bool {
        matches!(self, VideoPicture::Image(_))
    }

    /// The picture as an element of `w` x `h` with `fit`, its corners
    /// rounded by `radii` (images only; surfaces ignore it).
    pub(super) fn element(
        self,
        w: Pixels,
        h: Pixels,
        fit: ObjectFit,
        radii: Corners<Pixels>,
    ) -> AnyElement {
        match self {
            #[cfg(target_os = "macos")]
            VideoPicture::Surface(buffer) => gpui_kit::surface(buffer)
                .w(w)
                .h(h)
                .object_fit(fit)
                .into_any_element(),
            VideoPicture::Image(image) => img(ImageSource::Render(image))
                .w(w)
                .h(h)
                .object_fit(fit)
                .rounded_tl(radii.top_left)
                .rounded_tr(radii.top_right)
                .rounded_br(radii.bottom_right)
                .rounded_bl(radii.bottom_left)
                .into_any_element(),
        }
    }
}

enum Backend {
    #[cfg(target_os = "macos")]
    AvPlayer(avplayer::AvPlayerVideo),
    Ffmpeg(super::ffmpeg_video::FfmpegVideo),
}

pub(super) struct NativeVideo(Backend);

impl NativeVideo {
    /// Open a local file; playback starts paused. Must run on the main
    /// thread (AVPlayer is main-thread-only).
    pub(super) fn open(path: &Path, purpose: Purpose) -> Result<Self, String> {
        #[cfg(target_os = "macos")]
        if !ffmpeg_on_macos() {
            let _ = purpose;
            return avplayer::AvPlayerVideo::open(path).map(|v| Self(Backend::AvPlayer(v)));
        }
        super::ffmpeg_video::FfmpegVideo::open(path, purpose).map(|v| Self(Backend::Ffmpeg(v)))
    }

    pub(super) fn play(&mut self) {
        match &mut self.0 {
            #[cfg(target_os = "macos")]
            Backend::AvPlayer(v) => v.play(),
            Backend::Ffmpeg(v) => v.play(),
        }
    }

    pub(super) fn pause(&mut self) {
        match &mut self.0 {
            #[cfg(target_os = "macos")]
            Backend::AvPlayer(v) => v.pause(),
            Backend::Ffmpeg(v) => v.pause(),
        }
    }

    /// Whether playback is running (it stops by itself at the end).
    pub(super) fn is_playing(&mut self) -> bool {
        match &mut self.0 {
            #[cfg(target_os = "macos")]
            Backend::AvPlayer(v) => v.is_playing(),
            Backend::Ffmpeg(v) => v.is_playing(),
        }
    }

    /// Seek precisely to `secs` (clamped to the clip).
    pub(super) fn seek(&mut self, secs: f64) {
        match &mut self.0 {
            #[cfg(target_os = "macos")]
            Backend::AvPlayer(v) => v.seek(secs),
            Backend::Ffmpeg(v) => v.seek(secs),
        }
    }

    /// 0.0 (muted) – 1.0.
    pub(super) fn set_volume(&mut self, volume: f32) {
        match &mut self.0 {
            #[cfg(target_os = "macos")]
            Backend::AvPlayer(v) => v.set_volume(volume),
            Backend::Ffmpeg(v) => v.set_volume(volume),
        }
    }

    /// Playback speed while playing (1.0 = normal).
    pub(super) fn set_rate(&mut self, rate: f32) {
        match &mut self.0 {
            #[cfg(target_os = "macos")]
            Backend::AvPlayer(v) => v.set_rate(rate),
            Backend::Ffmpeg(v) => v.set_rate(rate),
        }
    }

    pub(super) fn position_secs(&self) -> f64 {
        match &self.0 {
            #[cfg(target_os = "macos")]
            Backend::AvPlayer(v) => v.position_secs(),
            Backend::Ffmpeg(v) => v.position_secs(),
        }
    }

    /// `None` until the clip has loaded its duration.
    pub(super) fn duration_secs(&self) -> Option<f64> {
        match &self.0 {
            #[cfg(target_os = "macos")]
            Backend::AvPlayer(v) => v.duration_secs(),
            Backend::Ffmpeg(v) => v.duration_secs(),
        }
    }

    /// Why the file can't play, once the backend has decided.
    pub(super) fn error(&self) -> Option<String> {
        match &self.0 {
            #[cfg(target_os = "macos")]
            Backend::AvPlayer(v) => v.error(),
            Backend::Ffmpeg(v) => v.error(),
        }
    }

    /// The frame to show now: a new one when there is one, else the last.
    pub(super) fn frame(&mut self) -> Option<VideoPicture> {
        match &mut self.0 {
            #[cfg(target_os = "macos")]
            Backend::AvPlayer(v) => v.frame().map(VideoPicture::Surface),
            Backend::Ffmpeg(v) => v.frame().map(VideoPicture::Image),
        }
    }
}

#[cfg(target_os = "macos")]
mod avplayer {
    use core_video::pixel_buffer::CVPixelBuffer as GpuiPixelBuffer;
    use objc2::rc::Retained;
    use objc2::runtime::AnyObject;
    use objc2::{AnyThread, MainThreadMarker};
    use objc2_av_foundation::{
        AVPlayer, AVPlayerActionAtItemEnd, AVPlayerItem, AVPlayerItemStatus,
        AVPlayerItemVideoOutput,
    };
    use objc2_core_media::CMTime;
    use objc2_core_video::{
        kCVPixelBufferPixelFormatTypeKey, kCVPixelFormatType_420YpCbCr8BiPlanarFullRange,
    };
    use objc2_foundation::{NSDictionary, NSNumber, NSString, NSURL};
    use std::path::Path;

    /// Timescale for seeks (CMTime units per second).
    const SEEK_TIMESCALE: i32 = 600;

    pub(super) struct AvPlayerVideo {
        player: Retained<AVPlayer>,
        item: Retained<AVPlayerItem>,
        output: Retained<AVPlayerItemVideoOutput>,
        /// The last frame handed out, kept so a paused or between-frames
        /// render still has a picture.
        frame: Option<GpuiPixelBuffer>,
        playing: bool,
    }

    impl AvPlayerVideo {
        pub(super) fn open(path: &Path) -> Result<Self, String> {
            let mtm =
                MainThreadMarker::new().ok_or("video playback must start on the main thread")?;
            let path = path.to_str().ok_or("video path is not valid UTF-8")?;
            // SAFETY: plain AVFoundation object construction on the main thread;
            // every argument is a valid, retained Objective-C object.
            unsafe {
                let url = NSURL::fileURLWithPath(&NSString::from_str(path));
                let item = AVPlayerItem::playerItemWithURL(&url, mtm);
                // GPUI's Metal surface path draws bi-planar 4:2:0 YUV.
                let key: &NSString =
                    &*(kCVPixelBufferPixelFormatTypeKey as *const _ as *const NSString);
                let format = NSNumber::new_u32(kCVPixelFormatType_420YpCbCr8BiPlanarFullRange);
                let attributes: Retained<NSDictionary<NSString, AnyObject>> =
                    NSDictionary::from_slices(&[key], &[&*format as &AnyObject]);
                let output = AVPlayerItemVideoOutput::initWithPixelBufferAttributes(
                    AVPlayerItemVideoOutput::alloc(),
                    Some(&attributes),
                );
                item.addOutput(&output);
                let player = AVPlayer::playerWithPlayerItem(Some(&item), mtm);
                player.setActionAtItemEnd(AVPlayerActionAtItemEnd::Pause);
                Ok(Self {
                    player,
                    item,
                    output,
                    frame: None,
                    playing: false,
                })
            }
        }

        pub(super) fn play(&mut self) {
            if self.ended() {
                self.seek(0.0);
            }
            // SAFETY: messaging a live AVPlayer on the main thread.
            unsafe { self.player.play() };
            self.playing = true;
        }

        pub(super) fn pause(&mut self) {
            // SAFETY: as above.
            unsafe { self.player.pause() };
            self.playing = false;
        }

        pub(super) fn is_playing(&mut self) -> bool {
            if self.playing && self.ended() {
                self.playing = false;
            }
            self.playing
        }

        pub(super) fn seek(&mut self, secs: f64) {
            let target = secs.clamp(0.0, self.duration_secs().unwrap_or(f64::MAX));
            // SAFETY: zero tolerances request a frame-accurate seek.
            unsafe {
                let time = CMTime::with_seconds(target, SEEK_TIMESCALE);
                let zero = CMTime::with_seconds(0.0, SEEK_TIMESCALE);
                self.player
                    .seekToTime_toleranceBefore_toleranceAfter(time, zero, zero);
            }
        }

        pub(super) fn set_volume(&self, volume: f32) {
            // SAFETY: as above.
            unsafe { self.player.setVolume(volume.clamp(0.0, 1.0)) };
        }

        pub(super) fn set_rate(&mut self, rate: f32) {
            if self.playing {
                // SAFETY: as above.
                unsafe { self.player.setRate(rate) };
            }
        }

        pub(super) fn position_secs(&self) -> f64 {
            // SAFETY: reading the clock of a live player.
            let secs = unsafe { self.player.currentTime().seconds() };
            if secs.is_finite() { secs.max(0.0) } else { 0.0 }
        }

        pub(super) fn duration_secs(&self) -> Option<f64> {
            // SAFETY: as above.
            let secs = unsafe { self.item.duration().seconds() };
            (secs.is_finite() && secs > 0.0).then_some(secs)
        }

        fn ended(&self) -> bool {
            self.duration_secs()
                .is_some_and(|total| self.position_secs() >= total - 0.05)
        }

        pub(super) fn error(&self) -> Option<String> {
            // SAFETY: reading the item's status and error.
            unsafe {
                (self.item.status() == AVPlayerItemStatus::Failed).then(|| {
                    self.item
                        .error()
                        .map(|err| err.localizedDescription().to_string())
                        .unwrap_or_else(|| "This video can't be played.".to_string())
                })
            }
        }

        pub(super) fn frame(&mut self) -> Option<GpuiPixelBuffer> {
            // SAFETY: the copy returns a +1 CVPixelBuffer that ownership moves
            // into the core-video wrapper GPUI draws (`wrap_under_create_rule`).
            unsafe {
                let now = self.item.currentTime();
                if self.output.hasNewPixelBufferForItemTime(now)
                    && let Some(buffer) = self
                        .output
                        .copyPixelBufferForItemTime_itemTimeForDisplay(now, std::ptr::null_mut())
                {
                    let raw = Retained::into_raw(buffer);
                    self.frame = Some(core_foundation_wrap(raw.cast()));
                }
            }
            self.frame.clone()
        }
    }

    impl Drop for AvPlayerVideo {
        fn drop(&mut self) {
            // SAFETY: stop audio before the player is released.
            unsafe { self.player.pause() };
        }
    }

    /// Take ownership of a +1 `CVPixelBufferRef` as GPUI's wrapper type.
    unsafe fn core_foundation_wrap(raw: *mut std::ffi::c_void) -> GpuiPixelBuffer {
        use core_foundation::base::TCFType;
        // SAFETY: the caller passes an owned (+1) CVPixelBufferRef.
        unsafe { GpuiPixelBuffer::wrap_under_create_rule(raw.cast()) }
    }
}
