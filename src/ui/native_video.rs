//! Native video playback on macOS: `AVPlayer` decodes (hardware, with
//! audio and A/V sync), an `AVPlayerItemVideoOutput` hands back the
//! current frame as a `CVPixelBuffer`, and GPUI's `surface` element draws
//! it straight from the GPU. This replaces the old ffmpeg PNG-frame
//! extraction plus its separate audio process.
//!
//! The UI pulls a frame each animation frame while playing
//! ([`NativeVideo::frame`]); nothing is decoded ahead of time.

pub(super) use imp::NativeVideo;

/// Whether this platform has the native player; elsewhere the viewer
/// keeps the ffmpeg frame path.
pub(super) const SUPPORTED: bool = cfg!(target_os = "macos");

#[cfg(target_os = "macos")]
mod imp {
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

    pub(in crate::ui) struct NativeVideo {
        player: Retained<AVPlayer>,
        item: Retained<AVPlayerItem>,
        output: Retained<AVPlayerItemVideoOutput>,
        /// The last frame handed out, kept so a paused or between-frames
        /// render still has a picture.
        frame: Option<GpuiPixelBuffer>,
        playing: bool,
    }

    impl NativeVideo {
        /// Open a local file; playback starts paused. Must run on the main
        /// thread (AVPlayer is main-thread-only).
        pub(in crate::ui) fn open(path: &Path) -> Result<Self, String> {
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

        pub(in crate::ui) fn play(&mut self) {
            if self.ended() {
                self.seek(0.0);
            }
            // SAFETY: messaging a live AVPlayer on the main thread.
            unsafe { self.player.play() };
            self.playing = true;
        }

        pub(in crate::ui) fn pause(&mut self) {
            // SAFETY: as above.
            unsafe { self.player.pause() };
            self.playing = false;
        }

        /// Whether playback is running (it stops by itself at the end).
        pub(in crate::ui) fn is_playing(&mut self) -> bool {
            if self.playing && self.ended() {
                self.playing = false;
            }
            self.playing
        }

        /// Seek precisely to `secs` (clamped to the clip).
        pub(in crate::ui) fn seek(&mut self, secs: f64) {
            let target = secs.clamp(0.0, self.duration_secs().unwrap_or(f64::MAX));
            // SAFETY: zero tolerances request a frame-accurate seek.
            unsafe {
                let time = CMTime::with_seconds(target, SEEK_TIMESCALE);
                let zero = CMTime::with_seconds(0.0, SEEK_TIMESCALE);
                self.player
                    .seekToTime_toleranceBefore_toleranceAfter(time, zero, zero);
            }
        }

        /// 0.0 (muted) – 1.0.
        pub(in crate::ui) fn set_volume(&self, volume: f32) {
            // SAFETY: as above.
            unsafe { self.player.setVolume(volume.clamp(0.0, 1.0)) };
        }

        /// Playback speed while playing (1.0 = normal).
        pub(in crate::ui) fn set_rate(&mut self, rate: f32) {
            if self.playing {
                // SAFETY: as above.
                unsafe { self.player.setRate(rate) };
            }
        }

        pub(in crate::ui) fn position_secs(&self) -> f64 {
            // SAFETY: reading the clock of a live player.
            let secs = unsafe { self.player.currentTime().seconds() };
            if secs.is_finite() { secs.max(0.0) } else { 0.0 }
        }

        /// `None` until the asset has loaded its duration.
        pub(in crate::ui) fn duration_secs(&self) -> Option<f64> {
            // SAFETY: as above.
            let secs = unsafe { self.item.duration().seconds() };
            (secs.is_finite() && secs > 0.0).then_some(secs)
        }

        fn ended(&self) -> bool {
            self.duration_secs()
                .is_some_and(|total| self.position_secs() >= total - 0.05)
        }

        /// Why the file can't play, once AVFoundation has decided.
        pub(in crate::ui) fn error(&self) -> Option<String> {
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

        /// The frame to show now: a new one when the output has it, else the
        /// last one.
        pub(in crate::ui) fn frame(&mut self) -> Option<GpuiPixelBuffer> {
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

    impl Drop for NativeVideo {
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

#[cfg(not(target_os = "macos"))]
mod imp {
    use std::path::Path;

    /// No native player on this platform yet.
    pub(in crate::ui) struct NativeVideo;

    impl NativeVideo {
        pub(in crate::ui) fn open(_path: &Path) -> Result<Self, String> {
            Err("native video playback is not available on this platform".into())
        }
        pub(in crate::ui) fn play(&mut self) {}
        pub(in crate::ui) fn pause(&mut self) {}
        pub(in crate::ui) fn is_playing(&mut self) -> bool {
            false
        }
        pub(in crate::ui) fn seek(&mut self, _secs: f64) {}
        pub(in crate::ui) fn set_volume(&self, _volume: f32) {}
        pub(in crate::ui) fn set_rate(&mut self, _rate: f32) {}
        pub(in crate::ui) fn position_secs(&self) -> f64 {
            0.0
        }
        pub(in crate::ui) fn error(&self) -> Option<String> {
            None
        }
    }
}
