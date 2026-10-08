//! Quill's own call tones, synthesized in code (48 kHz mono `f32` PCM).
//!
//! Telegram's call sounds are GPL and can't ship in an MIT app, and the
//! system sounds differ per OS, so every platform plays these. Pure maths,
//! no audio or GPUI types: the playback lives in `call_sounds`.

use std::f32::consts::{PI, TAU};

pub(super) const SAMPLE_RATE: u32 = 48_000;

/// Pauses between repeats of the looping tones.
pub(super) const INCOMING_GAP_MS: u64 = 1000;
pub(super) const RINGBACK_GAP_MS: u64 = 2500;

/// Master level: calls are heard over speech, so keep cues modest.
const LEVEL: f32 = 0.5;

fn frames(seconds: f32) -> usize {
    (seconds * SAMPLE_RATE as f32).round() as usize
}

/// Sum a note into `out` starting at `at` seconds.
fn mix(out: &mut [f32], at: f32, note: &[f32]) {
    let start = frames(at);
    for (slot, sample) in out.iter_mut().skip(start).zip(note) {
        *slot += sample;
    }
}

/// Raised-cosine fade factor: 0 at the edge, 1 after `len` frames.
fn fade(position: usize, len: usize) -> f32 {
    if position >= len {
        1.0
    } else {
        0.5 - 0.5 * (PI * position as f32 / len as f32).cos()
    }
}

/// A sustained tone of `partials` (frequency, amplitude) with a smooth
/// attack and release.
fn tone(partials: &[(f32, f32)], seconds: f32, attack: f32, release: f32, amp: f32) -> Vec<f32> {
    let len = frames(seconds);
    let (attack, release) = (frames(attack), frames(release));
    (0..len)
        .map(|i| {
            let t = i as f32 / SAMPLE_RATE as f32;
            let wave: f32 = partials.iter().map(|&(f, a)| a * (TAU * f * t).sin()).sum();
            wave * amp * fade(i, attack) * fade(len - 1 - i, release)
        })
        .collect()
}

/// A soft bell-like chime: a fundamental with two quiet overtones that rings
/// out exponentially (time constant `ring`).
fn chime(freq: f32, seconds: f32, ring: f32, amp: f32) -> Vec<f32> {
    let len = frames(seconds);
    let (attack, release) = (frames(0.006), frames(0.03));
    (0..len)
        .map(|i| {
            let t = i as f32 / SAMPLE_RATE as f32;
            let wave = (TAU * freq * t).sin()
                + 0.28 * (TAU * 2.0 * freq * t).sin() * (-t / (ring * 0.5)).exp()
                + 0.1 * (TAU * 3.0 * freq * t).sin() * (-t / (ring * 0.3)).exp();
            wave * amp * (-t / ring).exp() * fade(i, attack) * fade(len - 1 - i, release)
        })
        .collect()
}

fn finish(mut out: Vec<f32>) -> Vec<f32> {
    for sample in &mut out {
        *sample *= LEVEL;
    }
    out
}

/// Incoming call: a soft rising three-note phrase, ~1.5 s (the caller loops
/// it after a pause).
pub(super) fn incoming() -> Vec<f32> {
    let mut out = vec![0.0; frames(1.5)];
    for (at, freq) in [(0.0, 659.25), (0.22, 783.99), (0.44, 1046.5)] {
        mix(&mut out, at, &chime(freq, 1.0, 0.28, 0.5));
    }
    finish(out)
}

/// Outgoing ringing: the classic double ring of 440 + 480 Hz (0.4 s, 0.2 s
/// pause, 0.4 s), ~1 s. The caller adds the long pause between rings.
pub(super) fn ringback() -> Vec<f32> {
    let ring = tone(&[(440.0, 0.5), (480.0, 0.5)], 0.4, 0.03, 0.05, 0.6);
    let mut out = vec![0.0; frames(1.0)];
    mix(&mut out, 0.0, &ring);
    mix(&mut out, 0.6, &ring);
    finish(out)
}

/// Call connected: a short rising two-note chime.
pub(super) fn connect() -> Vec<f32> {
    let mut out = vec![0.0; frames(0.55)];
    mix(&mut out, 0.0, &chime(659.25, 0.4, 0.14, 0.55));
    mix(&mut out, 0.12, &chime(987.77, 0.43, 0.16, 0.55));
    finish(out)
}

/// Message notification: one soft two-note "ding" (the default tone for
/// notification sounds, ~0.45 s). TDLib ships no default sound file, so the
/// client's own is synthesized like the call tones.
pub(super) fn notification() -> Vec<f32> {
    let mut out = vec![0.0; frames(0.45)];
    mix(&mut out, 0.0, &chime(880.0, 0.3, 0.1, 0.5));
    mix(&mut out, 0.09, &chime(1318.5, 0.36, 0.12, 0.5));
    finish(out)
}

/// Call ended: a short falling two-note chime.
pub(super) fn end() -> Vec<f32> {
    let mut out = vec![0.0; frames(0.55)];
    mix(&mut out, 0.0, &chime(987.77, 0.4, 0.14, 0.55));
    mix(&mut out, 0.12, &chime(659.25, 0.43, 0.16, 0.55));
    finish(out)
}

/// Busy: three short 480 + 620 Hz beeps.
pub(super) fn busy() -> Vec<f32> {
    let beep = tone(&[(480.0, 0.5), (620.0, 0.5)], 0.25, 0.01, 0.02, 0.6);
    let mut out = vec![0.0; frames(1.25)];
    for n in 0..3 {
        mix(&mut out, n as f32 * 0.5, &beep);
    }
    finish(out)
}

/// A tiny windowed blip, a soft "click" rather than a beep.
fn blip(freq: f32) -> Vec<f32> {
    let len = frames(0.06);
    (0..len)
        .map(|i| {
            let window = (PI * i as f32 / len as f32).sin().powi(2);
            let t = i as f32 / SAMPLE_RATE as f32;
            (TAU * freq * t).sin() * window * 0.5
        })
        .collect()
}

pub(super) fn mute() -> Vec<f32> {
    finish(blip(600.0))
}

pub(super) fn unmute() -> Vec<f32> {
    finish(blip(900.0))
}

#[cfg(test)]
mod tests {
    use super::{SAMPLE_RATE, busy, connect, end, incoming, mute, notification, ringback, unmute};

    fn all() -> Vec<(&'static str, Vec<f32>)> {
        vec![
            ("incoming", incoming()),
            ("ringback", ringback()),
            ("connect", connect()),
            ("end", end()),
            ("busy", busy()),
            ("mute", mute()),
            ("unmute", unmute()),
            ("notification", notification()),
        ]
    }

    /// Dev tool, not run by default: `cargo test --features ui --bin quill
    /// export_wavs -- --ignored` writes every tone as a 16-bit WAV into
    /// `$QUILL_TONES_OUT` (looping tones as two cycles with their gap).
    #[test]
    #[ignore = "writes files for listening"]
    fn export_wavs() {
        use super::{INCOMING_GAP_MS, RINGBACK_GAP_MS};
        let Some(dir) = std::env::var_os("QUILL_TONES_OUT") else {
            return;
        };
        let twice = |tone: Vec<f32>, gap_ms: u64| {
            let gap = vec![0.0; (SAMPLE_RATE as u64 * gap_ms / 1000) as usize];
            [tone.clone(), gap, tone].concat()
        };
        let files = [
            ("incoming", twice(incoming(), INCOMING_GAP_MS)),
            ("ringback", twice(ringback(), RINGBACK_GAP_MS)),
            ("connect", connect()),
            ("end", end()),
            ("busy", busy()),
            ("mute", mute()),
            ("unmute", unmute()),
        ];
        for (name, samples) in files {
            let data_len = (samples.len() * 2) as u32;
            let mut wav = Vec::new();
            wav.extend(b"RIFF");
            wav.extend((36 + data_len).to_le_bytes());
            wav.extend(b"WAVEfmt ");
            wav.extend(16u32.to_le_bytes());
            wav.extend(1u16.to_le_bytes());
            wav.extend(1u16.to_le_bytes());
            wav.extend(SAMPLE_RATE.to_le_bytes());
            wav.extend((SAMPLE_RATE * 2).to_le_bytes());
            wav.extend(2u16.to_le_bytes());
            wav.extend(16u16.to_le_bytes());
            wav.extend(b"data");
            wav.extend(data_len.to_le_bytes());
            for s in samples {
                wav.extend(((s * 32767.0) as i16).to_le_bytes());
            }
            let path = std::path::Path::new(&dir).join(format!("{name}.wav"));
            std::fs::write(path, wav).expect("write wav");
        }
    }

    fn seconds(samples: &[f32]) -> f32 {
        samples.len() as f32 / SAMPLE_RATE as f32
    }

    #[test]
    fn lengths_match_the_design() {
        assert!((seconds(&incoming()) - 1.5).abs() < 0.01);
        assert!((seconds(&ringback()) - 1.0).abs() < 0.01);
        assert!((seconds(&busy()) - 1.25).abs() < 0.01);
        assert!(seconds(&connect()) < 0.6 && seconds(&end()) < 0.6);
        assert!(seconds(&mute()) < 0.1 && seconds(&unmute()) < 0.1);
    }

    #[test]
    fn never_clips_and_is_audible() {
        for (name, samples) in all() {
            let peak = samples.iter().fold(0.0_f32, |p, s| p.max(s.abs()));
            assert!(peak < 0.8, "{name} peaks at {peak}");
            assert!(peak > 0.1, "{name} is nearly silent ({peak})");
            assert!(samples.iter().all(|s| s.is_finite()), "{name}");
        }
    }

    #[test]
    fn starts_and_ends_near_silence() {
        for (name, samples) in all() {
            assert!(samples[0].abs() < 0.01, "{name} starts with a click");
            let last = samples[samples.len() - 1];
            assert!(last.abs() < 0.01, "{name} ends with a click");
        }
    }

    #[test]
    fn busy_has_three_beeps_separated_by_silence() {
        let samples = busy();
        let rate = SAMPLE_RATE as f32;
        let loud = |from: f32, to: f32| {
            samples[(from * rate) as usize..(to * rate) as usize]
                .iter()
                .any(|s| s.abs() > 0.1)
        };
        for n in 0..3 {
            let start = n as f32 * 0.5;
            assert!(loud(start + 0.05, start + 0.2), "beep {n}");
            if n < 2 {
                assert!(!loud(start + 0.27, start + 0.48), "gap {n}");
            }
        }
    }
}
