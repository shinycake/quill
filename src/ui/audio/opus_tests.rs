//! opus-decoder (vendored in third_party/opus-decoder, whose CELT inverse MDCT
//! now runs on an FFT: docs/decisions/opus-decode-cpu.md) against rusty-opus's
//! decoder, which is bit-exact on the RFC 8251 vectors.

use std::f32::consts::PI;

use opus_decoder::OpusDecoder;
use rusty_opus::Application;

const RATE: usize = 48_000;
/// One step of the 16-bit PCM opus-decoder rounds its output to.
const LSB: f32 = 1.0 / 32_768.0;

fn noise(seed: &mut u32) -> f32 {
    *seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
    (*seed >> 8) as f32 / (1u32 << 23) as f32 - 1.0
}

/// Voice-like mono: a 140 Hz voice with vibrato and 12 harmonics, in 200 ms
/// syllables with hard onsets, over a little hiss.
fn voice(seconds: f32) -> Vec<f32> {
    let mut seed = 7;
    (0..(RATE as f32 * seconds) as usize)
        .map(|i| {
            let t = i as f32 / RATE as f32;
            let f0 = 140.0 + 10.0 * (2.0 * PI * 5.0 * t).sin();
            let tone: f32 = (1..=12)
                .map(|h| (2.0 * PI * f0 * h as f32 * t).sin() / h as f32)
                .sum();
            let on = if t % 0.25 < 0.2 { 0.25 } else { 0.0 };
            on * tone + 0.02 * noise(&mut seed)
        })
        .collect()
}

/// Music-like interleaved stereo: a different chord per side and a decaying
/// noise burst (a transient, coded with short blocks) twice a second.
fn music(seconds: f32) -> Vec<f32> {
    let mut seed = 11;
    let chord = |t: f32, notes: [f32; 3]| -> f32 {
        notes.iter().map(|f| 0.12 * (2.0 * PI * f * t).sin()).sum()
    };
    (0..(RATE as f32 * seconds) as usize)
        .flat_map(|i| {
            let t = i as f32 / RATE as f32;
            let left = chord(t, [220.0, 277.18, 329.63])
                + 0.3 * (-25.0 * (t % 0.5)).exp() * noise(&mut seed);
            let right = chord(t, [196.0, 246.94, 293.66])
                + 0.3 * (-25.0 * ((t + 0.25) % 0.5)).exp() * noise(&mut seed);
            [left, right]
        })
        .collect()
}

/// CELT-only packets of `frame` samples per channel.
fn encode(pcm: &[f32], channels: usize, bitrate: i32, frame: usize) -> Vec<Vec<u8>> {
    let mut encoder =
        rusty_opus::OpusEncoder::new(RATE as i32, channels, Application::RestrictedLowDelay)
            .unwrap();
    encoder.bitrate_bps = bitrate;
    let mut out = [0u8; 4000];
    pcm.chunks_exact(frame * channels)
        .map(|chunk| {
            let len = encoder.encode(chunk, frame, &mut out).unwrap();
            out[..len].to_vec()
        })
        .collect()
}

#[test]
fn celt_streams_decode_within_one_lsb_of_the_reference_decoder() {
    let voice = voice(0.5);
    let music = music(0.5);
    let mono_music: Vec<f32> = music.iter().step_by(2).copied().collect();
    // Every CELT transform size: 20 ms frames run a 480-point FFT, 10 ms
    // 240, 5 ms 120, and 2.5 ms frames and transient short blocks 60.
    let streams: [(&str, &[f32], usize, i32, usize); 6] = [
        ("voice 24 kbps 20 ms", &voice, 1, 24_000, 960),
        ("voice 32 kbps 10 ms", &voice, 1, 32_000, 480),
        ("voice 64 kbps 60 ms", &voice, 1, 64_000, 2880),
        ("music stereo 128 kbps 20 ms", &music, 2, 128_000, 960),
        ("music stereo 96 kbps 5 ms", &music, 2, 96_000, 240),
        ("music 48 kbps 2.5 ms", &mono_music, 1, 48_000, 120),
    ];
    for (name, pcm, channels, bitrate, frame) in streams {
        let packets = encode(pcm, channels, bitrate, frame);
        let mut ours = OpusDecoder::new(RATE as u32, channels).unwrap();
        let mut reference = rusty_opus::OpusDecoder::new(RATE as i32, channels).unwrap();
        let mut got = vec![0.0f32; 5760 * channels];
        let mut want = vec![0.0f32; 5760 * channels];
        let (mut worst, mut energy, mut samples) = (0.0f32, 0.0f32, 0);
        for packet in &packets {
            let n = ours.decode_float(packet, &mut got, false).unwrap();
            assert_eq!(reference.decode(packet, 5760, &mut want).unwrap(), n);
            for (a, b) in got[..n * channels].iter().zip(&want[..n * channels]) {
                worst = worst.max((a - b).abs());
                energy += b * b;
            }
            samples += n * channels;
        }
        assert!(
            (energy / samples as f32).sqrt() > 0.03,
            "{name}: the reference decoded near silence"
        );
        // Rounding to 16 bits puts opus-decoder up to half a step from the
        // reference; the same f32 CELT math adds far less than another half
        // (0.73 steps at most here). opus-decoder 0.1.1's direct DFT, whose
        // twiddles carried the error of an f32 pi, was up to 1.07 steps off.
        assert!(worst < LSB, "{name}: {} LSB apart", worst / LSB);
    }
}
