//! Ogg/Opus decoding for voice notes. Symphonia (behind rodio) has no Opus
//! decoder, so this demuxes with the pure-Rust `ogg` crate and decodes with
//! the pure-Rust `opus-decoder` (RFC 8251 conformant, no C dependency).
//!
//! The compressed packets are read up front (voice notes are ~4 KB/s) so the
//! exact duration is known and seeking is an index lookup plus the 80 ms
//! pre-roll Opus needs to converge. They sit back to back in one block: a long
//! file is hundreds of thousands of packets, and the audio thread drops a
//! finished source.

use std::io::{Read, Seek, SeekFrom};
use std::time::Duration;

use opus_decoder::OpusDecoder;
use rodio::source::SeekError;
use rodio::{ChannelCount, Source};

/// Opus always decodes at 48 kHz here.
pub(super) const RATE: u32 = 48_000;
/// Decoder convergence before the seek target (RFC 7845: at least 80 ms).
const PREROLL: u64 = RATE as u64 / 1000 * 80;
/// Largest packet: 120 ms.
const MAX_PACKET_FRAMES: usize = RATE as usize / 1000 * 120;

/// Samples per channel (48 kHz) one Opus packet decodes to, from its TOC
/// byte (RFC 6716 section 3.1). `None` for an empty or malformed packet.
pub(super) fn packet_frames(packet: &[u8]) -> Option<u64> {
    let toc = *packet.first()?;
    let config = toc >> 3;
    // Frame length in tenths of a millisecond.
    let tenth_ms: u64 = match config {
        0..=11 => match config % 4 {
            0 => 100,
            1 => 200,
            2 => 400,
            _ => 600,
        },
        12..=15 => {
            if config % 2 == 0 {
                100
            } else {
                200
            }
        }
        _ => match config % 4 {
            0 => 25,
            1 => 50,
            2 => 100,
            _ => 200,
        },
    };
    let count = match toc & 3 {
        0 => 1,
        1 | 2 => 2,
        _ => u64::from(packet.get(1)? & 0x3f),
    };
    if count == 0 {
        return None;
    }
    let frames = tenth_ms * count * u64::from(RATE) / 10_000;
    (frames <= MAX_PACKET_FRAMES as u64).then_some(frames)
}

pub(super) struct OpusSource {
    bytes: Vec<u8>,
    ends: Vec<usize>,
    /// Decoded frame index (48 kHz, pre-skip included) where packet `i` starts.
    starts: Vec<u64>,
    channels: u16,
    pre_skip: u64,
    /// Frames of the whole stream, pre-skip included.
    end: u64,
    decoder: OpusDecoder,
    next_packet: usize,
    /// Frames still to drop (pre-skip, or the run-up to a seek target).
    skip: u64,
    /// Frames still to emit before the end of the stream.
    remaining: u64,
    pcm: Vec<f32>,
    scratch: Vec<f32>,
    cursor: usize,
}

impl OpusSource {
    pub(super) fn new<R: Read + Seek>(mut reader: R) -> Result<Self, String> {
        let file_len = reader.seek(SeekFrom::End(0)).map_err(|e| e.to_string())?;
        reader.rewind().map_err(|e| e.to_string())?;
        let mut reader = ogg::PacketReader::new(reader);
        let head = reader
            .read_packet()
            .map_err(|e| format!("ogg: {e}"))?
            .ok_or("empty ogg stream")?;
        let serial = head.stream_serial();
        let head = head.data;
        let [
            b'O',
            b'p',
            b'u',
            b's',
            b'H',
            b'e',
            b'a',
            b'd',
            _,
            channels,
            pre_skip_lo,
            pre_skip_hi,
            _,
            _,
            _,
            _,
            _,
            _,
            mapping_family,
            ..,
        ] = head.as_slice()
        else {
            return Err("not an Opus stream".into());
        };
        let channels = u16::from(*channels);
        let pre_skip = u64::from(u16::from_le_bytes([*pre_skip_lo, *pre_skip_hi]));
        if !(1..=2).contains(&channels) || *mapping_family != 0 {
            return Err("unsupported Opus channel layout".into());
        }
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(usize::try_from(file_len).unwrap_or(0))
            .map_err(|e| e.to_string())?;
        let mut ends = Vec::new();
        let mut starts = Vec::new();
        let mut at = 0u64;
        let mut last_granule = None;
        let mut first = true;
        while let Some(packet) = reader.read_packet().map_err(|e| format!("ogg: {e}"))? {
            if packet.stream_serial() != serial {
                continue;
            }
            if first {
                first = false;
                if packet.data.starts_with(b"OpusTags") {
                    continue;
                }
            }
            if packet.last_in_stream() || packet.last_in_page() {
                let granule = packet.absgp_page();
                if granule != u64::MAX {
                    last_granule = Some(granule);
                }
            }
            let Some(frames) = packet_frames(&packet.data) else {
                continue;
            };
            starts.push(at);
            at += frames;
            bytes.extend_from_slice(&packet.data);
            ends.push(bytes.len());
        }
        if ends.is_empty() {
            return Err("no audio in the Opus stream".into());
        }
        // The last page's granule position trims the final packet's padding.
        let end = last_granule
            .filter(|g| *g <= at && *g > pre_skip)
            .unwrap_or(at);
        let decoder = OpusDecoder::new(RATE, usize::from(channels)).map_err(|e| e.to_string())?;
        Ok(Self {
            bytes,
            ends,
            starts,
            channels,
            pre_skip,
            end,
            decoder,
            next_packet: 0,
            skip: pre_skip,
            remaining: end.saturating_sub(pre_skip),
            pcm: Vec::new(),
            scratch: vec![0.0; MAX_PACKET_FRAMES * usize::from(channels)],
            cursor: 0,
        })
    }

    pub(super) fn duration(&self) -> Duration {
        Duration::from_secs_f64(self.end.saturating_sub(self.pre_skip) as f64 / f64::from(RATE))
    }

    /// Position the stream at `pos`: restart decoding a few packets early
    /// and drop the frames before the target.
    fn seek_frames(&mut self, pos: Duration) {
        let target = (pos.as_secs_f64() * f64::from(RATE)) as u64 + self.pre_skip;
        let target = target.min(self.end);
        let from = target.saturating_sub(PREROLL);
        let index = self
            .starts
            .partition_point(|s| *s <= from)
            .saturating_sub(1);
        self.decoder.reset();
        self.next_packet = index;
        let start = self.starts.get(index).copied().unwrap_or(0);
        self.skip = target - start.min(target);
        self.remaining = self.end - target;
        self.pcm.clear();
        self.cursor = 0;
    }

    fn refill(&mut self) -> bool {
        let ch = usize::from(self.channels);
        while self.remaining > 0 {
            let from = self.next_packet.checked_sub(1).and_then(|i| self.ends.get(i));
            let from = from.copied().unwrap_or(0);
            let packet = self.ends.get(self.next_packet);
            let Some(packet) = packet.and_then(|&to| self.bytes.get(from..to)) else {
                return false;
            };
            self.next_packet += 1;
            let frames = match self.decoder.decode_float(packet, &mut self.scratch, false) {
                Ok(frames) => frames,
                // A bad packet becomes silence of its length, not a stop.
                Err(_) => {
                    let frames = packet_frames(packet).unwrap_or(0) as usize;
                    if let Some(silence) = self.scratch.get_mut(..frames * ch) {
                        silence.fill(0.0);
                    }
                    frames
                }
            };
            let drop = (self.skip as usize).min(frames);
            self.skip -= drop as u64;
            let keep = (frames - drop).min(self.remaining as usize);
            if keep == 0 {
                continue;
            }
            self.remaining -= keep as u64;
            self.pcm.clear();
            let Some(decoded) = self.scratch.get(drop * ch..(drop + keep) * ch) else {
                return false;
            };
            self.pcm.extend_from_slice(decoded);
            self.cursor = 0;
            return true;
        }
        false
    }
}

impl Iterator for OpusSource {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        if self.cursor >= self.pcm.len() && !self.refill() {
            return None;
        }
        let sample = *self.pcm.get(self.cursor)?;
        self.cursor += 1;
        Some(sample)
    }
}

impl Source for OpusSource {
    fn current_span_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> ChannelCount {
        ChannelCount::new(self.channels).unwrap_or(ChannelCount::MIN)
    }

    fn sample_rate(&self) -> rodio::SampleRate {
        rodio::SampleRate::new(RATE).unwrap_or(rodio::SampleRate::MIN)
    }

    fn total_duration(&self) -> Option<Duration> {
        Some(self.duration())
    }

    fn try_seek(&mut self, pos: Duration) -> Result<(), SeekError> {
        self.seek_frames(pos);
        Ok(())
    }
}

#[cfg(test)]
#[path = "opus_tests.rs"]
mod tests;
