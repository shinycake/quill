//! The pieces of building and counting a track: the frame counter that
//! reads the position off the sound, the player a build thread makes, and
//! the thread's cancel and end (`engine.rs` owns the stage that holds them).
#![deny(clippy::indexing_slicing)]

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::mpsc::Receiver;
use std::time::Duration;

use rodio::{ChannelCount, SampleRate, Source};

use super::tempo::{SharedSpeed, Tempo};
use super::{AudioError, Outlet, OutputPlayer, open_decoded};

/// Frames the mixer has taken from a track, as an absolute position.
#[derive(Debug, Default)]
pub(super) struct Probe {
    pub(super) frames: AtomicU64,
    /// Of `frames`, those the speed stretcher has read but not yet played.
    pub(super) lag: Arc<AtomicU64>,
    pub(super) rate: AtomicU32,
}

impl Probe {
    pub(super) fn secs(&self) -> f64 {
        let rate = self.rate.load(Ordering::Relaxed).max(1);
        let played = self
            .frames
            .load(Ordering::Relaxed)
            .saturating_sub(self.lag.load(Ordering::Relaxed));
        played as f64 / f64::from(rate)
    }
}

/// Frames are published in batches, so the audio thread does one atomic add
/// per few milliseconds of sound.
const PUBLISH_FRAMES: u64 = 256;

/// Counts the frames pulled through it. Sits below the speed stretcher, so
/// the count is in track time whatever the speed, and the stretcher reports
/// how far it has read ahead ([`Probe::lag`]).
pub(super) struct Counted<S: Source> {
    inner: S,
    probe: Arc<Probe>,
    channels: u64,
    samples: u64,
    pending: u64,
}

impl<S: Source> Counted<S> {
    /// `start_secs` is where `inner` already sits.
    fn new(inner: S, probe: Arc<Probe>, start_secs: f64) -> Self {
        let rate = inner.sample_rate().get();
        probe.rate.store(rate, Ordering::Relaxed);
        probe
            .frames
            .store((start_secs * f64::from(rate)) as u64, Ordering::Relaxed);
        let channels = u64::from(inner.channels().get());
        Self {
            inner,
            probe,
            channels,
            samples: 0,
            pending: 0,
        }
    }

    fn publish(&mut self) {
        if self.pending > 0 {
            self.probe.frames.fetch_add(self.pending, Ordering::Relaxed);
            self.pending = 0;
        }
    }
}

impl<S: Source> Iterator for Counted<S> {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        let sample = self.inner.next();
        if sample.is_some() {
            self.samples += 1;
            if self.samples >= self.channels {
                self.samples = 0;
                self.pending += 1;
                if self.pending >= PUBLISH_FRAMES {
                    self.publish();
                }
            }
        } else {
            self.publish();
        }
        sample
    }
}

impl<S: Source> Source for Counted<S> {
    fn current_span_len(&self) -> Option<usize> {
        self.inner.current_span_len()
    }

    fn channels(&self) -> ChannelCount {
        self.inner.channels()
    }

    fn sample_rate(&self) -> SampleRate {
        self.inner.sample_rate()
    }

    fn total_duration(&self) -> Option<Duration> {
        self.inner.total_duration()
    }
}

/// A freshly built player and the probe counting its frames.
pub(super) struct Built {
    pub(super) player: Arc<OutputPlayer>,
    pub(super) probe: Arc<Probe>,
}

/// One loaded track: its player and its position probe. Dropping it stops
/// the sound.
pub(super) struct Track {
    pub(super) player: Arc<OutputPlayer>,
    pub(super) probe: Arc<Probe>,
}

impl Drop for Track {
    fn drop(&mut self) {
        self.player.stop();
    }
}

/// What the sound a build brings does once it lands, and so what the old
/// sound does meanwhile: `AudioEngine::set_landing` sets both.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Landing {
    /// It plays on landing; the old sound plays on until then.
    Playing,
    /// It lands paused; the old sound is paused already.
    Held,
}

pub(super) struct Building {
    pub(super) built_for: f64,
    pub(super) latest_target: f64,
    pub(super) landing: Landing,
    pub(super) built: Receiver<Result<Built, AudioError>>,
    pub(super) _cancel: CancelOnDrop,
}

/// Tells the build thread nobody waits for it any more (a stop, a newer
/// start), so it skips the seek scan and the player.
pub(super) struct CancelOnDrop(pub(super) Arc<AtomicBool>);

impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

/// The build thread's end. Dropped on a return and on a panic alike, it
/// closes the channel before it wakes the app, so the landing the wake
/// brings finds the result or the dead thread.
pub(super) struct BuildEnd {
    pub(super) done: Option<mpsc::Sender<Result<Built, AudioError>>>,
    pub(super) wake: Option<fn()>,
}

impl BuildEnd {
    pub(super) fn send(&self, result: Result<Built, AudioError>) {
        if let Some(done) = &self.done {
            let _ = done.send(result);
        }
    }
}

impl Drop for BuildEnd {
    fn drop(&mut self) {
        self.done = None;
        if let Some(wake) = self.wake {
            wake();
        }
    }
}

pub(super) fn build_paused_player(
    outlet: &Outlet,
    path: &Path,
    offset: f64,
    speed: &SharedSpeed,
    cancelled: &AtomicBool,
) -> Result<Built, AudioError> {
    // A cancelled build's answer goes nowhere, so any error will do.
    let check = || {
        if cancelled.load(Ordering::Relaxed) {
            Err(AudioError::Open)
        } else {
            Ok(())
        }
    };
    check()?;
    let probe = Arc::new(Probe::default());
    let mut decoded = open_decoded(path, cancelled)?;
    check()?;
    // Seeked below `Counted`, so its `try_seek` answers only the player's
    // seeks. A source that can't seek starts from the top.
    let start = if offset > 0.05 && decoded.try_seek(Duration::from_secs_f64(offset)).is_ok() {
        offset
    } else {
        0.0
    };
    check()?;
    let source = Tempo::new(Counted::new(decoded, probe.clone(), start), speed.clone())
        .reporting_lag(probe.lag.clone());
    let player = outlet.player();
    player.pause();
    player.append(source);
    Ok(Built {
        player: Arc::new(player),
        probe,
    })
}
