//! Recolouring decoded frames: the live story Stars burst is the server's
//! generic reaction burst drawn in gold.

/// The colour each pixel takes, scaled by the pixel's luminance, in BGR
/// (the decoded frames' byte order).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Tint(pub [u8; 3]);

/// Rec. 709 luma weights in 256ths, in BGR order; they sum to 256, so
/// white stays at full strength.
const LUMA: [u32; 3] = [19, 183, 54];

impl Tint {
    /// Recolours straight-alpha BGRA `pixels` in place. Each pixel keeps
    /// its alpha.
    pub fn apply(self, pixels: &mut [u8]) {
        let [tint_b, tint_g, tint_r] = self.0.map(u32::from);
        let [luma_b, luma_g, luma_r] = LUMA;
        for pixel in pixels.as_chunks_mut::<4>().0 {
            let [b, g, r, a] = pixel.map(u32::from);
            let luminance = (luma_b * b + luma_g * g + luma_r * r + 128) >> 8;
            // Both factors are at most 255, so the result is too.
            let scaled = |tint: u32| ((tint * luminance + 127) / 255) as u8;
            *pixel = [scaled(tint_b), scaled(tint_g), scaled(tint_r), a as u8];
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Tint;

    const GOLD: Tint = Tint([0, 184, 255]);

    fn tinted(mut pixels: Vec<u8>) -> Vec<u8> {
        GOLD.apply(&mut pixels);
        pixels
    }

    #[test]
    fn white_takes_the_full_tint_and_keeps_its_alpha() {
        assert_eq!(tinted(vec![255, 255, 255, 128]), vec![0, 184, 255, 128]);
    }

    #[test]
    fn black_stays_black() {
        assert_eq!(tinted(vec![0, 0, 0, 200]), vec![0, 0, 0, 200]);
    }

    #[test]
    fn green_tints_brighter_than_blue() {
        let green = tinted(vec![0, 255, 0, 255]);
        let blue = tinted(vec![255, 0, 0, 255]);
        assert!(green[2] > blue[2]);
    }

    #[test]
    fn every_pixel_is_tinted() {
        assert_eq!(
            tinted(vec![255, 255, 255, 255, 0, 0, 0, 0]),
            vec![0, 184, 255, 255, 0, 0, 0, 0]
        );
    }
}
