use super::{
    FramePlan, Placement, WebmFrame, WebmFrames, WebmStep, center_into, decode_in_process,
    sample_fps,
};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

const TAKE: Placement = Placement {
    repeat_previous: 0,
    take: true,
};
const SKIP: Placement = Placement {
    repeat_previous: 0,
    take: false,
};

#[test]
fn sample_rate_matches_the_ffmpeg_path() {
    // max / (seconds + 1), at most 24 fps.
    assert_eq!(sample_fps(Some(3.0), 120), Ok(24.0));
    assert_eq!(sample_fps(Some(3.0), 36), Ok(9.0));
    assert_eq!(sample_fps(Some(2.0), 36), Ok(12.0));
    // Under half a second rounds up to one second.
    assert_eq!(sample_fps(Some(0.2), 36), Ok(18.0));
    assert_eq!(sample_fps(None, 120), Ok(24.0));
    assert_eq!(sample_fps(Some(30.4), 120), Ok(120.0 / 31.0));
    assert!(sample_fps(Some(30.6), 120).is_err());
}

#[test]
fn plan_keeps_the_nearest_picture_per_slot() {
    // 30 fps source sampled at 24 fps: one picture in five is dropped.
    let mut plan = FramePlan::new(24.0, 100, None);
    let placed: Vec<_> = (0..10)
        .map(|i| plan.place(f64::from(i) / 30.0).take)
        .collect();
    assert_eq!(placed.iter().filter(|take| **take).count(), 8);
    // Same rate: every picture.
    let mut plan = FramePlan::new(12.0, 100, None);
    assert!((0..6).all(|i| plan.place(f64::from(i) / 12.0) == TAKE));
}

#[test]
fn plan_repeats_over_gaps_and_stops_at_the_cap() {
    let mut plan = FramePlan::new(10.0, 5, None);
    assert_eq!(plan.place(0.0), TAKE);
    // A picture three slots on: two copies of the previous one first.
    assert_eq!(
        plan.place(0.3),
        Placement {
            repeat_previous: 2,
            take: true
        }
    );
    // A picture for a slot already filled is skipped.
    assert_eq!(plan.place(0.32), SKIP);
    // A long gap is cut so the cap still holds.
    assert_eq!(
        plan.place(5.0),
        Placement {
            repeat_previous: 0,
            take: true
        }
    );
    assert!(plan.full());
    assert_eq!(plan.place(5.1), SKIP);
    // A stream starting late counts from its first picture; bad times are safe.
    let mut plan = FramePlan::new(10.0, 3, None);
    assert_eq!(plan.place(7.0), TAKE);
    assert_eq!(plan.place(f64::NAN), SKIP);
    assert!(plan.place(f64::INFINITY).take);
}

#[test]
fn plan_holds_the_last_frame_to_the_end() {
    // Six pictures at 12 fps sampled at 24 fps: eleven frames placed, one
    // more holds the last picture to 0.5 s.
    let mut plan = FramePlan::new(24.0, 100, Some(0.5));
    let placed: usize = (0..6)
        .map(|i| {
            let p = plan.place(f64::from(i) / 12.0);
            p.repeat_previous + usize::from(p.take)
        })
        .sum();
    assert_eq!(placed, 11);
    assert_eq!(plan.tail(), 1);
    assert_eq!(plan.tail(), 0);
    // A picture that rounds to the slot at the very end is not an extra
    // frame: 2 s of 30 fps at 12 fps is 24 frames, not 25.
    let mut plan = FramePlan::new(12.0, 100, Some(2.0));
    let placed: usize = (0..60)
        .map(|i| {
            let p = plan.place(f64::from(i) / 30.0);
            p.repeat_previous + usize::from(p.take)
        })
        .sum();
    assert_eq!(placed + plan.tail(), 24);
    // Never past the cap, nothing without a duration or a frame.
    let mut plan = FramePlan::new(24.0, 4, Some(10.0));
    plan.place(0.0);
    assert_eq!(plan.tail(), 3);
    assert_eq!(FramePlan::new(24.0, 4, Some(1.0)).tail(), 0);
    let mut plan = FramePlan::new(24.0, 4, None);
    plan.place(0.0);
    assert_eq!(plan.tail(), 0);
}

#[test]
fn frames_center_on_a_square() {
    // 2x1 picture of opaque pixels A B onto a 4x4 square.
    let frame = [1, 1, 1, 255, 2, 2, 2, 255];
    let square = center_into(&frame, 2, 1, 4, 4);
    assert_eq!(square.len(), 4 * 4 * 4);
    let px = |x: usize, y: usize| &square[(y * 4 + x) * 4..(y * 4 + x) * 4 + 4];
    assert_eq!(px(1, 1), &[1, 1, 1, 255]);
    assert_eq!(px(2, 1), &[2, 2, 2, 255]);
    assert_eq!(px(0, 0), &[0, 0, 0, 0]);
    assert_eq!(px(1, 2), &[0, 0, 0, 0]);
    // Larger than the box: the middle is kept.
    let wide = [1, 0, 0, 0, 2, 0, 0, 0, 3, 0, 0, 0];
    assert_eq!(center_into(&wide, 3, 1, 1, 1), vec![2, 0, 0, 0]);
    // Truncated input never panics.
    assert_eq!(center_into(&[], 2, 2, 2, 2), vec![0; 16]);
    let picture = WebmFrame {
        width: 2,
        height: 1,
        bgra: frame.to_vec(),
    };
    assert_eq!(picture.into_square(4), square);
}

#[test]
fn gathered_steps_hold_each_picture_as_long_as_it_says() {
    let picture = |n| {
        WebmStep::Frame(WebmFrame {
            width: 1,
            height: 1,
            bgra: vec![n; 4],
        })
    };
    let mut frames = WebmFrames::default();
    frames.keep(WebmStep::Repeat(3));
    for step in [picture(1), WebmStep::Repeat(2), picture(2)] {
        frames.keep(step);
    }
    assert_eq!((frames.width, frames.height), (1, 1));
    let held: Vec<u8> = frames
        .frames
        .iter()
        .filter_map(|f| f.first().copied())
        .collect();
    assert_eq!(held, [1, 1, 1, 2]);
}

fn gathered(
    path: &Path,
    edge: usize,
    cancelled: &AtomicBool,
) -> Result<Option<WebmFrames>, String> {
    let mut frames = WebmFrames::default();
    let fps = decode_in_process(path, edge, 120, cancelled, &mut |step| {
        frames.keep(step);
        Ok(())
    })?;
    Ok(fps.map(|fps| WebmFrames { fps, ..frames }))
}

// ---- The real decoder: skipped where the quillvideo library (with its
// sticker decoder) isn't installed; set QUILL_VIDEO_LIB to run it. The
// `video_decode` in the name puts it in the Linux package job's
// `cargo test --lib video_decode` run against the freshly built FFmpeg. ----

#[test]
fn video_decode_webm_sticker_keeps_vp9_alpha() {
    let cancelled = AtomicBool::new(false);
    let path = fixture("sticker-vp9-alpha.webm");
    // 64x48, 12 fps, 0.5 s (six pictures); the left half is transparent,
    // the right half opaque (R 200, G 40 * frame, B 60).
    let Some(decoded) = gathered(&path, 64, &cancelled).expect("decode fixture") else {
        eprintln!("skipping: the quillvideo sticker decoder is not installed");
        return;
    };
    assert_eq!((decoded.width, decoded.height), (64, 48));
    // Rounded to one second: 120 / 2 = 60, capped at 24 fps; 0.5 s of it
    // with every picture's slot filled (repeats in the gaps): 12 frames.
    assert_eq!(decoded.fps, 24.0);
    assert_eq!(decoded.frames.len(), 12);
    let px = |frame: &[u8], x: usize, y: usize| -> [u8; 4] {
        let at = (y * 64 + x) * 4;
        [frame[at], frame[at + 1], frame[at + 2], frame[at + 3]]
    };
    for frame in &decoded.frames {
        assert_eq!(frame.len(), 64 * 48 * 4);
        assert!(
            px(frame, 2, 24)[3] < 16,
            "transparent half: {:?}",
            px(frame, 2, 24)
        );
        let opaque = px(frame, 60, 24);
        assert!(opaque[3] > 240, "opaque half: {opaque:?}");
        // BGRA order: red is the third byte.
        assert!((185..=215).contains(&opaque[2]) && (45..=75).contains(&opaque[0]));
    }
    // The picture changes over time (green ramps up).
    let first = px(&decoded.frames[0], 60, 24)[1];
    let last = px(decoded.frames.last().unwrap(), 60, 24)[1];
    assert!(last > first + 150, "green {first} -> {last}");

    // Fit into a smaller box (and a larger one: enlarged, as ffmpeg's
    // `scale` did).
    let small = gathered(&path, 32, &cancelled).unwrap().unwrap();
    assert_eq!((small.width, small.height), (32, 24));
    assert_eq!(small.frames.len(), 12);
    let large = gathered(&path, 128, &cancelled).unwrap().unwrap();
    assert_eq!((large.width, large.height), (128, 96));
    assert!(px_at(&small.frames[0], 32, 1, 12)[3] < 16);
    assert!(px_at(&small.frames[0], 32, 30, 12)[3] > 240);

    // Cancelled before the first picture.
    let stop = AtomicBool::new(true);
    assert!(gathered(&path, 64, &stop).is_err());
    // Not a sticker.
    assert!(gathered(&fixture("clip-h264-aac.mp4"), 64, &cancelled).is_err());
}

fn px_at(frame: &[u8], width: usize, x: usize, y: usize) -> [u8; 4] {
    let at = (y * width + x) * 4;
    [frame[at], frame[at + 1], frame[at + 2], frame[at + 3]]
}

/// Per-sticker decode time and memory for docs/decisions/sweep-webm-stickers.md:
/// `QUILL_VIDEO_LIB=<libquillvideo> cargo test --lib webm_decode_bench -- --ignored --nocapture`
/// (`QUILL_WEBM_BENCH=<a.webm>:<b.webm>` picks the files). Without the library
/// it times the `ffmpeg` fallback.
#[cfg(unix)]
#[test]
#[ignore = "benchmark; prints timings"]
fn webm_decode_bench() {
    use std::time::Instant;
    fn max_rss_kb(who: i32) -> i64 {
        // SAFETY: getrusage fills the zeroed struct it is given.
        let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
        // SAFETY: valid out pointer.
        unsafe { libc::getrusage(who, &mut usage) };
        let raw = usage.ru_maxrss as i64;
        if cfg!(target_os = "macos") {
            raw / 1024
        } else {
            raw
        }
    }
    let files: Vec<_> = match std::env::var_os("QUILL_WEBM_BENCH") {
        Some(list) => std::env::split_paths(&list).collect(),
        None => vec![
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("docs/screenshots/fixtures/demo-sticker.webm"),
        ],
    };
    println!("rss_self_start_kb={}", max_rss_kb(libc::RUSAGE_SELF));
    let cancel = AtomicBool::new(false);
    let child = std::sync::Arc::new(std::sync::Mutex::new(None));
    for path in &files {
        for (edge, max) in [(224usize, 120usize), (56, 36)] {
            let mut times = Vec::new();
            let mut count = 0;
            for i in 0..7 {
                let cache = crate::scratch::process_dir(&format!("quill-webm-bench-{i}"));
                let start = Instant::now();
                let decoded = crate::sticker_playback::decode_webm_sized(
                    path, &cache, edge, max, &child, &cancel,
                )
                .unwrap();
                times.push(start.elapsed().as_secs_f64() * 1000.0);
                count = decoded.frames.len();
            }
            times.sort_by(f64::total_cmp);
            println!(
                "{} edge={edge} max={max} frames={count} ms_median={:.1} ms_min={:.1} ms_max={:.1} rss_self_kb={} rss_children_kb={}",
                path.file_name().unwrap().to_string_lossy(),
                times[3],
                times[0],
                times[6],
                max_rss_kb(libc::RUSAGE_SELF),
                max_rss_kb(libc::RUSAGE_CHILDREN)
            );
        }
    }
    println!(
        "rss_self_end_kb={} rss_children_max_kb={}",
        max_rss_kb(libc::RUSAGE_SELF),
        max_rss_kb(libc::RUSAGE_CHILDREN)
    );
}
