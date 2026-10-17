//! Hardware decoding through the real shim. CI runners have no GPU, so there
//! these prove the software path behind "hardware allowed"; on a machine with
//! one (VideoToolbox on a Mac) the first test decodes on it.

use super::{decoder, fixture};
use crate::video_decode::ffi::FfiDemuxer;
use crate::video_decode::{Demuxer, Item, MediaInfo, OpenOptions, PixelOutput, Pixels};

struct Decoded {
    opened: MediaInfo,
    finished: MediaInfo,
    pts: Vec<f64>,
    pictures: Vec<Pixels>,
    samples: usize,
}

fn decode(mut demuxer: FfiDemuxer, output: PixelOutput) -> Decoded {
    let opened = demuxer.info();
    demuxer.set_output(output);
    let (mut pts, mut pictures, mut samples) = (Vec::new(), Vec::new(), 0);
    while let Some(item) = demuxer.next().expect("decode the fixture") {
        match item {
            Item::Video(frame) => {
                pts.push(frame.pts);
                pictures.push(frame.pixels);
            }
            Item::Audio(chunk) => samples += chunk.samples.len(),
        }
    }
    Decoded {
        opened,
        finished: demuxer.info(),
        pts,
        pictures,
        samples,
    }
}

fn options(hardware: bool) -> OpenOptions {
    OpenOptions {
        hardware,
        ..OpenOptions::viewer()
    }
}

#[test]
fn every_picture_arrives_with_hardware_decoding_on_and_off() {
    if !decoder() {
        return;
    }
    let clip = fixture("clip-h264-aac.mp4");
    let software = decode(
        FfiDemuxer::open(&clip, &options(false)).expect("open"),
        PixelOutput::Bgra,
    );
    assert!(!software.opened.hardware && !software.finished.hardware);
    assert_eq!(software.pts.len(), 10);
    for output in [PixelOutput::Bgra, PixelOutput::Nv12] {
        let hardware = decode(
            FfiDemuxer::open(&clip, &options(true)).expect("open"),
            output,
        );
        eprintln!(
            "hardware allowed, {output:?}: decoded in {}",
            if hardware.finished.hardware {
                "hardware"
            } else {
                "software"
            }
        );
        assert_eq!(hardware.pts, software.pts, "{output:?}");
        assert_eq!(hardware.samples, software.samples);
        match output {
            PixelOutput::Bgra => assert!(
                hardware.pictures == software.pictures,
                "H.264 decodes bit-exact, so the pictures copied back match software"
            ),
            PixelOutput::Nv12 => assert!(
                hardware
                    .pictures
                    .iter()
                    .all(|pixels| matches!(pixels, Pixels::Nv12 { .. })),
                "pictures copied back come out as NV12"
            ),
        }
    }
}

#[test]
fn a_failing_hardware_decoder_hands_over_to_software_without_losing_a_picture() {
    if !decoder() {
        return;
    }
    let clip = fixture("clip-h264-aac.mp4");
    let software = decode(
        FfiDemuxer::open(&clip, &options(false)).expect("open"),
        PixelOutput::Bgra,
    );
    let failing = decode(
        FfiDemuxer::open_failing_hardware(&clip, &options(true)).expect("open"),
        PixelOutput::Bgra,
    );
    assert!(failing.opened.hardware, "the stand-in reports hardware");
    assert!(!failing.finished.hardware, "software after the failure");
    assert_eq!(failing.pts, software.pts, "no picture lost or repeated");
    let drift = failing.samples.abs_diff(software.samples);
    assert!(drift <= 2048, "sound kept: {drift} samples apart");
}
