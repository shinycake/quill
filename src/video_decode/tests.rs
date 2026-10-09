use super::{
    AUDIO_HIGH_SECS, AUDIO_LOW_SECS, AUDIO_OUTPUT_LATENCY, AudioChunk, Clock, Demuxer, Item,
    MediaInfo, OpenOptions, Player, VideoFrame, rotate_bgra, take_due, wants_more,
};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

fn frame(pts: f64) -> VideoFrame {
    VideoFrame {
        pts,
        width: 1,
        height: 1,
        bgra: vec![0; 4],
    }
}

fn wait_until(mut done: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if done() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    done()
}

#[test]
fn rotate_turns_pictures_clockwise_and_packs_rows() {
    // 2x1 picture, stride 12 (4 bytes of row padding): A B.
    let a = [1, 1, 1, 255];
    let b = [2, 2, 2, 255];
    let mut src = Vec::new();
    src.extend(a);
    src.extend(b);
    src.extend([9; 4]);
    let (same, w, h) = rotate_bgra(&src, 2, 1, 12, 0);
    assert_eq!((w, h), (2, 1));
    assert_eq!(same, [a, b].concat());
    // 90° clockwise: A on top, B below.
    let (cw, w, h) = rotate_bgra(&src, 2, 1, 12, 90);
    assert_eq!((w, h), (1, 2));
    assert_eq!(cw, [a, b].concat());
    let (half, w, h) = rotate_bgra(&src, 2, 1, 12, 180);
    assert_eq!((w, h), (2, 1));
    assert_eq!(half, [b, a].concat());
    let (ccw, w, h) = rotate_bgra(&src, 2, 1, 12, 270);
    assert_eq!((w, h), (1, 2));
    assert_eq!(ccw, [b, a].concat());
}

#[test]
fn rotate_90_maps_the_left_column_to_the_top_row() {
    // 2x2: [A B / C D] turned clockwise is [C A / D B].
    let px = |v: u8| [v, v, v, 255];
    let src = [px(1), px(2), px(3), px(4)].concat();
    let (out, w, h) = rotate_bgra(&src, 2, 2, 8, 90);
    assert_eq!((w, h), (2, 2));
    assert_eq!(out, [px(3), px(1), px(4), px(2)].concat());
}

#[test]
fn clock_holds_after_a_seek_until_released() {
    let t0 = Instant::now();
    let mut clock = Clock::new();
    clock.play(t0);
    clock.seek(2.0, t0);
    assert_eq!(clock.position(t0 + Duration::from_secs(1)), 2.0);
    let t1 = t0 + Duration::from_secs(1);
    clock.release(t1);
    let p = clock.position(t1 + Duration::from_millis(500));
    assert!((p - 2.5).abs() < 1e-9, "{p}");
}

#[test]
fn clock_pauses_and_scales_with_rate() {
    let t0 = Instant::now();
    let mut clock = Clock::new();
    clock.release(t0);
    clock.play(t0);
    let t1 = t0 + Duration::from_secs(1);
    clock.set_rate(2.0, t1);
    let t2 = t1 + Duration::from_secs(1);
    assert!((clock.position(t2) - 3.0).abs() < 1e-9);
    clock.pause(t2);
    assert!((clock.position(t2 + Duration::from_secs(5)) - 3.0).abs() < 1e-9);
    // A nonsense rate falls back to normal speed.
    clock.set_rate(f64::NAN, t2);
    clock.play(t2);
    assert!((clock.position(t2 + Duration::from_secs(1)) - 4.0).abs() < 1e-9);
}

#[test]
fn take_due_shows_the_newest_due_picture_and_drops_late_ones() {
    let mut queue: VecDeque<VideoFrame> = [0.0, 0.04, 0.08, 0.12].map(frame).into();
    // Nothing due yet before the first picture's time…
    let mut early: VecDeque<VideoFrame> = [0.5].map(frame).into();
    assert!(take_due(&mut early, 0.0, false).is_none());
    // …unless it is the first picture after a seek.
    assert_eq!(take_due(&mut early, 0.0, true).map(|f| f.pts), Some(0.5));
    // At 0.09 the 0.08 picture is due; 0.0 and 0.04 are late and dropped.
    assert_eq!(take_due(&mut queue, 0.09, false).map(|f| f.pts), Some(0.08));
    assert_eq!(queue.len(), 1);
    // Half a refresh early still counts as due.
    assert_eq!(
        take_due(&mut queue, 0.115, false).map(|f| f.pts),
        Some(0.12)
    );
}

#[test]
fn decoding_stops_at_the_queue_bounds() {
    // Pictures short: decode.
    assert!(wants_more(Some((1, 3)), None));
    // Pictures full, no sound: wait.
    assert!(!wants_more(Some((3, 3)), None));
    // Pictures full but sound below its low mark: keep going for the sound.
    assert!(wants_more(Some((3, 3)), Some(AUDIO_LOW_SECS / 2.0)));
    assert!(!wants_more(Some((3, 3)), Some(AUDIO_LOW_SECS)));
    // Never past the sound cap, even when pictures are short.
    assert!(!wants_more(Some((0, 3)), Some(AUDIO_HIGH_SECS)));
    // Sound only: keep the low mark buffered.
    assert!(wants_more(None, Some(0.0)));
    assert!(!wants_more(None, Some(AUDIO_LOW_SECS)));
}

#[test]
fn open_options_budget_picture_memory() {
    let inline = OpenOptions::inline(640);
    assert!(!inline.want_audio);
    assert_eq!(inline.video_budget_bytes(), 640 * 640 * 4 * 3);
    let viewer = OpenOptions::viewer();
    assert!(viewer.want_audio);
    // Four 1920x1920 BGRA pictures at most: under 60 MB.
    assert!(viewer.video_budget_bytes() < 60 * 1024 * 1024);
}

/// A clip that never ends unless `frames` is set: pictures at `fps`, sound
/// in 20 ms chunks, in presentation order like a real interleaved file.
struct Scripted {
    fps: f64,
    frames: Option<usize>,
    audio: bool,
    next_video: f64,
    next_audio: f64,
    calls: Arc<AtomicUsize>,
}

const RATE: u32 = 1000;

impl Scripted {
    fn new(fps: f64, frames: Option<usize>, audio: bool, calls: Arc<AtomicUsize>) -> Self {
        Self {
            fps,
            frames,
            audio,
            next_video: 0.0,
            next_audio: 0.0,
            calls,
        }
    }

    fn end(&self) -> Option<f64> {
        self.frames.map(|n| n as f64 / self.fps)
    }
}

impl Demuxer for Scripted {
    fn info(&self) -> MediaInfo {
        MediaInfo {
            duration: self.end(),
            frame_rate: self.fps,
            width: 1,
            height: 1,
            has_video: true,
            has_audio: self.audio,
            sample_rate: RATE,
            channels: 1,
            ..MediaInfo::default()
        }
    }

    fn next(&mut self) -> Result<Option<Item>, String> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let end = self.end().unwrap_or(f64::MAX);
        let video_left = self.next_video < end - 1e-9;
        let audio_left = self.audio && self.next_audio < end - 1e-9;
        if audio_left && (!video_left || self.next_audio <= self.next_video) {
            let pts = self.next_audio;
            self.next_audio += 0.02;
            return Ok(Some(Item::Audio(AudioChunk {
                pts,
                samples: vec![0.25; (RATE as f64 * 0.02) as usize],
            })));
        }
        if video_left {
            let pts = self.next_video;
            self.next_video += 1.0 / self.fps;
            return Ok(Some(Item::Video(frame(pts))));
        }
        Ok(None)
    }

    fn seek(&mut self, secs: f64) -> Result<(), String> {
        // Restart at the picture covering `secs`.
        let index = (secs * self.fps).floor();
        self.next_video = index / self.fps;
        self.next_audio = secs;
        Ok(())
    }
}

fn scripted_player(frames: Option<usize>, audio: bool, queue: usize) -> (Player, Arc<AtomicUsize>) {
    let calls = Arc::new(AtomicUsize::new(0));
    let demuxer = Scripted::new(25.0, frames, audio, calls.clone());
    let options = OpenOptions {
        max_width: 0,
        max_height: 0,
        threads: 1,
        want_audio: audio,
        video_frames: queue,
    };
    (Player::with_demuxer(Box::new(demuxer), options), calls)
}

#[test]
fn a_paused_player_decodes_only_its_queue() {
    let (player, calls) = scripted_player(None, false, 3);
    assert!(wait_until(|| calls.load(Ordering::SeqCst) >= 3));
    std::thread::sleep(Duration::from_millis(100));
    // Three pictures queued, then the thread sleeps: no runaway decoding.
    assert_eq!(calls.load(Ordering::SeqCst), 3);
    drop(player);
}

#[test]
fn a_silent_idle_player_sleeps_long_but_wakes_when_a_picture_is_taken() {
    use super::{IDLE_POLL, idle_wait};
    // Sound is drained without the state lock, so it keeps the short poll;
    // a silent player is woken by every change and only needs a safety net.
    assert_eq!(idle_wait(true), IDLE_POLL);
    assert!(idle_wait(false) >= IDLE_POLL * 10);
    let (mut player, calls) = scripted_player(None, false, 3);
    assert!(wait_until(|| calls.load(Ordering::SeqCst) >= 3));
    std::thread::sleep(Duration::from_millis(60));
    // Taking the first picture frees a slot: the thread refills it at once,
    // not after its long idle wait.
    let taken = Instant::now();
    assert!(player.take_frame().is_some());
    assert!(wait_until(|| calls.load(Ordering::SeqCst) >= 4));
    assert!(taken.elapsed() < idle_wait(false) / 2);
}

#[test]
fn the_first_picture_shows_at_once_then_follows_the_clock() {
    let (mut player, _) = scripted_player(None, false, 3);
    assert!(wait_until(|| player.info().is_some()));
    let mut first = None;
    assert!(wait_until(|| {
        first = player.take_frame();
        first.is_some()
    }));
    assert_eq!(first.map(|f| f.pts), Some(0.0));
    // Paused: the clock doesn't run, no newer picture is due.
    std::thread::sleep(Duration::from_millis(100));
    assert!(player.take_frame().is_none());
    player.play();
    let mut shown = None;
    assert!(wait_until(|| {
        if let Some(frame) = player.take_frame() {
            shown = Some(frame.pts);
        }
        shown.is_some_and(|pts| pts >= 0.08)
    }));
    let position = player.position();
    assert!(position >= 0.08 - 0.01, "{position}");
}

#[test]
fn a_seek_drops_stale_pictures_and_restarts_there() {
    let (mut player, _) = scripted_player(Some(250), false, 3);
    assert!(wait_until(|| player.take_frame().is_some()));
    player.seek(4.0);
    let mut after = None;
    assert!(wait_until(|| {
        after = player.take_frame();
        after.is_some()
    }));
    let pts = after.map(|f| f.pts).unwrap_or_default();
    assert!(
        (4.0 - 0.04..=4.0).contains(&pts),
        "first picture after seek: {pts}"
    );
    assert!((player.position() - 4.0).abs() < 1e-9);
}

#[test]
fn a_clip_ends_and_play_starts_it_over() {
    let (mut player, _) = scripted_player(Some(5), false, 3);
    player.play();
    assert!(wait_until(|| {
        player.take_frame();
        !player.is_playing()
    }));
    assert!(
        (player.position() - 0.2).abs() < 1e-9,
        "{}",
        player.position()
    );
    player.play();
    assert!(player.is_playing());
    let mut first = None;
    assert!(wait_until(|| {
        first = player.take_frame();
        first.is_some()
    }));
    assert_eq!(first.map(|f| f.pts), Some(0.0));
}

#[test]
fn sound_is_the_clock() {
    let (mut player, _) = scripted_player(None, true, 3);
    let mut tap = None;
    assert!(wait_until(|| {
        tap = player.take_audio_tap();
        tap.is_some()
    }));
    let mut tap = tap.expect("tap");
    assert_eq!((tap.sample_rate(), tap.channels()), (RATE, 1));
    assert!(wait_until(|| player.take_frame().is_some()));
    player.play();
    // Pull exactly half a second of sound (the output would at 1x).
    let mut pulled = 0;
    let deadline = Instant::now() + Duration::from_secs(5);
    while pulled < RATE / 2 && Instant::now() < deadline {
        tap.next_sample();
        if tap.sounding() {
            pulled += 1;
        } else {
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    let position = player.position();
    let expected = 0.5 - AUDIO_OUTPUT_LATENCY;
    assert!(
        (position - expected).abs() <= super::SYNC_TOLERANCE + 0.002,
        "position {position} for half a second of sound"
    );
    // The picture due at that position is the one shown.
    let mut shown = None;
    while let Some(frame) = player.take_frame() {
        shown = Some(frame.pts);
    }
    if let Some(pts) = shown {
        assert!(pts <= position + 0.01 && pts > position - 0.05, "{pts}");
    }
}

#[test]
fn a_seek_restarts_the_sound_at_the_target() {
    let (mut player, _) = scripted_player(Some(500), true, 3);
    let mut tap = None;
    assert!(wait_until(|| {
        tap = player.take_audio_tap();
        tap.is_some()
    }));
    let mut tap = tap.expect("tap");
    player.play();
    for _ in 0..300 {
        tap.next_sample();
    }
    player.seek(10.0);
    // Until new sound plays, the clock reads the target.
    assert!((player.position() - 10.0).abs() < 1e-9);
    let mut pulled = 0;
    let deadline = Instant::now() + Duration::from_secs(5);
    while pulled < 100 && Instant::now() < deadline {
        tap.next_sample();
        if tap.sounding() {
            pulled += 1;
        } else {
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    let position = player.position();
    assert!(
        (10.0..10.2).contains(&position),
        "position after seek: {position}"
    );
}

#[test]
fn dropping_the_sound_falls_back_to_a_wall_clock_without_stalling() {
    let (mut player, calls) = scripted_player(None, true, 3);
    assert!(wait_until(|| player.info().is_some()));
    drop(player.take_audio_tap());
    assert!(!player.has_sound());
    player.play();
    let mut latest = 0.0;
    assert!(wait_until(|| {
        if let Some(frame) = player.take_frame() {
            latest = frame.pts;
        }
        latest >= 0.2
    }));
    // Pictures kept coming although nobody drains the sound.
    assert!(calls.load(Ordering::SeqCst) > 5);
}

#[test]
fn the_sound_buffer_is_bounded() {
    let (mut player, calls) = scripted_player(None, true, 1000);
    let mut tap = None;
    assert!(wait_until(|| {
        tap = player.take_audio_tap();
        tap.is_some()
    }));
    // Nobody pulls: the thread stops at the sound cap.
    std::thread::sleep(Duration::from_millis(300));
    let settled = calls.load(Ordering::SeqCst);
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(calls.load(Ordering::SeqCst), settled);
    // AUDIO_HIGH_SECS of 20 ms chunks plus the pictures between them.
    let chunks = (AUDIO_HIGH_SECS / 0.02) as usize;
    assert!(settled <= chunks * 3, "{settled} decode calls");
    drop(tap);
}

// ---- The real decoder (skipped where the quillvideo shim isn't installed;
// set QUILL_VIDEO_LIB to run these against a build of it). ----

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn decoder() -> bool {
    if super::available() {
        return true;
    }
    eprintln!(
        "skipping: quillvideo not available ({})",
        super::unavailable_reason().unwrap_or("unknown")
    );
    false
}

fn decode_all(path: &Path, options: &OpenOptions) -> (MediaInfo, Vec<VideoFrame>, usize) {
    let mut demuxer = super::ffi::FfiDemuxer::open(path, options).expect("open fixture");
    let info = demuxer.info();
    let mut frames = Vec::new();
    let mut samples = 0;
    while let Some(item) = demuxer.next().expect("decode fixture") {
        match item {
            Item::Video(frame) => frames.push(frame),
            Item::Audio(chunk) => samples += chunk.samples.len(),
        }
    }
    (info, frames, samples)
}

#[test]
fn ffmpeg_decodes_h264_with_aac() {
    if !decoder() {
        return;
    }
    let (info, frames, samples) = decode_all(&fixture("clip-h264-aac.mp4"), &OpenOptions::viewer());
    assert_eq!((info.width, info.height, info.rotation), (96, 64, 0));
    assert_eq!(info.video_codec, "h264");
    assert_eq!(info.audio_codec, "aac");
    assert!(info.has_audio && info.channels == 1 && info.sample_rate == 48_000);
    assert!(info.duration.is_some_and(|d| (0.9..1.2).contains(&d)));
    assert_eq!(frames.len(), 10);
    // Presentation order despite B-frames.
    assert!(frames.windows(2).all(|w| w[0].pts < w[1].pts));
    assert!(frames[0].pts.abs() < 1e-6);
    assert_eq!(frames[0].bgra.len(), 96 * 64 * 4);
    // testsrc2 is colourful and opaque.
    assert!(frames[0].bgra.chunks(4).all(|px| px[3] == 255));
    assert!(
        frames[0]
            .bgra
            .chunks(4)
            .any(|px| px[0] != px[1] || px[1] != px[2])
    );
    // About a second of mono sound.
    assert!((40_000..=56_000).contains(&samples), "{samples} samples");
}

#[test]
fn ffmpeg_scales_into_the_box_and_skips_sound_when_muted() {
    if !decoder() {
        return;
    }
    let (info, frames, samples) =
        decode_all(&fixture("clip-h264-aac.mp4"), &OpenOptions::inline(48));
    assert_eq!((info.width, info.height), (48, 32));
    assert!(!info.has_audio);
    assert_eq!(samples, 0);
    assert_eq!((frames[0].width, frames[0].height), (48, 32));
}

#[test]
fn ffmpeg_turns_rotated_clips_upright() {
    if !decoder() {
        return;
    }
    let (info, frames, _) = decode_all(&fixture("clip-rotated.mp4"), &OpenOptions::viewer());
    assert_eq!(info.rotation, 270);
    assert_eq!((info.width, info.height), (64, 96));
    assert_eq!((frames[0].width, frames[0].height), (64, 96));
}

#[test]
fn ffmpeg_decodes_vp9_webm() {
    if !decoder() {
        return;
    }
    let (info, frames, _) = decode_all(&fixture("clip-vp9.webm"), &OpenOptions::viewer());
    assert_eq!(info.video_codec, "vp9");
    assert_eq!((info.width, info.height), (64, 48));
    assert_eq!(frames.len(), 10);
}

#[test]
fn ffmpeg_seeks_to_the_picture_covering_the_target() {
    if !decoder() {
        return;
    }
    let mut demuxer =
        super::ffi::FfiDemuxer::open(&fixture("clip-h264-aac.mp4"), &OpenOptions::inline(96))
            .expect("open");
    demuxer.seek(0.55).expect("seek");
    let first = loop {
        match demuxer.next().expect("decode") {
            Some(Item::Video(frame)) => break frame,
            Some(Item::Audio(_)) => continue,
            None => panic!("no picture after seek"),
        }
    };
    assert!((first.pts - 0.5).abs() < 1e-6, "{}", first.pts);
}

#[test]
fn ffmpeg_reports_unreadable_files() {
    if !decoder() {
        return;
    }
    let err = super::ffi::FfiDemuxer::open(&fixture("missing.mp4"), &OpenOptions::viewer())
        .err()
        .expect("missing file fails");
    assert!(!err.is_empty());
    let not_video = fixture("tone-440hz.opus.ogg");
    // An audio-only file opens (sound, no picture) only when sound is wanted.
    assert!(super::ffi::FfiDemuxer::open(&not_video, &OpenOptions::inline(64)).is_err());
}

#[test]
fn ffmpeg_player_plays_a_clip_to_the_end() {
    if !decoder() {
        return;
    }
    let mut player =
        Player::open(&fixture("clip-h264-aac.mp4"), OpenOptions::inline(96)).expect("open");
    player.play();
    let mut shown = 0;
    assert!(wait_until(|| {
        if player.take_frame().is_some() {
            shown += 1;
        }
        !player.is_playing()
    }));
    assert!(player.error().is_none());
    // At 10 fps over ~1 s, polled every 2 ms: every picture shows.
    assert_eq!(shown, 10);
    assert!(player.position() >= 0.99);
}

#[test]
fn pictures_pace_on_the_wall_clock_until_the_sound_drifts_away() {
    use super::{SYNC_TOLERANCE, synced_position};
    // No sound: the wall clock.
    assert_eq!(synced_position(1.0, None), 1.0);
    // Small drift (the sound is pulled in bursts): stay smooth.
    assert_eq!(synced_position(1.0, Some(1.0 + SYNC_TOLERANCE * 0.9)), 1.0);
    assert_eq!(synced_position(1.0, Some(1.0 - SYNC_TOLERANCE * 0.9)), 1.0);
    // The sound stalled (decoder behind) or ran ahead: follow it.
    assert_eq!(synced_position(1.0, Some(0.9)), 0.9);
    assert_eq!(synced_position(1.0, Some(1.2)), 1.2);
}
