//! From a capture backend to the engine: a bounded sink that never blocks
//! the capture callback (a full sink drops its oldest frame), and a
//! feeder thread per call that drains it into the call's external source
//! (`ExternalFrameSender`) with capture
//! timestamps mapped to Unix milliseconds.
#![deny(clippy::indexing_slicing)]

use super::{AudioFrame, CaptureEvent, VideoFrame};
use crate::calls::engine::{
    ExternalDevice, ExternalFrameMeta, ExternalFrameSender, external_frame_valid,
};
use crate::locks::LockExt;
use crate::wake::Signal;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Audio frames queued at most (80 ms).
pub const AUDIO_SINK_FRAMES: usize = 8;
/// Video frames queued at most (one 1080p I420 frame is 3.1 MB).
pub const VIDEO_SINK_FRAMES: usize = 4;

struct SinkShared<T> {
    frames: Mutex<VecDeque<T>>,
    ready: Condvar,
    capacity: usize,
    events: Mutex<VecDeque<CaptureEvent>>,
    dropped: AtomicU64,
    closed: AtomicBool,
    wake: &'static Signal,
}

/// Where a capture backend puts its frames and events. Cloning shares
/// the same queue.
pub struct FrameSink<T> {
    shared: Arc<SinkShared<T>>,
}

impl<T> Clone for FrameSink<T> {
    fn clone(&self) -> Self {
        Self {
            shared: self.shared.clone(),
        }
    }
}

impl<T> FrameSink<T> {
    /// A sink whose events wake the poll loop.
    pub fn new(capacity: usize) -> Self {
        Self::waking(capacity, &crate::wake::UPDATES)
    }

    /// A sink whose events poke `wake`.
    pub fn waking(capacity: usize, wake: &'static Signal) -> Self {
        Self {
            shared: Arc::new(SinkShared {
                frames: Mutex::new(VecDeque::with_capacity(capacity.max(1))),
                ready: Condvar::new(),
                capacity: capacity.max(1),
                events: Mutex::new(VecDeque::new()),
                dropped: AtomicU64::new(0),
                closed: AtomicBool::new(false),
                wake,
            }),
        }
    }

    /// Queue a frame; when the sink is full the oldest frame goes. Never
    /// blocks beyond the queue's short lock.
    pub fn push(&self, frame: T) {
        if self.shared.closed.load(Ordering::Acquire) {
            return;
        }
        {
            let mut frames = self.shared.frames.lock_or_recover();
            while frames.len() >= self.shared.capacity {
                frames.pop_front();
                self.shared.dropped.fetch_add(1, Ordering::Relaxed);
            }
            frames.push_back(frame);
        }
        self.shared.ready.notify_one();
    }

    /// The oldest frame, waiting up to `timeout` for one.
    pub fn pop_timeout(&self, timeout: Duration) -> Option<T> {
        let frames = self.shared.frames.lock_or_recover();
        let (mut frames, _) = self
            .shared
            .ready
            .wait_timeout_while(frames, timeout, |frames| {
                frames.is_empty() && !self.shared.closed.load(Ordering::Acquire)
            })
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        frames.pop_front()
    }

    /// Frames dropped because the sink was full.
    pub fn dropped(&self) -> u64 {
        self.shared.dropped.load(Ordering::Relaxed)
    }

    /// Report `event` and wake whoever reads it.
    pub fn push_event(&self, event: CaptureEvent) {
        self.shared.events.lock_or_recover().push_back(event);
        self.shared.wake.poke();
    }

    /// Events reported since the last call, oldest first.
    pub fn take_events(&self) -> Vec<CaptureEvent> {
        self.shared.events.lock_or_recover().drain(..).collect()
    }

    /// Stop accepting frames and wake a waiting feeder.
    pub fn close(&self) {
        self.shared.closed.store(true, Ordering::Release);
        self.shared.frames.lock_or_recover().clear();
        self.shared.ready.notify_all();
    }

    pub fn is_closed(&self) -> bool {
        self.shared.closed.load(Ordering::Acquire)
    }
}

/// A frame the feeder can hand to the engine.
pub trait FeedFrame: Send + 'static {
    const DEVICE: ExternalDevice;
    /// The bytes the engine takes (PCM s16le, or I420).
    fn write_bytes(&self, out: &mut Vec<u8>);
    /// Width and height for video; zero for audio.
    fn size(&self) -> (u16, u16);
    fn captured_at(&self) -> Instant;
}

impl FeedFrame for AudioFrame {
    const DEVICE: ExternalDevice = ExternalDevice::Microphone;

    fn write_bytes(&self, out: &mut Vec<u8>) {
        out.clear();
        out.reserve(self.pcm.len().saturating_mul(2));
        for sample in &self.pcm {
            out.extend_from_slice(&sample.to_le_bytes());
        }
    }

    fn size(&self) -> (u16, u16) {
        (0, 0)
    }

    fn captured_at(&self) -> Instant {
        self.captured_at
    }
}

impl FeedFrame for VideoFrame {
    const DEVICE: ExternalDevice = ExternalDevice::Screen;

    fn write_bytes(&self, out: &mut Vec<u8>) {
        out.clear();
        out.extend_from_slice(&self.i420);
    }

    fn size(&self) -> (u16, u16) {
        (self.width, self.height)
    }

    fn captured_at(&self) -> Instant {
        self.captured_at
    }
}

/// Maps capture instants to Unix milliseconds: the wall clock is read
/// once, then the monotonic clock carries the offset, so timestamps never
/// go backwards when the system time changes.
#[derive(Debug, Clone)]
pub struct CaptureClock {
    origin: Instant,
    origin_unix_ms: i64,
    last_ms: i64,
}

impl CaptureClock {
    pub fn new() -> Self {
        let unix_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|since| i64::try_from(since.as_millis()).unwrap_or(i64::MAX))
            .unwrap_or(0);
        Self::starting_at(Instant::now(), unix_ms)
    }

    pub fn starting_at(origin: Instant, origin_unix_ms: i64) -> Self {
        Self {
            origin,
            origin_unix_ms,
            last_ms: i64::MIN,
        }
    }

    /// Unix milliseconds for `at`; never less than the previous result.
    pub fn map(&mut self, at: Instant) -> i64 {
        let ms = match at.checked_duration_since(self.origin) {
            Some(after) => self
                .origin_unix_ms
                .saturating_add(i64::try_from(after.as_millis()).unwrap_or(i64::MAX)),
            None => {
                let before = self.origin.saturating_duration_since(at);
                self.origin_unix_ms
                    .saturating_sub(i64::try_from(before.as_millis()).unwrap_or(i64::MAX))
            }
        };
        self.last_ms = self.last_ms.max(ms);
        self.last_ms
    }
}

impl Default for CaptureClock {
    fn default() -> Self {
        Self::new()
    }
}

/// What a feeder thread did, readable from the driver.
#[derive(Debug, Default)]
pub struct FeederStats {
    sent: AtomicU64,
    rejected: AtomicU64,
    failed: AtomicU64,
    last_error: Mutex<Option<String>>,
}

impl FeederStats {
    /// Frames the engine accepted.
    pub fn sent(&self) -> u64 {
        self.sent.load(Ordering::Relaxed)
    }

    /// Frames with the wrong size, never sent.
    pub fn rejected(&self) -> u64 {
        self.rejected.load(Ordering::Relaxed)
    }

    /// Frames the engine refused.
    pub fn failed(&self) -> u64 {
        self.failed.load(Ordering::Relaxed)
    }

    pub fn last_error(&self) -> Option<String> {
        self.last_error.lock_or_recover().clone()
    }
}

/// Engine errors are logged at most this often.
const ERROR_LOG_EVERY: Duration = Duration::from_secs(1);

/// The thread that drains one sink into one call. Dropping it stops the
/// thread (within one wait) and closes the sink.
pub struct CaptureFeeder {
    stop: Arc<AtomicBool>,
    forwarding: Arc<AtomicBool>,
    stats: Arc<FeederStats>,
    close_sink: Box<dyn Fn() + Send>,
    worker: Option<JoinHandle<Option<()>>>,
}

impl CaptureFeeder {
    pub fn start<T: FeedFrame>(
        sink: FrameSink<T>,
        sender: Arc<dyn ExternalFrameSender>,
    ) -> std::io::Result<Self> {
        let stop = Arc::new(AtomicBool::new(false));
        let forwarding = Arc::new(AtomicBool::new(true));
        let stats = Arc::new(FeederStats::default());
        let worker = {
            let (stop, forwarding, stats, sink) = (
                stop.clone(),
                forwarding.clone(),
                stats.clone(),
                sink.clone(),
            );
            crate::thread_guard::spawn_guarded("quill-call-feeder", move || {
                run(&sink, sender.as_ref(), &stop, &forwarding, &stats);
            })?
        };
        Ok(Self {
            stop,
            forwarding,
            stats,
            close_sink: Box::new(move || sink.close()),
            worker: Some(worker),
        })
    }

    /// Pause (`false`) or resume forwarding; paused frames are dropped.
    pub fn set_forwarding(&self, on: bool) {
        self.forwarding.store(on, Ordering::Release);
    }

    pub fn forwarding(&self) -> bool {
        self.forwarding.load(Ordering::Acquire)
    }

    pub fn stats(&self) -> &FeederStats {
        &self.stats
    }
}

impl Drop for CaptureFeeder {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        (self.close_sink)();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn run<T: FeedFrame>(
    sink: &FrameSink<T>,
    sender: &dyn ExternalFrameSender,
    stop: &AtomicBool,
    forwarding: &AtomicBool,
    stats: &FeederStats,
) {
    let mut clock = CaptureClock::new();
    let mut bytes = Vec::new();
    let mut last_log: Option<Instant> = None;
    while !stop.load(Ordering::Acquire) {
        let Some(frame) = sink.pop_timeout(Duration::from_millis(100)) else {
            if sink.is_closed() {
                break;
            }
            continue;
        };
        if !forwarding.load(Ordering::Acquire) {
            continue;
        }
        let (width, height) = frame.size();
        let meta = ExternalFrameMeta {
            timestamp_ms: clock.map(frame.captured_at()),
            width,
            height,
        };
        frame.write_bytes(&mut bytes);
        if !external_frame_valid(T::DEVICE, bytes.len(), meta) {
            stats.rejected.fetch_add(1, Ordering::Relaxed);
            continue;
        }
        match sender.send(T::DEVICE, &bytes, meta) {
            Ok(()) => {
                stats.sent.fetch_add(1, Ordering::Relaxed);
            }
            Err(err) => {
                stats.failed.fetch_add(1, Ordering::Relaxed);
                let text = err.to_string();
                if last_log.is_none_or(|at| at.elapsed() >= ERROR_LOG_EVERY) {
                    last_log = Some(Instant::now());
                    eprintln!("quill: call capture: {text}");
                }
                *stats.last_error.lock_or_recover() = Some(text);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calls::engine::EngineError;

    #[derive(Default)]
    struct Recorder {
        frames: Mutex<Vec<(ExternalDevice, Vec<u8>, ExternalFrameMeta)>>,
        fail: AtomicBool,
    }

    impl ExternalFrameSender for Recorder {
        fn send(
            &self,
            device: ExternalDevice,
            data: &[u8],
            meta: ExternalFrameMeta,
        ) -> Result<(), EngineError> {
            if self.fail.load(Ordering::Relaxed) {
                return Err(EngineError::Engine {
                    op: "qc_external_frame",
                    code: -1,
                });
            }
            self.frames
                .lock()
                .unwrap()
                .push((device, data.to_vec(), meta));
            Ok(())
        }
    }

    fn audio(value: i16) -> AudioFrame {
        AudioFrame {
            pcm: vec![value; 480],
            captured_at: Instant::now(),
        }
    }

    fn wait_until(mut done: impl FnMut() -> bool) {
        let start = Instant::now();
        while !done() {
            assert!(start.elapsed() < Duration::from_secs(5), "timed out");
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    #[test]
    fn full_sink_drops_the_oldest_frame() {
        let sink = FrameSink::new(3);
        for value in 0..5 {
            sink.push(audio(value));
        }
        assert_eq!(sink.dropped(), 2);
        let kept: Vec<i16> = std::iter::from_fn(|| sink.pop_timeout(Duration::ZERO))
            .map(|frame| frame.pcm[0])
            .collect();
        assert_eq!(kept, vec![2, 3, 4]);
    }

    #[test]
    fn closed_sink_takes_nothing_and_wakes_the_reader() {
        let sink: FrameSink<AudioFrame> = FrameSink::new(2);
        sink.close();
        sink.push(audio(1));
        let start = Instant::now();
        assert!(sink.pop_timeout(Duration::from_secs(5)).is_none());
        assert!(start.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn clock_is_monotonic_and_anchored() {
        let origin = Instant::now();
        let mut clock = CaptureClock::starting_at(origin, 1_000_000);
        assert_eq!(clock.map(origin), 1_000_000);
        assert_eq!(clock.map(origin + Duration::from_millis(10)), 1_000_010);
        // An older instant (a late frame) never moves time backwards.
        assert_eq!(clock.map(origin), 1_000_010);
        assert_eq!(clock.map(origin + Duration::from_millis(25)), 1_000_025);
    }

    #[test]
    fn audio_frames_become_little_endian_pcm() {
        let frame = AudioFrame {
            pcm: vec![0x0102, -2],
            captured_at: Instant::now(),
        };
        let mut bytes = Vec::new();
        frame.write_bytes(&mut bytes);
        assert_eq!(bytes, vec![0x02, 0x01, 0xfe, 0xff]);
    }

    #[test]
    fn feeder_sends_frames_with_rising_timestamps() {
        let recorder = Arc::new(Recorder::default());
        let sink = FrameSink::new(AUDIO_SINK_FRAMES);
        let feeder = CaptureFeeder::start(sink.clone(), recorder.clone()).unwrap();
        for value in 0..5 {
            sink.push(audio(value));
            std::thread::sleep(Duration::from_millis(3));
        }
        wait_until(|| feeder.stats().sent() == 5);
        let frames = recorder.frames.lock().unwrap().clone();
        assert!(frames.iter().all(|(device, bytes, meta)| {
            *device == ExternalDevice::Microphone && bytes.len() == 960 && meta.width == 0
        }));
        assert!(
            frames
                .windows(2)
                .all(|w| w[0].2.timestamp_ms <= w[1].2.timestamp_ms)
        );
        drop(feeder);
        assert!(sink.is_closed(), "dropping the feeder closes its sink");
    }

    #[test]
    fn bad_frame_sizes_never_reach_the_engine() {
        let recorder = Arc::new(Recorder::default());
        let sink = FrameSink::new(AUDIO_SINK_FRAMES);
        let feeder = CaptureFeeder::start(sink.clone(), recorder.clone()).unwrap();
        sink.push(AudioFrame {
            pcm: vec![0; 479],
            captured_at: Instant::now(),
        });
        sink.push(audio(1));
        wait_until(|| feeder.stats().sent() == 1);
        assert_eq!(feeder.stats().rejected(), 1);

        let video_recorder = Arc::new(Recorder::default());
        let video_sink = FrameSink::new(VIDEO_SINK_FRAMES);
        let video = CaptureFeeder::start(video_sink.clone(), video_recorder.clone()).unwrap();
        let picture = |len: usize| VideoFrame {
            i420: vec![0; len],
            width: 320,
            height: 240,
            captured_at: Instant::now(),
        };
        video_sink.push(picture(320 * 240));
        video_sink.push(picture(320 * 240 * 3 / 2));
        wait_until(|| video.stats().sent() == 1);
        assert_eq!(video.stats().rejected(), 1);
        let sent = video_recorder.frames.lock().unwrap();
        assert_eq!(sent[0].0, ExternalDevice::Screen);
        assert_eq!((sent[0].2.width, sent[0].2.height), (320, 240));
    }

    #[test]
    fn paused_feeder_forwards_nothing() {
        let recorder = Arc::new(Recorder::default());
        let sink = FrameSink::new(AUDIO_SINK_FRAMES);
        let feeder = CaptureFeeder::start(sink.clone(), recorder.clone()).unwrap();
        feeder.set_forwarding(false);
        for value in 0..4 {
            sink.push(audio(value));
        }
        wait_until(|| sink.pop_timeout(Duration::ZERO).is_none());
        std::thread::sleep(Duration::from_millis(20));
        assert_eq!(feeder.stats().sent(), 0);
        feeder.set_forwarding(true);
        sink.push(audio(9));
        wait_until(|| feeder.stats().sent() == 1);
        assert_eq!(recorder.frames.lock().unwrap()[0].1[0], 9);
    }

    #[test]
    fn engine_errors_are_counted_and_kept() {
        let recorder = Arc::new(Recorder::default());
        recorder.fail.store(true, Ordering::Relaxed);
        let sink = FrameSink::new(AUDIO_SINK_FRAMES);
        let feeder = CaptureFeeder::start(sink.clone(), recorder.clone()).unwrap();
        sink.push(audio(1));
        sink.push(audio(2));
        wait_until(|| feeder.stats().failed() == 2);
        assert!(
            feeder
                .stats()
                .last_error()
                .is_some_and(|err| err.contains("qc_external_frame"))
        );
    }
}
