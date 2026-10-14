use super::macos::{capped_long_edge, half_to_f32, peak_channel};
use super::{HDR_MAX_PIXELS, decode, probe, probe_file};
use std::path::PathBuf;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/hdr")
        .join(name)
}

/// ImageIO reads ISO 21496-1 gain maps from macOS 15 on.
fn reads_iso_gain_maps() -> bool {
    objc2_foundation::NSProcessInfo::processInfo().isOperatingSystemAtLeastVersion(
        objc2_foundation::NSOperatingSystemVersion {
            majorVersion: 15,
            minorVersion: 0,
            patchVersion: 0,
        },
    )
}

#[test]
fn probes_hdr_data_without_decoding() {
    assert!(probe(&fixture("pq.heic")));
    assert!(!probe(&fixture("sdr.heic")));
    assert_eq!(probe_file(&fixture("missing.heic")), None);
    assert_eq!(probe_file(&fixture("sdr.heic")), Some(false));
    if reads_iso_gain_maps() {
        assert!(probe(&fixture("gainmap.heic")));
    }
}

#[test]
fn pq_decodes_above_sdr_white() {
    let Ok(Some(pixels)) = decode(&fixture("pq.heic"), 4096) else {
        panic!("pq.heic should decode to HDR");
    };
    assert_eq!(pixels.size, (64, 64));
    assert!(
        pixels.peak > 3.5 && pixels.peak < 4.5,
        "peak {}",
        pixels.peak
    );
}

#[test]
fn gain_map_decodes_above_sdr_white() {
    if !reads_iso_gain_maps() {
        return;
    }
    let Ok(Some(pixels)) = decode(&fixture("gainmap.heic"), 4096) else {
        panic!("gainmap.heic should decode to HDR");
    };
    assert!(pixels.peak > 2.0, "peak {}", pixels.peak);
}

#[test]
fn sdr_picture_has_no_hdr_rendition() {
    assert!(matches!(decode(&fixture("sdr.heic"), 4096), Ok(None)));
}

#[test]
fn decode_honours_the_long_edge() {
    let Ok(Some(pixels)) = decode(&fixture("pq.heic"), 32) else {
        panic!("pq.heic should decode to HDR");
    };
    assert_eq!(pixels.size, (32, 32));
}

#[test]
fn long_edge_stays_within_the_picture_and_the_pixel_cap() {
    assert_eq!(capped_long_edge((4032, 3024), 2048), 2048);
    assert_eq!(capped_long_edge((4032, 3024), 8192), 4032);
    let edge = capped_long_edge((8000, 6000), 8192);
    let other = u64::from(edge) * 6000 / 8000;
    assert!(u64::from(edge) * other <= HDR_MAX_PIXELS, "edge {edge}");
    assert!(edge > 5500, "edge {edge}");
}

#[test]
fn half_floats_convert() {
    assert_eq!(half_to_f32(0x3c00), 1.0);
    assert_eq!(half_to_f32(0x4400), 4.0);
    assert_eq!(half_to_f32(0xc000), -2.0);
    assert_eq!(half_to_f32(0x0000), 0.0);
    assert_eq!(half_to_f32(0x0001), 2.0_f32.powi(-24));
    assert!(half_to_f32(0x7c00).is_infinite());
    assert!(half_to_f32(0x7e00).is_nan());
}

#[test]
fn peak_skips_alpha_and_row_padding() {
    // Two pixels per row plus two halves of padding; alpha is 8.0.
    let row = [
        0x3c00, 0x3c00, 0x3c00, 0x4800, 0x4000, 0, 0, 0x4800, 0x7bff, 0x7bff,
    ];
    assert_eq!(peak_channel(&row, 10, 2), 2.0);
}

#[test]
fn peak_ignores_negative_and_non_finite_values() {
    // -8.0, +inf, NaN, then 1.5.
    let row = [0xc800, 0x7c00, 0x7e00, 0x3c00, 0x3e00, 0, 0, 0x3c00];
    assert_eq!(peak_channel(&row, 8, 2), 1.5);
}
