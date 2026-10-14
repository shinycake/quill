//! The HDR still tone map, a twin of `hdr_tone_map` in the vendored Metal
//! shader (`third_party/gpui-pre-apple/src/shaders.metal`). The GPU runs the
//! shader; this copy pins the curve's shape in tests, so a change to one
//! side shows up as a failing test when the other is updated to match.

/// The headroom to hand the shader for content peaking at `peak` on a
/// display with `display` headroom. Content that fits under the display
/// draws untouched (the knee lands at the content's own peak); brighter
/// content rolls off into the display's range.
pub(crate) fn shader_headroom(display: f32, peak: f32) -> f32 {
    let display = display.max(1.0);
    if peak <= display {
        (peak / 0.8).max(display)
    } else {
        display
    }
}

/// Identity up to 80% of `headroom`, then a smooth roll-off that reaches
/// `headroom` only at infinity, with slope 1 at the knee so there is no
/// visible seam.
#[cfg(test)]
pub(crate) fn hdr_tone_map(x: f32, headroom: f32) -> f32 {
    let knee = 0.8 * headroom;
    if x <= knee {
        return x;
    }
    let span = headroom - knee;
    let over = x - knee;
    knee + span * (over / (over + span))
}

#[cfg(test)]
mod tests {
    use super::{hdr_tone_map, shader_headroom};

    #[test]
    fn content_under_the_display_draws_untouched() {
        let headroom = shader_headroom(4.0, 3.9);
        assert_eq!(hdr_tone_map(3.9, headroom), 3.9);
        assert_eq!(shader_headroom(16.0, 3.0), 16.0);
    }

    #[test]
    fn brighter_content_rolls_off_into_the_display() {
        assert_eq!(shader_headroom(2.0, 8.0), 2.0);
        assert!(hdr_tone_map(8.0, shader_headroom(2.0, 8.0)) < 2.0);
        assert_eq!(shader_headroom(0.5, 8.0), 1.0);
    }

    #[test]
    fn identity_below_the_knee() {
        for x in [0.0, 0.25, 1.0, 2.0, 3.2] {
            assert_eq!(hdr_tone_map(x, 4.0), x);
        }
    }

    #[test]
    fn rolls_off_under_the_headroom() {
        let mut last = 0.0;
        for step in 0..200 {
            let x = 3.2 + step as f32 * 0.5;
            let y = hdr_tone_map(x, 4.0);
            assert!(y >= last, "monotonic at {x}");
            assert!(y < 4.0, "stays under the headroom at {x}");
            last = y;
        }
        assert!(hdr_tone_map(1.0e6, 4.0) > 3.99);
    }

    #[test]
    fn slope_is_one_at_the_knee() {
        let knee = 0.8 * 2.0;
        let slope = (hdr_tone_map(knee + 1.0e-3, 2.0) - knee) / 1.0e-3;
        assert!((slope - 1.0).abs() < 0.01, "slope {slope}");
    }
}
