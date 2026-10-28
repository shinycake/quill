//! The UI half of the player: views of one clip share its frames and keep
//! their own heads; a frame not drawn yet counts as late; a clip's size
//! follows the window's scale.
use super::{ClipSpec, DISPLAY, HeadId, Player, ViewAsk};
use quill::ids::FileId;
use quill::sticker_playback::FitzType;
use quill::sticker_playback::player::{DecodedFrames, PlayMode, Rest, edge_for};
use quill::telegram::envelope::StickerFormat;
use std::sync::Arc;
use std::time::{Duration, Instant};

const EDGE: u32 = 16;
/// 30 frames a second: one frame every 33.3 ms.
const FPS: f64 = 30.0;

/// A player whose file 7 has `count` frames, frame `i` filled with `i`.
fn player(count: u8) -> (Player, ClipSpec) {
    let player = Player::new();
    player.set_fixture(
        7,
        Arc::new(move |edge| DecodedFrames {
            frames: (0..count)
                .map(|i| vec![i; (edge as usize).pow(2) * 4])
                .collect(),
            fps: FPS,
        }),
    );
    let spec = ClipSpec {
        file: FileId(7),
        format: StickerFormat::Webm,
        path: std::path::PathBuf::new(),
        fitz: FitzType::None,
        tint: None,
        mirror: false,
    };
    // Opens it; its first frame is drawn without anyone asking.
    assert!(!player.prepare(&spec, EDGE));
    player.settle();
    assert!(player.prepare(&spec, EDGE));
    (player, spec)
}

/// The frame `head` shows at `at`, by its fill; the workers then draw
/// what it asked for.
fn shown(player: &Player, spec: &ClipSpec, head: HeadId, mode: PlayMode, at: Instant) -> u8 {
    let image = player
        .frame(spec, head, mode, EDGE, false, at)
        .expect("a frame");
    let first = image
        .as_bytes(0)
        .and_then(|bytes| bytes.first().copied())
        .expect("pixels");
    player.settle();
    first
}

fn ms(value: u64) -> Duration {
    Duration::from_millis(value)
}

#[test]
fn views_of_one_sticker_share_its_clip_and_keep_their_own_heads() {
    let (player, spec) = player(4);
    let (a, b) = (HeadId::of("a"), HeadId::of("b"));
    let t0 = Instant::now();
    assert_eq!(shown(&player, &spec, a, PlayMode::Loop, t0), 0);
    assert_eq!(shown(&player, &spec, a, PlayMode::Loop, t0 + ms(34)), 1);
    assert_eq!(shown(&player, &spec, a, PlayMode::Loop, t0 + ms(67)), 2);
    // Another view of the same sticker starts at its own first frame.
    assert_eq!(shown(&player, &spec, b, PlayMode::Loop, t0 + ms(67)), 0);
    assert_eq!(shown(&player, &spec, a, PlayMode::Loop, t0 + ms(101)), 3);
    assert_eq!(shown(&player, &spec, b, PlayMode::Loop, t0 + ms(101)), 1);
    // Looping wraps.
    assert_eq!(shown(&player, &spec, a, PlayMode::Loop, t0 + ms(134)), 0);
    assert_eq!(player.engine().stats().clips, 1, "one clip for both views");
}

#[test]
fn a_lone_emoji_plays_once_rests_on_its_first_frame_and_replays_on_tap() {
    let (player, spec) = player(3);
    let head = HeadId::of("emoji");
    let once = PlayMode::Once(Rest::First);
    let t0 = Instant::now();
    let mut seen = Vec::new();
    for step in 0..6 {
        seen.push(shown(&player, &spec, head, once, t0 + ms(34) * step));
    }
    assert_eq!(seen, [0, 1, 2, 0, 0, 0]);
    player.restart(head, 7);
    let t1 = t0 + ms(400);
    assert_eq!(shown(&player, &spec, head, once, t1), 0);
    assert_eq!(shown(&player, &spec, head, once, t1 + ms(34)), 1);
}

#[test]
fn a_frame_not_drawn_in_time_keeps_the_last_one_up_and_counts_as_late() {
    let (player, spec) = player(8);
    let head = HeadId::of("late");
    let t0 = Instant::now();
    assert_eq!(shown(&player, &spec, head, PlayMode::Loop, t0), 0);
    // Frame 5 was never asked for (the head jumped past the frames ahead).
    let image = player
        .frame(&spec, head, PlayMode::Loop, EDGE, false, t0 + ms(167))
        .and_then(|image| image.as_bytes(0).and_then(|b| b.first().copied()));
    assert!(
        image.is_some_and(|fill| fill < 5),
        "an earlier frame stays up"
    );
    assert_eq!(player.screen_stats().late, 1);
    player.settle();
    assert_eq!(shown(&player, &spec, head, PlayMode::Loop, t0 + ms(170)), 5);
}

#[test]
fn a_clip_is_drawn_at_its_box_times_the_scale_factor() {
    let ask = ViewAsk::emoji(
        FileId(7),
        StickerFormat::Tgs,
        HeadId::of("scale"),
        PlayMode::Loop,
        22.0,
    );
    let before = DISPLAY.with(std::cell::Cell::get);
    DISPLAY.with(|display| display.set((2.0, 60.0)));
    assert_eq!(ask.key().edge, edge_for(44.0));
    DISPLAY.with(|display| display.set((1.0, 60.0)));
    assert_eq!(ask.key().edge, edge_for(22.0));
    DISPLAY.with(|display| display.set(before));
}
