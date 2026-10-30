//! Sample-rate conversion for one mono channel of sound.

/// Streaming sample-rate converter for one mono channel: linear
/// interpolation when upsampling, area averaging when downsampling.
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
    pub fn new(input_rate: u32, output_rate: u32) -> Self {
        Self {
            step: f64::from(input_rate.max(1)) / f64::from(output_rate.max(1)),
            pos: 0.0,
            prev: 0.0,
            acc: 0.0,
            weight: 0.0,
            identity: input_rate == output_rate,
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
    use super::Resampler;

    const RATE: u32 = 48_000;

    fn rms(samples: &[f32]) -> f32 {
        (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt()
    }

    #[test]
    fn resampler_converts_rates_and_preserves_the_tone() {
        for rate in [16_000u32, 44_100, 48_000, 96_000] {
            let secs = 1.0;
            let input: Vec<f32> = (0..rate as usize)
                .map(|i| 0.5 * (2.0 * std::f32::consts::PI * 1000.0 * i as f32 / rate as f32).sin())
                .collect();
            let mut resampler = Resampler::new(rate, RATE);
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
