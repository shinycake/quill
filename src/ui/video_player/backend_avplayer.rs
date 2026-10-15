//! The AVPlayer backend (macOS): hardware decode with its own sound and
//! A/V sync; an `AVPlayerItemVideoOutput` hands back the picture for an
//! item time as a bi-planar YUV `CVPixelBuffer` that GPUI's `surface`
//! draws straight from the GPU.
//!
//! The item time asked for is the one at the predicted present time of the
//! frame being drawn, converted from the display link's host ticks
//! (`itemTimeForMachAbsoluteTime`), which is how the API is meant to be
//! paced. The item ends paused; looping is the transport's (seek + play).
//! Every call here runs on the main thread (AVFoundation's rule; the
//! player only calls its backend from the UI thread).
#![deny(clippy::indexing_slicing)]

use super::transport::BackendStatus;
use super::{Backend, ErrorKind, Picture, PlayerError, Present, Pulled};
use core_video::pixel_buffer::CVPixelBuffer as GpuiPixelBuffer;
#[cfg(feature = "demo-capture")]
use gpui_kit::RenderImage;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{AnyThread, MainThreadMarker};
#[cfg(feature = "demo-capture")]
use objc2_av_foundation::{AVAssetImageGenerator, AVURLAsset};
use objc2_av_foundation::{
    AVAudioTimePitchAlgorithm, AVAudioTimePitchAlgorithmTimeDomain, AVPlayer,
    AVPlayerActionAtItemEnd, AVPlayerItem, AVPlayerItemStatus, AVPlayerItemVideoOutput,
    NSValueAVFoundationExtensions,
};
use objc2_core_media::CMTime;
use objc2_core_video::{
    kCVPixelBufferPixelFormatTypeKey, kCVPixelFormatType_420YpCbCr8BiPlanarFullRange,
};
use objc2_foundation::{NSDictionary, NSNumber, NSString, NSURL};
use std::ops::Range;
use std::path::Path;
#[cfg(feature = "demo-capture")]
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Timescale for seeks (CMTime units per second).
const SEEK_TIMESCALE: i32 = 600;
/// tdesktop `kBufferFor`: seconds of media kept ahead.
const BUFFER_AHEAD_SECS: f64 = 3.0;
/// The position counts as the end this close to the duration.
const END_SLACK: f64 = 0.05;
/// How many display frames ahead the next picture is looked for.
const PROBE_FRAMES: u32 = 8;
/// The buffer state (`loadedTimeRanges`, buffer empty, likely to keep up)
/// is read this often; position and rate every draw.
const BUFFER_POLL: Duration = Duration::from_millis(100);

pub(super) struct AvPlayerBackend {
    player: Retained<AVPlayer>,
    item: Retained<AVPlayerItem>,
    output: Retained<AVPlayerItemVideoOutput>,
    /// The item's pitch algorithm as opened, restored at normal speed.
    default_pitch: Retained<AVAudioTimePitchAlgorithm>,
    /// Play was asked for (AVPlayer's rate drops to 0 at the end).
    playing: bool,
    rate: f64,
    /// The present time of the last pull.
    last_present: Option<Instant>,
    /// The buffer state as last read, and when.
    buffer: BufferState,
    buffer_read: Option<Instant>,
}

#[derive(Clone, Default)]
struct BufferState {
    empty: bool,
    keeps_up: bool,
    ranges: Vec<Range<f64>>,
}

/// AVPlayer drops its rate to 0 on a stall and, not waiting to minimize
/// stalls, never raises it again: once the buffer keeps up while play is
/// still wanted (and the clip is not at its end), it must be told again.
fn resumes_after_stall(wants_play: bool, rate: f32, at_end: bool, keeps_up: bool) -> bool {
    wants_play && rate == 0.0 && !at_end && keeps_up
}

impl AvPlayerBackend {
    /// Create the item and player; `status` reports `ready` once AVPlayer
    /// says `readyToPlay` (the player polls it while opening).
    pub(super) fn open(path: &Path) -> Result<Self, PlayerError> {
        let mtm = MainThreadMarker::new().ok_or_else(|| {
            PlayerError::new(
                ErrorKind::Open,
                "video playback must start on the main thread",
            )
        })?;
        let path = path
            .to_str()
            .ok_or_else(|| PlayerError::new(ErrorKind::Open, "video path is not valid UTF-8"))?;
        // SAFETY: plain AVFoundation object construction on the main thread
        // (checked above); every argument is a valid, retained object, and
        // the pixel format key is a CoreVideo constant (a CFString, which
        // is toll-free bridged to NSString).
        unsafe {
            let url = NSURL::fileURLWithPath(&NSString::from_str(path));
            let item = AVPlayerItem::playerItemWithURL(&url, mtm);
            item.setPreferredForwardBufferDuration(BUFFER_AHEAD_SECS);
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
            let default_pitch = item.audioTimePitchAlgorithm();
            let player = AVPlayer::playerWithPlayerItem(Some(&item), mtm);
            player.setActionAtItemEnd(AVPlayerActionAtItemEnd::Pause);
            // A local file: start at once instead of waiting to buffer.
            player.setAutomaticallyWaitsToMinimizeStalling(false);
            Ok(Self {
                player,
                item,
                output,
                default_pitch,
                playing: false,
                rate: 1.0,
                last_present: None,
                buffer: BufferState::default(),
                buffer_read: None,
            })
        }
    }

    fn position(&self) -> f64 {
        // SAFETY: reading the clock of a live player on the main thread.
        let secs = unsafe { self.player.currentTime().seconds() };
        if secs.is_finite() { secs.max(0.0) } else { 0.0 }
    }

    fn duration(&self) -> Option<f64> {
        // SAFETY: as above.
        let secs = unsafe { self.item.duration().seconds() };
        (secs.is_finite() && secs > 0.0).then_some(secs)
    }

    /// Time-domain stretching keeps voices intelligible away from 1x.
    fn apply_pitch(&self, rate: f64) {
        // SAFETY: setting a copied string property on a live item; the
        // constant is an AVFoundation global.
        unsafe {
            let algorithm = if (rate - 1.0).abs() > f64::EPSILON {
                AVAudioTimePitchAlgorithmTimeDomain
            } else {
                Some(&*self.default_pitch)
            };
            if let Some(algorithm) = algorithm {
                self.item.setAudioTimePitchAlgorithm(algorithm);
            }
        }
    }

    fn buffered(&self) -> Vec<Range<f64>> {
        // SAFETY: `loadedTimeRanges` holds NSValues wrapping CMTimeRanges.
        let ranges = unsafe { self.item.loadedTimeRanges() };
        ranges
            .iter()
            .map(|value| {
                // SAFETY: each element is a CMTimeRange NSValue (documented).
                let range = unsafe { value.CMTimeRangeValue() };
                // SAFETY: plain CMTime arithmetic.
                let start = unsafe { range.start.seconds() };
                // SAFETY: as above.
                let length = unsafe { range.duration.seconds() };
                start..start + length
            })
            .collect()
    }

    /// The Mach host time at `at` (an `Instant` in the near future).
    fn host_ticks_at(at: Instant) -> i64 {
        let now = Instant::now();
        let now_host = core_video::host_time::get_current_host_time();
        let frequency = core_video::host_time::get_host_clock_frequency();
        let ahead = at.saturating_duration_since(now).as_secs_f64() * frequency;
        let ticks = now_host.saturating_add(if ahead.is_finite() { ahead as u64 } else { 0 });
        i64::try_from(ticks).unwrap_or(i64::MAX)
    }
}

impl Backend for AvPlayerBackend {
    fn status(&mut self) -> BackendStatus {
        let now = Instant::now();
        if self
            .buffer_read
            .is_none_or(|at| now.saturating_duration_since(at) >= BUFFER_POLL)
        {
            // SAFETY: reading the buffer state of a live item on the main
            // thread.
            let (empty, keeps_up) = unsafe {
                (
                    self.item.isPlaybackBufferEmpty(),
                    self.item.isPlaybackLikelyToKeepUp(),
                )
            };
            self.buffer = BufferState {
                empty,
                keeps_up,
                ranges: self.buffered(),
            };
            self.buffer_read = Some(now);
        }
        // SAFETY: reading the status and rate of a live item and player on
        // the main thread.
        let (status, rate) = unsafe { (self.item.status(), self.player.rate()) };
        let ready = status == AVPlayerItemStatus::ReadyToPlay;
        let position = self.position();
        let duration = self.duration();
        let at_end_position = duration.is_some_and(|total| position >= total - END_SLACK);
        if ready && resumes_after_stall(self.playing, rate, at_end_position, self.buffer.keeps_up) {
            // SAFETY: messaging a live AVPlayer on the main thread.
            unsafe { self.player.setRate(self.rate as f32) };
        }
        BackendStatus {
            ready,
            failed: status == AVPlayerItemStatus::Failed,
            position,
            duration,
            stalled: self.playing && self.buffer.empty && !self.buffer.keeps_up,
            at_end: self.playing && rate == 0.0 && at_end_position,
            buffered: self.buffer.ranges.clone(),
            // AVPlayer mixes the soundtrack itself; a silent clip just
            // plays nothing.
            has_sound: true,
        }
    }

    fn play(&mut self, rate: f64) {
        self.rate = rate;
        self.apply_pitch(rate);
        // SAFETY: messaging a live AVPlayer on the main thread; a non-zero
        // rate starts playback at that speed.
        unsafe { self.player.setRate(rate as f32) };
        self.playing = true;
    }

    fn pause(&mut self) {
        // SAFETY: as above.
        unsafe { self.player.pause() };
        self.playing = false;
    }

    fn seek(&mut self, secs: f64) {
        let target = secs.clamp(0.0, self.duration().unwrap_or(f64::MAX));
        // SAFETY: zero tolerances request a frame-accurate seek.
        unsafe {
            let time = CMTime::with_seconds(target, SEEK_TIMESCALE);
            let zero = CMTime::with_seconds(0.0, SEEK_TIMESCALE);
            self.player
                .seekToTime_toleranceBefore_toleranceAfter(time, zero, zero);
        }
    }

    fn set_volume(&mut self, volume: f32) {
        // SAFETY: messaging a live AVPlayer on the main thread.
        unsafe { self.player.setVolume(volume.clamp(0.0, 1.0)) };
    }

    fn set_rate(&mut self, rate: f64, playing: bool) {
        self.rate = rate;
        self.apply_pitch(rate);
        if playing && self.playing {
            // SAFETY: as above.
            unsafe { self.player.setRate(rate as f32) };
        }
    }

    fn step_frame(&mut self, forward: bool) {
        // SAFETY: stepping a paused item by one frame on the main thread.
        unsafe { self.item.stepByCount(if forward { 1 } else { -1 }) };
    }

    fn take_picture(&mut self, present: &Present) -> Pulled {
        let ticks = present
            .host_time
            .and_then(|ticks| i64::try_from(ticks).ok())
            .unwrap_or_else(|| Self::host_ticks_at(present.at));
        // SAFETY: plain CMTime construction.
        let mut display_time = unsafe { CMTime::with_seconds(0.0, SEEK_TIMESCALE) };
        // SAFETY: the copy returns a +1 CVPixelBuffer whose ownership moves
        // into the core-video wrapper GPUI draws (`wrap_under_create_rule`);
        // `display_time` outlives the call.
        let (item_time, buffer) = unsafe {
            let item_time = self.output.itemTimeForMachAbsoluteTime(ticks);
            let buffer = if self.output.hasNewPixelBufferForItemTime(item_time) {
                self.output
                    .copyPixelBufferForItemTime_itemTimeForDisplay(item_time, &mut display_time)
                    .map(|buffer| core_foundation_wrap(Retained::into_raw(buffer).cast()))
            } else {
                None
            };
            (item_time, buffer)
        };
        // SAFETY: plain CMTime conversion.
        let item_secs = unsafe { item_time.seconds() };
        let item_secs = if item_secs.is_finite() {
            item_secs
        } else {
            0.0
        };
        let display_secs = buffer.as_ref().map(|_| {
            // SAFETY: as above.
            let secs = unsafe { display_time.seconds() };
            if secs.is_finite() { secs } else { item_secs }
        });
        self.last_present = Some(present.at);
        Pulled {
            picture: buffer.map(Picture::Surface),
            item_secs,
            display_secs,
        }
    }

    /// The first display frame after the last pull whose item time has a
    /// new picture: AVPlayer decodes ahead, so asking the output about the
    /// item times of the next few vsyncs finds it exactly (none within
    /// eight frames: look again then).
    fn next_due(&self, _now: Instant, interval: Duration) -> Option<Instant> {
        if !self.playing {
            return None;
        }
        let present = self.last_present?;
        for frames in 1..=PROBE_FRAMES {
            let at = present.checked_add(interval * frames)?;
            let ticks = Self::host_ticks_at(at);
            // SAFETY: querying a live output on the main thread.
            let ready = unsafe {
                let item_time = self.output.itemTimeForMachAbsoluteTime(ticks);
                self.output.hasNewPixelBufferForItemTime(item_time)
            };
            if ready {
                return Some(at);
            }
        }
        present.checked_add(interval * PROBE_FRAMES)
    }

    fn error(&self) -> Option<PlayerError> {
        // SAFETY: reading the item's status and error on the main thread.
        unsafe {
            (self.item.status() == AVPlayerItemStatus::Failed).then(|| {
                let detail = self
                    .item
                    .error()
                    .map(|err| err.localizedDescription().to_string())
                    .unwrap_or_else(|| "This video can't be played.".to_string());
                PlayerError::new(ErrorKind::Decode, detail)
            })
        }
    }
}

impl Drop for AvPlayerBackend {
    fn drop(&mut self) {
        // SAFETY: stop audio before the player is released (main thread:
        // the player is dropped where it lives).
        unsafe { self.player.pause() };
    }
}

/// Take ownership of a +1 `CVPixelBufferRef` as GPUI's wrapper type.
///
/// # Safety
/// `raw` must be an owned (+1) `CVPixelBufferRef`.
unsafe fn core_foundation_wrap(raw: *mut std::ffi::c_void) -> GpuiPixelBuffer {
    use core_foundation::base::TCFType;
    // SAFETY: the caller passes an owned (+1) CVPixelBufferRef.
    unsafe { GpuiPixelBuffer::wrap_under_create_rule(raw.cast()) }
}

/// The picture at `at` seconds (upright) and the clip's duration, decoded
/// synchronously by AVFoundation's image generator: the still a
/// screenshot capture shows when the FFmpeg shim is not installed.
#[cfg(feature = "demo-capture")]
pub(super) fn still_picture(path: &Path, at: f64) -> Option<(Arc<RenderImage>, Option<f64>)> {
    MainThreadMarker::new()?;
    let path = path.to_str()?;
    // SAFETY: AVFoundation object construction and a synchronous image
    // request on the main thread; `actual_time` may be null.
    #[allow(deprecated)]
    let (image, duration) = unsafe {
        let url = NSURL::fileURLWithPath(&NSString::from_str(path));
        let asset = AVURLAsset::URLAssetWithURL_options(&url, None);
        let generator = AVAssetImageGenerator::assetImageGeneratorWithAsset(&asset);
        generator.setAppliesPreferredTrackTransform(true);
        let zero = CMTime::with_seconds(0.0, SEEK_TIMESCALE);
        generator.setRequestedTimeToleranceBefore(zero);
        generator.setRequestedTimeToleranceAfter(zero);
        let time = CMTime::with_seconds(at.max(0.0), SEEK_TIMESCALE);
        let image = generator
            .copyCGImageAtTime_actualTime_error(time, std::ptr::null_mut())
            .ok()?;
        let secs = asset.duration().seconds();
        (image, (secs.is_finite() && secs > 0.0).then_some(secs))
    };
    let mut rgba = super::super::image_decode::draw_srgb(&image).ok()?;
    // RenderImage holds BGRA.
    for pixel in rgba.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
    }
    let image = RenderImage::new(smallvec::SmallVec::from_buf([image::Frame::new(rgba)]));
    Some((Arc::new(image), duration))
}

#[cfg(test)]
mod tests {
    use super::resumes_after_stall;

    #[test]
    fn a_stalled_clip_is_told_to_play_again_once_the_buffer_keeps_up() {
        // Play wanted, rate dropped by a stall, buffer caught up.
        assert!(resumes_after_stall(true, 0.0, false, true));
        // Still starved: wait.
        assert!(!resumes_after_stall(true, 0.0, false, false));
        // Paused on purpose, already playing, or ended: leave it.
        assert!(!resumes_after_stall(false, 0.0, false, true));
        assert!(!resumes_after_stall(true, 1.0, false, true));
        assert!(!resumes_after_stall(true, 0.0, true, true));
    }
}
