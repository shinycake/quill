//! Gain-map JPEGs: Android's Ultra HDR and the ISO 21496-1 files other
//! cameras and editors write. The file is an ordinary JPEG (the SDR
//! picture) followed by a second, smaller JPEG, the gain map, located
//! through the Multi-Picture Format index (or the XMP `GContainer`
//! directory of Ultra HDR 1.0). Its metadata says how much each pixel
//! brightens: ISO 21496-1 binary (`urn:iso:std:iso:ts:21496:-1` in APP2)
//! or Adobe's `hdrgm` XMP.
//!
//! The HDR rendition is the gain map applied in full (`weight = 1`):
//! `hdr = (sdr + offset_sdr) * 2^(min * (1 - g) + max * g) - offset_hdr`,
//! with `g` the gain map value raised to `1 / gamma`. The display's own
//! headroom is applied later by the shader's tone map. The base picture is
//! colour-managed to sRGB before the gain is applied, which clips colours
//! outside sRGB that a wide-gamut base holds.

use super::{LinearImage, srgb_to_linear, target_size};
use crate::ui::image_decode::{DecodeError, area_allowed, to_srgb};
use image::{DynamicImage, ImageDecoder, codecs::jpeg::JpegDecoder, metadata::Orientation};
use std::io::Cursor;
use std::ops::Range;

const XMP_PREFIX: &[u8] = b"http://ns.adobe.com/xap/1.0/\0";
const MPF_PREFIX: &[u8] = b"MPF\0";
const ISO_PREFIX: &[u8] = b"urn:iso:std:iso:ts:21496:-1\0";

const APP1: u8 = 0xe1;
const APP2: u8 = 0xe2;

/// The `MPEntry` tag of the MPF index.
const MP_ENTRY_TAG: u16 = 0xb002;

/// Tags read from an MPF index before giving up on a damaged one.
const MAX_MPF_TAGS: usize = 64;

/// One marker segment of a JPEG header.
pub(super) struct Segment<'a> {
    pub marker: u8,
    /// Offset of `data` in the file.
    pub start: usize,
    pub data: &'a [u8],
}

/// The header segments of the JPEG at the start of `jpeg`, up to its first
/// scan.
pub(super) fn segments(jpeg: &[u8]) -> Vec<Segment<'_>> {
    let mut out = Vec::new();
    if !jpeg.starts_with(&[0xff, 0xd8]) {
        return out;
    }
    let mut at = 2;
    while jpeg.get(at) == Some(&0xff) {
        let mut marker_at = at + 1;
        while jpeg.get(marker_at) == Some(&0xff) {
            marker_at += 1;
        }
        let Some(&marker) = jpeg.get(marker_at) else {
            break;
        };
        // Markers without a length.
        if marker == 0x01 || (0xd0..=0xd8).contains(&marker) {
            at = marker_at + 1;
            continue;
        }
        // The scan (or the end): no more header segments.
        if marker == 0xda || marker == 0xd9 {
            break;
        }
        let Some(len) = be_u16(jpeg, marker_at + 1) else {
            break;
        };
        let len = usize::from(len);
        if len < 2 {
            break;
        }
        let start = marker_at + 3;
        let end = marker_at + 1 + len;
        let Some(data) = jpeg.get(start..end) else {
            break;
        };
        out.push(Segment {
            marker,
            start,
            data,
        });
        at = end;
    }
    out
}

/// How a gain map brightens the base picture, per red, green and blue.
/// `min` and `max` are log2 boosts.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct GainMap {
    pub min: [f32; 3],
    pub max: [f32; 3],
    pub gamma: [f32; 3],
    pub offset_sdr: [f32; 3],
    pub offset_hdr: [f32; 3],
}

impl GainMap {
    fn valid(self) -> Option<Self> {
        let all = [self.min, self.max, self.gamma, self.offset_sdr, self.offset_hdr];
        let finite = all.iter().flatten().all(|value| value.is_finite());
        let ordered = (0..3).all(|channel| {
            self.max.get(channel) >= self.min.get(channel)
                && self.gamma.get(channel).is_some_and(|gamma| *gamma > 0.0)
        });
        (finite && ordered).then_some(self)
    }
}

/// Whether the JPEG whose first bytes are `head` (of a `file_len` byte
/// file) announces a gain map and says where it is.
pub(super) fn probe(head: &[u8], file_len: u64) -> bool {
    let segments = segments(head);
    let announced = segments.iter().any(|segment| {
        (segment.marker == APP2 && segment.data.starts_with(ISO_PREFIX))
            || xmp(segment).is_some_and(|xmp| xmp.contains("hdrgm:Version"))
    });
    announced && gain_map_range(&segments, file_len).is_some()
}

/// The HDR rendition of the gain-map JPEG `file`, long edge at most
/// `long_edge`; `Ok(None)` when it has no usable gain map.
pub(super) fn decode(file: &[u8], long_edge: u32) -> Result<Option<LinearImage>, DecodeError> {
    let segments = segments(file);
    let Some(map_bytes) =
        gain_map_range(&segments, file.len() as u64).and_then(|range| file.get(range))
    else {
        return Ok(None);
    };
    let map_segments = self::segments(map_bytes);
    let gain_map = metadata(&map_segments).or_else(|| {
        // Some writers keep the `hdrgm` values in the primary's XMP.
        segments.iter().filter_map(xmp).find_map(xmp_gain_map)
    });
    let Some(gain_map) = gain_map else {
        return Ok(None);
    };

    let (base, icc, orientation) = read_jpeg(file)?;
    let (width, height) = (base.width(), base.height());
    area_allowed(width, height)?;
    let (target_width, target_height) = target_size((width, height), long_edge);
    let mut rgb = base.into_rgb8();
    if (target_width, target_height) != (width, height) {
        rgb = image::imageops::resize(
            &rgb,
            target_width,
            target_height,
            image::imageops::FilterType::Triangle,
        );
    }
    if let Some(icc) = icc {
        to_srgb(&mut rgb, moxcms::Layout::Rgb, &icc);
    }
    let mut base = DynamicImage::ImageRgb8(rgb);
    base.apply_orientation(orientation);

    let (map, _, _) = read_jpeg(map_bytes)?;
    let mut map = if map.color().channel_count() >= 3 {
        DynamicImage::ImageRgb8(map.into_rgb8())
    } else {
        DynamicImage::ImageLuma8(map.into_luma8())
    };
    // The gain map is stored the way the base picture is.
    map.apply_orientation(orientation);
    Ok(Some(apply(&base.into_rgb8(), &map, gain_map)))
}

fn read_jpeg(bytes: &[u8]) -> Result<(DynamicImage, Option<Vec<u8>>, Orientation), DecodeError> {
    let failed = |err: image::ImageError| DecodeError::Failed(err.to_string());
    let mut decoder = JpegDecoder::new(Cursor::new(bytes)).map_err(failed)?;
    let (width, height) = decoder.dimensions();
    area_allowed(width, height)?;
    let icc = decoder.icc_profile().ok().flatten();
    let orientation = decoder.orientation().unwrap_or(Orientation::NoTransforms);
    let image = DynamicImage::from_decoder(decoder).map_err(failed)?;
    Ok((image, icc, orientation))
}

/// Apply `gain_map` (sampled bilinearly to the base's size) to the sRGB
/// `base`.
fn apply(base: &image::RgbImage, map: &DynamicImage, gain_map: GainMap) -> LinearImage {
    let base_linear: Vec<f32> = (0..=255_u8)
        .map(|value| srgb_to_linear(f32::from(value) / 255.0))
        .collect();
    // The boost for each stored gain value, per channel.
    let boosts: Vec<Vec<f32>> = (0..3)
        .map(|channel| {
            let at = |values: [f32; 3]| values.get(channel).copied().unwrap_or(0.0);
            let (min, max, gamma) = (at(gain_map.min), at(gain_map.max), at(gain_map.gamma));
            (0..=255_u8)
                .map(|value| {
                    let gain = (f32::from(value) / 255.0).powf(1.0 / gamma);
                    (min * (1.0 - gain) + max * gain).exp2()
                })
                .collect()
        })
        .collect();
    let (width, height) = base.dimensions();
    let (map_width, map_height) = (map.width().max(1), map.height().max(1));
    let map_channels = usize::from(map.color().channel_count());
    let map_bytes = map.as_bytes();
    let gain_at = |x: u32, y: u32, channel: usize| -> u8 {
        let pixel = (y as usize) * (map_width as usize) + x as usize;
        let channel = if map_channels >= 3 { channel } else { 0 };
        map_bytes
            .get(pixel * map_channels + channel)
            .copied()
            .unwrap_or(0)
    };
    // Where output column `x` lands on the gain map: the two columns either
    // side and how far between them.
    let sample = |at: u32, size: u32, map_size: u32| -> (u32, u32, f32) {
        let position = ((at as f32 + 0.5) * map_size as f32 / size as f32 - 0.5)
            .clamp(0.0, (map_size - 1) as f32);
        let low = position.floor() as u32;
        (low, (low + 1).min(map_size - 1), position - low as f32)
    };
    let mut pixels = Vec::with_capacity((width as usize) * (height as usize));
    let columns: Vec<(u32, u32, f32)> = (0..width)
        .map(|x| sample(x, width, map_width))
        .collect();
    let base_bytes = base.as_raw();
    for y in 0..height {
        let (y0, y1, fy) = sample(y, height, map_height);
        for (x, &(x0, x1, fx)) in columns.iter().enumerate() {
            let mut out = [0.0_f32, 0.0, 0.0, 1.0];
            for (channel, value) in out.iter_mut().take(3).enumerate() {
                let Some(boost) = boosts.get(channel) else {
                    continue;
                };
                let boost_of = |x: u32, y: u32| {
                    boost
                        .get(usize::from(gain_at(x, y, channel)))
                        .copied()
                        .unwrap_or(1.0)
                };
                let top = lerp(boost_of(x0, y0), boost_of(x1, y0), fx);
                let bottom = lerp(boost_of(x0, y1), boost_of(x1, y1), fx);
                let factor = lerp(top, bottom, fy);
                let index = ((y as usize) * (width as usize) + x) * 3 + channel;
                let sdr = base_bytes
                    .get(index)
                    .and_then(|byte| base_linear.get(usize::from(*byte)))
                    .copied()
                    .unwrap_or(0.0);
                let offsets = |values: [f32; 3]| values.get(channel).copied().unwrap_or(0.0);
                *value = ((sdr + offsets(gain_map.offset_sdr)) * factor
                    - offsets(gain_map.offset_hdr))
                .max(0.0);
            }
            pixels.push(out);
        }
    }
    LinearImage {
        width,
        height,
        pixels,
    }
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// The gain map's metadata from its own header: ISO 21496-1 first (the
/// standard), then `hdrgm` XMP.
fn metadata(segments: &[Segment<'_>]) -> Option<GainMap> {
    segments
        .iter()
        .filter(|segment| segment.marker == APP2)
        .find_map(|segment| segment.data.strip_prefix(ISO_PREFIX).and_then(iso_gain_map))
        .or_else(|| segments.iter().filter_map(xmp).find_map(xmp_gain_map))
}

/// Where the gain map JPEG sits in the file: the second MPF image, else
/// the `GainMap` item of the XMP `GContainer` directory.
fn gain_map_range(segments: &[Segment<'_>], file_len: u64) -> Option<Range<usize>> {
    let file_len = usize::try_from(file_len).ok()?;
    let from_mpf = segments
        .iter()
        .filter(|segment| segment.marker == APP2)
        .find_map(|segment| {
            let tiff = segment.data.strip_prefix(MPF_PREFIX)?;
            let base = segment.start + MPF_PREFIX.len();
            mpf_entries(tiff)
                .into_iter()
                .find(|&(_, offset)| offset != 0)
                .and_then(|(size, offset)| {
                    let start = base.checked_add(usize::try_from(offset).ok()?)?;
                    let end = start.checked_add(usize::try_from(size).ok()?)?;
                    (end <= file_len && start < end).then_some(start..end)
                })
        });
    from_mpf.or_else(|| {
        let xmp = segments.iter().filter_map(xmp).next()?;
        gcontainer_gain_map(xmp, file_len)
    })
}

/// The `(size, offset)` of every image in an MPF index (`tiff` is the
/// segment after `MPF\0`; offsets count from its start).
fn mpf_entries(tiff: &[u8]) -> Vec<(u32, u32)> {
    let little = match tiff.get(..4) {
        Some(b"II*\0") => true,
        Some(b"MM\0*") => false,
        _ => return Vec::new(),
    };
    let u16_at = |at: usize| {
        let bytes: [u8; 2] = tiff.get(at..at + 2)?.try_into().ok()?;
        Some(if little {
            u16::from_le_bytes(bytes)
        } else {
            u16::from_be_bytes(bytes)
        })
    };
    let u32_at = |at: usize| {
        let bytes: [u8; 4] = tiff.get(at..at + 4)?.try_into().ok()?;
        Some(if little {
            u32::from_le_bytes(bytes)
        } else {
            u32::from_be_bytes(bytes)
        })
    };
    let Some(ifd) = u32_at(4).map(|ifd| ifd as usize) else {
        return Vec::new();
    };
    let tags = usize::from(u16_at(ifd).unwrap_or(0)).min(MAX_MPF_TAGS);
    let mut entries = Vec::new();
    for tag_index in 0..tags {
        let tag_at = ifd + 2 + tag_index * 12;
        if u16_at(tag_at) != Some(MP_ENTRY_TAG) {
            continue;
        }
        let (Some(count), Some(offset)) = (u32_at(tag_at + 4), u32_at(tag_at + 8)) else {
            break;
        };
        let images = (count / 16).min(MAX_MPF_TAGS as u32) as usize;
        for image in 0..images {
            let entry = offset as usize + image * 16;
            if let (Some(size), Some(start)) = (u32_at(entry + 4), u32_at(entry + 8)) {
                entries.push((size, start));
            }
        }
        break;
    }
    entries
}

/// The gain map item of an Ultra HDR 1.0 `GContainer` directory: items
/// follow the primary picture in order, so the gain map starts as many
/// bytes before the end as it and the items after it take.
fn gcontainer_gain_map(xmp: &str, file_len: usize) -> Option<Range<usize>> {
    let mut items = Vec::new();
    for (_, rest) in xmp.match_indices("<Container:Item").map(|(at, _)| xmp.split_at(at)) {
        let tag = rest.split_once('>').map_or(rest, |(tag, _)| tag);
        let semantic = attribute(tag, "Item:Semantic").unwrap_or_default();
        let length = attribute(tag, "Item:Length")
            .and_then(|value| value.trim().parse::<usize>().ok())
            .unwrap_or(0);
        items.push((semantic.to_owned(), length));
    }
    let position = items
        .iter()
        .position(|(semantic, _)| semantic == "GainMap")?;
    let tail: usize = items.iter().skip(position).map(|(_, length)| *length).sum();
    let length = items.get(position).map(|(_, length)| *length)?;
    let start = file_len.checked_sub(tail)?;
    (length > 0).then_some(start..start + length)
}

/// The XMP packet of an APP1 segment.
fn xmp<'a>(segment: &Segment<'a>) -> Option<&'a str> {
    if segment.marker != APP1 {
        return None;
    }
    std::str::from_utf8(segment.data.strip_prefix(XMP_PREFIX)?).ok()
}

/// The `hdrgm` gain map values of an XMP packet.
pub(super) fn xmp_gain_map(xmp: &str) -> Option<GainMap> {
    xmp_value(xmp, "Version")?;
    if xmp_value(xmp, "BaseRenditionIsHDR").is_some_and(|value| value.trim() == "True") {
        // The base is the HDR picture: its SDR rendition is the one to
        // compute, which is not what the viewer needs.
        return None;
    }
    GainMap {
        max: xmp_triple(xmp, "GainMapMax")?,
        min: xmp_triple(xmp, "GainMapMin").unwrap_or([0.0; 3]),
        gamma: xmp_triple(xmp, "Gamma").unwrap_or([1.0; 3]),
        offset_sdr: xmp_triple(xmp, "OffsetSDR").unwrap_or([1.0 / 64.0; 3]),
        offset_hdr: xmp_triple(xmp, "OffsetHDR").unwrap_or([1.0 / 64.0; 3]),
    }
    .valid()
}

/// An `hdrgm:` property, written as an attribute or as an element.
fn xmp_value<'a>(xmp: &'a str, name: &str) -> Option<&'a str> {
    let property = format!("hdrgm:{name}");
    if let Some(value) = attribute(xmp, &property) {
        return Some(value);
    }
    let open = format!("<{property}>");
    let close = format!("</{property}>");
    let (_, rest) = xmp.split_once(open.as_str())?;
    rest.split_once(close.as_str()).map(|(value, _)| value)
}

/// One value, or one per channel (`rdf:Seq` of three).
fn xmp_triple(xmp: &str, name: &str) -> Option<[f32; 3]> {
    let raw = xmp_value(xmp, name)?;
    if !raw.contains("<rdf:li") {
        let value = raw.trim().parse::<f32>().ok()?;
        return Some([value; 3]);
    }
    let values: Vec<f32> = raw
        .split("<rdf:li")
        .skip(1)
        .filter_map(|item| {
            let (_, body) = item.split_once('>')?;
            let (value, _) = body.split_once('<')?;
            value.trim().parse::<f32>().ok()
        })
        .collect();
    match values.as_slice() {
        [value] => Some([*value; 3]),
        [red, green, blue] => Some([*red, *green, *blue]),
        _ => None,
    }
}

/// The value of `name="…"` (or `'…'`) in `text`.
fn attribute<'a>(text: &'a str, name: &str) -> Option<&'a str> {
    for (_, rest) in text.match_indices(name).map(|(at, _)| text.split_at(at)) {
        let after = rest.get(name.len()..)?.trim_start();
        let Some(after) = after.strip_prefix('=') else {
            continue;
        };
        let after = after.trim_start();
        let quote = after.chars().next()?;
        if quote != '"' && quote != '\'' {
            continue;
        }
        let body = after.get(1..)?;
        return body.split_once(quote).map(|(value, _)| value);
    }
    None
}

/// The ISO 21496-1 gain map metadata after its URN.
pub(super) fn iso_gain_map(data: &[u8]) -> Option<GainMap> {
    let mut reader = Reader { data, at: 0 };
    let minimum_version = reader.u16()?;
    let _writer_version = reader.u16()?;
    if minimum_version != 0 {
        return None;
    }
    let flags = reader.u8()?;
    let channels = if flags & 0x80 != 0 { 3 } else { 1 };
    // The base is the HDR picture.
    if flags & 0x04 != 0 {
        return None;
    }
    let common = flags & 0x08 != 0;
    let mut values = [[0.0_f32; 3]; 5];
    if common {
        let denominator = reader.u32()?;
        if denominator == 0 {
            return None;
        }
        let _base_headroom = reader.u32()?;
        let _alternate_headroom = reader.u32()?;
        for channel in 0..channels {
            let read = [
                f64::from(reader.i32()?),
                f64::from(reader.i32()?),
                f64::from(reader.u32()?),
                f64::from(reader.i32()?),
                f64::from(reader.i32()?),
            ];
            for (slot, value) in values.iter_mut().zip(read) {
                if let Some(slot) = slot.get_mut(channel) {
                    *slot = (value / f64::from(denominator)) as f32;
                }
            }
        }
    } else {
        // Base and alternate headroom, each a fraction.
        for _ in 0..4 {
            reader.u32()?;
        }
        for channel in 0..channels {
            let read = [
                reader.signed_fraction()?,
                reader.signed_fraction()?,
                reader.fraction()?,
                reader.signed_fraction()?,
                reader.signed_fraction()?,
            ];
            for (slot, value) in values.iter_mut().zip(read) {
                if let Some(slot) = slot.get_mut(channel) {
                    *slot = value;
                }
            }
        }
    }
    if channels == 1 {
        for slot in &mut values {
            *slot = [slot[0]; 3];
        }
    }
    let [min, max, gamma, offset_sdr, offset_hdr] = values;
    GainMap {
        min,
        max,
        gamma,
        offset_sdr,
        offset_hdr,
    }
    .valid()
}

struct Reader<'a> {
    data: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn take<const N: usize>(&mut self) -> Option<[u8; N]> {
        let bytes = self.data.get(self.at..self.at + N)?.try_into().ok()?;
        self.at += N;
        Some(bytes)
    }

    fn u8(&mut self) -> Option<u8> {
        self.take::<1>().map(|[byte]| byte)
    }

    fn u16(&mut self) -> Option<u16> {
        self.take().map(u16::from_be_bytes)
    }

    fn u32(&mut self) -> Option<u32> {
        self.take().map(u32::from_be_bytes)
    }

    fn i32(&mut self) -> Option<i32> {
        self.take().map(i32::from_be_bytes)
    }

    fn fraction(&mut self) -> Option<f32> {
        let numerator = self.u32()?;
        let denominator = self.u32()?;
        (denominator != 0).then(|| (f64::from(numerator) / f64::from(denominator)) as f32)
    }

    fn signed_fraction(&mut self) -> Option<f32> {
        let numerator = self.i32()?;
        let denominator = self.u32()?;
        (denominator != 0).then(|| (f64::from(numerator) / f64::from(denominator)) as f32)
    }
}

fn be_u16(bytes: &[u8], at: usize) -> Option<u16> {
    let pair: [u8; 2] = bytes.get(at..at + 2)?.try_into().ok()?;
    Some(u16::from_be_bytes(pair))
}
