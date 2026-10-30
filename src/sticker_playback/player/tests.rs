use super::head::{PAUSE_GAP, pick};
use super::source::Source;
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
    let at = |t| pick(ms(t), PlayMode::Loop, tl, false).unwrap();
    assert_eq!(at(0).index, 0);
    assert_eq!(at(350).index, 3);
    assert_eq!(at(1250).index, 2, "second loop");
    assert!(at(1250).animating);
    assert_eq!(at(900).ahead, [Some(0), Some(1), Some(2)], "wraps ahead");
}

#[test]
fn a_one_shot_rests_on_the_frame_it_names() {
    let tl = timeline(10, 10.0, 1);
    let last = pick(ms(1500), PlayMode::Once(Rest::Last), tl, false).unwrap();
    assert_eq!((last.index, last.animating), (9, false));
    let first = pick(ms(1500), PlayMode::Once(Rest::First), tl, false).unwrap();
    assert_eq!((first.index, first.animating), (0, false));
    let playing = pick(ms(850), PlayMode::Once(Rest::First), tl, false).unwrap();
    assert_eq!((playing.index, playing.animating), (8, true));
    assert_eq!(
        playing.ahead,
        [Some(9), Some(0), None],
        "the frame it rests on, then nothing past the end"
    );
    let last = pick(ms(850), PlayMode::Once(Rest::Last), tl, false).unwrap();
    assert_eq!(last.ahead, [Some(9), None, None]);
}

#[test]
fn limited_loops_play_then_rest_on_the_first_frame() {
    let tl = timeline(4, 10.0, 1);
    let at = |t| pick(ms(t), PlayMode::Loops(2), tl, false).unwrap();
    assert_eq!((at(650).index, at(650).animating), (2, true));
    assert_eq!((at(800).index, at(800).animating), (0, false));
}

#[test]
fn a_fixed_position_shows_nothing_past_the_end() {
    let tl = timeline(4, 10.0, 1);
    assert_eq!(
        pick(ms(0), PlayMode::At(ms(250)), tl, false).map(|p| p.index),
        Some(2)
    );
    assert!(pick(ms(0), PlayMode::At(ms(400)), tl, false).is_none());
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
        shown.push(pick(pos, PlayMode::Loop, tl, false).unwrap().index);
    }
    for pair in shown.chunks(2) {
        assert_eq!(pair.len(), 2);
        assert_eq!(pair.first(), pair.last(), "{shown:?}");
    }
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

#[test]
fn a_low_rate_shows_only_the_files_own_frames() {
    // A 60 fps file on a 120 frame grid (step 2).
    let tl = timeline(120, 60.0, 2);
    let high = pick(ms(25), PlayMode::Loop, tl, true).unwrap();
    let low = pick(ms(25), PlayMode::Loop, tl, false).unwrap();
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
    let waiting = a.advance(t0, mode, tl, false).unwrap();
    assert_eq!((waiting.index, waiting.animating), (0, true));
    a.start(t0);
    // B comes on screen later than A.
    let step = |head: &mut PlayHead, from: Instant, until: u64| {
        let mut last = None;
        for t in (0..=until).step_by(50) {
            last = head.advance(from + ms(t), mode, tl, false);
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
    head.advance(t0 + ms(150), PlayMode::Loop, tl, false);
    let index = head
        .advance(t0 + ms(300), PlayMode::Loop, tl, false)
        .unwrap();
    assert_eq!(index.index, 3);
    // Scrolled away for seconds: the clock did not run meanwhile.
    let back = t0 + ms(300) + PAUSE_GAP + ms(5000);
    assert_eq!(
        head.advance(back, PlayMode::Loop, tl, false).unwrap().index,
        3
    );
    assert_eq!(
        head.advance(back + ms(100), PlayMode::Loop, tl, false)
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
    Origin::Decoded(Box::new(move |_| {
        Ok(DecodedFrames {
            frames: (0..count).map(|n| vec![n; len]).collect(),
            fps: 30.0,
        })
    }))
}

/// Waits for `want` of `key` to be drawn.
fn frames_of(engine: &Engine, key: ClipKey, want: &[u32]) -> Vec<Rendered> {
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut got: Vec<Rendered> = Vec::new();
    while Instant::now() < deadline {
        if engine.info(&key).is_some() {
            let missing: Vec<u32> = want
                .iter()
                .copied()
                .filter(|i| !got.iter().any(|r| r.index == *i))
                .collect();
            if missing.is_empty() {
                break;
            }
            engine.want(&key, &missing);
        }
        if let Some(ClipState::Failed(error)) = engine.state(&key) {
            panic!("{error}");
        }
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
        std::thread::sleep(ms(2));
    }
    // A clip draws its first frame on open, asked for or not.
    got.retain(|r| want.contains(&r.index));
    got.sort_by_key(|r| r.index);
    got
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
        std::thread::sleep(ms(520));
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
