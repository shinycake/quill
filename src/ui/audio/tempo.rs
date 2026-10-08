//! Pitch-preserving playback speed (what ffplay's `atempo` did): a WSOLA
//! time stretcher wrapped around any rodio source. The speed is read from a
//! shared atomic before every block, so it changes mid-track with no restart.
//!
//! Each output block is a 40 ms Hann-windowed slice of the input, overlapped
//! by half. The slice start follows `block * hop * speed`, nudged within a
//! +-12 ms window to where it best continues the previous slice (normalised
//! cross-correlation), which keeps voices free of the warble plain
//! overlap-add produces. At 1x the natural continuation always wins, so the
//! output is the input.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use rodio::source::SeekError;
use rodio::{ChannelCount, SampleRate, Source};

/// Playback speed bounds (the TGX span, `quill::playback`).
const MIN_SPEED: f32 = 0.5;
const MAX_SPEED: f32 = 2.0;

/// A speed handle shared with the audio thread (`f32` bits in an atomic).
#[derive(Clone, Debug)]
pub(super) struct SharedSpeed(Arc<AtomicU32>);

impl SharedSpeed {
    pub(super) fn new(speed: f64) -> Self {
        let this = Self(Arc::new(AtomicU32::new(1.0f32.to_bits())));
        this.set(speed);
        this
    }

    pub(super) fn set(&self, speed: f64) {
        self.0.store(clamp_speed(speed).to_bits(), Ordering::Relaxed);
    }

    pub(super) fn get(&self) -> f32 {
        f32::from_bits(self.0.load(Ordering::Relaxed))
    }
}

/// Clamp a requested speed into the supported span (NaN becomes 1x).
pub(super) fn clamp_speed(speed: f64) -> f32 {
    if speed.is_nan() {
        return 1.0;
    }
    (speed as f32).clamp(MIN_SPEED, MAX_SPEED)
}

pub(super) struct Tempo<S: Source> {
    inner: S,
    speed: SharedSpeed,
    ch: usize,
    /// Block length, hop (half a block) and search radius, in frames.
    n: usize,
    hs: usize,
    delta: usize,
    window: Vec<f32>,
    /// Interleaved input from frame `input_base` on.
    input: Vec<f32>,
    input_base: usize,
    inner_done: bool,
    /// Overlap accumulator (`n` frames).
    acc: Vec<f32>,
    out: Vec<f32>,
    out_pos: usize,
    /// Nominal start of the next slice, in input frames.
    nominal: f64,
    /// Start of the previous slice.
    prev: Option<usize>,
    done: bool,
}

impl<S: Source> Tempo<S> {
    pub(super) fn new(inner: S, speed: SharedSpeed) -> Self {
        let ch = usize::from(inner.channels().get());
        let rate = inner.sample_rate().get() as usize;
        let n = (rate * 40 / 1000).max(64) & !1;
        let hs = n / 2;
        let window = (0..n)
            .map(|i| 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / n as f32).cos())
            .collect();
        Self {
            inner,
            speed,
            ch,
            n,
            hs,
            delta: rate * 12 / 1000,
            window,
            input: Vec::new(),
            input_base: 0,
            inner_done: false,
            acc: vec![0.0; n * ch],
            out: Vec::new(),
            out_pos: 0,
            nominal: 0.0,
            prev: None,
            done: false,
        }
    }

    fn reset(&mut self) {
        self.input.clear();
        self.input_base = 0;
        self.inner_done = false;
        self.acc.fill(0.0);
        self.out.clear();
        self.out_pos = 0;
        self.nominal = 0.0;
        self.prev = None;
        self.done = false;
    }

    fn input_end(&self) -> usize {
        self.input_base + self.input.len() / self.ch
    }

    /// Pull input until frame `frame` (exclusive) is buffered or it ends.
    fn fill_to(&mut self, frame: usize) {
        while !self.inner_done && self.input_end() < frame {
            let want = (frame - self.input_end()).max(1024) * self.ch;
            for _ in 0..want {
                match self.inner.next() {
                    Some(sample) => self.input.push(sample),
                    None => {
                        self.inner_done = true;
                        break;
                    }
                }
            }
            // Keep whole frames.
            let extra = self.input.len() % self.ch;
            if extra != 0 && self.inner_done {
                self.input.truncate(self.input.len() - extra);
            }
        }
    }

    fn sample(&self, frame: usize, channel: usize) -> f32 {
        if frame < self.input_base {
            return 0.0;
        }
        self.input
            .get((frame - self.input_base) * self.ch + channel)
            .copied()
            .unwrap_or(0.0)
    }

    fn mono(&self, frame: usize) -> f32 {
        (0..self.ch).map(|c| self.sample(frame, c)).sum::<f32>() / self.ch as f32
    }

    /// Normalised correlation of the `hs` frames at `a` with those at `b`.
    fn score(&self, a: usize, b: usize) -> f32 {
        let (mut dot, mut ea, mut eb) = (0.0f32, 1e-9f32, 1e-9f32);
        for i in (0..self.hs).step_by(2) {
            let (x, y) = (self.mono(a + i), self.mono(b + i));
            dot += x * y;
            ea += x * x;
            eb += y * y;
        }
        dot / (ea * eb).sqrt()
    }

    /// Choose where the next slice starts.
    fn pick(&mut self, center: usize) -> usize {
        let Some(prev) = self.prev else {
            return 0;
        };
        let natural = prev + self.hs;
        self.fill_to(center + self.delta + self.n + self.hs);
        let low = center.saturating_sub(self.delta);
        let high = center + self.delta;
        // The natural continuation is the best possible match (ties go to
        // it), so at 1x nothing else can win.
        let mut best = natural;
        let mut best_score = self.score(natural, natural);
        let mut candidate = low;
        while candidate <= high {
            if candidate != natural {
                let score = self.score(natural, candidate);
                if score > best_score + 1e-4 {
                    best = candidate;
                    best_score = score;
                }
            }
            candidate += 2;
        }
        best
    }

    /// Produce the next `hs` output frames into `out`; false at the end.
    fn step(&mut self) -> bool {
        if self.done {
            return false;
        }
        let speed = f64::from(self.speed.get());
        let center = self.nominal.round() as usize;
        self.fill_to(center + self.n);
        if self.prev.is_some() && self.inner_done && center >= self.input_end() {
            // Past the input: flush the held-back half block and stop.
            self.done = true;
            self.emit_front();
            return true;
        }
        let start = self.pick(center);
        self.fill_to(start + self.n);
        let first = self.prev.is_none();
        for i in 0..self.n {
            let w = if first && i < self.hs {
                1.0
            } else {
                self.window[i]
            };
            for c in 0..self.ch {
                self.acc[i * self.ch + c] += w * self.sample(start + i, c);
            }
        }
        self.prev = Some(start);
        self.nominal += self.hs as f64 * speed;
        // Frames before the earliest next search/template position are spent.
        let keep_from = (start + self.hs).min((self.nominal.round() as usize).saturating_sub(self.delta));
        if keep_from > self.input_base {
            let drop = (keep_from - self.input_base).min(self.input.len() / self.ch);
            self.input.drain(..drop * self.ch);
            self.input_base += drop;
        }
        self.emit_front();
        true
    }

    /// Move the finished first half of the accumulator to the output.
    fn emit_front(&mut self) {
        self.out.clear();
        self.out_pos = 0;
        self.out.extend_from_slice(&self.acc[..self.hs * self.ch]);
        self.acc.copy_within(self.hs * self.ch.., 0);
        let tail = self.acc.len() - self.hs * self.ch;
        self.acc[tail..].fill(0.0);
    }
}

impl<S: Source> Iterator for Tempo<S> {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        if self.out_pos >= self.out.len() && !self.step() {
            return None;
        }
        let sample = self.out.get(self.out_pos).copied();
        self.out_pos += 1;
        sample
    }
}

impl<S: Source> Source for Tempo<S> {
    fn current_span_len(&self) -> Option<usize> {
        None
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

    fn try_seek(&mut self, pos: Duration) -> Result<(), SeekError> {
        self.inner.try_seek(pos)?;
        self.reset();
        Ok(())
    }
}
