//! Ogg/Opus decoding for voice notes. Symphonia (behind rodio) has no Opus
//! decoder, so this demuxes with the pure-Rust `ogg` crate and decodes with
//! the pure-Rust `opus-decoder` (RFC 8251 conformant, no C dependency).
//!
//! The compressed packets are read up front (voice notes are ~4 KB/s) so the
//! exact duration is known and seeking is an index lookup plus the 80 ms
//! pre-roll Opus needs to converge.

use std::io::{Read, Seek};
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
        0..=11 => [100, 200, 400, 600][usize::from(config % 4)],
        12..=15 => [100, 200][usize::from(config % 2)],
        _ => [25, 50, 100, 200][usize::from(config % 4)],
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
    packets: Vec<Vec<u8>>,
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
    pub(super) fn new<R: Read + Seek>(reader: R) -> Result<Self, String> {
        let mut reader = ogg::PacketReader::new(reader);
        let head = reader
            .read_packet()
            .map_err(|e| format!("ogg: {e}"))?
            .ok_or("empty ogg stream")?;
        let serial = head.stream_serial();
        let head = head.data;
        if head.len() < 19 || &head[..8] != b"OpusHead" {
            return Err("not an Opus stream".into());
        }
        let channels = u16::from(head[9]);
        let pre_skip = u64::from(u16::from_le_bytes([head[10], head[11]]));
        if !(1..=2).contains(&channels) || head[18] != 0 {
            return Err("unsupported Opus channel layout".into());
        }
        let mut packets = Vec::new();
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
            packets.push(packet.data);
        }
        if packets.is_empty() {
            return Err("no audio in the Opus stream".into());
        }
        // The last page's granule position trims the final packet's padding.
        let end = last_granule
            .filter(|g| *g <= at && *g > pre_skip)
            .unwrap_or(at);
        let decoder = OpusDecoder::new(RATE, usize::from(channels)).map_err(|e| e.to_string())?;
        Ok(Self {
            packets,
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
        self.skip = target - self.starts[index].min(target);
        self.remaining = self.end - target;
        self.pcm.clear();
        self.cursor = 0;
    }

    fn refill(&mut self) -> bool {
        let ch = usize::from(self.channels);
        while self.remaining > 0 {
            let Some(packet) = self.packets.get(self.next_packet) else {
                return false;
            };
            self.next_packet += 1;
            let frames = match self.decoder.decode_float(packet, &mut self.scratch, false) {
                Ok(frames) => frames,
                // A bad packet becomes silence of its length, not a stop.
                Err(_) => {
                    let frames = packet_frames(packet).unwrap_or(0) as usize;
                    self.scratch[..frames * ch].fill(0.0);
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
            self.pcm
                .extend_from_slice(&self.scratch[drop * ch..(drop + keep) * ch]);
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
        let sample = self.pcm[self.cursor];
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
