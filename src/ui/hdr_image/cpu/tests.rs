use super::super::half_float::{f32_to_half, half_to_f32};
use super::jxl::pq_to_nits;
use super::ultra_hdr::{self, GainMap, iso_gain_map, segments, xmp_gain_map};
use super::{CpuFrame, decode, probe, target_size};
use std::path::PathBuf;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/hdr")
        .join(name)
}

fn pixel(frame: &CpuFrame, x: u32, y: u32) -> [f32; 4] {
    let at = ((y * frame.width + x) * 8) as usize;
    let half = |offset: usize| {
        half_to_f32(u16::from_le_bytes([
            frame.bytes[at + offset],
            frame.bytes[at + offset + 1],
        ]))
    };
    [half(0), half(2), half(4), half(6)]
}

fn assert_close(actual: [f32; 4], expected: [f32; 3], at: (u32, u32)) {
    for (channel, (got, want)) in actual.iter().zip(expected).enumerate() {
        let tolerance = 0.02 + want * 0.04;
        assert!(
            (got - want).abs() <= tolerance,
            "pixel {at:?} channel {channel}: {got} vs {want}"
        );
    }
}

/// libultrahdr's own linear decode of the fixture (`ultrahdr_app -m 1 -o 0
/// -O 4`), 1.0 = SDR white.
const ULTRA_HDR_REFERENCE: [((u32, u32), [f32; 3]); 6] = [
    ((0, 10), [0.050, 0.039, 0.029]),
    ((16, 10), [1.109, 0.654, 0.335]),
    ((32, 10), [2.117, 1.275, 0.640]),
    ((48, 10), [3.125, 1.890, 0.954]),
    ((63, 10), [3.895, 2.418, 1.221]),
    ((5, 1), [0.180, 0.180, 0.181]),
];

#[test]
fn ultra_hdr_matches_the_reference_decoder() {
    let path = fixture("ultrahdr.jpg");
    assert_eq!(probe(&path), Some(true));
    let frame = decode(&path, 4096).unwrap().unwrap();
    assert_eq!((frame.width, frame.height), (64, 32));
    for (at, expected) in ULTRA_HDR_REFERENCE {
        let actual = pixel(&frame, at.0, at.1);
        assert_close(actual, expected, at);
        assert_eq!(actual[3], 1.0);
    }
    assert!(frame.peak > 3.8 && frame.peak < 4.2, "peak {}", frame.peak);
}

#[test]
fn ultra_hdr_honours_the_long_edge() {
    let frame = decode(&fixture("ultrahdr.jpg"), 32).unwrap().unwrap();
    assert_eq!((frame.width, frame.height), (32, 16));
    assert_eq!(frame.bytes.len(), 32 * 16 * 8);
}

#[test]
fn iso_metadata_reads_per_channel_boosts() {
    let file = std::fs::read(fixture("ultrahdr.jpg")).unwrap();
    // The gain map JPEG is the second one in the file.
    let second = file
        .windows(3)
        .skip(2)
        .position(|window| window == [0xff, 0xd8, 0xff])
        .unwrap()
        + 2;
    let map = segments(&file[second..]);
    let iso = map
        .iter()
        .find_map(|segment| segment.data.strip_prefix(b"urn:iso:std:iso:ts:21496:-1\0"))
        .unwrap();
    let gain_map = iso_gain_map(iso).unwrap();
    let expected_max = [3.96875_f32.log2(), 2.4209_f32.log2(), 1.2207_f32.log2()];
    for (got, want) in gain_map.max.iter().zip(expected_max) {
        assert!((got - want).abs() < 1e-3, "{got} vs {want}");
    }
    assert!((gain_map.min[0] - 0.995621_f32.log2()).abs() < 1e-4);
    assert_eq!(gain_map.gamma, [1.0; 3]);
}

#[test]
fn iso_metadata_with_a_common_denominator() {
    // Version 0/0, single channel, common denominator 1000.
    let mut data = vec![0, 0, 0, 0, 0x08];
    for value in [1000_u32, 0, 2000] {
        data.extend_from_slice(&value.to_be_bytes());
    }
    for value in [-500_i32, 2000, 1000, 15, 15] {
        data.extend_from_slice(&value.to_be_bytes());
    }
    let gain_map = iso_gain_map(&data).unwrap();
    assert_eq!(gain_map.min, [-0.5; 3]);
    assert_eq!(gain_map.max, [2.0; 3]);
    assert_eq!(gain_map.gamma, [1.0; 3]);
    assert_eq!(gain_map.offset_sdr, [0.015; 3]);
}

#[test]
fn iso_metadata_rejects_hdr_bases_and_newer_versions() {
    assert_eq!(iso_gain_map(&[0, 0, 0, 0, 0x04]), None);
    assert_eq!(iso_gain_map(&[0, 1, 0, 1, 0x00]), None);
    assert_eq!(iso_gain_map(&[0, 0]), None);
}

#[test]
fn xmp_attributes_and_sequences() {
    let attributes = r#"<rdf:Description hdrgm:Version="1.0" hdrgm:GainMapMin="0"
        hdrgm:GainMapMax="2.5" hdrgm:Gamma="1" hdrgm:OffsetSDR="0" hdrgm:OffsetHDR="0"
        hdrgm:HDRCapacityMin="0" hdrgm:HDRCapacityMax="2.5"/>"#;
    assert_eq!(
        xmp_gain_map(attributes),
        Some(GainMap {
            min: [0.0; 3],
            max: [2.5; 3],
            gamma: [1.0; 3],
            offset_sdr: [0.0; 3],
            offset_hdr: [0.0; 3],
        })
    );
    let sequence = r#"<rdf:Description hdrgm:Version="1.0">
        <hdrgm:GainMapMax><rdf:Seq><rdf:li>2.0</rdf:li><rdf:li>1.5</rdf:li>
        <rdf:li>1.0</rdf:li></rdf:Seq></hdrgm:GainMapMax></rdf:Description>"#;
    let gain_map = xmp_gain_map(sequence).unwrap();
    assert_eq!(gain_map.max, [2.0, 1.5, 1.0]);
    // The spec's defaults.
    assert_eq!(gain_map.gamma, [1.0; 3]);
    assert_eq!(gain_map.offset_sdr, [1.0 / 64.0; 3]);
}

#[test]
fn xmp_without_a_version_or_with_an_hdr_base_is_ignored() {
    assert_eq!(xmp_gain_map(r#"hdrgm:GainMapMax="2""#), None);
    assert_eq!(
        xmp_gain_map(r#"hdrgm:Version="1.0" hdrgm:GainMapMax="2" hdrgm:BaseRenditionIsHDR="True""#),
        None
    );
    // A gamma of zero would divide by zero.
    assert_eq!(
        xmp_gain_map(r#"hdrgm:Version="1.0" hdrgm:GainMapMax="2" hdrgm:Gamma="0""#),
        None
    );
}

#[test]
fn pq_jpeg_xl_decodes_to_sdr_white_units() {
    let path = fixture("pq.jxl");
    assert_eq!(probe(&path), Some(true));
    let frame = decode(&path, 4096).unwrap().unwrap();
    assert_eq!((frame.width, frame.height), (32, 16));
    // 203 nits on the left, 812 on the right.
    assert_close(pixel(&frame, 2, 8), [1.0; 3], (2, 8));
    assert_close(pixel(&frame, 29, 8), [4.0; 3], (29, 8));
}

#[test]
fn sdr_files_have_no_hdr_rendition() {
    let sdr = fixture("sdr.heic");
    assert_eq!(probe(&sdr), Some(false));
    assert!(decode(&sdr, 1024).unwrap().is_none());
    assert_eq!(probe(&fixture("missing.jpg")), None);
}

#[test]
fn truncated_files_never_panic() {
    let file = std::fs::read(fixture("ultrahdr.jpg")).unwrap();
    let scratch = std::env::temp_dir().join(format!("quill-hdr-cut-{}", std::process::id()));
    std::fs::create_dir_all(&scratch).unwrap();
    for cut in (0..file.len()).step_by(97) {
        let path = scratch.join("cut.jpg");
        std::fs::write(&path, &file[..cut]).unwrap();
        let _ = probe(&path);
        let _ = decode(&path, 64);
        let _ = ultra_hdr::probe(&file[..cut], file.len() as u64);
    }
    std::fs::remove_dir_all(&scratch).unwrap();
}

#[test]
fn target_size_fits_the_long_edge_and_cap() {
    assert_eq!(target_size((4000, 3000), 1024), (1024, 768));
    assert_eq!(target_size((640, 480), 4096), (640, 480));
    assert_eq!(target_size((3000, 4000), 1024), (768, 1024));
    let (width, height) = target_size((12_000, 9_000), u32::MAX);
    assert!(u64::from(width) * u64::from(height) <= super::super::HDR_MAX_PIXELS);
    assert!(width <= 8192 && height <= 8192);
}

#[test]
fn pq_curve_endpoints() {
    assert_eq!(pq_to_nits(0.0), 0.0);
    assert!((pq_to_nits(1.0) - 10_000.0).abs() < 1.0);
    // 0.58069 encodes 203 nits.
    assert!((pq_to_nits(0.580_69) - 203.0).abs() < 0.5);
    assert!(pq_to_nits(-0.580_69) < 0.0);
}

#[test]
fn half_floats_round_trip() {
    assert_eq!(f32_to_half(1.0), 0x3c00);
    assert_eq!(f32_to_half(-2.0), 0xc000);
    assert_eq!(f32_to_half(0.0), 0x0000);
    assert_eq!(f32_to_half(65504.0), 0x7bff);
    assert_eq!(f32_to_half(65520.0), 0x7c00);
    assert_eq!(f32_to_half(f32::INFINITY), 0x7c00);
    assert!(half_to_f32(f32_to_half(f32::NAN)).is_nan());
    assert_eq!(f32_to_half(2.0_f32.powi(-24)), 0x0001);
    assert_eq!(f32_to_half(2.0_f32.powi(-26)), 0x0000);
    // 1 + 2^-11 is halfway between 1.0 and the next half: ties to even.
    assert_eq!(f32_to_half(1.0 + 2.0_f32.powi(-11)), 0x3c00);
    assert_eq!(f32_to_half(1.0 + 3.0 * 2.0_f32.powi(-11)), 0x3c02);
    for bits in 0..=0x7bff_u16 {
        assert_eq!(f32_to_half(half_to_f32(bits)), bits, "{bits:#06x}");
    }
}
