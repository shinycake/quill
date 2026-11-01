//! Hardware decoding through the real shim. CI runners have no GPU, so there
//! these prove the software path behind "hardware allowed"; on a machine with
//! one (VideoToolbox on a Mac) the first test decodes on it.

use super::{decoder, fixture};
use crate::video_decode::ffi::{FfiDemuxer, StandIn};
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
        FfiDemuxer::open_stand_in(&clip, &options(true), StandIn::FailThirdPicture).expect("open"),
        PixelOutput::Bgra,
    );
    assert!(failing.opened.hardware, "the stand-in reports hardware");
    assert!(!failing.finished.hardware, "software after the failure");
    assert_eq!(failing.pts, software.pts, "no picture lost or repeated");
    let drift = failing.samples.abs_diff(software.samples);
    assert!(drift <= 2048, "sound kept: {drift} samples apart");
}

#[test]
fn a_hardware_decoder_that_software_cannot_replace_stays_in_charge() {
    if !decoder() {
        return;
    }
    let clip = fixture("clip-h264-aac.mp4");
    let software = decode(
        FfiDemuxer::open(&clip, &options(false)).expect("open"),
        PixelOutput::Bgra,
    );
    let mut demuxer =
        FfiDemuxer::open_stand_in(&clip, &options(true), StandIn::FailThirdPictureAndReopen)
            .expect("open");
    let mut pictures = 0;
    let failed = loop {
        match demuxer.next() {
            Ok(Some(Item::Video(_))) => pictures += 1,
            Ok(Some(Item::Audio(_))) => {}
            Ok(None) => break false,
            Err(_) => break true,
        }
    };
    assert!(failed, "the failed reopen ends decoding with an error");
    assert_eq!(pictures, 2, "the third picture is the one that failed");
    assert!(
        demuxer.info().hardware,
        "the hardware decoder is still the one installed"
    );
    demuxer.seek(0.0).expect("seek back");
    let again = decode(demuxer, PixelOutput::Bgra);
    assert_eq!(again.pts, software.pts, "a seek plays the whole clip on it");
    assert!(again.finished.hardware);
}

#[test]
fn a_seek_past_the_end_still_shows_the_last_picture_when_hardware_fails_on_the_way() {
    if !decoder() {
        return;
    }
    let clip = fixture("clip-h264-aac.mp4");
    let mut demuxer =
        FfiDemuxer::open_stand_in(&clip, &options(true), StandIn::FailThirdPicture).expect("open");
    let total = demuxer.info().duration.expect("a duration");
    demuxer.seek(total).expect("seek to the end");
    let after = decode(demuxer, PixelOutput::Bgra);
    let shown: Vec<f64> = after.pts.iter().map(|pts| (pts * 10.0).round()).collect();
    assert_eq!(shown, vec![9.0], "the last picture, once");
    assert!(
        !after.finished.hardware,
        "software took over during the seek"
    );
}
