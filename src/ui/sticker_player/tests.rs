//! The UI half of the player: views of one clip share its frames and keep
//! their own heads; a frame not drawn yet counts as late; every frame shown
//! leaves the atlas; a clip's size follows the window's scale.
use super::super::image_budget::{begin_frame, take_unshowable};
use super::{CLOSE_AFTER, ClipSpec, DISPLAY, HeadId, Player, ViewAsk};
use quill::ids::{ChatId, FileId};
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
        hz: 60,
    };
    // Opens it; its first frame is drawn without anyone asking.
    assert!(!player.prepare(&spec, EDGE, None));
    player.settle();
    assert!(player.prepare(&spec, EDGE, None));
    (player, spec)
}

/// The frame `head` shows at `at`, by its fill; the workers then draw
/// what it asked for.
fn shown(player: &Player, spec: &ClipSpec, head: HeadId, mode: PlayMode, at: Instant) -> u8 {
    let image = player
        .frame(spec, head, mode, EDGE, false, None, at)
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
        .frame(&spec, head, PlayMode::Loop, EDGE, false, None, t0 + ms(167))
        .and_then(|image| image.as_bytes(0).and_then(|b| b.first().copied()));
    assert!(
        image.is_some_and(|fill| fill < 5),
        "an earlier frame stays up"
    );
    assert_eq!(player.screen_stats().late, 1);
    player.settle();
    assert_eq!(shown(&player, &spec, head, PlayMode::Loop, t0 + ms(170)), 5);
}

/// A head that runs ahead of the drawn frames shows an earlier one. On a
/// 120 Hz Mac that was most frames, and each such frame was retired twice
/// and never left the atlas (20 MB a second for a chat of stickers).
#[test]
fn every_frame_a_late_head_showed_leaves_once_the_clip_stops() {
    let (player, spec) = player(8);
    let head = HeadId::of("late");
    let t0 = Instant::now();
    let mut showed = Vec::new();
    for step in 0..24 {
        let at = t0 + ms(167) * step;
        let image = player
            .frame(&spec, head, PlayMode::Loop, EDGE, false, None, at)
            .expect("a frame");
        showed.push((image.id, Arc::downgrade(&image)));
        player.settle();
    }
    assert!(player.screen_stats().late > 0, "the head ran ahead");
    player.stop();
    begin_frame();
    begin_frame();
    let left: Vec<_> = take_unshowable().iter().map(|image| image.id).collect();
    let kept = showed.iter().filter(|(id, _)| !left.contains(id));
    assert_eq!(kept.count(), 0, "shown frames that never left the atlas");
    let held = showed.iter().filter(|(_, frame)| frame.strong_count() > 0);
    assert_eq!(held.count(), 0, "frames still held after the clip stopped");
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

/// The emoji panel closed over a chat of text: no view draws its clips any
/// more, and clips used to close only while some view drew, so every clip
/// it opened stayed open with its frames and canvas for good.
#[test]
fn a_clip_no_view_draws_closes_with_nothing_drawing() {
    let (player, spec) = player(4);
    let t0 = Instant::now();
    begin_frame();
    let image = player
        .frame(
            &spec,
            HeadId::of("panel"),
            PlayMode::Loop,
            EDGE,
            false,
            None,
            t0,
        )
        .expect("a frame");
    let shown = Arc::downgrade(&image);
    drop(image);
    player.settle();
    assert!(player.engine().stats().store_bytes > 0);
    // The next frame no longer draws it.
    begin_frame();
    let closed = player.close_stale(t0 + ms(1000));
    assert_eq!((closed.clips, closed.next), (0, Some(ms(4000))), "too soon");
    let closed = player.close_stale(t0 + ms(5000));
    assert_eq!((closed.clips, closed.open), (1, false), "nothing left");
    let stats = player.engine().stats();
    assert_eq!((stats.clips, stats.store_bytes), (0, 0));
    assert!(
        closed
            .frames
            .iter()
            .any(|frame| shown.upgrade().is_some_and(|s| Arc::ptr_eq(frame, &s))),
        "its frame goes to the atlas sweep with the clip"
    );
    drop(closed);
    assert_eq!(shown.strong_count(), 0, "nothing else holds its frame");
}

/// A view at rest is not drawn again while nothing changes, but its frame
/// is on screen: closing its clip would show the still at the next draw.
/// Once a frame leaves it out, it closes [`CLOSE_AFTER`] later.
#[test]
fn a_clip_whose_frame_is_on_screen_stays_open() {
    let (player, spec) = player(4);
    let t0 = Instant::now();
    begin_frame();
    let rest = PlayMode::Once(Rest::First);
    let _ = player.frame(&spec, HeadId::of("rest"), rest, EDGE, false, None, t0);
    player.settle();
    let closed = player.close_stale(t0 + ms(60_000));
    assert_eq!((closed.clips, closed.next), (0, None), "on screen");
    // A frame without it.
    begin_frame();
    let closed = player.close_stale(t0 + ms(60_000));
    assert_eq!((closed.clips, closed.next), (0, Some(CLOSE_AFTER)));
    assert_eq!(player.close_stale(t0 + ms(65_000)).clips, 1);
}

/// A clip on screen for a minute with nothing drawn leaves it at the next
/// frame: its wait starts there, not at the closer's last look.
#[test]
fn a_clip_waits_its_full_time_from_the_frame_that_left_it_out() {
    let (player, spec) = player(4);
    let t0 = player.now();
    begin_frame();
    let _ = player.frame(
        &spec,
        HeadId::of("left"),
        PlayMode::Loop,
        EDGE,
        false,
        None,
        t0,
    );
    player.settle();
    assert!(player.close_stale(t0).open);
    let mut next_frame = player.next_frame();
    player.skip(ms(60_000));
    player.frame_starts(None);
    begin_frame();
    assert_eq!(next_frame.try_recv(), Ok(Some(())), "the closer wakes");
    let left = player.now();
    let closed = player.close_stale(left + ms(4_000));
    assert_eq!((closed.clips, closed.open), (0, true), "too soon");
    assert_eq!(player.close_stale(left + CLOSE_AFTER).clips, 1);
}

/// A frame paints its views one by one. Painting one never closes a clip
/// at rest that the frame has not painted yet, however long nothing drew:
/// closed, it would show nothing, then its still, then play again.
#[test]
fn painting_a_view_closes_no_clip_the_frame_has_not_painted_yet() {
    let (player, spec) = player(4);
    let rest = PlayMode::Once(Rest::First);
    let t0 = Instant::now();
    begin_frame();
    let _ = player.frame(&spec, HeadId::of("a"), rest, EDGE, false, None, t0);
    let _ = player.frame(&spec, HeadId::of("b"), rest, EDGE * 2, false, None, t0);
    player.settle();
    let _ = player.frame(&spec, HeadId::of("b"), rest, EDGE * 2, false, None, t0);
    let first = player.first_frames();
    assert_eq!(first.iter().flatten().count(), 2, "both show a frame");
    // Nothing drew for a minute; the next frame paints `a` first.
    begin_frame();
    let later = t0 + ms(60_000);
    let _ = player.frame(&spec, HeadId::of("a"), rest, EDGE, false, None, later);
    assert_eq!(player.first_frames(), first, "`b` is still open");
}

/// A chat window's clips are judged by its own frames: the main window may
/// stop drawing (minimized, covered) or draw on without them.
#[test]
fn each_window_judges_the_clips_it_drew_by_its_own_frames() {
    let (player, spec) = player(4);
    let (chat, rest) = (ChatId(11), PlayMode::Once(Rest::First));
    let t0 = Instant::now();
    player.frame_starts(Some(chat));
    let _ = player.frame(&spec, HeadId::of("chat"), rest, EDGE, false, Some(chat), t0);
    player.settle();
    // The main window draws on: the chat window still shows it.
    begin_frame();
    begin_frame();
    let closed = player.close_stale(t0 + ms(60_000));
    assert_eq!(
        (closed.clips, closed.next),
        (0, None),
        "on screen in its window"
    );
    // The main window stops; the chat window draws a frame without it.
    player.frame_starts(Some(chat));
    let closed = player.close_stale(t0 + ms(60_000));
    assert_eq!((closed.clips, closed.next), (0, Some(CLOSE_AFTER)));
    assert_eq!(player.close_stale(t0 + ms(65_000)).clips, 1);
    // Drawn in a window that then closed: nothing shows it.
    player.frame_starts(Some(chat));
    let _ = player.frame(&spec, HeadId::of("chat"), rest, EDGE, false, Some(chat), t0);
    player.window_closed(chat);
    let closed = player.close_stale(t0 + ms(1_000));
    assert_eq!((closed.clips, closed.next), (0, Some(ms(4_000))));
}

/// A window lets go of its chat (the main window opens another one): the
/// heads and clips only its views used go, and what views in another
/// window use stays as it is, frames and position. Stopping everything
/// instead closed a chat window's clips and started its stickers over
/// whenever the main window opened another chat.
#[test]
fn a_window_leaving_its_chat_keeps_what_another_window_uses() {
    let (player, spec) = player(4);
    let chat = ChatId(11);
    let once = PlayMode::Once(Rest::Last);
    let (main, own, both) = (HeadId::of("main"), HeadId::of("own"), HeadId::of("both"));
    let main_only = HeadId::of("main-only");
    let t0 = Instant::now();
    begin_frame();
    player.frame_starts(Some(chat));
    // One clip both windows draw, one only the main window draws, and one
    // the chat window is opening.
    let views = [
        (main, EDGE, None),
        (both, EDGE, None),
        (own, EDGE, Some(chat)),
        (both, EDGE, Some(chat)),
        (main_only, EDGE * 2, None),
    ];
    for _ in 0..2 {
        for (head, edge, window) in views {
            let _ = player.frame(&spec, head, once, edge, false, window, t0);
        }
        player.settle();
    }
    let _ = player.prepare(&spec, EDGE * 3, Some(chat));
    player.settle();
    let first = player.first_frames();
    assert_eq!(player.engine().stats().clips, 3);

    let frames = player.leave(None);
    assert!(!frames.is_empty(), "the main window's own clip closed");
    assert_eq!(player.engine().stats().clips, 2, "the chat window's stay");
    let kept = player.first_frames();
    assert!(
        kept.iter().all(|at| first.contains(at)),
        "none opened again: {first:?} {kept:?}"
    );
    assert_eq!(player.head_known(main, 7), None, "it plays again");
    assert_eq!(player.head_known(main_only, 7), None);
    assert_eq!(player.head_known(own, 7), Some(true), "where it was");
    assert_eq!(
        player.head_known(both, 7),
        Some(true),
        "the window draws it"
    );

    // The chat window closes too: nothing is left.
    let frames = player.leave(Some(chat));
    assert!(!frames.is_empty());
    assert_eq!(player.engine().stats().clips, 0);
    assert_eq!(player.head_known(both, 7), None);
}
