//! NV12 pictures: the layout the shim hands over without a colour
//! conversion when asked (`qv_set_output`), for renderers that draw YUV
//! themselves (Windows, `docs/decisions/video-player-nv12.md`). Packing
//! the planes tightly, turning them upright, the colour matrices, and a
//! CPU conversion to BGRA for when a renderer can't draw them after all.
#![deny(clippy::indexing_slicing)]

/// The picture layout asked of the decoder.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PixelOutput {
    /// BGRA, converted by the decoder (every picture).
    #[default]
    Bgra,
    /// NV12 where the stream allows (8-bit 4:2:0, BT.601/709, even sides);
    /// BGRA for the rest.
    Nv12,
}

/// The matrix a YUV picture was encoded with.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum YuvMatrix {
    Bt601,
    #[default]
    Bt709,
}

/// `QV_MATRIX_BT601` / `QV_MATRIX_BT709` in `quillvideo.h`.
pub(super) const SHIM_BT601: i32 = 1;
pub(super) const SHIM_BT709: i32 = 2;

impl YuvMatrix {
    /// The matrix the shim reported; an unknown value falls back to the
    /// shim's own rule for untagged streams (HD is BT.709, SD BT.601).
    pub fn from_shim(code: i32, height: u32) -> Self {
        match code {
            SHIM_BT601 => YuvMatrix::Bt601,
            SHIM_BT709 => YuvMatrix::Bt709,
            _ if height >= 720 => YuvMatrix::Bt709,
            _ => YuvMatrix::Bt601,
        }
    }
}

/// How a YUV picture's samples map to RGB.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct YuvColor {
    pub matrix: YuvMatrix,
    /// Full range (luma 0-255); otherwise limited (16-235).
    pub full_range: bool,
}

impl YuvColor {
    /// The affine map from (Y, Cb, Cr) in 0..1 to R'G'B' in 0..1, row by
    /// row: `[y, cb, cr, offset]` coefficients. The same numbers as the
    /// vendored GPUI core's `YuvColor::rgb_rows`, which the DirectX
    /// renderer uploads (a UI test keeps the two equal).
    pub fn rgb_rows(self) -> [[f32; 4]; 3] {
        let (kr, kb) = match self.matrix {
            YuvMatrix::Bt601 => (0.299_f32, 0.114_f32),
            YuvMatrix::Bt709 => (0.2126_f32, 0.0722_f32),
        };
        let kg = 1.0 - kr - kb;
        let (y0, ys, cs) = if self.full_range {
            (0.0, 1.0, 1.0)
        } else {
            (16.0 / 255.0, 255.0 / 219.0, 255.0 / 224.0)
        };
        let c0 = 128.0 / 255.0;
        let r_cr = 2.0 * (1.0 - kr) * cs;
        let g_cb = -2.0 * kb * (1.0 - kb) / kg * cs;
        let g_cr = -2.0 * kr * (1.0 - kr) / kg * cs;
        let b_cb = 2.0 * (1.0 - kb) * cs;
        let offset = |cb: f32, cr: f32| -y0 * ys - c0 * (cb + cr);
        [
            [ys, 0.0, r_cr, offset(0.0, r_cr)],
            [ys, g_cb, g_cr, offset(g_cb, g_cr)],
            [ys, b_cb, 0.0, offset(b_cb, 0.0)],
        ]
    }
}

/// A decoded picture's pixels, upright and tightly packed.
#[derive(Clone, Debug, PartialEq)]
pub enum Pixels {
    /// Four bytes a pixel: B, G, R, A.
    Bgra(Vec<u8>),
    /// The luma plane (`width * height`), then `height / 2` rows of
    /// `width / 2` interleaved (Cb, Cr) pairs. Even sides.
    Nv12 {
        bytes: std::sync::Arc<[u8]>,
        color: YuvColor,
    },
}

impl Pixels {
    /// The BGRA bytes, when the picture is BGRA.
    pub fn bgra(&self) -> Option<&[u8]> {
        match self {
            Pixels::Bgra(bytes) => Some(bytes),
            Pixels::Nv12 { .. } => None,
        }
    }

    /// Bytes held (what a renderer uploads for the picture).
    pub fn len(&self) -> usize {
        match self {
            Pixels::Bgra(bytes) => bytes.len(),
            Pixels::Nv12 { bytes, .. } => bytes.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The layout's name in `QUILL_TRACE_VIDEO` lines.
    pub fn trace_name(&self) -> &'static str {
        match self {
            Pixels::Bgra(_) => "bgra",
            Pixels::Nv12 { .. } => "nv12",
        }
    }
}

/// Bytes of a tightly packed NV12 picture; `None` for a zero or odd side.
pub fn nv12_len(width: u32, height: u32) -> Option<usize> {
    if width == 0 || height == 0 || !width.is_multiple_of(2) || !height.is_multiple_of(2) {
        return None;
    }
    let luma = (width as usize).checked_mul(height as usize)?;
    luma.checked_add(luma / 2)
}

/// Bytes `rows` rows of `row` bytes span at `stride` apart (the last row
/// may stop at its pixels); `None` when a row is wider than its stride.
pub(super) fn plane_span(stride: usize, rows: usize, row: usize) -> Option<usize> {
    if row > stride || rows == 0 {
        return None;
    }
    (rows - 1).checked_mul(stride)?.checked_add(row)
}

/// Turn a plane of `N`-byte elements (`width` x `height`, rows `stride`
/// bytes apart) upright by `rotation` clockwise degrees into `out`,
/// tightly packed. `false` (and nothing written) when `src` is shorter
/// than its rows say.
pub(super) fn turn_plane_into<const N: usize>(
    src: &[u8],
    (width, height): (usize, usize),
    stride: usize,
    rotation: u32,
    out: &mut Vec<u8>,
) -> bool {
    let fits = width
        .checked_mul(N)
        .and_then(|row| plane_span(stride, height, row))
        .is_some_and(|need| need <= src.len());
    if !fits {
        return false;
    }
    let (w, h) = (width, height);
    let at = |x: usize, y: usize| -> [u8; N] {
        let start = y * stride + x * N;
        src.get(start..start + N)
            .and_then(|p| p.try_into().ok())
            .unwrap_or([0; N])
    };
    match rotation {
        90 | 180 | 270 => {
            let (ow, oh) = if rotation == 180 { (w, h) } else { (h, w) };
            out.reserve(ow * oh * N);
            for oy in 0..oh {
                for ox in 0..ow {
                    let (sx, sy) = match rotation {
                        // Clockwise: the left column becomes the top row.
                        90 => (oy, h - 1 - ox),
                        180 => (w - 1 - ox, h - 1 - oy),
                        _ => (w - 1 - oy, ox),
                    };
                    out.extend_from_slice(&at(sx, sy));
                }
            }
        }
        _ => {
            let row = w * N;
            out.reserve(row * h);
            for y in 0..h {
                out.extend_from_slice(src.get(y * stride..y * stride + row).unwrap_or_default());
            }
        }
    }
    true
}

/// An NV12 picture from the shim's planes, packed tightly and turned
/// upright: the bytes and the upright size. `None` for an odd or zero
/// side, a stride narrower than a row, or planes shorter than their rows
/// (`luma` and `chroma` are what the shim handed over).
pub fn pack_nv12(
    luma: &[u8],
    luma_stride: usize,
    chroma: &[u8],
    chroma_stride: usize,
    (width, height): (u32, u32),
    rotation: u32,
) -> Option<(Vec<u8>, u32, u32)> {
    let total = nv12_len(width, height)?;
    let (w, h) = (width as usize, height as usize);
    let mut out = Vec::with_capacity(total);
    if !turn_plane_into::<1>(luma, (w, h), luma_stride, rotation, &mut out) {
        return None;
    }
    if !turn_plane_into::<2>(chroma, (w / 2, h / 2), chroma_stride, rotation, &mut out) {
        return None;
    }
    if out.len() != total {
        return None;
    }
    let (ow, oh) = match rotation {
        90 | 270 => (height, width),
        _ => (width, height),
    };
    Some((out, ow, oh))
}

/// Convert a tightly packed NV12 picture to BGRA on the CPU (opaque),
/// with the same matrix the GPU path uses. `None` when `bytes` doesn't
/// hold a `width` x `height` NV12 picture.
pub fn nv12_to_bgra(bytes: &[u8], width: u32, height: u32, color: YuvColor) -> Option<Vec<u8>> {
    let total = nv12_len(width, height)?;
    if bytes.len() != total {
        return None;
    }
    let (w, h) = (width as usize, height as usize);
    let (luma, chroma) = bytes.split_at_checked(w * h)?;
    let rows = color.rgb_rows();
    let channel = |row: &[f32; 4], y: f32, cb: f32, cr: f32| -> u8 {
        let value = row[0] * y + row[1] * cb + row[2] * cr + row[3];
        (value.clamp(0.0, 1.0) * 255.0).round() as u8
    };
    let [r_row, g_row, b_row] = &rows;
    let mut out = Vec::with_capacity(w * h * 4);
    for (y, luma_row) in luma.chunks_exact(w).enumerate() {
        let chroma_row = chroma.get((y / 2) * w..(y / 2) * w + w).unwrap_or_default();
        for (x, &sample) in luma_row.iter().enumerate() {
            let pair = chroma_row.get((x / 2) * 2..(x / 2) * 2 + 2);
            let (cb, cr) = match pair {
                Some(&[cb, cr]) => (cb, cr),
                _ => (128, 128),
            };
            let (yf, cbf, crf) = (
                f32::from(sample) / 255.0,
                f32::from(cb) / 255.0,
                f32::from(cr) / 255.0,
            );
            out.extend_from_slice(&[
                channel(b_row, yf, cbf, crf),
                channel(g_row, yf, cbf, crf),
                channel(r_row, yf, cbf, crf),
                255,
            ]);
        }
    }
    Some(out)
}
