//! The few SPA pods a PipeWire screen-cast stream needs, built and read in
//! plain Rust: libpipewire is loaded at run time (`pipewire.rs`), and SPA's
//! pod builder and parser are inline C in headers that are not loaded with
//! it.
//!
//! A pod is a `{u32 size, u32 type}` header and a body padded to 8 bytes
//! (`spa/pod/pod.h`). The constants below are SPA's ABI; the tests compare
//! the built pods with the bytes libspa 1.0.5 builds for the same values.
#![deny(clippy::indexing_slicing)]

/// `SPA_TYPE_*`.
const TYPE_ID: u32 = 3;
const TYPE_INT: u32 = 4;
const TYPE_RECTANGLE: u32 = 10;
const TYPE_OBJECT: u32 = 15;
const TYPE_CHOICE: u32 = 19;
/// `SPA_TYPE_OBJECT_Format`, `_ParamBuffers`, `_ParamMeta`.
const OBJECT_FORMAT: u32 = 0x40003;
const OBJECT_PARAM_BUFFERS: u32 = 0x40004;
const OBJECT_PARAM_META: u32 = 0x40005;
/// `SPA_PARAM_*`.
pub const PARAM_ENUM_FORMAT: u32 = 3;
pub const PARAM_FORMAT: u32 = 4;
const PARAM_BUFFERS: u32 = 5;
const PARAM_META: u32 = 6;
/// `SPA_FORMAT_*`.
const FORMAT_MEDIA_TYPE: u32 = 1;
const FORMAT_MEDIA_SUBTYPE: u32 = 2;
const FORMAT_VIDEO_FORMAT: u32 = 0x20001;
const FORMAT_VIDEO_SIZE: u32 = 0x20003;
const MEDIA_TYPE_VIDEO: u32 = 2;
const MEDIA_SUBTYPE_RAW: u32 = 1;
/// `SPA_CHOICE_Range`, `SPA_CHOICE_Flags`.
const CHOICE_RANGE: u32 = 1;
const CHOICE_FLAGS: u32 = 4;
/// `SPA_PARAM_BUFFERS_dataType`, `SPA_PARAM_META_type`, `_size`.
const BUFFERS_DATA_TYPE: u32 = 6;
const META_TYPE: u32 = 1;
const META_SIZE: u32 = 2;
/// `SPA_DATA_MemPtr`, `SPA_DATA_MemFd`.
pub const DATA_MEM_PTR: u32 = 1;
pub const DATA_MEM_FD: u32 = 2;
/// `SPA_META_VideoCrop` and `sizeof(struct spa_meta_region)`.
pub const META_VIDEO_CROP: u32 = 2;
const META_REGION_SIZE: i32 = 16;

/// The raw video formats Quill reads (`SPA_VIDEO_FORMAT_*`), I420 first:
/// it is what the call sends, so it needs no conversion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoFormat {
    I420,
    Nv12,
    Bgrx,
    Bgra,
    Rgbx,
    Rgba,
}

impl VideoFormat {
    /// In the order offered to the producer.
    pub const OFFERED: [VideoFormat; 6] = [
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
        Self::OFFERED
            .into_iter()
            .find(|format| format.spa_id() == id)
    }
}

/// Appends pods to a byte buffer.
#[derive(Default)]
struct PodWriter {
    bytes: Vec<u8>,
}

impl PodWriter {
    fn u32(&mut self, value: u32) {
        self.bytes.extend_from_slice(&value.to_ne_bytes());
    }

    fn pad(&mut self) {
        while !self.bytes.len().is_multiple_of(8) {
            self.bytes.push(0);
        }
    }

    /// Write a header with a size patched in once `body` has written.
    fn sized(&mut self, kind: u32, body: impl FnOnce(&mut Self)) {
        let at = self.bytes.len();
        self.u32(0);
        self.u32(kind);
        let start = self.bytes.len();
        body(self);
        let size = u32::try_from(self.bytes.len() - start).unwrap_or(u32::MAX);
        if let Some(slot) = self.bytes.get_mut(at..at + 4) {
            slot.copy_from_slice(&size.to_ne_bytes());
        }
        self.pad();
    }

    fn object(&mut self, object_type: u32, param: u32, props: impl FnOnce(&mut Self)) {
        self.sized(TYPE_OBJECT, |w| {
            w.u32(object_type);
            w.u32(param);
            props(w);
        });
    }

    fn prop(&mut self, key: u32, value: impl FnOnce(&mut Self)) {
        self.u32(key);
        self.u32(0);
        value(self);
    }

    fn id(&mut self, value: u32) {
        self.sized(TYPE_ID, |w| w.u32(value));
    }

    fn int(&mut self, value: i32) {
        self.sized(TYPE_INT, |w| {
            w.bytes.extend_from_slice(&value.to_ne_bytes())
        });
    }

    /// A choice of `values`, each `child_size` bytes of `child_type`.
    fn choice(&mut self, choice: u32, child_type: u32, child_size: u32, values: &[u32]) {
        self.sized(TYPE_CHOICE, |w| {
            w.u32(choice);
            w.u32(0);
            w.u32(child_size);
            w.u32(child_type);
            for value in values {
                w.u32(*value);
            }
        });
    }
}

/// One `EnumFormat` per offered format, as webrtc's PipeWire capturer
/// builds them: raw video of that format at any size from 1 x 1 to 8192 x
/// 8192 (Quill scales to the call's size itself). `default` is the
/// suggested size.
pub fn enum_formats(default: (u32, u32)) -> Vec<Vec<u8>> {
    VideoFormat::OFFERED
        .iter()
        .map(|format| {
            let mut w = PodWriter::default();
            w.object(OBJECT_FORMAT, PARAM_ENUM_FORMAT, |w| {
                w.prop(FORMAT_MEDIA_TYPE, |w| w.id(MEDIA_TYPE_VIDEO));
                w.prop(FORMAT_MEDIA_SUBTYPE, |w| w.id(MEDIA_SUBTYPE_RAW));
                w.prop(FORMAT_VIDEO_FORMAT, |w| w.id(format.spa_id()));
                w.prop(FORMAT_VIDEO_SIZE, |w| {
                    w.choice(
                        CHOICE_RANGE,
                        TYPE_RECTANGLE,
                        8,
                        &[default.0, default.1, 1, 1, 8192, 8192],
                    );
                });
            });
            w.bytes
        })
        .collect()
}

/// The buffers Quill can read: plain memory or a memfd it maps, never a
/// DMA-BUF (no GPU import here).
pub fn buffers_param() -> Vec<u8> {
    let mut w = PodWriter::default();
    w.object(OBJECT_PARAM_BUFFERS, PARAM_BUFFERS, |w| {
        w.prop(BUFFERS_DATA_TYPE, |w| {
            w.choice(
                CHOICE_FLAGS,
                TYPE_INT,
                4,
                &[(1 << DATA_MEM_PTR) | (1 << DATA_MEM_FD)],
            );
        });
    });
    w.bytes
}

/// Ask for the crop region, so a window cast is cut to the window.
pub fn crop_meta_param() -> Vec<u8> {
    let mut w = PodWriter::default();
    w.object(OBJECT_PARAM_META, PARAM_META, |w| {
        w.prop(META_TYPE, |w| w.id(META_VIDEO_CROP));
        w.prop(META_SIZE, |w| w.int(META_REGION_SIZE));
    });
    w.bytes
}

/// What a negotiated `Format` pod says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RawVideoFormat {
    pub format: VideoFormat,
    pub width: u32,
    pub height: u32,
}

/// Reads native-endian `u32`s from a pod body.
struct PodReader<'a> {
    bytes: &'a [u8],
}

impl<'a> PodReader<'a> {
    fn u32_at(bytes: &[u8], at: usize) -> Option<u32> {
        let raw = bytes.get(at..at.checked_add(4)?)?.first_chunk::<4>()?;
        Some(u32::from_ne_bytes(*raw))
    }

    /// The next pod: `(type, body)`, and the reader moved past it.
    fn next_pod(&mut self) -> Option<(u32, &'a [u8])> {
        let size = usize::try_from(Self::u32_at(self.bytes, 0)?).ok()?;
        let kind = Self::u32_at(self.bytes, 4)?;
        let body = self.bytes.get(8..8usize.checked_add(size)?)?;
        let padded = 8usize.checked_add(size.checked_next_multiple_of(8)?)?;
        self.bytes = self.bytes.get(padded..).unwrap_or(&[]);
        Some((kind, body))
    }
}

/// A property value's current value: the value itself, or a choice's
/// first value (its default).
fn current_value(kind: u32, body: &[u8]) -> Option<(u32, &[u8])> {
    if kind != TYPE_CHOICE {
        return Some((kind, body));
    }
    let child_size = usize::try_from(PodReader::u32_at(body, 8)?).ok()?;
    let child_type = PodReader::u32_at(body, 12)?;
    Some((child_type, body.get(16..16usize.checked_add(child_size)?)?))
}

/// Read a negotiated video `Format` (a `param_changed` pod). `None` for a
/// pod that is not raw video in a format Quill reads.
pub fn parse_format(pod: &[u8]) -> Option<RawVideoFormat> {
    let mut reader = PodReader { bytes: pod };
    let (kind, body) = reader.next_pod()?;
    if kind != TYPE_OBJECT || PodReader::u32_at(body, 0)? != OBJECT_FORMAT {
        return None;
    }
    let mut props = body.get(8..)?;
    let (mut media, mut subtype, mut format, mut size) = (None, None, None, None);
    // A property is `{key, flags}` then a pod.
    while props.len() >= 16 {
        let key = PodReader::u32_at(props, 0)?;
        let mut value = PodReader {
            bytes: props.get(8..)?,
        };
        let (value_kind, value_body) = value.next_pod()?;
        props = value.bytes;
        let Some((kind, body)) = current_value(value_kind, value_body) else {
            continue;
        };
        match (key, kind) {
            (FORMAT_MEDIA_TYPE, TYPE_ID) => media = PodReader::u32_at(body, 0),
            (FORMAT_MEDIA_SUBTYPE, TYPE_ID) => subtype = PodReader::u32_at(body, 0),
            (FORMAT_VIDEO_FORMAT, TYPE_ID) => format = PodReader::u32_at(body, 0),
            (FORMAT_VIDEO_SIZE, TYPE_RECTANGLE) => {
                size = PodReader::u32_at(body, 0).zip(PodReader::u32_at(body, 4));
            }
            _ => {}
        }
    }
    if media != Some(MEDIA_TYPE_VIDEO) || subtype != Some(MEDIA_SUBTYPE_RAW) {
        return None;
    }
    let (width, height) = size.filter(|(w, h)| *w > 0 && *h > 0)?;
    Some(RawVideoFormat {
        format: VideoFormat::from_spa_id(format?)?,
        width,
        height,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    fn unhex(text: &str) -> Vec<u8> {
        (0..text.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(text.get(i..i + 2).unwrap(), 16).unwrap())
            .collect()
    }

    // Built by libspa 1.0.5 (`spa_pod_builder_add_object`) on x86_64 for
    // the same values; see docs/decisions/call-capture-screen-linux.md.
    const LIBSPA_BUFFERS: &str = "300000000f000000040004000500000006000000000000001400000013000000040000000000000004000000040000000600000000000000";
    const LIBSPA_META: &str = "380000000f0000000500040006000000010000000000000004000000030000000200000000000000020000000000000004000000040000001000000000000000";
    const LIBSPA_ENUM_BGRX_1080: &str = "880000000f0000000300040003000000010000000000000004000000030000000200000000000000020000000000000004000000030000000100000000000000010002000000000004000000030000000800000000000000030002000000000028000000130000000100000000000000080000000a000000800700003804000001000000010000000020000000200000";
    const LIBSPA_FORMAT_BGRX_640: &str = "800000000f00000003000400040000000100000000000000040000000300000002000000000000000200000000000000040000000300000001000000000000000100020000000000040000000300000008000000000000000300020000000000080000000a00000080020000e00100000400020000000000080000000b0000001e00000001000000";

    #[test]
    fn buffers_and_meta_params_match_libspa() {
        assert_eq!(hex(&buffers_param()), LIBSPA_BUFFERS);
        assert_eq!(hex(&crop_meta_param()), LIBSPA_META);
    }

    #[test]
    fn enum_format_matches_libspa() {
        let formats = enum_formats((1920, 1080));
        assert_eq!(formats.len(), VideoFormat::OFFERED.len());
        assert_eq!(hex(&formats[2]), LIBSPA_ENUM_BGRX_1080);
        // The others differ only in the format id.
        for (pod, format) in formats.iter().zip(VideoFormat::OFFERED) {
            let mut expected = unhex(LIBSPA_ENUM_BGRX_1080);
            expected[80] = format.spa_id() as u8;
            assert_eq!(pod, &expected);
        }
    }

    #[test]
    fn a_negotiated_format_parses() {
        let pod = unhex(LIBSPA_FORMAT_BGRX_640);
        assert_eq!(
            parse_format(&pod),
            Some(RawVideoFormat {
                format: VideoFormat::Bgrx,
                width: 640,
                height: 480,
            })
        );
        // Our own enum pods read back as their default.
        for (pod, format) in enum_formats((800, 600)).iter().zip(VideoFormat::OFFERED) {
            assert_eq!(
                parse_format(pod),
                Some(RawVideoFormat {
                    format,
                    width: 800,
                    height: 600,
                })
            );
        }
    }

    #[test]
    fn bad_pods_are_refused() {
        assert_eq!(parse_format(&[]), None);
        assert_eq!(parse_format(&[0xff; 7]), None);
        let pod = unhex(LIBSPA_FORMAT_BGRX_640);
        for cut in 0..pod.len() {
            // Truncated pods never panic; most are refused.
            let _ = parse_format(&pod[..cut]);
        }
        assert_eq!(parse_format(&pod[..pod.len() - 24]), None, "truncated");
        assert_eq!(parse_format(&buffers_param()), None, "not a format");
        let mut wrong = pod.clone();
        // An unread format (YUY2) is refused.
        let at = hex(&pod).find("0800000000000000030002").unwrap() / 2;
        wrong[at] = 4;
        assert_eq!(parse_format(&wrong), None);
    }

    #[test]
    fn formats_round_trip_their_ids() {
        for format in VideoFormat::OFFERED {
            assert_eq!(VideoFormat::from_spa_id(format.spa_id()), Some(format));
        }
        assert_eq!(VideoFormat::from_spa_id(4), None);
    }
}
