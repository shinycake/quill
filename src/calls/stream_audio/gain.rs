#![deny(clippy::indexing_slicing)]

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

pub const MAX_GAIN: f32 = 1.0;

/// A volume (0.0 to [`MAX_GAIN`]) and a mute that silences without
/// losing the volume.
pub struct Gain {
    volume_bits: AtomicU32,
    muted: AtomicBool,
}

impl Gain {
    pub fn new(volume: f32, muted: bool) -> Self {
        Self {
            volume_bits: AtomicU32::new(clamp_volume(volume).to_bits()),
            muted: AtomicBool::new(muted),
        }
    }

    pub fn set_volume(&self, volume: f32) {
        self.volume_bits
            .store(clamp_volume(volume).to_bits(), Ordering::Relaxed);
    }

    pub fn volume(&self) -> f32 {
        f32::from_bits(self.volume_bits.load(Ordering::Relaxed))
    }

    pub fn set_muted(&self, muted: bool) {
        self.muted.store(muted, Ordering::Relaxed);
    }

    pub fn muted(&self) -> bool {
        self.muted.load(Ordering::Relaxed)
    }

    pub fn factor(&self) -> f32 {
        if self.muted() { 0.0 } else { self.volume() }
    }
}

pub fn clamp_volume(volume: f32) -> f32 {
    if volume.is_finite() {
        volume.clamp(0.0, MAX_GAIN)
    } else {
        0.0
    }
}

pub fn scaled(sample: i16, factor: f32) -> f32 {
    (f32::from(sample) / 32_768.0 * factor).clamp(-1.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::{Gain, MAX_GAIN, scaled};

    #[test]
    fn the_volume_scales_and_clips() {
        assert_eq!(scaled(16_384, 1.0), 0.5);
        assert_eq!(scaled(16_384, 0.5), 0.25);
        assert_eq!(scaled(-32_768, 1.0), -1.0);
        assert_eq!(scaled(12_345, 0.0), 0.0);
    }

    #[test]
    fn mute_silences_and_keeps_the_volume() {
        let gain = Gain::new(0.4, false);
        assert_eq!(gain.factor(), 0.4);
        gain.set_muted(true);
        assert_eq!(gain.factor(), 0.0);
        assert_eq!(gain.volume(), 0.4);
        gain.set_muted(false);
        assert_eq!(gain.factor(), 0.4);
    }

    #[test]
    fn the_volume_stays_in_range() {
        let gain = Gain::new(5.0, false);
        assert_eq!(gain.volume(), MAX_GAIN);
        gain.set_volume(-1.0);
        assert_eq!(gain.volume(), 0.0);
        gain.set_volume(f32::NAN);
        assert_eq!(gain.volume(), 0.0);
        gain.set_volume(0.75);
        assert_eq!(gain.volume(), 0.75);
    }
}
