//! The few SPA POD values a PipeWire screen cast needs, written and read
//! by hand so Quill does not link libpipewire at build time
//! (`pipewire_dl.rs` loads it at run time):
//! - [`video_enum_format`]: the formats Quill takes (I420 first, then
//!   NV12 and the packed RGB orders), any size, up to 60 fps.
//! - [`parse_video_format`]: the format the compositor picked.
//!
//! Layout (`spa/pod/pod.h`): every POD is a `u32` body size, a `u32`
//! type, then the body padded to 8 bytes. An object body is its object
//! type and id, then properties (`u32` key, `u32` flags, a POD). A choice
//! body is its kind and flags, one child POD header, then the values
//! packed back to back.
//!
//! Plain Rust, tested on every platform.
#![deny(clippy::indexing_slicing)]

// POD types (`spa/utils/type.h`).
const TYPE_ID: u32 = 3;
const TYPE_RECTANGLE: u32 = 10;
const TYPE_FRACTION: u32 = 11;
const TYPE_OBJECT: u32 = 15;
const TYPE_CHOICE: u32 = 19;
const TYPE_OBJECT_FORMAT: u32 = 0x0004_0003;

// Choice kinds (`spa/pod/pod.h`).
const CHOICE_NONE: u32 = 0;
const CHOICE_RANGE: u32 = 1;
const CHOICE_ENUM: u32 = 3;

/// `SPA_PARAM_EnumFormat` and `SPA_PARAM_Format` (`spa/param/param.h`).
pub const PARAM_ENUM_FORMAT: u32 = 3;
pub const PARAM_FORMAT: u32 = 4;

// Format keys (`spa/param/format.h`).
const FORMAT_MEDIA_TYPE: u32 = 1;
const FORMAT_MEDIA_SUBTYPE: u32 = 2;
const FORMAT_VIDEO_FORMAT: u32 = 0x0002_0001;
const FORMAT_VIDEO_MODIFIER: u32 = 0x0002_0002;
const FORMAT_VIDEO_SIZE: u32 = 0x0002_0003;
const FORMAT_VIDEO_FRAMERATE: u32 = 0x0002_0004;

const MEDIA_TYPE_VIDEO: u32 = 2;
const MEDIA_SUBTYPE_RAW: u32 = 1;

/// The `spa_video_format` values Quill converts (`spa/param/video/raw.h`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoFormat {
    I420,
    Rgbx,
    Bgrx,
    Rgba,
    Bgra,
    Nv12,
}

impl VideoFormat {
    /// Preferred first: I420 needs no conversion.
    pub const ACCEPTED: [VideoFormat; 6] = [
        VideoFormat::I420,
        VideoFormat::Nv12,
        VideoFormat::Bgrx,
        VideoFormat::Bgra,
        VideoFormat::Rgbx,
        VideoFormat::Rgba,
    ];

    pub fn spa_id(self) -> u32 {
        match self {
            VideoFormat::I420 => 2,
            VideoFormat::Rgbx => 7,
            VideoFormat::Bgrx => 8,
            VideoFormat::Rgba => 11,
            VideoFormat::Bgra => 12,
            VideoFormat::Nv12 => 23,
        }
    }

    pub fn from_spa_id(id: u32) -> Option<Self> {
        Self::ACCEPTED
            .into_iter()
            .find(|format| format.spa_id() == id)
    }
}

/// What the compositor will send.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NegotiatedFormat {
    pub format: VideoFormat,
    pub width: u32,
    pub height: u32,
}

/// Writes PODs into 8-byte aligned bytes.
#[derive(Default)]
struct Writer {
    bytes: Vec<u8>,
}

impl Writer {
    fn u32(&mut self, value: u32) {
        self.bytes.extend_from_slice(&value.to_ne_bytes());
    }

    fn pad(&mut self) {
        while !self.bytes.len().is_multiple_of(8) {
            self.bytes.push(0);
        }
    }

    /// One property whose value is the POD `write` produces.
    fn prop(&mut self, key: u32, write: impl FnOnce(&mut Self)) {
        self.u32(key);
        self.u32(0);
        write(self);
        self.pad();
    }

    fn id(&mut self, value: u32) {
        self.u32(4);
        self.u32(TYPE_ID);
        self.u32(value);
    }

    /// A choice of `values` (each `child_size` bytes of `child_type`).
    fn choice(&mut self, kind: u32, child_type: u32, child_size: u32, values: &[u32]) {
        let words = u32::try_from(values.len()).unwrap_or(0);
        self.u32(16 + 4 * words);
        self.u32(TYPE_CHOICE);
        self.u32(kind);
        self.u32(0);
        self.u32(child_size);
        self.u32(child_type);
        for value in values {
            self.u32(*value);
        }
    }
}

/// The EnumFormat object offered when connecting the stream: raw video in
/// [`VideoFormat::ACCEPTED`], 1920 x 1080 preferred within 1..8192, and
/// 15 fps preferred within 0..60. No modifier, so compositors send
/// shared-memory buffers Quill can map.
pub fn video_enum_format() -> Vec<u8> {
    let mut body = Writer::default();
    body.u32(TYPE_OBJECT_FORMAT);
    body.u32(PARAM_ENUM_FORMAT);
    body.prop(FORMAT_MEDIA_TYPE, |w| w.id(MEDIA_TYPE_VIDEO));
    body.prop(FORMAT_MEDIA_SUBTYPE, |w| w.id(MEDIA_SUBTYPE_RAW));
    body.prop(FORMAT_VIDEO_FORMAT, |w| {
        // An enum choice: the default, then the alternatives.
        let first = VideoFormat::ACCEPTED.first().map_or(2, |f| f.spa_id());
        let mut values = vec![first];
        values.extend(VideoFormat::ACCEPTED.iter().map(|f| f.spa_id()));
        w.choice(CHOICE_ENUM, TYPE_ID, 4, &values);
    });
    body.prop(FORMAT_VIDEO_SIZE, |w| {
        // Rectangles: default, min, max.
        w.choice(
            CHOICE_RANGE,
            TYPE_RECTANGLE,
            8,
            &[1920, 1080, 1, 1, 8192, 8192],
        );
    });
    body.prop(FORMAT_VIDEO_FRAMERATE, |w| {
        // Fractions: default, min, max.
        w.choice(CHOICE_RANGE, TYPE_FRACTION, 8, &[15, 1, 0, 1, 60, 1]);
    });
    let mut pod = Writer::default();
    pod.u32(u32::try_from(body.bytes.len()).unwrap_or(0));
    pod.u32(TYPE_OBJECT);
    pod.bytes.extend_from_slice(&body.bytes);
    pod.bytes
}

/// Reads PODs from bytes.
struct Reader<'a> {
    bytes: &'a [u8],
}

impl<'a> Reader<'a> {
    fn u32_at(&self, at: usize) -> Option<u32> {
        let word = self.bytes.get(at..at.checked_add(4)?)?.first_chunk::<4>()?;
        Some(u32::from_ne_bytes(*word))
    }

    /// The POD at `at`: (type, body), and where the next one starts.
    fn pod(&self, at: usize) -> Option<(u32, &'a [u8], usize)> {
        let size = usize::try_from(self.u32_at(at)?).ok()?;
        let kind = self.u32_at(at.checked_add(4)?)?;
        let start = at.checked_add(8)?;
        let body = self.bytes.get(start..start.checked_add(size)?)?;
        let next = start.checked_add(size.checked_next_multiple_of(8)?)?;
        Some((kind, body, next))
    }
}

/// The first value of a POD that is either a plain value of `want` or a
/// choice of them (a fixed format may still wrap values in a choice).
fn first_value(kind: u32, body: &[u8], want: u32) -> Option<&[u8]> {
    if kind == want {
        return Some(body);
    }
    if kind != TYPE_CHOICE {
        return None;
    }
    let reader = Reader { bytes: body };
    let choice = reader.u32_at(0)?;
    let child_size = usize::try_from(reader.u32_at(8)?).ok()?;
    let child_type = reader.u32_at(12)?;
    let known = matches!(choice, CHOICE_NONE | CHOICE_RANGE | CHOICE_ENUM);
    (known && child_type == want).then_some(())?;
    body.get(16..16usize.checked_add(child_size)?)
}

fn word(bytes: &[u8], index: usize) -> Option<u32> {
    let at = index.checked_mul(4)?;
    Some(u32::from_ne_bytes(
        *bytes.get(at..at.checked_add(4)?)?.first_chunk::<4>()?,
    ))
}

/// The video format and size of a Format object, or `None` when it is not
/// raw video in a format Quill converts, or carries a DMA-BUF modifier.
pub fn parse_video_format(pod: &[u8]) -> Option<NegotiatedFormat> {
    let reader = Reader { bytes: pod };
    let (kind, body, _) = reader.pod(0)?;
    (kind == TYPE_OBJECT).then_some(())?;
    let object = Reader { bytes: body };
    (object.u32_at(0)? == TYPE_OBJECT_FORMAT).then_some(())?;
    let (mut media, mut subtype, mut format, mut size) = (None, None, None, None);
    let mut at = 8;
    while at < body.len() {
        let key = object.u32_at(at)?;
        let (value_kind, value, next) = object.pod(at.checked_add(8)?)?;
        match key {
            FORMAT_MEDIA_TYPE => media = word(first_value(value_kind, value, TYPE_ID)?, 0),
            FORMAT_MEDIA_SUBTYPE => subtype = word(first_value(value_kind, value, TYPE_ID)?, 0),
            FORMAT_VIDEO_FORMAT => format = word(first_value(value_kind, value, TYPE_ID)?, 0),
            FORMAT_VIDEO_SIZE => {
                let rect = first_value(value_kind, value, TYPE_RECTANGLE)?;
                size = Some((word(rect, 0)?, word(rect, 1)?));
            }
            FORMAT_VIDEO_MODIFIER => return None,
            _ => {}
        }
        at = next;
    }
    (media? == MEDIA_TYPE_VIDEO && subtype? == MEDIA_SUBTYPE_RAW).then_some(())?;
    let (width, height) = size?;
    (width > 0 && height > 0).then_some(())?;
    Some(NegotiatedFormat {
        format: VideoFormat::from_spa_id(format?)?,
        width,
        height,
    })
}

#[cfg(test)]
#[allow(clippy::indexing_slicing)]
mod tests {
    use super::*;

    fn words(bytes: &[u8]) -> Vec<u32> {
        bytes
            .as_chunks::<4>()
            .0
            .iter()
            .map(|w| u32::from_ne_bytes(*w))
            .collect()
    }

    /// A fixed Format object, as a compositor sends it.
    fn fixed_format(format: u32, width: u32, height: u32, modifier: bool) -> Vec<u8> {
        let mut body = Writer::default();
        body.u32(TYPE_OBJECT_FORMAT);
        body.u32(PARAM_FORMAT);
        body.prop(FORMAT_MEDIA_TYPE, |w| w.id(MEDIA_TYPE_VIDEO));
        body.prop(FORMAT_MEDIA_SUBTYPE, |w| w.id(MEDIA_SUBTYPE_RAW));
        body.prop(FORMAT_VIDEO_FORMAT, |w| w.id(format));
        if modifier {
            body.prop(FORMAT_VIDEO_MODIFIER, |w| {
                w.u32(8);
                w.u32(5);
                w.u32(0);
                w.u32(0);
            });
        }
        body.prop(FORMAT_VIDEO_SIZE, |w| {
            w.u32(8);
            w.u32(TYPE_RECTANGLE);
            w.u32(width);
            w.u32(height);
        });
        let mut pod = Writer::default();
        pod.u32(body.bytes.len() as u32);
        pod.u32(TYPE_OBJECT);
        pod.bytes.extend_from_slice(&body.bytes);
        pod.bytes
    }

    #[test]
    fn the_offer_has_spa_layout() {
        let pod = video_enum_format();
        assert!(pod.len().is_multiple_of(8));
        let w = words(&pod);
        // Header: body size, Object; then Format object, EnumFormat id.
        assert_eq!(w[0] as usize, pod.len() - 8);
        assert_eq!(
            &w[1..4],
            &[TYPE_OBJECT, TYPE_OBJECT_FORMAT, PARAM_ENUM_FORMAT]
        );
        // mediaType: key, flags, Id POD (size 4), video, padding.
        assert_eq!(&w[4..10], &[FORMAT_MEDIA_TYPE, 0, 4, TYPE_ID, 2, 0]);
        assert_eq!(&w[10..16], &[FORMAT_MEDIA_SUBTYPE, 0, 4, TYPE_ID, 1, 0]);
        // format: an enum choice of Ids, I420 as the default and first.
        assert_eq!(&w[16..18], &[FORMAT_VIDEO_FORMAT, 0]);
        assert_eq!(
            &w[18..24],
            &[16 + 4 * 7, TYPE_CHOICE, CHOICE_ENUM, 0, 4, TYPE_ID]
        );
        assert_eq!(&w[24..31], &[2, 2, 23, 8, 12, 7, 11]);
    }

    #[test]
    fn the_offer_reads_back_as_its_defaults() {
        assert_eq!(
            parse_video_format(&video_enum_format()),
            Some(NegotiatedFormat {
                format: VideoFormat::I420,
                width: 1920,
                height: 1080,
            })
        );
    }

    #[test]
    fn a_fixed_format_is_read() {
        let parsed = parse_video_format(&fixed_format(8, 2560, 1440, false));
        assert_eq!(
            parsed,
            Some(NegotiatedFormat {
                format: VideoFormat::Bgrx,
                width: 2560,
                height: 1440,
            })
        );
    }

    #[test]
    fn dma_bufs_unknown_formats_and_short_bytes_are_refused() {
        assert_eq!(parse_video_format(&fixed_format(8, 64, 64, true)), None);
        // YUY2 is not converted.
        assert_eq!(parse_video_format(&fixed_format(4, 64, 64, false)), None);
        assert_eq!(parse_video_format(&fixed_format(8, 0, 64, false)), None);
        let pod = fixed_format(8, 64, 64, false);
        for cut in [0, 4, 8, 20, pod.len() - 4] {
            assert_eq!(parse_video_format(&pod[..cut]), None, "cut at {cut}");
        }
    }
}
