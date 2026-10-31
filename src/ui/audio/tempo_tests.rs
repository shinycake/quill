//! The stretcher against [`MainTempo`], a frozen copy of the routine as it
//! was before 1x stopped searching (`docs/decisions/tempo-1x.md`). Every
//! output sample is compared bit for bit, and so is how much input had been
//! read after it: that count is the player's position (`Counted` sits below
//! the stretcher) and a video's audio clock, and a pause leaves it wherever
//! the last pull did, so equal counts after every sample mean equal pause
//! and resume points.

use std::cell::Cell;
use std::num::NonZero;
use std::rc::Rc;
use std::time::Duration;

use rodio::buffer::SamplesBuffer;
use rodio::source::SeekError;
use rodio::{ChannelCount, SampleRate, Source};

use super::{SharedSpeed, Tempo};

/// Main's `Tempo` before the 1x change, kept verbatim apart from the name
/// and the `tried` counter. Do not "fix" it: it is the expected output.
struct MainTempo<S: Source> {
    inner: S,
    speed: SharedSpeed,
    ch: usize,
    n: usize,
    hs: usize,
    delta: usize,
    window: Vec<f32>,
    input: Vec<f32>,
    input_base: usize,
    inner_done: bool,
    acc: Vec<f32>,
    out: Vec<f32>,
    out_pos: usize,
    nominal: f64,
    prev: Option<usize>,
    done: bool,
    tried: usize,
}

impl<S: Source> MainTempo<S> {
    fn new(inner: S, speed: SharedSpeed) -> Self {
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
            tried: 0,
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

    fn pick(&mut self, center: usize) -> usize {
        let Some(prev) = self.prev else {
            return 0;
        };
        let natural = prev + self.hs;
        self.fill_to(center + self.delta + self.n + self.hs);
        let low = center.saturating_sub(self.delta);
        let high = center + self.delta;
        let in_window = (low..=high).contains(&natural);
        let (mut best, mut best_score) = if in_window {
            (natural, 1.0)
        } else {
            (center, f32::MIN)
        };
        let mut candidate = low;
        while candidate <= high {
            if candidate != natural {
                self.tried += 1;
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

    fn step(&mut self) -> bool {
        if self.done {
            return false;
        }
        let speed = f64::from(self.speed.get());
        let center = self.nominal.round() as usize;
        self.fill_to(center + self.n);
        if self.prev.is_some() && self.inner_done && center >= self.input_end() {
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
                self.window.get(i).copied().unwrap_or(0.0)
            };
            for c in 0..self.ch {
                let add = w * self.sample(start + i, c);
                if let Some(acc) = self.acc.get_mut(i * self.ch + c) {
                    *acc += add;
                }
            }
        }
        self.prev = Some(start);
        self.nominal += self.hs as f64 * speed;
        let keep_from =
            (start + self.hs).min((self.nominal.round() as usize).saturating_sub(self.delta));
        if keep_from > self.input_base {
            let drop = (keep_from - self.input_base).min(self.input.len() / self.ch);
            self.input.drain(..drop * self.ch);
            self.input_base += drop;
        }
        self.emit_front();
        true
    }

    fn emit_front(&mut self) {
        self.out.clear();
        self.out_pos = 0;
        let split = self.hs * self.ch;
        let Some(front) = self.acc.get(..split) else {
            return;
        };
        self.out.extend_from_slice(front);
        self.acc.copy_within(split.., 0);
        let tail = self.acc.len() - split;
        if let Some(rest) = self.acc.get_mut(tail..) {
            rest.fill(0.0);
        }
    }
}

impl<S: Source> Iterator for MainTempo<S> {
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

impl<S: Source> Source for MainTempo<S> {
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

/// Counts the samples the stretcher reads, like the engine's `Counted`.
struct Pulled<S> {
    inner: S,
    count: Rc<Cell<usize>>,
}

impl<S: Source> Iterator for Pulled<S> {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        let sample = self.inner.next();
        if sample.is_some() {
            self.count.set(self.count.get() + 1);
        }
        sample
    }
}

impl<S: Source> Source for Pulled<S> {
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

    fn try_seek(&mut self, pos: Duration) -> Result<(), SeekError> {
        self.inner.try_seek(pos)
    }
}

/// A deterministic test sound, interleaved: a tone whose period is a whole
/// even number of frames (so positions a period away score within rounding
/// of the natural one), silence, a 100 Hz to 4 kHz sweep, then noise. Each
/// channel differs.
fn sound(rate: u32, channels: u16, seconds: f64) -> Vec<f32> {
    let frames = (f64::from(rate) * seconds) as usize;
    let mut seed = 0x2545_f491_u32;
    let mut phase = 0.0f64;
    let mut out = Vec::with_capacity(frames * usize::from(channels));
    for i in 0..frames {
        let at = i as f64 / frames as f64;
        let t = i as f64 / f64::from(rate);
        let freq = 100.0 + 3900.0 * ((at - 0.35) / 0.4).clamp(0.0, 1.0);
        phase += std::f64::consts::TAU * freq / f64::from(rate);
        for c in 0..channels {
            let gain = 0.5 / f64::from(c + 1);
            let value = if at < 0.25 {
                gain * (std::f64::consts::TAU * 200.0 * f64::from(c + 1) * t).sin()
            } else if at < 0.35 {
                0.0
            } else if at < 0.75 {
                gain * (phase * f64::from(c + 1)).sin()
            } else {
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                gain * (f64::from(seed >> 8) / f64::from(1u32 << 24) * 2.0 - 1.0)
            };
            out.push(value as f32);
        }
    }
    out
}

/// What happens to the playing stretcher before a given output sample.
#[derive(Clone, Copy, Debug)]
enum Event {
    Speed(f64),
    Seek(Duration),
}

/// What a listener and the player observe: every output sample's bits and
/// the input read once it was out.
#[derive(Default)]
struct Trace {
    out: Vec<u32>,
    read: Vec<usize>,
}

type Input = Pulled<SamplesBuffer>;

/// Play `samples` through a stretcher built by `make` at `speed` to the
/// end, applying `script` (sorted by output sample) on the way.
fn play<T: Source>(
    make: impl FnOnce(Input, SharedSpeed) -> T,
    (rate, channels): (u32, u16),
    samples: &[f32],
    speed: f64,
    script: &[(usize, Event)],
) -> (Trace, T) {
    let count = Rc::new(Cell::new(0));
    let inner = SamplesBuffer::new(
        NonZero::new(channels).unwrap(),
        NonZero::new(rate).unwrap(),
        samples.to_vec(),
    );
    let shared = SharedSpeed::new(speed);
    let mut tempo = make(
        Pulled {
            inner,
            count: count.clone(),
        },
        shared.clone(),
    );
    let mut trace = Trace::default();
    let mut events = script.iter().peekable();
    loop {
        while let Some((_, event)) = events.next_if(|(at, _)| *at <= trace.out.len()) {
            match event {
                Event::Speed(speed) => shared.set(*speed),
                Event::Seek(pos) => tempo.try_seek(*pos).unwrap(),
            }
        }
        let Some(sample) = tempo.next() else {
            break;
        };
        trace.out.push(sample.to_bits());
        trace.read.push(count.get());
    }
    (trace, tempo)
}

/// Fails at the first output sample, or input count, that differs.
fn assert_same(new: &Trace, main: &Trace, what: &str) {
    let len = new.out.len().min(main.out.len());
    if let Some(i) = (0..len).find(|&i| new.out[i] != main.out[i]) {
        panic!(
            "{what}: sample {i} is {} on main, {} now",
            f32::from_bits(main.out[i]),
            f32::from_bits(new.out[i])
        );
    }
    assert_eq!(new.out.len(), main.out.len(), "{what}: output length");
    if let Some(i) = (0..len).find(|&i| new.read[i] != main.read[i]) {
        panic!(
            "{what}: after sample {i} main had read {} input samples, now {}",
            main.read[i], new.read[i]
        );
    }
}

/// The same sound and script through the stretcher and main's copy.
fn both(
    format: (u32, u16),
    seconds: f64,
    speed: f64,
    script: &[(usize, Event)],
) -> ((Trace, Tempo<Input>), (Trace, MainTempo<Input>)) {
    let samples = sound(format.0, format.1, seconds);
    (
        play(Tempo::new, format, &samples, speed, script),
        play(MainTempo::new, format, &samples, speed, script),
    )
}

/// Output sample index of the first sample of the frame `frames` in.
fn frame(format: (u32, u16), frames: usize) -> usize {
    frames * usize::from(format.1)
}

/// 1x through a seek back, a seek forward, the end, and a pause at any
/// sample: the same samples and the same position as main, at the rates
/// voice notes (Opus, 48 kHz), music (44.1 kHz) and old recordings use.
#[test]
fn at_1x_the_stretcher_plays_what_main_played() {
    for format in [(48_000, 2), (44_100, 1), (16_000, 1)] {
        let rate = format.0 as usize;
        let script = [
            (
                frame(format, rate * 7 / 10 + 3),
                Event::Seek(Duration::from_millis(200)),
            ),
            (
                frame(format, rate * 11 / 10 + 1),
                Event::Seek(Duration::from_millis(900)),
            ),
        ];
        let ((new, _), (main, _)) = both(format, 1.6, 1.0, &script);
        // Both seeks landed: 0.7 s + 0.4 s + the last 0.7 s.
        assert!(new.out.len() > frame(format, rate * 17 / 10), "{format:?}");
        assert_same(&new, &main, &format!("{format:?}"));
    }
}

/// At 1x the natural continuation always wins, so nothing is searched. Main
/// scored 576 positions (the window's 577 but the natural one) per 20 ms
/// block at 48 kHz.
#[test]
fn at_1x_the_stretcher_never_searches() {
    let format = (48_000, 2);
    let script = [(
        frame(format, 48_001),
        Event::Seek(Duration::from_millis(300)),
    )];
    let ((new, tempo), (_, main)) = both(format, 1.2, 1.0, &script);
    let blocks = new.out.len() / frame(format, 960);
    assert!(
        main.tried >= 576 * (blocks - 4),
        "main tried {} in {blocks} blocks",
        main.tried
    );
    assert_eq!(tempo.tried, 0, "positions scored at 1x in {blocks} blocks");
}

/// Back at 1x after another speed, the search runs at most once (the natural
/// continuation may have drifted out of the window), then never again.
#[test]
fn back_at_1x_the_search_stops_within_a_block() {
    for other in [1.5, 0.75, 2.0, 0.5] {
        let speed = SharedSpeed::new(1.0);
        let input = SamplesBuffer::new(
            NonZero::<u16>::MIN,
            NonZero::new(48_000).unwrap(),
            sound(48_000, 1, 1.6),
        );
        let mut tempo = Tempo::new(input, speed.clone());
        tempo.by_ref().take(24_003).count();
        speed.set(other);
        tempo.by_ref().take(16_001).count();
        let searched = tempo.tried;
        assert!(searched > 0, "{other}x never searched");
        speed.set(1.0);
        let rest = tempo.by_ref().count();
        assert!(rest > 12_000, "{other}x: {rest} samples left at 1x");
        assert!(
            tempo.tried - searched <= 577,
            "{other}x then 1x scored {}",
            tempo.tried - searched
        );
    }
}

/// Every other speed the player offers plays exactly what main played,
/// through a seek and the end.
#[test]
fn other_speeds_play_what_main_played() {
    for speed in [0.5, 0.7, 0.75, 1.2, 1.5, 2.0] {
        for (format, seconds) in [((16_000, 1), 1.0), ((48_000, 2), 0.4)] {
            let at = |part: f64| frame(format, (f64::from(format.0) * seconds * part) as usize + 1);
            let script = [(at(0.4), Event::Seek(Duration::from_secs_f64(seconds * 0.1)))];
            let ((new, _), (main, _)) = both(format, seconds, speed, &script);
            assert_same(&new, &main, &format!("{speed}x at {format:?}"));
        }
    }
}

/// 1x to another speed and back, switched mid-block: the same samples (so
/// no new click or jump) and the same position as main.
#[test]
fn speed_switches_mid_track_play_what_main_played() {
    for other in [1.5, 0.75, 2.0, 0.5] {
        for (format, seconds) in [((48_000, 2), 0.6), ((16_000, 1), 1.6)] {
            let at = |part: f64| frame(format, (f64::from(format.0) * seconds * part) as usize);
            let script = [
                (at(0.2) + 1, Event::Speed(other)),
                (at(0.4) + 7, Event::Speed(1.0)),
                (at(0.55) + 5, Event::Speed(other)),
                (
                    at(0.65),
                    Event::Seek(Duration::from_secs_f64(seconds * 0.3)),
                ),
                (at(0.75) + 9, Event::Speed(1.0)),
            ];
            let ((new, _), (main, _)) = both(format, seconds, 1.0, &script);
            assert_same(&new, &main, &format!("1x/{other}x at {format:?}"));
        }
    }
}
