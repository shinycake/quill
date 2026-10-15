//! A raw video picture as PipeWire hands it over (one of the formats in
//! [`VideoFormat`], with per-plane strides and an optional crop) to the
//! call's I420 frame at the call's size.
//!
//! I420 and NV12 are scaled plane by plane (nearest sample); the packed
//! formats go through `screen_bgra`. Plain Rust with no OS calls, tested on
//! every platform.
#![deny(clippy::indexing_slicing)]

use super::screen_bgra::{PackedOrder, packed_to_i420};
use super::spa_pod::VideoFormat;

/// One plane: its bytes from the first row on, and the bytes per row.
#[derive(Debug, Clone, Copy)]
pub struct RawPlane<'a> {
    pub data: &'a [u8],
    pub stride: usize,
}

/// A picture in one of PipeWire's raw formats.
#[derive(Debug, Clone)]
pub struct RawPicture<'a> {
    pub format: VideoFormat,
    pub width: usize,
    pub height: usize,
    /// One plane for packed formats, two for NV12, three for I420.
    pub planes: Vec<RawPlane<'a>>,
}

/// The visible part of a picture (`SPA_META_VideoCrop`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Crop {
    pub x: usize,
    pub y: usize,
    pub width: usize,
    pub height: usize,
}

impl<'a> RawPicture<'a> {
    /// The planes of a picture that came as one block of memory, laid out
    /// the way PipeWire's single-buffer I420 and NV12 are (each plane
    /// right after the previous one, chroma rows half the luma stride).
    pub fn from_single_buffer(
        format: VideoFormat,
        width: usize,
        height: usize,
        data: &'a [u8],
        stride: usize,
    ) -> Option<Self> {
        let luma_len = stride.checked_mul(height)?;
        let chroma_rows = height.div_ceil(2);
        let planes = match format {
            VideoFormat::I420 => {
                let chroma_stride = stride.div_ceil(2);
                let chroma_len = chroma_stride.checked_mul(chroma_rows)?;
                let (y, rest) = data.split_at_checked(luma_len)?;
                let (u, v) = rest.split_at_checked(chroma_len)?;
                vec![
                    RawPlane { data: y, stride },
                    RawPlane {
                        data: u,
                        stride: chroma_stride,
                    },
                    RawPlane {
                        data: v,
                        stride: chroma_stride,
                    },
                ]
            }
            VideoFormat::Nv12 => {
                let (y, uv) = data.split_at_checked(luma_len)?;
                vec![RawPlane { data: y, stride }, RawPlane { data: uv, stride }]
            }
            _ => vec![RawPlane { data, stride }],
        };
        Some(Self {
            format,
            width,
            height,
            planes,
        })
    }

    /// The same picture cut to `crop` (clamped to the picture; planar
    /// formats keep an even origin). `None` when nothing is left.
    pub fn cropped(&self, crop: Crop) -> Option<Self> {
        let planar = matches!(self.format, VideoFormat::I420 | VideoFormat::Nv12);
        let (mut x, mut y) = (crop.x.min(self.width), crop.y.min(self.height));
        if planar {
            x &= !1;
            y &= !1;
        }
        let width = crop.width.min(self.width - x);
        let height = crop.height.min(self.height - y);
        if width == 0 || height == 0 {
            return None;
        }
        let shift = |plane: &RawPlane<'a>, col_bytes: usize, row: usize| {
            let at = row.checked_mul(plane.stride)?.checked_add(col_bytes)?;
            Some(RawPlane {
                data: plane.data.get(at..)?,
                stride: plane.stride,
            })
        };
        let planes = match self.format {
            VideoFormat::I420 => vec![
                shift(self.planes.first()?, x, y)?,
                shift(self.planes.get(1)?, x / 2, y / 2)?,
                shift(self.planes.get(2)?, x / 2, y / 2)?,
            ],
            VideoFormat::Nv12 => vec![
                shift(self.planes.first()?, x, y)?,
                shift(self.planes.get(1)?, x, y / 2)?,
            ],
            _ => vec![shift(self.planes.first()?, x.checked_mul(4)?, y)?],
        };
        Some(Self {
            format: self.format,
            width,
            height,
            planes,
        })
    }
}

/// `picture` scaled to `dst` (even sides) as I420. `None` when the planes
/// are too short for the picture or the sizes are unusable.
pub fn raw_to_i420(picture: &RawPicture<'_>, (dst_w, dst_h): (usize, usize)) -> Option<Vec<u8>> {
    let (w, h) = (picture.width, picture.height);
    if w == 0 || h == 0 || dst_w == 0 || dst_h == 0 {
        return None;
    }
    if !dst_w.is_multiple_of(2) || !dst_h.is_multiple_of(2) {
        return None;
    }
    let packed = |order| {
        let plane = picture.planes.first()?;
        packed_to_i420(plane.data, order, plane.stride, (w, h), (dst_w, dst_h))
    };
    match picture.format {
        VideoFormat::Bgrx | VideoFormat::Bgra => packed(PackedOrder::Bgra),
        VideoFormat::Rgbx | VideoFormat::Rgba => packed(PackedOrder::Rgba),
        VideoFormat::I420 => {
            let mut out = Vec::with_capacity(dst_w * dst_h * 3 / 2);
            scale_plane(
                picture.planes.first()?,
                (w, h),
                (dst_w, dst_h),
                1,
                0,
                &mut out,
            )?;
            let chroma = (w.div_ceil(2), h.div_ceil(2));
            let dst_chroma = (dst_w / 2, dst_h / 2);
            scale_plane(picture.planes.get(1)?, chroma, dst_chroma, 1, 0, &mut out)?;
            scale_plane(picture.planes.get(2)?, chroma, dst_chroma, 1, 0, &mut out)?;
            Some(out)
        }
        VideoFormat::Nv12 => {
            let mut out = Vec::with_capacity(dst_w * dst_h * 3 / 2);
            scale_plane(
                picture.planes.first()?,
                (w, h),
                (dst_w, dst_h),
                1,
                0,
                &mut out,
            )?;
            let chroma = (w.div_ceil(2), h.div_ceil(2));
            let dst_chroma = (dst_w / 2, dst_h / 2);
            let uv = picture.planes.get(1)?;
            scale_plane(uv, chroma, dst_chroma, 2, 0, &mut out)?;
            scale_plane(uv, chroma, dst_chroma, 2, 1, &mut out)?;
            Some(out)
        }
    }
}

/// Append `plane` (`src` samples, each `step` bytes apart starting at
/// byte `offset`) scaled to `dst` by nearest sample.
fn scale_plane(
    plane: &RawPlane<'_>,
    (src_w, src_h): (usize, usize),
    (dst_w, dst_h): (usize, usize),
    step: usize,
    offset: usize,
    out: &mut Vec<u8>,
) -> Option<()> {
    let row_bytes = src_w.checked_mul(step)?;
    if plane.stride < row_bytes {
        return None;
    }
    let columns: Vec<usize> = (0..dst_w)
        .map(|x| x * src_w / dst_w * step + offset)
        .collect();
    for y in 0..dst_h {
        let start = (y * src_h / dst_h).checked_mul(plane.stride)?;
        let row = plane.data.get(start..start.checked_add(row_bytes)?)?;
        if step == 1 && src_w == dst_w {
            out.extend_from_slice(row);
            continue;
        }
        for &x in &columns {
            out.push(*row.get(x)?);
        }
    }
    Some(())
}

#[cfg(test)]
#[allow(clippy::indexing_slicing)]
mod tests {
    use super::*;

    /// A `w` x `h` I420 picture whose Y samples count up, with `pad` spare
    /// bytes per luma row, as one block.
    fn i420_block(w: usize, h: usize, pad: usize) -> (Vec<u8>, usize) {
        let stride = w + pad;
        let cstride = stride.div_ceil(2);
        let mut data = vec![0u8; stride * h + 2 * cstride * h.div_ceil(2)];
        for y in 0..h {
            for x in 0..w {
                data[y * stride + x] = (y * w + x) as u8;
            }
        }
        let u_at = stride * h;
        let v_at = u_at + cstride * h.div_ceil(2);
        for y in 0..h / 2 {
            for x in 0..w / 2 {
                data[u_at + y * cstride + x] = 100 + (y * w / 2 + x) as u8;
                data[v_at + y * cstride + x] = 200 + (y * w / 2 + x) as u8;
            }
        }
        (data, stride)
    }

    #[test]
    fn i420_at_its_own_size_drops_the_row_padding() {
        let (data, stride) = i420_block(4, 2, 4);
        let picture =
            RawPicture::from_single_buffer(VideoFormat::I420, 4, 2, &data, stride).unwrap();
        let out = raw_to_i420(&picture, (4, 2)).unwrap();
        assert_eq!(out, [0, 1, 2, 3, 4, 5, 6, 7, 100, 101, 200, 201]);
    }

    #[test]
    fn i420_scales_down_by_nearest_sample() {
        let (data, stride) = i420_block(8, 4, 0);
        let picture =
            RawPicture::from_single_buffer(VideoFormat::I420, 8, 4, &data, stride).unwrap();
        let out = raw_to_i420(&picture, (4, 2)).unwrap();
        assert_eq!(&out[..8], &[0, 2, 4, 6, 16, 18, 20, 22]);
        assert_eq!(&out[8..], &[100, 102, 200, 202]);
    }

    #[test]
    fn nv12_splits_its_chroma() {
        // 4 x 2: Y 0..8, then UV pairs (10, 20), (11, 21).
        let mut data: Vec<u8> = (0..8).collect();
        data.extend_from_slice(&[10, 20, 11, 21]);
        let picture = RawPicture::from_single_buffer(VideoFormat::Nv12, 4, 2, &data, 4).unwrap();
        let out = raw_to_i420(&picture, (4, 2)).unwrap();
        assert_eq!(out, [0, 1, 2, 3, 4, 5, 6, 7, 10, 11, 20, 21]);
    }

    #[test]
    fn packed_formats_go_through_the_packed_path() {
        let white = vec![255u8; 4 * 4 * 2];
        for format in [
            VideoFormat::Bgrx,
            VideoFormat::Bgra,
            VideoFormat::Rgbx,
            VideoFormat::Rgba,
        ] {
            let picture = RawPicture::from_single_buffer(format, 4, 2, &white, 16).unwrap();
            let out = raw_to_i420(&picture, (4, 2)).unwrap();
            assert!(out[..8].iter().all(|&y| y == 235), "{format:?}");
            assert!(out[8..].iter().all(|&c| c == 128), "{format:?}");
        }
    }

    #[test]
    fn a_crop_cuts_every_plane() {
        let (data, stride) = i420_block(8, 4, 0);
        let picture =
            RawPicture::from_single_buffer(VideoFormat::I420, 8, 4, &data, stride).unwrap();
        // An odd origin moves to the even sample below it.
        let crop = Crop {
            x: 3,
            y: 2,
            width: 4,
            height: 2,
        };
        let cut = picture.cropped(crop).unwrap();
        assert_eq!((cut.width, cut.height), (4, 2));
        let out = raw_to_i420(&cut, (4, 2)).unwrap();
        assert_eq!(&out[..8], &[18, 19, 20, 21, 26, 27, 28, 29]);
        assert_eq!(&out[8..], &[105, 106, 205, 206]);
        // Clamped to the picture; nothing left is refused.
        let wide = picture
            .cropped(Crop {
                x: 6,
                y: 0,
                width: 100,
                height: 100,
            })
            .unwrap();
        assert_eq!((wide.width, wide.height), (2, 4));
        assert!(
            picture
                .cropped(Crop {
                    x: 8,
                    y: 0,
                    width: 4,
                    height: 4,
                })
                .is_none()
        );
        let packed = vec![0u8; 8 * 4 * 4];
        let bgrx = RawPicture::from_single_buffer(VideoFormat::Bgrx, 8, 4, &packed, 32).unwrap();
        let cut = bgrx
            .cropped(Crop {
                x: 1,
                y: 1,
                width: 2,
                height: 2,
            })
            .unwrap();
        assert_eq!(cut.planes[0].data.len(), packed.len() - 32 - 4);
    }

    #[test]
    fn short_planes_and_bad_sizes_are_refused() {
        let (data, stride) = i420_block(4, 4, 0);
        assert!(
            RawPicture::from_single_buffer(VideoFormat::I420, 4, 4, &data[..10], stride).is_none()
        );
        let picture =
            RawPicture::from_single_buffer(VideoFormat::I420, 4, 4, &data, stride).unwrap();
        assert!(raw_to_i420(&picture, (3, 4)).is_none());
        assert!(raw_to_i420(&picture, (0, 4)).is_none());
        let short = RawPicture {
            planes: vec![RawPlane {
                data: &data[..8],
                stride: 4,
            }],
            ..picture.clone()
        };
        assert!(raw_to_i420(&short, (4, 4)).is_none());
        let narrow = RawPicture {
            planes: vec![RawPlane {
                data: &data,
                stride: 2,
            }],
            format: VideoFormat::Bgrx,
            ..picture
        };
        assert!(raw_to_i420(&narrow, (4, 4)).is_none());
    }
}
