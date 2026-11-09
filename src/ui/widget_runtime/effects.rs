//! Pixel effects the painter needs beyond fills: Gaussian blur (box blur
//! three times, as browsers approximate it) and the CSS filter functions.
use tiny_skia::{Mask, Pixmap, PremultipliedColorU8};

/// `target` takes `drawn`'s pixels where `mask` covers it (blended by its
/// coverage) and keeps its own elsewhere; all three are the same size.
pub fn take_masked(target: &mut Pixmap, drawn: &Pixmap, mask: &Mask) {
    let mix = |from: u8, to: u8, m: u16| {
        ((u16::from(from) * (255 - m) + u16::from(to) * m + 127) / 255) as u8
    };
    for ((out, new), m) in target
        .pixels_mut()
        .iter_mut()
        .zip(drawn.pixels())
        .zip(mask.data())
    {
        let m = u16::from(*m);
        let mixed = PremultipliedColorU8::from_rgba(
            mix(out.red(), new.red(), m),
            mix(out.green(), new.green(), m),
            mix(out.blue(), new.blue(), m),
            mix(out.alpha(), new.alpha(), m),
        );
        if let Some(mixed) = mixed {
            *out = mixed;
        }
    }
}

/// Blurs `pixmap` (premultiplied) with standard deviation `sigma`, px.
pub fn blur(pixmap: &mut Pixmap, sigma: f32) {
    if sigma.is_nan() || sigma <= 0.4 || !sigma.is_finite() {
        return;
    }
    let (width, height) = (pixmap.width() as usize, pixmap.height() as usize);
    // Three box widths whose passes add up to the Gaussian (W3C filter spec).
    let d = ((sigma * 3.0 * (2.0 * std::f32::consts::PI).sqrt() / 4.0) + 0.5)
        .floor()
        .max(1.0) as usize;
    let data = pixmap.data_mut();
    let mut scratch = vec![0u8; data.len()];
    for _ in 0..3 {
        box_pass(data, &mut scratch, width, height, d, true);
        box_pass(&scratch, data, width, height, d, false);
    }
}

/// One box pass of `size`, along rows (`horizontal`) or columns.
fn box_pass(
    src: &[u8],
    dst: &mut [u8],
    width: usize,
    height: usize,
    size: usize,
    horizontal: bool,
) {
    let (lines, len) = if horizontal {
        (height, width)
    } else {
        (width, height)
    };
    let index = |line: usize, at: usize| {
        if horizontal {
            (line * width + at) * 4
        } else {
            (at * width + line) * 4
        }
    };
    let before = size / 2;
    let after = size - before - 1;
    let size = size as u32;
    for line in 0..lines {
        let mut sum = [0u32; 4];
        let get = |at: isize| -> [u32; 4] {
            if at < 0 || at as usize >= len {
                return [0; 4];
            }
            let i = index(line, at as usize);
            match src.get(i..i + 4) {
                Some([r, g, b, a]) => [u32::from(*r), u32::from(*g), u32::from(*b), u32::from(*a)],
                _ => [0; 4],
            }
        };
        for k in -(before as isize)..=(after as isize) {
            let px = get(k);
            for (s, p) in sum.iter_mut().zip(px) {
                *s += p;
            }
        }
        for at in 0..len {
            let i = index(line, at);
            if let Some(out) = dst.get_mut(i..i + 4) {
                for (o, s) in out.iter_mut().zip(sum) {
                    *o = ((s + size / 2) / size).min(255) as u8;
                }
            }
            let add = get(at as isize + after as isize + 1);
            let sub = get(at as isize - before as isize);
            for ((s, a), r) in sum.iter_mut().zip(add).zip(sub) {
                *s = (*s + a).saturating_sub(r);
            }
        }
    }
}

/// A color filter function of CSS (Filter Effects 1), on straight RGB.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ColorFilter {
    Brightness(f32),
    Contrast(f32),
    Grayscale(f32),
    HueRotate(f32),
    Invert(f32),
    Opacity(f32),
    Saturate(f32),
    Sepia(f32),
}

impl ColorFilter {
    /// The 4×5 matrix of the filter (rows R, G, B, A; last column the offset, 0..1).
    fn matrix(self) -> [[f32; 5]; 4] {
        let identity = [
            [1.0, 0.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 0.0, 1.0, 0.0],
        ];
        let rgb = |m: [[f32; 3]; 3]| {
            let mut out = identity;
            for (row, [r, g, b]) in out.iter_mut().zip(m) {
                (row[0], row[1], row[2]) = (r, g, b);
            }
            out
        };
        match self {
            Self::Brightness(v) => rgb([[v, 0.0, 0.0], [0.0, v, 0.0], [0.0, 0.0, v]]),
            Self::Contrast(v) => {
                let mut out = rgb([[v, 0.0, 0.0], [0.0, v, 0.0], [0.0, 0.0, v]]);
                for row in out.iter_mut().take(3) {
                    row[4] = 0.5 - 0.5 * v;
                }
                out
            }
            Self::Grayscale(v) => {
                let s = 1.0 - v.clamp(0.0, 1.0);
                saturation_like(s)
            }
            Self::Saturate(v) => saturation_like(v.max(0.0)),
            Self::Sepia(v) => {
                let s = 1.0 - v.clamp(0.0, 1.0);
                rgb([
                    [0.393 + 0.607 * s, 0.769 - 0.769 * s, 0.189 - 0.189 * s],
                    [0.349 - 0.349 * s, 0.686 + 0.314 * s, 0.168 - 0.168 * s],
                    [0.272 - 0.272 * s, 0.534 - 0.534 * s, 0.131 + 0.869 * s],
                ])
            }
            Self::HueRotate(degrees) => {
                let (sin, cos) = degrees.to_radians().sin_cos();
                rgb([
                    [
                        0.213 + cos * 0.787 - sin * 0.213,
                        0.715 - cos * 0.715 - sin * 0.715,
                        0.072 - cos * 0.072 + sin * 0.928,
                    ],
                    [
                        0.213 - cos * 0.213 + sin * 0.143,
                        0.715 + cos * 0.285 + sin * 0.140,
                        0.072 - cos * 0.072 - sin * 0.283,
                    ],
                    [
                        0.213 - cos * 0.213 - sin * 0.787,
                        0.715 - cos * 0.715 + sin * 0.715,
                        0.072 + cos * 0.928 + sin * 0.072,
                    ],
                ])
            }
            Self::Invert(v) => {
                let v = v.clamp(0.0, 1.0);
                let mut out = rgb([
                    [1.0 - 2.0 * v, 0.0, 0.0],
                    [0.0, 1.0 - 2.0 * v, 0.0],
                    [0.0, 0.0, 1.0 - 2.0 * v],
                ]);
                for row in out.iter_mut().take(3) {
                    row[4] = v;
                }
                out
            }
            Self::Opacity(v) => {
                let mut out = identity;
                out[3][3] = v.clamp(0.0, 1.0);
                out
            }
        }
    }
}

/// `saturate(s)`; `grayscale(v)` is `saturate(1 - v)`.
fn saturation_like(s: f32) -> [[f32; 5]; 4] {
    [
        [
            0.213 + 0.787 * s,
            0.715 - 0.715 * s,
            0.072 - 0.072 * s,
            0.0,
            0.0,
        ],
        [
            0.213 - 0.213 * s,
            0.715 + 0.285 * s,
            0.072 - 0.072 * s,
            0.0,
            0.0,
        ],
        [
            0.213 - 0.213 * s,
            0.715 - 0.715 * s,
            0.072 + 0.928 * s,
            0.0,
            0.0,
        ],
        [0.0, 0.0, 0.0, 1.0, 0.0],
    ]
}

/// Runs `filter` over every pixel of `pixmap` (premultiplied).
pub fn apply(pixmap: &mut Pixmap, filter: ColorFilter) {
    let m = filter.matrix();
    let lifts_transparent = m.iter().take(3).any(|row| row[4] > 0.0);
    for [pr, pg, pb, pa] in pixmap.data_mut().as_chunks_mut::<4>().0 {
        let [r, g, b, a] = [*pr, *pg, *pb, *pa].map(|c| f32::from(c) / 255.0);
        if a <= 0.0 && !lifts_transparent {
            continue;
        }
        let (r, g, b) = if a > 0.0 {
            (r / a, g / a, b / a)
        } else {
            (0.0, 0.0, 0.0)
        };
        let [nr, ng, nb, alpha] = m.map(|row| {
            (row[0] * r + row[1] * g + row[2] * b + row[3] * a + row[4]).clamp(0.0, 1.0)
        });
        let channel = |v: f32| (v * alpha * 255.0 + 0.5) as u8;
        (*pr, *pg, *pb, *pa) = (
            channel(nr),
            channel(ng),
            channel(nb),
            (alpha * 255.0 + 0.5) as u8,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tiny_skia::{Color, Pixmap};

    #[test]
    fn blur_spreads_a_dot_and_keeps_its_weight() {
        let mut pixmap = Pixmap::new(21, 21).unwrap();
        let i = (10 * 21 + 10) * 4;
        pixmap.data_mut()[i..i + 4].copy_from_slice(&[255, 255, 255, 255]);
        blur(&mut pixmap, 2.0);
        let alpha: u32 = pixmap
            .data()
            .as_chunks::<4>()
            .0
            .iter()
            .map(|px| u32::from(px[3]))
            .sum();
        assert!(pixmap.data()[i + 3] < 255);
        assert!(
            pixmap.data()[((10 * 21 + 12) * 4) + 3] > 0,
            "the blur reaches 2 px over"
        );
        assert!(
            (200..=310).contains(&alpha),
            "about one pixel's worth of alpha: {alpha}"
        );
    }

    #[test]
    fn invert_and_grayscale_map_colors() {
        let mut pixmap = Pixmap::new(1, 1).unwrap();
        pixmap.fill(Color::from_rgba8(255, 0, 0, 255));
        apply(&mut pixmap, ColorFilter::Invert(1.0));
        assert_eq!(pixmap.data(), &[0, 255, 255, 255]);
        apply(&mut pixmap, ColorFilter::Grayscale(1.0));
        let px = pixmap.data();
        assert_eq!(px[0], px[1]);
        assert_eq!(px[1], px[2]);
    }
}
