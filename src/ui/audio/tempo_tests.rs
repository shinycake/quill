//! The stretcher skips its search where the natural continuation is in the
//! window. Under test, `Tempo::pick` still runs the search on every such
//! block and asserts it picks the same position, so every test that plays
//! through the stretcher (here and in `audio::tests`) checks that skipping
//! it changed nothing. The tests here drive that check across sample rates,
//! speeds, switches and seeks with a sound built to come close to a tie, and
//! pin what skipping must keep: the read-ahead and no search at 1x.

use std::cell::Cell;
use std::num::NonZero;
use std::rc::Rc;
use std::time::Duration;

use rodio::buffer::SamplesBuffer;
use rodio::source::SeekError;
use rodio::{ChannelCount, SampleRate, Source};

use super::{SharedSpeed, Tempo};

/// Counts the samples the stretcher reads, like the engine's `Counted`:
/// that count is the player's position and a video's audio clock.
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

/// A deterministic test sound, interleaved: a tone that repeats exactly
/// every whole, even number of frames (so positions a period away score
/// within rounding of the natural one), silence, a 100 Hz to 4 kHz sweep,
/// then noise. Each channel differs.
fn sound(rate: u32, channels: u16, seconds: f64) -> Vec<f32> {
    let frames = (f64::from(rate) * seconds) as usize;
    let period = 2 * (rate as usize / 400);
    let mut seed = 0x2545_f491_u32;
    let mut phase = 0.0f64;
    let mut out = Vec::with_capacity(frames * usize::from(channels));
    for i in 0..frames {
        let at = i as f64 / frames as f64;
        let freq = 100.0 + 3900.0 * ((at - 0.35) / 0.4).clamp(0.0, 1.0);
        phase += std::f64::consts::TAU * freq / f64::from(rate);
        for c in 0..channels {
            let gain = 0.5 / f64::from(c + 1);
            let value = if at < 0.25 {
                let turn = (i * usize::from(c + 1)) % period;
                gain * (std::f64::consts::TAU * turn as f64 / period as f64).sin()
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

type Stretcher = Tempo<Pulled<SamplesBuffer>>;

/// The stretcher over [`sound`], and the count of samples it has read.
fn stretcher(
    format: (u32, u16),
    seconds: f64,
    speed: &SharedSpeed,
) -> (Stretcher, Rc<Cell<usize>>) {
    let (rate, channels) = format;
    let count = Rc::new(Cell::new(0));
    let inner = SamplesBuffer::new(
        NonZero::new(channels).unwrap(),
        NonZero::new(rate).unwrap(),
        sound(rate, channels, seconds),
    );
    let pulled = Pulled {
        inner,
        count: count.clone(),
    };
    (Tempo::new(pulled, speed.clone()), count)
}

/// What happens to the playing stretcher before a given output sample.
#[derive(Clone, Copy, Debug)]
enum Event {
    Speed(f64),
    Seek(Duration),
}

/// Play [`sound`] at `speed` to the end, applying `script` (sorted by output
/// sample) on the way: the samples played and the stretcher.
fn play(
    format: (u32, u16),
    seconds: f64,
    speed: f64,
    script: &[(usize, Event)],
) -> (usize, Stretcher) {
    let shared = SharedSpeed::new(speed);
    let (mut tempo, _) = stretcher(format, seconds, &shared);
    let mut played = 0;
    let mut events = script.iter().peekable();
    loop {
        while let Some((_, event)) = events.next_if(|(at, _)| *at <= played) {
            match event {
                Event::Speed(speed) => shared.set(*speed),
                Event::Seek(pos) => tempo.try_seek(*pos).unwrap(),
            }
        }
        if tempo.next().is_none() {
            return (played, tempo);
        }
        played += 1;
    }
}

/// The output sample `part` of the way into `seconds` of `format`, at a
/// frame boundary.
fn at(format: (u32, u16), seconds: f64, part: f64) -> usize {
    (f64::from(format.0) * seconds * part) as usize * usize::from(format.1)
}

/// 1x through a seek back, a seek forward and the end, at the rates voice
/// notes (Opus, 48 kHz), music (44.1 kHz), old recordings and hi-res files
/// use: every block is what the search would have picked.
#[test]
fn at_1x_every_block_is_what_the_search_picks() {
    for (format, seconds) in [
        ((48_000, 2), 1.2),
        ((44_100, 1), 1.2),
        ((16_000, 1), 1.6),
        ((96_000, 2), 0.5),
    ] {
        let script = [
            (
                at(format, seconds, 0.45),
                Event::Seek(Duration::from_secs_f64(seconds * 0.15)),
            ),
            (
                at(format, seconds, 0.7),
                Event::Seek(Duration::from_secs_f64(seconds * 0.6)),
            ),
        ];
        let (played, _) = play(format, seconds, 1.0, &script);
        // Both seeks landed: 0.45 + 0.25 + 0.4 of the sound.
        assert!(played > at(format, seconds, 1.05), "{format:?}: {played}");
    }
}

/// Every other speed the player offers, through a seek and the end: every
/// block in the window is what the search would have picked, and the
/// length is the speed's.
#[test]
fn at_other_speeds_every_block_is_what_the_search_picks() {
    for speed in [0.5, 0.7, 0.75, 1.2, 1.5, 2.0] {
        for (format, seconds) in [((16_000, 1), 1.0), ((48_000, 2), 0.4)] {
            let script = [(
                at(format, seconds, 0.4),
                Event::Seek(Duration::from_secs_f64(seconds * 0.1)),
            )];
            let (played, _) = play(format, seconds, speed, &script);
            let secs = played as f64 / f64::from(format.0) / f64::from(format.1);
            let expect = seconds * (0.4 + 0.9 / speed);
            assert!(
                (secs - expect).abs() < 0.05,
                "{speed}x at {format:?}: {secs}s, not {expect}s"
            );
        }
    }
}

/// 1x to another speed and back, switched mid-block and across a seek:
/// every block is what the search would have picked.
#[test]
fn across_speed_switches_every_block_is_what_the_search_picks() {
    for other in [1.5, 0.75, 2.0, 0.5] {
        for (format, seconds) in [((48_000, 2), 0.6), ((16_000, 1), 1.6)] {
            let when = |part: f64| at(format, seconds, part);
            let script = [
                (when(0.2) + 1, Event::Speed(other)),
                (when(0.4) + 7, Event::Speed(1.0)),
                (when(0.55) + 5, Event::Speed(other)),
                (
                    when(0.65),
                    Event::Seek(Duration::from_secs_f64(seconds * 0.3)),
                ),
                (when(0.75) + 9, Event::Speed(1.0)),
            ];
            let (played, _) = play(format, seconds, 1.0, &script);
            assert!(played > when(0.8), "1x/{other}x at {format:?}: {played}");
        }
    }
}

/// 1x reads ahead as far as a search could look, although none runs: what
/// has been read is the player's position, where a pause leaves it, and a
/// video's audio clock. Before block k (from 0) plays, input through
/// k hops + the window + a block + a hop is read: 8,832 samples before the
/// second block at 48 kHz stereo, as before the search was skipped.
#[test]
fn at_1x_the_stretcher_reads_ahead_as_far_as_a_search_would() {
    let format = (48_000, 2);
    let (hop, window, block) = (960, 576, 1920);
    let (mut tempo, read) = stretcher(format, 1.0, &SharedSpeed::new(1.0));
    for _ in 0..2 {
        let start = read.get();
        tempo.by_ref().take(hop * 2 + 1).count();
        assert_eq!(read.get() - start, 8_832, "before the second block");
        for k in 2..30 {
            tempo.by_ref().take(hop * 2).count();
            let frames = (read.get() - start) / 2;
            assert!(
                frames >= k * hop + window + block + hop,
                "block {k}: {frames} frames read"
            );
        }
        tempo.try_seek(Duration::from_millis(100)).unwrap();
    }
}

/// At 1x the natural continuation always wins, so nothing is searched. Main
/// scored 576 positions (the window's 577 but the natural one) per 20 ms
/// block at 48 kHz.
#[test]
fn at_1x_the_stretcher_never_searches() {
    let format = (48_000, 2);
    let script = [(
        at(format, 1.2, 0.4),
        Event::Seek(Duration::from_millis(300)),
    )];
    let (played, tempo) = play(format, 1.2, 1.0, &script);
    let blocks = played / (960 * 2);
    // 1.2 s is 60 blocks; the seek back replays 0.18 s.
    assert!(blocks > 65, "{blocks} blocks");
    assert_eq!(tempo.tried, 0, "positions scored at 1x in {blocks} blocks");
}

/// Back at 1x after another speed, the search runs at most once (the natural
/// continuation may have drifted out of the window), then never again.
#[test]
fn back_at_1x_the_search_stops_within_a_block() {
    for other in [1.5, 0.75, 2.0, 0.5] {
        let speed = SharedSpeed::new(1.0);
        let (mut tempo, _) = stretcher((48_000, 1), 1.6, &speed);
        tempo.by_ref().take(24_003).count();
        speed.set(other);
        tempo.by_ref().take(16_001).count();
        let searched = tempo.tried;
        assert!(searched > 0, "{other}x never searched");
        speed.set(1.0);
        let rest = tempo.by_ref().count();
        assert!(rest > 12_000, "{other}x: {rest} samples left at 1x");
        let after = tempo.tried - searched;
        assert!(after <= 577, "{other}x then 1x scored {after}");
    }
}
