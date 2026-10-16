#![deny(clippy::indexing_slicing)]

use std::sync::atomic::{AtomicI16, AtomicUsize, Ordering};

const POP_ATTEMPTS: usize = 4;

pub struct SampleRing {
    slots: Box<[AtomicI16]>,
    read: AtomicUsize,
    write: AtomicUsize,
    drop_chunk: usize,
}

impl SampleRing {
    /// A ring of `capacity` samples that drops `drop_chunk` samples (at
    /// least one) at a time when it overruns. Both are multiples of the
    /// channel count, so a drop never splits a frame.
    pub fn new(capacity: usize, drop_chunk: usize) -> Self {
        let capacity = capacity.max(1);
        Self {
            slots: (0..capacity).map(|_| AtomicI16::new(0)).collect(),
            read: AtomicUsize::new(0),
            write: AtomicUsize::new(0),
            drop_chunk: drop_chunk.clamp(1, capacity),
        }
    }

    pub fn capacity(&self) -> usize {
        self.slots.len()
    }

    pub fn len(&self) -> usize {
        let write = self.write.load(Ordering::Acquire);
        write.wrapping_sub(self.read.load(Ordering::Acquire))
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn slot(&self, index: usize) -> Option<&AtomicI16> {
        self.slots.get(index % self.slots.len())
    }

    /// Producer: append `samples`. When they do not fit, the oldest audio
    /// goes first, in whole drop chunks. Returns how many samples were
    /// dropped (old ones, or the front of a push longer than the ring).
    pub fn push(&self, samples: &[i16]) -> usize {
        let capacity = self.capacity();
        let skip = samples.len().saturating_sub(capacity);
        let samples = samples.get(skip..).unwrap_or_default();
        let count = samples.len();
        let write = self.write.load(Ordering::Relaxed);
        let mut dropped = skip;
        loop {
            let read = self.read.load(Ordering::Acquire);
            let used = write.wrapping_sub(read);
            let needed = (used + count).saturating_sub(capacity);
            if needed == 0 {
                break;
            }
            let chunks = needed.div_ceil(self.drop_chunk);
            let drop = chunks.saturating_mul(self.drop_chunk).min(used);
            if self
                .read
                .compare_exchange(
                    read,
                    read.wrapping_add(drop),
                    Ordering::AcqRel,
                    Ordering::Acquire,
                )
                .is_ok()
            {
                dropped += drop;
                break;
            }
        }
        for (offset, &sample) in samples.iter().enumerate() {
            if let Some(slot) = self.slot(write.wrapping_add(offset)) {
                slot.store(sample, Ordering::Relaxed);
            }
        }
        self.write
            .store(write.wrapping_add(count), Ordering::Release);
        dropped
    }

    /// Consumer: move up to `out.len()` samples into `out`. Returns how
    /// many were written; the rest of `out` is left as it was.
    pub fn pop(&self, out: &mut [i16]) -> usize {
        for _ in 0..POP_ATTEMPTS {
            let write = self.write.load(Ordering::Acquire);
            let read = self.read.load(Ordering::Acquire);
            let count = write.wrapping_sub(read).min(out.len());
            for (offset, sample) in out.iter_mut().take(count).enumerate() {
                if let Some(slot) = self.slot(read.wrapping_add(offset)) {
                    *sample = slot.load(Ordering::Relaxed);
                }
            }
            if self
                .read
                .compare_exchange(
                    read,
                    read.wrapping_add(count),
                    Ordering::AcqRel,
                    Ordering::Acquire,
                )
                .is_ok()
            {
                return count;
            }
        }
        0
    }
}

/// The consumer's side of playback: it waits until `prime` samples are
/// buffered before playing, plays silence on an underrun, and waits for
/// `prime` again after one, so jitter between the two clocks does not
/// click on every buffer.
pub struct Playout {
    prime: usize,
    primed: bool,
}

impl Playout {
    pub fn new(prime: usize) -> Self {
        Self {
            prime,
            primed: false,
        }
    }

    /// Fill `out` from `ring`, silence where there is no audio. Returns
    /// how many samples came from the ring.
    pub fn fill(&mut self, ring: &SampleRing, out: &mut [i16]) -> usize {
        if !self.primed && ring.len() < self.prime.min(ring.capacity()) {
            out.fill(0);
            return 0;
        }
        self.primed = true;
        let got = ring.pop(out);
        if got < out.len() {
            self.primed = false;
            if let Some(rest) = out.get_mut(got..) {
                rest.fill(0);
            }
        }
        got
    }
}

#[cfg(test)]
mod tests {
    use super::{Playout, SampleRing};

    fn ramp(from: i16, count: usize) -> Vec<i16> {
        (0..count).map(|i| from + i as i16).collect()
    }

    #[test]
    fn samples_come_out_in_order() {
        let ring = SampleRing::new(16, 4);
        assert_eq!(ring.push(&ramp(1, 6)), 0);
        let mut out = [0; 4];
        assert_eq!(ring.pop(&mut out), 4);
        assert_eq!(out, [1, 2, 3, 4]);
        assert_eq!(ring.push(&ramp(7, 12)), 0);
        let mut rest = [0; 14];
        assert_eq!(ring.pop(&mut rest), 14);
        assert_eq!(rest.to_vec(), ramp(5, 14));
        assert!(ring.is_empty());
    }

    #[test]
    fn an_overrun_drops_the_oldest_whole_chunks() {
        let ring = SampleRing::new(8, 4);
        ring.push(&ramp(1, 8));
        assert_eq!(ring.push(&[9]), 4);
        assert_eq!(ring.len(), 5);
        let mut out = [0; 5];
        assert_eq!(ring.pop(&mut out), 5);
        assert_eq!(out, [5, 6, 7, 8, 9]);
    }

    #[test]
    fn a_push_longer_than_the_ring_keeps_its_newest_samples() {
        let ring = SampleRing::new(4, 2);
        ring.push(&[100, 101]);
        assert_eq!(ring.push(&ramp(1, 6)), 4);
        let mut out = [0; 4];
        assert_eq!(ring.pop(&mut out), 4);
        assert_eq!(out, [3, 4, 5, 6]);
    }

    #[test]
    fn an_underrun_returns_what_there_is() {
        let ring = SampleRing::new(8, 2);
        let mut out = [7; 4];
        assert_eq!(ring.pop(&mut out), 0);
        assert_eq!(out, [7; 4]);
        ring.push(&[1, 2]);
        assert_eq!(ring.pop(&mut out), 2);
        assert_eq!(out, [1, 2, 7, 7]);
    }

    #[test]
    fn the_indices_wrap_without_losing_samples() {
        let ring = SampleRing::new(6, 2);
        let mut next = 0i16;
        let mut expected = 0i16;
        for _ in 0..1000 {
            let batch = ramp(next, 4);
            next += 4;
            assert_eq!(ring.push(&batch), 0);
            let mut out = [0; 4];
            assert_eq!(ring.pop(&mut out), 4);
            assert_eq!(out.to_vec(), ramp(expected, 4));
            expected += 4;
        }
    }

    #[test]
    fn playout_waits_to_prime_and_again_after_an_underrun() {
        let ring = SampleRing::new(32, 4);
        let mut playout = Playout::new(8);
        let mut out = [9; 4];
        ring.push(&ramp(1, 4));
        assert_eq!(playout.fill(&ring, &mut out), 0, "not primed yet");
        assert_eq!(out, [0; 4]);
        ring.push(&ramp(5, 4));
        assert_eq!(playout.fill(&ring, &mut out), 4);
        assert_eq!(out, [1, 2, 3, 4]);
        let mut big = [9; 6];
        assert_eq!(playout.fill(&ring, &mut big), 4, "underrun");
        assert_eq!(big, [5, 6, 7, 8, 0, 0]);
        ring.push(&ramp(20, 4));
        assert_eq!(playout.fill(&ring, &mut out), 0, "primes again");
    }

    #[test]
    fn producer_and_consumer_threads_keep_the_order() {
        use std::sync::Arc;
        let ring = Arc::new(SampleRing::new(64, 8));
        let producer = {
            let ring = ring.clone();
            std::thread::spawn(move || {
                for n in 0..2000i32 {
                    let frame: Vec<i16> = (0..8).map(|i| ((n * 8 + i) % 30_000) as i16).collect();
                    ring.push(&frame);
                }
            })
        };
        let mut last: Option<i16> = None;
        let mut out = [0; 8];
        while !producer.is_finished() || !ring.is_empty() {
            let got = ring.pop(&mut out);
            for &sample in out.iter().take(got) {
                if let Some(prev) = last
                    && sample != 0
                {
                    assert!(sample > prev || prev > 29_000, "{prev} then {sample}");
                }
                last = Some(sample);
            }
        }
        producer.join().unwrap();
    }
}
