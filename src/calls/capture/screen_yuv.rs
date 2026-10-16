//! Planar YUV pictures (I420 and NV12, what PipeWire screen casts may
//! deliver) to the call's I420 frame size: nearest-pixel scaling of each
//! plane, with any row stride.
//!
//! Plain Rust with no OS calls, so it is unit-tested on every platform.
#![deny(clippy::indexing_slicing)]

/// One plane of a picture: its bytes and the distance between rows.
#[derive(Debug, Clone, Copy)]
pub struct Plane<'a> {
    pub data: &'a [u8],
    pub stride: usize,
}

impl<'a> Plane<'a> {
    pub fn new(data: &'a [u8], stride: usize) -> Self {
        Self { data, stride }
    }

    /// `width` bytes of row `y`, or `None` past the end.
    fn row(&self, y: usize, width: usize) -> Option<&'a [u8]> {
        let start = y.checked_mul(self.stride)?;
        self.data.get(start..start.checked_add(width)?)
    }
}

/// Source and output sizes worked out once per frame.
struct Scale {
    src: (usize, usize),
    dst: (usize, usize),
}

impl Scale {
    fn new(src: (usize, usize), dst: (usize, usize)) -> Option<Self> {
        let ok = src.0 > 0
            && src.1 > 0
            && dst.0 > 0
            && dst.1 > 0
            && dst.0.is_multiple_of(2)
            && dst.1.is_multiple_of(2);
        ok.then_some(Self { src, dst })
    }

    /// Chroma planes are half size, rounded up for odd pictures.
    fn chroma_src(&self) -> (usize, usize) {
        (self.src.0.div_ceil(2), self.src.1.div_ceil(2))
    }

    fn chroma_dst(&self) -> (usize, usize) {
        (self.dst.0 / 2, self.dst.1 / 2)
    }
}

/// Nearest-pixel copy of a `src` plane (`step` bytes per sample, starting
/// at byte `offset` of each sample) into `out`, `dst` samples in size.
fn scale_plane(
    plane: Plane<'_>,
    (src_w, src_h): (usize, usize),
    (dst_w, dst_h): (usize, usize),
    step: usize,
    offset: usize,
    out: &mut [u8],
) -> Option<()> {
    let columns: Vec<usize> = (0..dst_w)
        .map(|x| x * src_w / dst_w * step + offset)
        .collect();
    let row_bytes = src_w.checked_mul(step)?;
    for (y, out_row) in out.chunks_exact_mut(dst_w).take(dst_h).enumerate() {
        let row = plane.row(y * src_h / dst_h, row_bytes)?;
        for (out, &x) in out_row.iter_mut().zip(&columns) {
            *out = *row.get(x)?;
        }
    }
    Some(())
}

/// An I420 picture (three planes) at `src` size to a packed I420 frame at
/// `dst` size. `dst` must be even; `None` when the planes are too short.
pub fn i420_scaled(
    planes: [Plane<'_>; 3],
    src: (usize, usize),
    dst: (usize, usize),
) -> Option<Vec<u8>> {
    let scale = Scale::new(src, dst)?;
    let [y, u, v] = planes;
    let (cw, ch) = scale.chroma_dst();
    let mut out = vec![0u8; dst.0 * dst.1 + 2 * cw * ch];
    let (y_out, chroma) = out.split_at_mut(dst.0 * dst.1);
    let (u_out, v_out) = chroma.split_at_mut(cw * ch);
    scale_plane(y, src, dst, 1, 0, y_out)?;
    scale_plane(u, scale.chroma_src(), (cw, ch), 1, 0, u_out)?;
    scale_plane(v, scale.chroma_src(), (cw, ch), 1, 0, v_out)?;
    Some(out)
}

/// An NV12 picture (a Y plane and interleaved UV) at `src` size to a
/// packed I420 frame at `dst` size.
pub fn nv12_scaled(
    y: Plane<'_>,
    uv: Plane<'_>,
    src: (usize, usize),
    dst: (usize, usize),
) -> Option<Vec<u8>> {
    let scale = Scale::new(src, dst)?;
    let (cw, ch) = scale.chroma_dst();
    let mut out = vec![0u8; dst.0 * dst.1 + 2 * cw * ch];
    let (y_out, chroma) = out.split_at_mut(dst.0 * dst.1);
    let (u_out, v_out) = chroma.split_at_mut(cw * ch);
    scale_plane(y, src, dst, 1, 0, y_out)?;
    scale_plane(uv, scale.chroma_src(), (cw, ch), 2, 0, u_out)?;
    scale_plane(uv, scale.chroma_src(), (cw, ch), 2, 1, v_out)?;
    Some(out)
}

#[cfg(test)]
#[allow(clippy::indexing_slicing)]
mod tests {
    use super::*;

    /// A plane where each sample is `base + x + 10 * y`, rows padded.
    fn ramp(w: usize, h: usize, stride: usize, base: u8) -> Vec<u8> {
        let mut out = vec![0xee; stride * h];
        for y in 0..h {
            for x in 0..w {
                out[y * stride + x] = base + (x + 10 * y) as u8;
            }
        }
        out
    }

    #[test]
    fn i420_at_its_own_size_is_copied_without_the_padding() {
        let y = ramp(4, 4, 6, 0);
        let u = ramp(2, 2, 3, 100);
        let v = ramp(2, 2, 3, 200);
        let out = i420_scaled(
            [Plane::new(&y, 6), Plane::new(&u, 3), Plane::new(&v, 3)],
            (4, 4),
            (4, 4),
        )
        .expect("i420");
        assert_eq!(&out[..4], &[0, 1, 2, 3]);
        assert_eq!(&out[12..16], &[30, 31, 32, 33]);
        assert_eq!(&out[16..20], &[100, 101, 110, 111]);
        assert_eq!(&out[20..], &[200, 201, 210, 211]);
    }

    #[test]
    fn i420_halves_by_taking_every_other_sample() {
        let y = ramp(8, 4, 8, 0);
        let u = ramp(4, 2, 4, 100);
        let v = ramp(4, 2, 4, 200);
        let out = i420_scaled(
            [Plane::new(&y, 8), Plane::new(&u, 4), Plane::new(&v, 4)],
            (8, 4),
            (4, 2),
        )
        .expect("i420");
        assert_eq!(out.len(), 4 * 2 * 3 / 2);
        assert_eq!(&out[..8], &[0, 2, 4, 6, 20, 22, 24, 26]);
        assert_eq!(&out[8..10], &[100, 102]);
        assert_eq!(&out[10..], &[200, 202]);
    }

    #[test]
    fn nv12_splits_its_chroma() {
        let y = ramp(2, 2, 2, 0);
        // U = 50, V = 60, padded row.
        let uv = vec![50, 60, 0xee, 0xee];
        let out = nv12_scaled(Plane::new(&y, 2), Plane::new(&uv, 4), (2, 2), (2, 2)).expect("i420");
        assert_eq!(out, vec![0, 1, 10, 11, 50, 60]);
    }

    #[test]
    fn short_planes_and_odd_outputs_are_refused() {
        let y = ramp(4, 4, 4, 0);
        let c = ramp(2, 2, 2, 0);
        let planes = [Plane::new(&y, 4), Plane::new(&c, 2), Plane::new(&c, 2)];
        assert!(i420_scaled(planes, (4, 4), (3, 4)).is_none());
        assert!(i420_scaled(planes, (4, 4), (0, 0)).is_none());
        let short = [
            Plane::new(&y[..10], 4),
            Plane::new(&c, 2),
            Plane::new(&c, 2),
        ];
        assert!(i420_scaled(short, (4, 4), (4, 4)).is_none());
        assert!(nv12_scaled(Plane::new(&y, 4), Plane::new(&c[..2], 4), (4, 4), (4, 4)).is_none());
    }

    #[test]
    #[ignore]
    fn zz_timing_probe() {
        let (w, h) = (1920usize, 1080usize);
        let y: Vec<u8> = (0..w * h).map(|i| (i % 251) as u8).collect();
        let c: Vec<u8> = (0..w * h / 4).map(|i| (i % 241) as u8).collect();
        let uv: Vec<u8> = (0..w * h / 2).map(|i| (i % 239) as u8).collect();
        let runs = 20;
        let t = std::time::Instant::now();
        for _ in 0..runs {
            std::hint::black_box(i420_scaled(
                [
                    Plane::new(&y, w),
                    Plane::new(&c, w / 2),
                    Plane::new(&c, w / 2),
                ],
                (w, h),
                (w, h),
            ));
        }
        let i420 = t.elapsed() / runs;
        let t = std::time::Instant::now();
        for _ in 0..runs {
            std::hint::black_box(nv12_scaled(
                Plane::new(&y, w),
                Plane::new(&uv, w),
                (w, h),
                (w, h),
            ));
        }
        let nv12 = t.elapsed() / runs;
        let big: Vec<u8> = (0..2560 * 1440 * 4).map(|i| (i % 251) as u8).collect();
        let t = std::time::Instant::now();
        for _ in 0..runs {
            std::hint::black_box(crate::calls::capture::screen_bgra::packed_to_i420(
                &big,
                crate::calls::capture::screen_bgra::PixelOrder::Rgb,
                2560 * 4,
                (2560, 1440),
                (w, h),
            ));
        }
        let rgbx = t.elapsed() / runs;
        println!("PROBE i420 {i420:?} nv12 {nv12:?} rgbx1440 {rgbx:?}");
    }
}
