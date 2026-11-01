use super::head::{PAUSE_GAP, REPAINT_GAP, pick};
use super::source::{Packer, Source};
use super::store::{self, Scratch, Store};
use super::*;
use std::io::Write;
use std::path::Path;
use std::time::{Duration, Instant};

fn timeline(frames: u32, file_fps: f64, step: u32) -> Timeline {
    Timeline {
        frames,
        file_fps,
        step,
    }
}

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

#[test]
fn a_looping_head_wraps_and_asks_ahead() {
    // 10 frames at 10 fps: frame n at n × 100 ms.
    let tl = timeline(10, 10.0, 1);
    let at = |t| pick(ms(t), PlayMode::Loop, tl, false, 60.0).unwrap();
    assert_eq!(at(0).index, 0);
    assert_eq!(at(350).index, 3);
    assert_eq!(at(1250).index, 2, "second loop");
    assert!(at(1250).animating);
    assert_eq!(at(900).ahead, [Some(0), Some(1), Some(2)], "wraps ahead");
}

#[test]
fn a_one_shot_rests_on_the_frame_it_names() {
    let tl = timeline(10, 10.0, 1);
    let last = pick(ms(1500), PlayMode::Once(Rest::Last), tl, false, 60.0).unwrap();
    assert_eq!((last.index, last.animating), (9, false));
    let first = pick(ms(1500), PlayMode::Once(Rest::First), tl, false, 60.0).unwrap();
    assert_eq!((first.index, first.animating), (0, false));
    let playing = pick(ms(850), PlayMode::Once(Rest::First), tl, false, 60.0).unwrap();
    assert_eq!((playing.index, playing.animating), (8, true));
    assert_eq!(
        playing.ahead,
        [Some(9), Some(0), None],
        "the frame it rests on, then nothing past the end"
    );
    let last = pick(ms(850), PlayMode::Once(Rest::Last), tl, false, 60.0).unwrap();
    assert_eq!(last.ahead, [Some(9), None, None]);
}

#[test]
fn limited_loops_play_then_rest_on_the_first_frame() {
    let tl = timeline(4, 10.0, 1);
    let at = |t| pick(ms(t), PlayMode::Loops(2), tl, false, 60.0).unwrap();
    assert_eq!((at(650).index, at(650).animating), (2, true));
    assert_eq!((at(800).index, at(800).animating), (0, false));
}

#[test]
fn a_fixed_position_shows_nothing_past_the_end() {
    let tl = timeline(4, 10.0, 1);
    assert_eq!(
        pick(ms(0), PlayMode::At(ms(250)), tl, false, 60.0).map(|p| p.index),
        Some(2)
    );
    assert!(pick(ms(0), PlayMode::At(ms(400)), tl, false, 60.0).is_none());
}

#[test]
fn jittered_paints_hold_each_frame_for_the_same_number_of_refreshes() {
    // A 60 fps file on a 120 Hz screen, shown at the file's rate: paints
    // every 8.33 ms, up to 1 ms early or late, show each frame twice.
    let tl = timeline(120, 60.0, 2);
    let mut shown = Vec::new();
    for paint in 0..60u32 {
        let jitter = [-1.0, 0.6, -0.4, 1.0][paint as usize % 4];
        let at = (f64::from(paint) * 1000.0 / 120.0 + jitter).max(0.0);
        let pos = Duration::from_secs_f64(at / 1000.0);
        shown.push(pick(pos, PlayMode::Loop, tl, false, 120.0).unwrap().index);
    }
    for pair in shown.chunks(2) {
        assert_eq!(pair.len(), 2);
        assert_eq!(pair.first(), pair.last(), "{shown:?}");
    }
}

/// `index` on the lap of a 60 frame loop nearest `along`.
fn nearest_lap(index: u32, along: i64) -> i64 {
    let off = (i64::from(index) - along).rem_euclid(60);
    along + if off > 30 { off - 60 } else { off }
}

/// Steps between the frames a 30 fps head shows when painted every fourth
/// refresh of a 120 Hz display, started `offset` refreshes after that beat,
/// with every paint up to 1.5 ms early or late.
fn emoji_steps_at_offset(offset: u32) -> Vec<i64> {
    let tl = timeline(60, 30.0, 1);
    let refresh = 1.0 / 120.0;
    let mut seed: u64 = 0x9e37_79b9 ^ u64::from(offset);
    let mut jitter = move || {
        seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
        ((seed >> 33) as f64 / f64::from(1u32 << 31) - 0.5) * 0.003
    };
    let start = f64::from(offset) * refresh + jitter();
    let mut shown: Vec<i64> = Vec::new();
    for beat in 1..2000u32 {
        let paint = f64::from(beat * 4) * refresh + jitter();
        if paint <= start {
            continue;
        }
        let pos = Duration::from_secs_f64(paint - start);
        let along = (pos.as_secs_f64() * 30.0 + 0.5) as i64;
        let index = pick(pos, PlayMode::Loop, tl, false, 120.0).unwrap().index;
        shown.push(nearest_lap(index, along));
    }
    shown
        .windows(2)
        .map(|pair| match pair {
            [a, b] => b - a,
            _ => 0,
        })
        .collect()
}

#[test]
fn heads_painted_with_jitter_show_every_frame_once_at_any_phase() {
    for offset in 0..4 {
        let steps = emoji_steps_at_offset(offset);
        let uneven = steps.iter().filter(|step| **step != 1).count();
        assert_eq!(uneven, 0, "offset {offset}: {uneven} of {}", steps.len());
    }
}

/// A view of `tl` painted for a second on a 120 Hz display, on the
/// refreshes where a clock running free at `tick_fps` found its frame
/// changing. A tick asks about the refresh after it, and at display rates
/// the clock's wait is 92% of a frame (`frame_clock::tick_period`). The
/// refreshes it painted on, and those it showed a new frame on.
fn paints_when_changed(tl: Timeline, high_rate: bool, tick_fps: f64) -> (Vec<u32>, Vec<u32>) {
    let t0 = Instant::now();
    let refresh = Duration::from_secs_f64(1.0 / 120.0);
    let tick_every = Duration::from_secs_f64(0.92 / tick_fps);
    let mut head = PlayHead::default();
    head.start(t0);
    let mut shown = Some(0);
    let mut tick = t0 + tick_every;
    let (mut paints, mut changes) = (Vec::new(), Vec::new());
    for vsync in 1..=120u32 {
        let paint = t0 + refresh * vsync + Duration::from_micros(u64::from(vsync * 37 % 900));
        let mut changing = false;
        while tick <= paint {
            changing |=
                head.changes_at(tick + refresh, shown, PlayMode::Loop, tl, high_rate, 120.0);
            tick += tick_every;
        }
        if !changing {
            continue;
        }
        paints.push(vsync);
        let index = head
            .advance(paint, PlayMode::Loop, tl, high_rate, 120.0)
            .map(|p| p.index);
        if index != shown {
            changes.push(vsync);
            shown = index;
        }
    }
    (paints, changes)
}

fn gaps(at: &[u32]) -> Vec<u32> {
    at.windows(2)
        .map(|pair| match pair {
            [a, b] => b - a,
            _ => 0,
        })
        .collect()
}

#[test]
fn a_clip_at_its_files_rate_paints_only_for_its_own_frames() {
    for (tl, frames, hold) in [
        (timeline(480, 30.0, 4), 30, 4),
        (timeline(480, 60.0, 2), 60, 2),
    ] {
        let (paints, changes) = paints_when_changed(tl, false, tl.tick_fps(true));
        assert_eq!(changes.len(), frames, "{changes:?}");
        assert!(gaps(&changes).iter().all(|gap| *gap == hold), "{changes:?}");
        assert!(paints.len() < frames * 3 / 2, "{} paints", paints.len());
    }
}

#[test]
fn a_clip_on_the_fine_grid_paints_every_refresh() {
    let tl = timeline(240, 60.0, 2);
    let (paints, changes) = paints_when_changed(tl, true, tl.tick_fps(true));
    assert_eq!(changes.len(), 120);
    assert_eq!(paints.len(), 120);
}

#[test]
fn a_slow_clip_paints_often_enough_to_keep_playing() {
    let refreshes = |gap: Duration| (gap.as_secs_f64() * 120.0).round() as u32;
    let fps = 1.0 / PAUSE_GAP.as_secs_f64();
    let (paints, changes) = paints_when_changed(timeline(20, fps, 1), false, 120.0);
    let longest_paint_gap = refreshes(REPAINT_GAP);
    let frame_gap = refreshes(PAUSE_GAP);
    assert!(
        gaps(&paints).iter().all(|gap| *gap <= longest_paint_gap),
        "{paints:?}"
    );
    assert!(
        gaps(&changes).iter().all(|gap| *gap == frame_gap),
        "{changes:?}"
    );
}

#[test]
fn a_clip_too_slow_for_the_fine_grid_drops_to_its_files_rate() {
    use super::engine::too_slow_for_grid;
    // A 60 fps file on a 120 frame grid has 8.3 ms a frame.
    let fine = timeline(120, 60.0, 2);
    assert!(!too_slow_for_grid(ms(2), fine));
    assert!(too_slow_for_grid(ms(5), fine));
    // Already at its own rate: nothing coarser to drop to.
    assert!(!too_slow_for_grid(ms(50), timeline(60, 60.0, 1)));
}

/// Feeds `draws` (ms) to a fresh cost for a 60 fps file on a 120 grid;
/// the draw that first reads as too slow, if one does.
fn first_coarse(draws: &[f64]) -> Option<usize> {
    let fine = timeline(120, 60.0, 2);
    let mut cost = super::engine::DrawCost::default();
    draws
        .iter()
        .position(|draw| cost.note(Duration::from_secs_f64(draw / 1000.0), fine))
}

#[test]
fn a_cold_first_draw_keeps_a_fast_clip_on_the_fine_grid() {
    let mut draws = vec![6.0];
    draws.extend([0.8; 200]);
    assert_eq!(first_coarse(&draws), None);
}

#[test]
fn one_slow_draw_keeps_a_fast_clip_on_the_fine_grid() {
    for at in [1, 2, 5, 9, 40] {
        let mut draws = vec![0.8; 200];
        if let Some(draw) = draws.get_mut(at) {
            *draw = 14.0;
        }
        assert_eq!(first_coarse(&draws), None, "spike at draw {at}");
    }
}

const TAP_EFFECT_DRAWS_MS: [f64; 8] = [2.6, 6.0, 3.1, 2.6, 4.4, 2.8, 6.0, 2.6];

#[test]
fn a_clip_slow_from_the_start_drops_to_its_files_rate_within_16_draws() {
    let draws: Vec<f64> = TAP_EFFECT_DRAWS_MS
        .iter()
        .copied()
        .cycle()
        .take(40)
        .collect();
    let at = first_coarse(&draws);
    assert!(at.is_some_and(|at| at < 16), "{at:?}");
}

#[test]
fn a_low_rate_shows_only_the_files_own_frames() {
    // A 60 fps file on a 120 frame grid (step 2).
    let tl = timeline(120, 60.0, 2);
    let high = pick(ms(25), PlayMode::Loop, tl, true, 60.0).unwrap();
    let low = pick(ms(25), PlayMode::Loop, tl, false, 60.0).unwrap();
    assert_eq!(high.index, 3, "120 fps: an in-between frame");
    assert_eq!(low.index, 2, "60 fps: whole file frames only");
    assert_eq!(low.ahead, [Some(4), Some(6), Some(8)]);
    assert_eq!(tl.tick_fps(true), 120.0);
    assert_eq!(tl.tick_fps(false), 60.0);
    assert_eq!(tl.duration(), Duration::from_secs(1));
}

#[test]
fn each_head_plays_on_its_own_and_restarts() {
    let tl = timeline(10, 10.0, 1);
    let mode = PlayMode::Once(Rest::First);
    let t0 = Instant::now();
    let mut a = PlayHead::default();
    let mut b = PlayHead::default();
    // Until its first frame shows, a head waits on frame 0 and keeps ticking.
    let waiting = a.advance(t0, mode, tl, false, 60.0).unwrap();
    assert_eq!((waiting.index, waiting.animating), (0, true));
    a.start(t0);
    // B comes on screen later than A.
    let step = |head: &mut PlayHead, from: Instant, until: u64| {
        let mut last = None;
        for t in (0..=until).step_by(50) {
            last = head.advance(from + ms(t), mode, tl, false, 60.0);
        }
        last.unwrap()
    };
    assert_eq!(step(&mut a, t0, 450).index, 4);
    b.start(t0 + ms(450));
    assert_eq!(step(&mut b, t0 + ms(450), 100).index, 1);
    // A plays out and rests; B goes on.
    let done = step(&mut a, t0 + ms(450), 700);
    assert_eq!((done.index, done.animating), (0, false));
    assert!(step(&mut b, t0 + ms(550), 300).animating);
    // A tap plays A again from the start.
    a.restart();
    assert!(!a.started());
    a.start(t0 + ms(2000));
    assert_eq!(step(&mut a, t0 + ms(2000), 300).index, 3);
}

#[test]
fn a_head_not_drawn_for_a_while_resumes_where_it_was() {
    let tl = timeline(10, 10.0, 1);
    let t0 = Instant::now();
    let mut head = PlayHead::default();
    head.start(t0);
    head.advance(t0 + ms(150), PlayMode::Loop, tl, false, 60.0);
    let index = head
        .advance(t0 + ms(300), PlayMode::Loop, tl, false, 60.0)
        .unwrap();
    assert_eq!(index.index, 3);
    // Scrolled away for seconds: the clock did not run meanwhile.
    let back = t0 + ms(300) + PAUSE_GAP + ms(5000);
    assert_eq!(
        head.advance(back, PlayMode::Loop, tl, false, 60.0)
            .unwrap()
            .index,
        3
    );
    assert_eq!(
        head.advance(back + ms(100), PlayMode::Loop, tl, false, 60.0)
            .unwrap()
            .index,
        4
    );
}

#[test]
fn rate_steps_fill_the_display_up_to_120_fps() {
    assert_eq!(rate_step(60.0, 60.0), 1, "60 Hz: the file's own rate");
    assert_eq!(rate_step(30.0, 60.0), 1);
    assert_eq!(rate_step(60.0, 120.0), 2);
    assert_eq!(rate_step(30.0, 120.0), 4);
    assert_eq!(rate_step(25.0, 120.0), 4, "never past 120 fps");
    assert_eq!(rate_step(60.0, 144.0), 2);
    assert_eq!(rate_step(60.0, f64::NAN), 1);
    // Clips are keyed by display class: every 60 Hz display shares one.
    assert_eq!(grid_hz(59.94), 60);
    assert_eq!(grid_hz(f64::NAN), 60);
    assert_eq!(grid_hz(119.88), 120);
    assert_eq!(grid_hz(240.0), 120);
}

#[test]
fn edges_round_up_to_a_multiple_of_eight() {
    assert_eq!(edge_for(448.0), 448);
    assert_eq!(edge_for(447.2), 448);
    assert_eq!(edge_for(449.0), 456);
    assert_eq!(edge_for(3.0), MIN_EDGE);
    assert_eq!(edge_for(f32::NAN), MIN_EDGE);
    assert_eq!(edge_for(5000.0), MAX_EDGE);
}

/// A 64 px frame, transparent but for a band of pixels that differs with
/// `seed`.
fn sparse_frame(seed: usize) -> Vec<u8> {
    let mut frame = vec![0u8; 64 * 64 * 4];
    for (i, byte) in frame.iter_mut().enumerate().skip(4096).take(2048) {
        *byte = ((i * 7 + seed * 13) % 251) as u8;
    }
    frame
}

#[test]
fn stored_frames_unpack_to_the_same_pixels() {
    let mut scratch = Scratch::default();
    let frame = sparse_frame(0);
    let mut store = Store::default();
    store.insert(3, store::encode(&frame, &mut scratch).unwrap());
    assert!(
        store.bytes() < frame.len() / 2,
        "transparent margins pack small"
    );
    let stored = store.get(3).unwrap();
    let mut out = vec![1; frame.len()];
    assert!(store::decode(&stored, &mut out));
    assert_eq!(out, frame);
    let mut short = vec![0; 10];
    assert!(!store::decode(&stored, &mut short));
    assert!(store.get(4).is_none());
}

/// A packed frame of a few KB freed on its own stays in malloc's small
/// regions on macOS, charged to Quill after its clip closed; a clip's
/// frames packed into one large block go back to the system with it.
#[test]
fn a_clips_frames_share_one_large_block() {
    let mut scratch = Scratch::default();
    let frames: Vec<Vec<u8>> = (0..60).map(sparse_frame).collect();
    let mut store = Store::default();
    for (index, frame) in (0u32..).zip(&frames) {
        store.insert(index, store::encode(frame, &mut scratch).unwrap());
    }
    let blocks = store.blocks();
    assert_eq!(blocks.len(), 1, "one block for the clip: {blocks:?}");
    let (len, capacity) = blocks[0];
    assert_eq!(len, store.bytes());
    assert!(
        capacity >= 64 * 1024,
        "a large allocation (64 KiB and up): {capacity}"
    );
    for (index, frame) in (0u32..).zip(&frames) {
        let mut out = vec![0; frame.len()];
        assert!(store::decode(&store.get(index).unwrap(), &mut out));
        assert_eq!(&out, frame, "frame {index}");
    }
}

/// Bytes that do not pack (xorshift).
fn noise(len: usize) -> Vec<u8> {
    let mut x: u64 = 0x9E37_79B9_7F4A_7C15;
    (0..len)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            (x >> 24) as u8
        })
        .collect()
}

#[test]
fn frames_go_to_a_new_block_when_one_is_full_or_being_unpacked() {
    let mut scratch = Scratch::default();
    let mut store = Store::default();
    // About 100 KB packed: two fit a block.
    let frame = noise(100 * 1024);
    for index in 0..5 {
        store.insert(index, store::encode(&frame, &mut scratch).unwrap());
    }
    let counts = |store: &Store| store.blocks().len();
    assert_eq!(counts(&store), 3, "{:?}", store.blocks());
    // A worker unpacking from the last block holds it.
    let held = store.get(4).unwrap();
    store.insert(5, store::encode(&sparse_frame(1), &mut scratch).unwrap());
    assert_eq!(counts(&store), 4);
    let mut out = vec![0; frame.len()];
    assert!(store::decode(&held, &mut out));
    assert_eq!(out, frame);
    // A frame larger than a block gets one of its own size.
    let big = noise(300 * 1024);
    store.insert(6, store::encode(&big, &mut scratch).unwrap());
    assert_eq!(counts(&store), 5);
    let mut out = vec![0; big.len()];
    assert!(store::decode(&store.get(6).unwrap(), &mut out));
    assert_eq!(out, big);
    assert!(store.blocks().iter().all(|(len, capacity)| len <= capacity));
    assert_eq!(
        store.blocks().iter().map(|(len, _)| len).sum::<usize>(),
        store.bytes()
    );
}

#[test]
fn mirroring_flips_each_row() {
    // Two rows of three pixels; pixel i has bytes [i, i, i, i].
    let mut bytes: Vec<u8> = (0..6u8).flat_map(|i| [i; 4]).collect();
    mirror_rows(&mut bytes, 3);
    let pixels: Vec<u8> = bytes.chunks(4).map(|p| p[0]).collect();
    assert_eq!(pixels, vec![2, 1, 0, 5, 4, 3]);
}

fn key(file: i32, edge: u32) -> ClipKey {
    ClipKey {
        file,
        edge,
        fitz: FitzType::None,
        tint: None,
        mirror: false,
        hz: 60,
    }
}

/// Frames of `edge` px, frame n filled with byte n.
fn synthetic(edge: u32, count: u8) -> Origin {
    let len = (edge * edge * 4) as usize;
    Origin::Decoded(Box::new(move |_, sink| {
        DecodedFrames {
            frames: (0..count).map(|n| vec![n; len]).collect(),
            fps: 30.0,
        }
        .feed(sink)
    }))
}

/// Asks for `want` of `key` until it is drawn, or a round draws nothing.
fn frames_of(engine: &Engine, key: ClipKey, want: &[u32]) -> Vec<Rendered> {
    let mut got: Vec<Rendered> = Vec::new();
    let mut asked = false;
    loop {
        engine.wait_idle();
        if let Some(ClipState::Failed(error)) = engine.state(&key) {
            panic!("{error}");
        }
        let before = got.len();
        for frame in engine.take_rendered() {
            assert!(
                frame.key != key || !got.iter().any(|r| r.index == frame.index),
                "frame {} drawn twice",
                frame.index
            );
            if frame.key == key {
                got.push(frame);
            }
        }
        let missing: Vec<u32> = want
            .iter()
            .copied()
            .filter(|i| !got.iter().any(|r| r.index == *i))
            .collect();
        if missing.is_empty() || (asked && got.len() == before) {
            break;
        }
        engine.want(&key, &missing);
        asked = true;
    }
    // A clip draws its first frame on open, asked for or not.
    got.retain(|r| want.contains(&r.index));
    got.sort_by_key(|r| r.index);
    got
}

/// Frames asked of a clip that failed are never drawn.
#[test]
fn the_workers_go_idle_with_frames_asked_of_a_failed_clip() {
    let engine = Engine::new(1, 1 << 20);
    let key = key(1, 16);
    engine.open(key, Origin::Decoded(Box::new(|_, _| Err("no".to_string()))));
    engine.wait_idle();
    engine.want(&key, &[0, 1]);
    engine.wait_idle();
    assert_eq!(
        engine.state(&key),
        Some(ClipState::Failed("no".to_string()))
    );
}

/// A worker that panics leaves its clip busy for good.
#[test]
fn waiting_for_the_workers_ends_when_one_panics() {
    let engine = Engine::new(1, 1 << 20);
    let key = key(1, 16);
    engine.open(key, Origin::Decoded(Box::new(|_, _| panic!("decoder"))));
    engine.wait_idle();
    assert_eq!(engine.state(&key), Some(ClipState::Opening));
}

#[test]
fn decoded_clips_play_from_the_store_tinted_and_mirrored() {
    let engine = Engine::new(2, 64 << 20);
    let mut key = key(1, 16);
    key.tint = Some(Tint([0, 184, 255]));
    engine.open(key, synthetic(16, 40));
    let frames = frames_of(&engine, key, &[0, 1, 17, 39, 5]);
    assert_eq!(frames.len(), 5);
    let info = engine.info(&key).unwrap();
    assert_eq!(info.timeline, timeline(40, 30.0, 1), "own rate");
    let f17 = frames.iter().find(|r| r.index == 17).unwrap();
    let mut expected = vec![17u8; 16 * 16 * 4];
    Tint([0, 184, 255]).apply(&mut expected);
    assert_eq!(f17.pixels, expected);
}

/// A gzipped copy of a Lottie fixture (TGS).
fn tgs_fixture(json: &Path, name: &str) -> std::path::PathBuf {
    let out = std::env::temp_dir().join(format!("quill-player-{}-{name}.tgs", std::process::id()));
    let mut gzip = flate2::write::GzEncoder::new(
        std::fs::File::create(&out).unwrap(),
        flate2::Compression::fast(),
    );
    gzip.write_all(&std::fs::read(json).unwrap()).unwrap();
    gzip.finish().unwrap();
    out
}

fn fixtures() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/stickers")
}

#[test]
fn skin_tones_are_separate_clips_with_their_own_colors() {
    let path = tgs_fixture(&fixtures().join("fitz-face.json"), "fitz");
    let engine = Engine::new(2, 64 << 20);
    let plain = key(7, 64);
    let toned = ClipKey {
        fitz: FitzType::Type5,
        ..plain
    };
    engine.open(plain, Origin::Lottie(path.clone()));
    engine.open(toned, Origin::Lottie(path.clone()));
    // Opening the same key again is a no-op.
    engine.open(plain, Origin::Lottie(path.clone()));
    assert_eq!(engine.stats().clips, 2);
    let a = frames_of(&engine, plain, &[0]);
    let b = frames_of(&engine, toned, &[0]);
    assert_ne!(a[0].pixels, b[0].pixels, "the tone recolours the face");
    let _ = std::fs::remove_file(path);
}

#[test]
fn a_high_rate_grid_draws_in_between_frames() {
    let path = tgs_fixture(&fixtures().join("trim-paths.json"), "grid");
    let engine = Engine::new(1, 64 << 20);
    let low = key(8, 32);
    let high = ClipKey {
        hz: grid_hz(120.0),
        ..key(9, 32)
    };
    engine.open(low, Origin::Lottie(path.clone()));
    engine.open(high, Origin::Lottie(path.clone()));
    let a = frames_of(&engine, low, &[1]);
    let b = frames_of(&engine, high, &[1, 2]);
    let (low_info, high_info) = (engine.info(&low).unwrap(), engine.info(&high).unwrap());
    assert_eq!(high_info.timeline.step * low_info.timeline.step, 2);
    assert_eq!(
        high_info.timeline.frames,
        low_info.timeline.frames * high_info.timeline.step
    );
    // Grid frame 2 at step 2 is file frame 1.
    assert_eq!(a[0].pixels, b[1].pixels);
    assert_ne!(b[0].pixels, b[1].pixels, "frame 1.5 differs from frame 1");
    let _ = std::fs::remove_file(path);
}

#[test]
fn stores_are_evicted_least_recently_used_under_the_budget() {
    let path = tgs_fixture(&fixtures().join("many-paths.json"), "evict");
    // Room for the canvases of four 64 px clips (a frame each) and about
    // as much again in stored frames.
    let budget = 4 * 2 * 64 * 64 * 4 + 24 * 1024;
    let engine = Engine::new(2, budget);
    let keys: Vec<ClipKey> = (0..4).map(|n| key(100 + n, 64)).collect();
    for key in &keys {
        engine.open(*key, Origin::Lottie(path.clone()));
    }
    for (n, key) in keys.iter().enumerate() {
        let frames: Vec<u32> = (0..12).collect();
        assert_eq!(frames_of(&engine, *key, &frames).len(), 12);
        assert!(
            engine.held_bytes() <= budget,
            "clip {n}: {} > {budget}",
            engine.held_bytes()
        );
        // The next clip comes after this one left the screen.
        engine.left_screen(key);
    }
    let stats = engine.stats();
    assert!(stats.evicted > 0, "older stores made room");
    assert!(stats.store_bytes > 0, "the newest clip keeps its frames");
    // Replaying the newest clip unpacks from its store.
    let last = *keys.last().unwrap();
    let before = engine.stats().unpacked;
    frames_of(&engine, last, &[0]);
    frames_of(&engine, last, &[1, 2]);
    assert!(engine.stats().unpacked > before);
    let _ = std::fs::remove_file(path);
}

#[test]
fn a_closed_clip_is_gone_with_its_frames() {
    let engine = Engine::new(1, 64 << 20);
    let key = key(3, 16);
    engine.open(key, synthetic(16, 4));
    frames_of(&engine, key, &[0]);
    assert!(engine.stats().store_bytes > 0);
    engine.close(&key);
    assert!(engine.state(&key).is_none());
    assert_eq!(engine.stats().store_bytes, 0);
}

/// Packing and drawing costs on real stickers, for the decision doc:
/// `QUILL_STICKER_BENCH_DIR=<dir of .tgs> cargo test --release --lib
/// --no-default-features sticker_store_bench -- --ignored --nocapture`.
#[test]
#[ignore = "benchmark; needs a directory of TGS files"]
fn sticker_store_bench() {
    let Some(dir) = std::env::var_os("QUILL_STICKER_BENCH_DIR") else {
        panic!("set QUILL_STICKER_BENCH_DIR");
    };
    let mut files: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e == "tgs"))
        .collect();
    files.sort();
    for (edge, hz) in [(224u32, 60), (448, 60), (448, 120), (112, 60)] {
        let mut draw = Vec::new();
        let mut unpack = Vec::new();
        let mut pack = Vec::new();
        let (mut raw, mut stored, mut frames) = (0usize, 0usize, 0usize);
        let mut scratch = Scratch::default();
        for file in &files {
            let Ok(mut source) = Source::lottie(file, ClipKey { hz, ..key(1, edge) }) else {
                continue;
            };
            for index in 0..source.timeline.frames {
                let t = Instant::now();
                let Some(pixels) = source.draw(index) else {
                    break;
                };
                draw.push(t.elapsed());
                let t = Instant::now();
                let mut kept = Store::default();
                kept.insert(0, store::encode(&pixels, &mut scratch).unwrap());
                pack.push(t.elapsed());
                let packed = kept.get(0).unwrap();
                let mut out = vec![0; pixels.len()];
                let t = Instant::now();
                assert!(store::decode(&packed, &mut out));
                unpack.push(t.elapsed());
                assert_eq!(out, pixels);
                raw += pixels.len();
                stored += kept.bytes();
                frames += 1;
            }
        }
        let median = |v: &mut Vec<Duration>| {
            v.sort();
            v.get(v.len() / 2).map_or(0.0, |d| d.as_secs_f64() * 1000.0)
        };
        let p90 = |v: &Vec<Duration>| {
            v.get(v.len() * 9 / 10)
                .map_or(0.0, |d| d.as_secs_f64() * 1000.0)
        };
        let (md, mu, mp) = (median(&mut draw), median(&mut unpack), median(&mut pack));
        println!(
            "edge {edge} @{hz}Hz: {} files, {frames} frames; draw ms median {md:.3} p90 {:.3}; \
             pack {mp:.3}; unpack median {mu:.3} p90 {:.3}; raw {:.1} MB, stored {:.1} MB ({:.1}%), \
             per frame {:.1} KB",
            files.len(),
            p90(&draw),
            p90(&unpack),
            raw as f64 / 1e6,
            stored as f64 / 1e6,
            stored as f64 * 100.0 / raw.max(1) as f64,
            stored as f64 / frames.max(1) as f64 / 1024.0,
        );
    }
}

#[test]
fn clips_decoding_at_once_do_not_wait_for_each_other() {
    use std::sync::{Barrier, mpsc};
    let engine = Engine::new(2, 64 << 20);
    let (woke, wakes) = mpsc::channel::<()>();
    engine.set_waker(Arc::new(move || {
        let _ = woke.send(());
    }));
    let both_decoding = Arc::new(Barrier::new(2));
    let len = 16 * 16 * 4;
    let keys = [key(1, 16), key(2, 16)];
    for key in keys {
        let both_decoding = both_decoding.clone();
        engine.open(
            key,
            Origin::Decoded(Box::new(move |_, sink| {
                both_decoding.wait();
                DecodedFrames {
                    frames: vec![vec![7; len]; 3],
                    fps: 30.0,
                }
                .feed(sink)
            })),
        );
    }
    while keys.iter().any(|key| engine.info(key).is_none()) {
        wakes.recv().unwrap();
    }
}

#[test]
fn a_clip_decoding_holds_back_no_frames_of_other_clips() {
    use std::sync::mpsc;
    let engine = Engine::new(1, 64 << 20);
    let (woke, wakes) = mpsc::channel::<()>();
    engine.set_waker(Arc::new(move || {
        let _ = woke.send(());
    }));
    let a = key(1, 16);
    engine.open(a, synthetic(16, 10));
    while !engine.take_rendered().iter().any(|r| r.key == a) {
        wakes.recv().unwrap();
    }
    let (started, decoding) = mpsc::channel::<Option<String>>();
    let (release, released) = mpsc::channel::<()>();
    let b = key(2, 16);
    let len = 16 * 16 * 4;
    engine.open(
        b,
        Origin::Decoded(Box::new(move |_, sink| {
            let _ = started.send(std::thread::current().name().map(str::to_owned));
            let _ = released.recv();
            DecodedFrames {
                frames: vec![vec![7; len]; 3],
                fps: 30.0,
            }
            .feed(sink)
        })),
    );
    let thread = decoding.recv().unwrap();
    assert_eq!(
        thread.as_deref(),
        Some("quill-sticker-open-0"),
        "B decodes off the worker"
    );
    engine.want(&a, &[1]);
    while !engine
        .take_rendered()
        .iter()
        .any(|r| r.key == a && r.index == 1)
    {
        std::thread::yield_now();
    }
    assert_eq!(engine.state(&b), Some(ClipState::Opening));
    release.send(()).unwrap();
    while engine.info(&b).is_none() {
        wakes.recv().unwrap();
    }
    engine.want(&b, &[2]);
    while !engine
        .take_rendered()
        .iter()
        .any(|r| r.key == b && r.index == 2)
    {
        std::thread::yield_now();
    }
}

#[test]
fn a_held_frame_takes_no_more_of_the_store() {
    let key = key(1, 16);
    let len = 16 * 16 * 4;
    let mut scratch = Scratch::default();
    let mut packer = Packer::new(key, &mut scratch);
    packer.repeat(5).unwrap();
    assert_eq!(
        packer.timeline(30.0),
        Err("Animated sticker has no frames".into())
    );
    packer.frame(&mut vec![1; len]).unwrap();
    let one = packer.store.bytes();
    packer.repeat(2).unwrap();
    assert_eq!(packer.store.bytes(), one);
    packer.frame(&mut vec![2; len]).unwrap();
    let timeline = packer.timeline(30.0).unwrap();
    assert_eq!(timeline.frames, 4);
    let store = packer.store;
    let mut out = vec![0; len];
    for (index, byte) in [1u8, 1, 1, 2].into_iter().enumerate() {
        let stored = store.get(u32::try_from(index).unwrap()).unwrap();
        assert!(store::decode(&stored, &mut out));
        assert_eq!(out, vec![byte; len], "frame {index}");
    }
    assert!(store.get(4).is_none());
}

#[test]
fn a_clip_failing_partway_keeps_none_of_its_frames() {
    use std::sync::mpsc;
    let engine = Engine::new(1, 64 << 20);
    let (woke, wakes) = mpsc::channel::<()>();
    engine.set_waker(Arc::new(move || {
        let _ = woke.send(());
    }));
    let key = key(1, 16);
    let len = 16 * 16 * 4;
    engine.open(
        key,
        Origin::Decoded(Box::new(move |_, sink| {
            sink.frame(&mut vec![1; len])?;
            sink.frame(&mut vec![2; len - 4])?;
            Ok(30.0)
        })),
    );
    while !matches!(engine.state(&key), Some(ClipState::Failed(_))) {
        wakes.recv().unwrap();
    }
    assert_eq!(engine.info(&key), None);
    assert_eq!(engine.held_bytes(), 0);
}

#[test]
fn a_clip_reopened_while_its_old_decode_runs_keeps_its_new_frames() {
    use std::sync::mpsc;
    let engine = Engine::new(2, 64 << 20);
    let (woke, wakes) = mpsc::channel::<Option<String>>();
    engine.set_waker(Arc::new(move || {
        let _ = woke.send(std::thread::current().name().map(str::to_owned));
    }));
    let len = 16 * 16 * 4;
    let key = key(1, 16);
    let (started, decoding) = mpsc::channel::<Option<String>>();
    let (release, released) = mpsc::channel::<()>();
    engine.open(
        key,
        Origin::Decoded(Box::new(
            move |cancel: &Cancel, sink: &mut dyn FrameSink| {
                let _ = started.send(std::thread::current().name().map(str::to_owned));
                let _ = released.recv();
                if cancel.flag.load(std::sync::atomic::Ordering::SeqCst) {
                    return Err("Sticker playback cancelled".to_string());
                }
                DecodedFrames {
                    frames: vec![vec![1; len]; 3],
                    fps: 30.0,
                }
                .feed(sink)
            },
        )),
    );
    let old_opener = decoding.recv().unwrap();
    // Scrolled away and back while it decodes: the same key opens again,
    // and the other opener finishes it.
    engine.close(&key);
    engine.open(key, synthetic(16, 3));
    while engine.info(&key).is_none() {
        assert!(!matches!(engine.state(&key), Some(ClipState::Failed(_))));
        wakes.recv().unwrap();
    }
    release.send(()).unwrap();
    while wakes.recv().unwrap() != old_opener {}
    assert!(
        matches!(engine.state(&key), Some(ClipState::Ready(_))),
        "{:?}",
        engine.state(&key)
    );
}

#[test]
fn video_stickers_open_when_no_opener_thread_starts() {
    // The worker thread "starts" without running: the test does its jobs.
    let spawn = |name: String, _body: Box<dyn FnOnce() + Send>| {
        if name.starts_with("quill-sticker-open") {
            return Err(std::io::Error::other("no more threads"));
        }
        Ok(())
    };
    let engine = engine::Engine::start(1, 64 << 20, &spawn);
    let key = key(1, 16);
    engine.open(key, synthetic(16, 3));
    let job = crate::locks::lock_or_recover(&engine.shared().state).next_job(0);
    let Some(job @ engine::Job::Decode { key: decoding, .. }) = job else {
        panic!("no thread takes the clip to decode it");
    };
    assert_eq!(decoding, key);
    worker::work(
        engine.shared(),
        &mut std::collections::HashMap::new(),
        &mut Scratch::default(),
        job,
    );
    assert!(engine.info(&key).is_some(), "{:?}", engine.state(&key));
}
