//! GPUI tests of a live player drawn by `video_frame`: a paused sender
//! shows its blurred last frame, a stalled feed reads Buffering and keeps
//! its picture, and a tile painted out of sight stops redrawing and
//! stops asking the frame clock.
use super::super::frame_clock::take_video_targets;
use super::{PlayState, Purpose, STALL_AFTER, VideoPlayer, video_frame};
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    App, Context, Entity, IntoElement, ParentElement, Render, Styled, TestAppContext,
    VisualTestContext, Window, div, px,
};
use quill::calls::live_video::convert::gradient_frame;
use quill::calls::live_video::{LiveFrame, LiveRotation, LiveSink, LiveSource, LiveState};
use std::time::{Duration, Instant};

/// One tile, 160 × 90, at `top` in a window that clips.
struct TileView {
    player: VideoPlayer,
    top: f32,
}

impl Render for TileView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().relative().overflow_hidden().child(
            div()
                .absolute()
                .left(px(0.))
                .top(px(self.top))
                .w(px(160.))
                .h(px(90.))
                .child(video_frame(&self.player).paused_overlay(true)),
        )
    }
}

fn push(source: &LiveSource, seq: u64) {
    let frame = LiveFrame::new(
        seq,
        160,
        90,
        LiveRotation::None,
        gradient_frame(160, 90, seq as u32),
        Instant::now(),
    );
    source.push(frame.expect("frame"));
}

fn draw(vcx: &mut VisualTestContext) {
    vcx.run_until_parked();
    vcx.update(|window, cx| window.render_frame(cx));
}

fn tile(
    cx: &mut TestAppContext,
    top: f32,
) -> (
    Entity<TileView>,
    &mut VisualTestContext,
    VideoPlayer,
    LiveSink,
    LiveSource,
) {
    cx.update(gpui_kit::init);
    let (sink, source) = LiveSink::new();
    let made = sink.clone();
    let player =
        cx.update(|cx: &mut App| VideoPlayer::live(made, Purpose::Live { tile: true }, cx));
    let shown = player.clone();
    let (view, vcx) = cx.add_window_view(move |_, _| TileView { player: shown, top });
    draw(vcx);
    (view, vcx, player, sink, source)
}

#[gpui_kit::test]
fn a_paused_sender_shows_its_last_frame_blurred(cx: &mut TestAppContext) {
    let (_view, vcx, player, sink, source) = tile(cx, 0.);
    push(&source, 1);
    draw(vcx);
    assert_eq!(player.state(), PlayState::Playing);
    assert!(vcx.debug_bounds("live-paused-picture").is_none());
    sink.set_state(LiveState::Paused);
    draw(vcx);
    assert_eq!(player.state(), PlayState::Paused);
    assert!(
        vcx.debug_bounds("live-paused-picture").is_some(),
        "the blurred picture covers the tile"
    );
    sink.set_state(LiveState::Active);
    draw(vcx);
    assert!(vcx.debug_bounds("live-paused-picture").is_none());
}

#[gpui_kit::test]
fn a_stalled_feed_reads_buffering_and_keeps_its_picture(cx: &mut TestAppContext) {
    let (view, vcx, player, _sink, source) = tile(cx, 0.);
    push(&source, 1);
    draw(vcx);
    assert_eq!(player.state(), PlayState::Playing);
    let size = player.picture_size();
    assert!(size.is_some(), "the frame is drawn");
    std::thread::sleep(STALL_AFTER + Duration::from_millis(100));
    vcx.update(|_, cx| view.update(cx, |_, cx| cx.notify()));
    draw(vcx);
    assert_eq!(player.state(), PlayState::Buffering);
    assert_eq!(player.picture_size(), size, "the last picture stays");
    // A frame again: playing.
    push(&source, 2);
    draw(vcx);
    assert_eq!(player.state(), PlayState::Playing);
}

/// Frames 120 ms apart (beyond the 100 ms grace) to the tile, drawn each
/// time; how many redrew it and whether it asked the frame clock.
fn redraws(cx: &mut TestAppContext, top: f32) -> (u64, bool) {
    let (view, vcx, player, _sink, source) = tile(cx, top);
    let _ = take_video_targets();
    for seq in 1..=6 {
        push(&source, seq);
        draw(vcx);
        std::thread::sleep(Duration::from_millis(120));
    }
    let asked = take_video_targets().contains(&view.entity_id());
    (player.live_asks(), asked)
}

#[gpui_kit::test]
fn an_on_screen_tile_redraws_for_every_frame(cx: &mut TestAppContext) {
    let (asks, asked) = redraws(cx, 0.);
    assert_eq!(asks, 6);
    assert!(asked, "it asks the frame clock for the stall check");
}

#[gpui_kit::test]
fn an_off_screen_tile_stops_redrawing_and_asking(cx: &mut TestAppContext) {
    let (asks, asked) = redraws(cx, 5000.);
    // The first wake redraws; after two unseen wakes it waits for the
    // once-a-second look (0.7 s here).
    assert!(asks <= 1, "redrawn {asks} times");
    assert!(!asked, "it asks the frame clock nothing");
}
