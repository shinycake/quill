//! In-process voice-note encoding: 48 kHz mono PCM to an Ogg/Opus file.
//!
//! The same shape as tdesktop `media_audio_capture.cpp`: 32 kbit/s Opus in
//! 20 ms frames, mono, with the first [`SKIP_MS`] muted and the next
//! [`FADE_MS`] faded in (the microphone's start-up thump), the tail faded
//! out, and a 10 ms-peak level track that becomes the 5-bit waveform.
//!
//! The encoder (`rusty-opus`) and the Ogg muxer (`ogg`) are pure Rust, so
//! no libopus or ffmpeg is needed on any platform.

use std::io::Write;

use ogg::writing::{PacketWriteEndInfo, PacketWriter};
use rusty_opus::{Application, OpusEncoder};

/// Capture and encode rate (tdesktop `kCaptureFrequency`).
pub const RATE: u32 = 48_000;
/// One Opus frame: 20 ms.
pub const FRAME: usize = 960;
/// Samples per level (10 ms), tdesktop's `waveformEach` at this rate.
pub const LEVEL_EACH: usize = (RATE / 100) as usize;
/// tdesktop `kCaptureSkipDuration`: muted lead-in.
pub const SKIP_MS: usize = 400;
/// tdesktop `kCaptureFadeInDuration` (also used for the fade-out).
pub const FADE_MS: usize = 300;
/// Encoder delay of libopus at 48 kHz (`OPUS_GET_LOOKAHEAD`), written as
/// the Ogg pre-skip so players trim it.
pub const PRE_SKIP: u16 = 312;

const BITRATE: i32 = 32_000;
/// Fade at a pause (out) and at the resume (in), so the joint does not click.
const PAUSE_FADE: usize = 30 * RATE as usize / 1000;
const SKIP: usize = SKIP_MS * RATE as usize / 1000;
const FADE: usize = FADE_MS * RATE as usize / 1000;
/// A page is flushed about every second, so a killed process still leaves
/// a playable file.
const PAGE_EVERY: u64 = 50;

/// Streams mono 48 kHz samples into an Ogg/Opus file.
pub struct VoiceEncoder<W: Write> {
    encoder: OpusEncoder,
    writer: PacketWriter<'static, W>,
    serial: u32,
    /// Samples waiting for a full frame.
    pending: Vec<f32>,
    /// Samples handed to [`Self::push`] so far (the fade clock).
    seen: usize,
    /// Encoded packet held back so the last one can end the stream.
    held: Option<Vec<u8>>,
    packets: u64,
    /// Frames encoded (written packets plus the held one).
    frames: u64,
    /// Granule position of the last page end written.
    last_granule: u64,
    /// Real samples encoded (without frame padding).
    real_samples: u64,
    /// Peak of the current 10 ms block and how much of it is filled.
    peak: f32,
    in_block: usize,
    /// Per-10 ms peaks, `peak / 256` on the 16-bit scale (tdesktop levels).
    levels: Vec<u8>,
    /// Samples still to fade in after a resume.
    resume_fade: usize,
}

impl<W: Write> VoiceEncoder<W> {
    pub fn new(out: W) -> Result<Self, String> {
        let mut encoder = OpusEncoder::new(RATE as i32, 1, Application::Voip)
            .map_err(|err| format!("Opus encoder: {err:?}"))?;
        encoder.bitrate_bps = BITRATE;
        let mut writer = PacketWriter::new(out);
        // The serial only needs to be unique inside one file.
        let serial = std::process::id() ^ 0x5155_494c;
        let mut head = Vec::with_capacity(19);
        head.extend_from_slice(b"OpusHead");
        head.push(1); // version
        head.push(1); // channels
        head.extend_from_slice(&PRE_SKIP.to_le_bytes());
        head.extend_from_slice(&RATE.to_le_bytes()); // original input rate
        head.extend_from_slice(&0i16.to_le_bytes()); // output gain
        head.push(0); // channel mapping family
        writer
            .write_packet(head, serial, PacketWriteEndInfo::EndPage, 0)
            .map_err(|err| err.to_string())?;
        let vendor = b"Quill";
        let mut tags = Vec::new();
        tags.extend_from_slice(b"OpusTags");
        tags.extend_from_slice(&(vendor.len() as u32).to_le_bytes());
        tags.extend_from_slice(vendor);
        tags.extend_from_slice(&0u32.to_le_bytes()); // no user comments
        writer
            .write_packet(tags, serial, PacketWriteEndInfo::EndPage, 0)
            .map_err(|err| err.to_string())?;
        Ok(Self {
            encoder,
            writer,
            serial,
            pending: Vec::with_capacity(FRAME * 2),
            seen: 0,
            held: None,
            packets: 0,
            frames: 0,
            last_granule: 0,
            real_samples: 0,
            peak: 0.0,
            in_block: 0,
            levels: Vec::new(),
            resume_fade: 0,
        })
    }

    /// The writer behind the muxer (a test reads the bytes so far).
    pub fn get_ref(&self) -> &W {
        self.writer.inner()
    }

    /// Samples pushed so far.
    pub fn samples(&self) -> usize {
        self.seen
    }

    /// Per-10 ms levels so far.
    pub fn levels(&self) -> &[u8] {
        &self.levels
    }

    /// Append mono 48 kHz samples in `-1.0..=1.0`.
    pub fn push(&mut self, samples: &[f32]) -> Result<(), String> {
        for &raw in samples {
            let mut sample = raw.clamp(-1.0, 1.0);
            // Muted lead-in, then a linear fade-in.
            if self.seen < SKIP {
                sample = 0.0;
            } else if self.seen < SKIP + FADE {
                sample *= (self.seen - SKIP) as f32 / FADE as f32;
            }
            if self.resume_fade > 0 {
                sample *= (PAUSE_FADE - self.resume_fade) as f32 / PAUSE_FADE as f32;
                self.resume_fade -= 1;
            }
            self.seen += 1;
            self.peak = self.peak.max(sample.abs());
            self.in_block += 1;
            if self.in_block == LEVEL_EACH {
                self.levels
                    .push((self.peak * 32768.0 / 256.0).min(255.0) as u8);
                self.peak = 0.0;
                self.in_block = 0;
            }
            self.pending.push(sample);
        }
        // Keep the last FADE samples back so `finish` can fade them out.
        while self.pending.len() >= FRAME + FADE {
            let frame: Vec<f32> = self.pending.drain(..FRAME).collect();
            self.encode_frame(&frame, FRAME as u64)?;
        }
        Ok(())
    }

    /// Make everything pushed so far playable: fade the last few
    /// milliseconds out, encode every whole frame and flush a page to the
    /// writer. Less than one frame (20 ms) stays queued and joins the audio
    /// that follows a [`Self::resume`].
    pub fn pause(&mut self) -> Result<(), String> {
        let tail = self.pending.len();
        let fade = tail.min(PAUSE_FADE);
        for (i, sample) in self.pending[tail - fade..].iter_mut().enumerate() {
            *sample *= 1.0 - (i as f32 / fade as f32);
        }
        while self.pending.len() >= FRAME {
            let frame: Vec<f32> = self.pending.drain(..FRAME).collect();
            self.encode_frame(&frame, FRAME as u64)?;
        }
        if let Some(held) = self.held.take() {
            self.packets += 1;
            let granule = u64::from(PRE_SKIP) + self.packets * FRAME as u64;
            self.writer
                .write_packet(held, self.serial, PacketWriteEndInfo::EndPage, granule)
                .map_err(|err| err.to_string())?;
            self.last_granule = granule;
        }
        self.writer
            .inner_mut()
            .flush()
            .map_err(|err| err.to_string())
    }

    /// Continue after [`Self::pause`]: the next few milliseconds fade in.
    pub fn resume(&mut self) {
        self.resume_fade = PAUSE_FADE;
    }

    /// Fade the tail out, flush the last frame and end the stream.
    pub fn finish(mut self) -> Result<W, String> {
        let tail = self.pending.len();
        let fade = tail.min(FADE);
        for (i, sample) in self.pending[tail - fade..].iter_mut().enumerate() {
            *sample *= 1.0 - (i as f32 / fade as f32);
        }
        let mut rest = std::mem::take(&mut self.pending);
        while !rest.is_empty() {
            let real = rest.len().min(FRAME);
            let mut frame: Vec<f32> = rest.drain(..real).collect();
            frame.resize(FRAME, 0.0);
            self.encode_frame(&frame, real as u64)?;
        }
        // Flush the encoder delay: the decoder drops PRE_SKIP samples, so the
        // file must carry that many extra samples after the real ones.
        let wanted = u64::from(PRE_SKIP) + self.real_samples;
        while self.frames * (FRAME as u64) < wanted {
            self.encode_frame(&[0.0; FRAME], 0)?;
        }
        // Right after a pause nothing is held back: a silent frame can
        // still carry the end-of-stream mark.
        if self.held.is_none() {
            self.encode_frame(&[0.0; FRAME], 0)?;
        }
        if let Some(last) = self.held.take() {
            let granule = wanted.max(self.last_granule);
            self.writer
                .write_packet(last, self.serial, PacketWriteEndInfo::EndStream, granule)
                .map_err(|err| err.to_string())?;
        }
        Ok(self.writer.into_inner())
    }

    fn encode_frame(&mut self, frame: &[f32], real: u64) -> Result<(), String> {
        let mut packet = [0u8; 1276];
        let len = self
            .encoder
            .encode(frame, FRAME, &mut packet)
            .map_err(|err| format!("Opus encode: {err:?}"))?;
        // The held packet is not the last one: write it with its granule.
        if let Some(previous) = self.held.replace(packet[..len].to_vec()) {
            self.packets += 1;
            let granule = u64::from(PRE_SKIP) + self.packets * FRAME as u64;
            let end = if self.packets.is_multiple_of(PAGE_EVERY) {
                PacketWriteEndInfo::EndPage
            } else {
                PacketWriteEndInfo::NormalPacket
            };
            self.writer
                .write_packet(previous, self.serial, end, granule)
                .map_err(|err| err.to_string())?;
            if end == PacketWriteEndInfo::EndPage {
                self.last_granule = granule;
            }
        }
        self.frames += 1;
        self.real_samples += real;
        Ok(())
    }
}

/// Streaming sample-rate converter to [`RATE`] for one mono channel:
/// linear interpolation when upsampling, area averaging when downsampling.
pub struct Resampler {
    /// Input samples per output sample.
    step: f64,
    /// Position of the next output sample, in input samples since `prev`.
    pos: f64,
    prev: f32,
    /// Running sum/weight for the downsampling box.
    acc: f64,
    weight: f64,
    identity: bool,
}

impl Resampler {
    pub fn new(input_rate: u32) -> Self {
        Self {
            step: f64::from(input_rate) / f64::from(RATE),
            pos: 0.0,
            prev: 0.0,
            acc: 0.0,
            weight: 0.0,
            identity: input_rate == RATE,
        }
    }

    pub fn process(&mut self, input: &[f32], out: &mut Vec<f32>) {
        if self.identity {
            out.extend_from_slice(input);
            return;
        }
        if self.step > 1.0 {
            // Box filter: each input sample contributes its overlap with the
            // output sample's span.
            for &sample in input {
                let mut left: f64 = 1.0;
                while left > 0.0 {
                    let take = left.min(self.step - self.weight);
                    self.acc += f64::from(sample) * take;
                    self.weight += take;
                    left -= take;
                    if self.weight >= self.step - 1e-9 {
                        out.push((self.acc / self.step) as f32);
                        self.acc = 0.0;
                        self.weight = 0.0;
                    }
                }
            }
        } else {
            for &sample in input {
                // Output samples that fall between `prev` and `sample`.
                while self.pos < 1.0 {
                    let t = self.pos as f32;
                    out.push(self.prev + (sample - self.prev) * t);
                    self.pos += self.step;
                }
                self.pos -= 1.0;
                self.prev = sample;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ogg::reading::PacketReader;
    use rusty_opus::OpusDecoder;
    use std::io::Cursor;

    fn sine(freq: f32, secs: f32, amp: f32) -> Vec<f32> {
        (0..(RATE as f32 * secs) as usize)
            .map(|i| amp * (2.0 * std::f32::consts::PI * freq * i as f32 / RATE as f32).sin())
            .collect()
    }

    fn encode(samples: &[f32], chunk: usize) -> (Vec<u8>, Vec<u8>) {
        let mut encoder = VoiceEncoder::new(Vec::new()).unwrap();
        for part in samples.chunks(chunk) {
            encoder.push(part).unwrap();
        }
        let levels = encoder.levels().to_vec();
        (encoder.finish().unwrap(), levels)
    }

    /// Decode a whole Ogg/Opus file; returns pre-skip, final granule and PCM.
    fn decode(file: &[u8]) -> (u16, u64, Vec<f32>) {
        let mut reader = PacketReader::new(Cursor::new(file));
        let head = reader.read_packet().unwrap().unwrap();
        assert_eq!(&head.data[..8], b"OpusHead");
        assert_eq!(head.data[9], 1, "mono");
        let pre_skip = u16::from_le_bytes([head.data[10], head.data[11]]);
        assert_eq!(
            u32::from_le_bytes(head.data[12..16].try_into().unwrap()),
            RATE
        );
        let tags = reader.read_packet().unwrap().unwrap();
        assert_eq!(&tags.data[..8], b"OpusTags");
        let mut decoder = OpusDecoder::new(RATE as i32, 1).unwrap();
        let (mut pcm, mut granule, mut ended) = (Vec::new(), 0, false);
        while let Some(packet) = reader.read_packet().unwrap() {
            let mut out = vec![0.0f32; FRAME];
            let n = decoder.decode(&packet.data, FRAME, &mut out).unwrap();
            pcm.extend_from_slice(&out[..n]);
            granule = packet.absgp_page();
            ended = packet.last_in_stream();
        }
        assert!(ended, "stream ends with the end-of-stream page");
        (pre_skip, granule, pcm)
    }

    /// Decode the packets of a file that may not be finished: the PCM
    /// (pre-skip included) and the last page granule.
    fn decode_partial(file: &[u8]) -> (Vec<f32>, u64) {
        let mut reader = PacketReader::new(Cursor::new(file));
        reader.read_packet().unwrap().unwrap();
        reader.read_packet().unwrap().unwrap();
        let mut decoder = OpusDecoder::new(RATE as i32, 1).unwrap();
        let (mut pcm, mut granule) = (Vec::new(), 0);
        while let Some(packet) = reader.read_packet().unwrap() {
            let mut out = vec![0.0f32; FRAME];
            let n = decoder.decode(&packet.data, FRAME, &mut out).unwrap();
            pcm.extend_from_slice(&out[..n]);
            if packet.last_in_page() {
                granule = packet.absgp_page();
            }
        }
        (pcm, granule)
    }

    #[test]
    fn a_paused_recording_is_playable_and_resumes_into_one_stream() {
        let first = sine(440.0, 1.5, 0.5);
        let second = sine(330.0, 1.0, 0.5);
        let mut encoder = VoiceEncoder::new(Vec::new()).unwrap();
        for part in first.chunks(441) {
            encoder.push(part).unwrap();
        }
        encoder.pause().unwrap();
        // Everything but under one frame is in the file, and it decodes.
        let (pcm, _) = decode_partial(encoder.get_ref());
        let heard = (pcm.len() - usize::from(PRE_SKIP)) as u64;
        // The decoder delay and under one frame are still queued.
        assert!(
            first.len() as u64 - heard < (FRAME + usize::from(PRE_SKIP)) as u64,
            "{heard} of {}",
            first.len()
        );
        // The end fades out instead of stopping dead.
        let tail = &pcm[pcm.len() - 200..];
        assert!(rms(tail) < 0.1, "tail rms {}", rms(tail));
        encoder.resume();
        for part in second.chunks(441) {
            encoder.push(part).unwrap();
        }
        let file = encoder.finish().unwrap();
        let (_, granule, _) = decode(&file);
        assert_eq!(
            granule - u64::from(PRE_SKIP),
            (first.len() + second.len()) as u64,
            "no audio lost or added across the pause"
        );
    }

    #[test]
    fn finishing_right_after_a_pause_still_ends_the_stream() {
        let mut encoder = VoiceEncoder::new(Vec::new()).unwrap();
        // A whole number of frames: nothing is left queued by the pause.
        encoder.push(&vec![0.2; FRAME * 40]).unwrap();
        encoder.pause().unwrap();
        let file = encoder.finish().unwrap();
        let (pre_skip, granule, _) = decode(&file);
        assert_eq!(granule - u64::from(pre_skip), (FRAME * 40) as u64);
    }

    fn rms(samples: &[f32]) -> f32 {
        (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt()
    }

    #[test]
    fn encoded_sine_round_trips_with_duration_and_energy() {
        let input = sine(440.0, 2.0, 0.5);
        // Odd chunk sizes, like a real device callback.
        let (file, _) = encode(&input, 441);
        let (pre_skip, granule, pcm) = decode(&file);
        assert_eq!(pre_skip, PRE_SKIP);
        // Duration: granule minus pre-skip is exactly what was recorded.
        assert_eq!(granule - u64::from(pre_skip), input.len() as u64);
        let decoded = &pcm[usize::from(pre_skip)..usize::from(pre_skip) + input.len()];
        // Mute + fade-in: nothing in the first 400 ms.
        assert!(rms(&decoded[..SKIP - 480]) < 0.01, "lead-in is muted");
        // Steady state after the fade-in keeps the sine's level (+-1.5 dB)
        // and is the same 440 Hz tone (zero-crossing count).
        let steady = &decoded[SKIP + FADE + 4800..input.len() - FADE - 4800];
        let (got, want) = (rms(steady), 0.5 / 2f32.sqrt());
        let db = 20.0 * (got / want).log10();
        assert!(db.abs() < 1.5, "level off by {db} dB");
        let crossings = steady
            .windows(2)
            .filter(|w| w[0] <= 0.0 && w[1] > 0.0)
            .count();
        let expected = 440.0 * steady.len() as f32 / RATE as f32;
        assert!(
            (crossings as f32 - expected).abs() < 3.0,
            "{crossings} vs {expected}"
        );
        // The tail fades out.
        assert!(rms(&decoded[input.len() - 480..]) < got * 0.2);
    }

    #[test]
    fn short_recordings_and_odd_lengths_keep_exact_duration() {
        for len in [1usize, 959, 960, 961, 5000, 48_000 + 17] {
            let (file, _) = encode(&vec![0.1; len], 1000);
            let (pre_skip, granule, pcm) = decode(&file);
            assert_eq!(granule - u64::from(pre_skip), len as u64, "len {len}");
            assert!(pcm.len() >= usize::from(pre_skip) + len, "len {len}");
        }
    }

    /// libopus (through ffmpeg) accepts the file and reads the same duration
    /// Telegram will. Skipped where ffprobe isn't installed.
    #[test]
    fn ffprobe_reads_the_file_as_opus_with_the_recorded_duration() {
        if !crate::media_tools::is_installed("ffprobe") {
            return;
        }
        let (file, _) = encode(&sine(300.0, 1.5, 0.4), 480);
        let path = std::env::temp_dir().join(format!("quill-voice-{}.ogg", std::process::id()));
        std::fs::write(&path, &file).unwrap();
        let out = crate::media_tools::command("ffprobe")
            .args([
                "-v",
                "error",
                "-show_entries",
                "stream=codec_name,channels,sample_rate",
            ])
            .args(["-show_entries", "format=duration", "-of", "default=nw=1"])
            .arg(&path)
            .output()
            .unwrap();
        let _ = std::fs::remove_file(&path);
        let text = String::from_utf8_lossy(&out.stdout);
        assert!(
            text.contains("codec_name=opus"),
            "{text} {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(
            text.contains("channels=1") && text.contains("sample_rate=48000"),
            "{text}"
        );
        let secs: f64 = text
            .lines()
            .find_map(|l| l.strip_prefix("duration="))
            .and_then(|v| v.parse().ok())
            .unwrap();
        assert!((secs - 1.5).abs() < 0.03, "{secs}");
    }

    #[test]
    fn levels_follow_the_signal_per_ten_milliseconds() {
        let mut input = vec![0.0; RATE as usize / 2];
        input.extend(sine(300.0, 1.0, 0.5));
        let (_, levels) = encode(&input, 960);
        // 1.5 s of audio -> 150 ten-millisecond levels.
        assert_eq!(levels.len(), 150);
        // Silent half second plus the muted lead-in stays at zero...
        assert!(levels[..50].iter().all(|&l| l == 0));
        // ...and a half-scale sine peaks near 0.5 * 32768 / 256 = 64 once
        // the fade-in is over.
        assert!(levels[140] > 55 && levels[140] <= 64, "{}", levels[140]);
    }

    #[test]
    fn resampler_converts_rates_and_preserves_the_tone() {
        for rate in [16_000u32, 44_100, 48_000, 96_000] {
            let secs = 1.0;
            let input: Vec<f32> = (0..rate as usize)
                .map(|i| 0.5 * (2.0 * std::f32::consts::PI * 1000.0 * i as f32 / rate as f32).sin())
                .collect();
            let mut resampler = Resampler::new(rate);
            let mut out = Vec::new();
            for part in input.chunks(333) {
                resampler.process(part, &mut out);
            }
            let want = (RATE as f32 * secs) as i64;
            assert!(
                (out.len() as i64 - want).abs() <= 2,
                "{rate}: {}",
                out.len()
            );
            let level = rms(&out[200..out.len() - 200]);
            // Upsampling by interpolation / box averaging loses a little at
            // 1 kHz but stays within 1 dB of 0.354.
            let db = 20.0 * (level / (0.5 / 2f32.sqrt())).log10();
            assert!(db.abs() < 1.0, "{rate}: {db} dB");
        }
    }
}
