//! GPUI tests of a live player drawn by `video_frame`: a paused sender
//! shows its blurred last frame, a stalled feed reads Buffering and keeps
//! its picture, a tile painted out of sight stops redrawing, converting
//! and asking the frame clock (and takes the waiting frame once back in
//! sight), a size change mid-stream shows at the new size, and a refused
//! NV12 frame (Windows, Linux) is replaced by its image. Time is
//! the player's injected clock: nothing sleeps.
use super::super::frame_clock::take_video_targets;
use super::backend_ffmpeg::NV12_RETRY;
use super::{PlayState, Purpose, STALL_AFTER, VideoPlayer, take_made_images, video_frame};
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    App, Context, Entity, IntoElement, ParentElement, Render, Styled, TestAppContext,
    VisualTestContext, Window, div, px,
};
use quill::calls::live_video::convert::gradient_frame;
use quill::calls::live_video::{LiveEnd, LiveFrame, LiveRotation, LiveSink, LiveSource, LiveState};
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

fn push_sized(source: &LiveSource, seq: u64, width: u16, height: u16, at: Instant) {
    let frame = LiveFrame::new(
        seq,
        width,
        height,
        LiveRotation::None,
        gradient_frame(width, height, seq as u32),
        at,
    );
    source.push(frame.expect("frame"));
}

fn push(source: &LiveSource, seq: u64, at: Instant) {
    push_sized(source, seq, 160, 90, at);
}

fn draw(vcx: &mut VisualTestContext) {
    vcx.run_until_parked();
    vcx.update(|window, cx| window.render_frame(cx));
}

/// A tile at `top`, its player reading `t0` as the time.
fn tile(
    cx: &mut TestAppContext,
    top: f32,
    t0: Instant,
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
    player.set_live_clock(t0);
    let shown = player.clone();
    let (view, vcx) = cx.add_window_view(move |_, _| TileView { player: shown, top });
    draw(vcx);
    (view, vcx, player, sink, source)
}

#[gpui_kit::test]
fn a_paused_sender_shows_its_last_frame_blurred(cx: &mut TestAppContext) {
    let t0 = Instant::now();
    let (_view, vcx, player, sink, source) = tile(cx, 0., t0);
    push(&source, 1, t0);
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
    let t0 = Instant::now();
    let (view, vcx, player, _sink, source) = tile(cx, 0., t0);
    push(&source, 1, t0);
    draw(vcx);
    assert_eq!(player.state(), PlayState::Playing);
    let size = player.picture_size();
    assert!(size.is_some(), "the frame is drawn");
    // The stall time passes without a frame.
    let later = t0 + STALL_AFTER + Duration::from_millis(100);
    player.set_live_clock(later);
    vcx.update(|_, cx| view.update(cx, |_, cx| cx.notify()));
    draw(vcx);
    assert_eq!(player.state(), PlayState::Buffering);
    assert_eq!(player.picture_size(), size, "the last picture stays");
    // A frame again: playing.
    push(&source, 2, later);
    draw(vcx);
    assert_eq!(player.state(), PlayState::Playing);
}

/// Six frames to the tile, drawn each time; how many redrew it, how many
/// converted, and whether it asked the frame clock.
fn redraws(cx: &mut TestAppContext, top: f32) -> (u64, u64, bool) {
    let t0 = Instant::now();
    let (view, vcx, player, _sink, source) = tile(cx, top, t0);
    let _ = take_video_targets();
    for seq in 1..=6 {
        push(&source, seq, t0);
        draw(vcx);
    }
    let asked = take_video_targets().contains(&view.entity_id());
    (player.live_asks(), player.live_conversions(), asked)
}

#[gpui_kit::test]
fn an_on_screen_tile_redraws_for_every_frame(cx: &mut TestAppContext) {
    let (asks, conversions, asked) = redraws(cx, 0.);
    assert_eq!(asks, 6);
    assert_eq!(conversions, 6);
    assert!(asked, "it asks the frame clock for the stall check");
}

#[gpui_kit::test]
fn an_off_screen_tile_stops_redrawing_and_asking(cx: &mut TestAppContext) {
    let (asks, _, asked) = redraws(cx, 5000.);
    // The first wake redraws; after two unseen wakes it waits for the
    // once-a-second look, which the frozen clock never reaches.
    assert_eq!(asks, 1);
    assert!(!asked, "it asks the frame clock nothing");
}

#[gpui_kit::test]
fn a_hidden_tile_converts_nothing_and_catches_up_in_sight(cx: &mut TestAppContext) {
    let t0 = Instant::now();
    let (view, vcx, player, _sink, source) = tile(cx, 5000., t0);
    for seq in 1..=30 {
        push(&source, seq, t0);
        draw(vcx);
    }
    // The first frame, before the tile counted as unseen; none after,
    // though every draw here paints it (out of sight).
    assert_eq!(player.live_conversions(), 1);
    // Scrolled into sight: the frame waiting in the feed is drawn.
    vcx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.top = 0.;
            cx.notify();
        })
    });
    draw(vcx);
    draw(vcx);
    assert_eq!(player.live_conversions(), 2);
    // And every frame again.
    push(&source, 31, t0);
    draw(vcx);
    assert_eq!(player.live_conversions(), 3);
}

#[gpui_kit::test]
fn a_size_change_mid_stream_redraws_at_the_new_size(cx: &mut TestAppContext) {
    let t0 = Instant::now();
    let (_view, vcx, player, _sink, source) = tile(cx, 0., t0);
    push_sized(&source, 1, 160, 90, t0);
    draw(vcx);
    assert_eq!(player.picture_size(), Some((160, 90)));
    push_sized(&source, 2, 1280, 720, t0);
    draw(vcx);
    // A large frame converts in the background: one more turn.
    draw(vcx);
    assert_eq!(player.picture_size(), Some((1280, 720)));
    push_sized(&source, 3, 320, 180, t0);
    draw(vcx);
    draw(vcx);
    assert_eq!(player.picture_size(), Some((320, 180)));
}

/// The Windows and Linux NV12 path (forced on, so it runs on every machine): the
/// renderer refuses the frame on screen, the tile shows the same picture
/// as an image at once, new frames are images for the file player's
/// pause, then NV12 again; and nothing is left once the player goes. On
/// Windows and Linux the player's own pull sees the refusal; on macOS no
/// renderer draws NV12, so the test makes the same call the pull would.
#[gpui_kit::test]
fn a_refused_nv12_frame_shows_as_an_image_and_nv12_comes_back(cx: &mut TestAppContext) {
    let t0 = Instant::now();
    let (_view, vcx, player, _sink, source) = tile(cx, 0., t0);
    player.set_live_nv12(true);
    let _ = take_made_images();
    push(&source, 1, t0);
    draw(vcx);
    let frame = player.live_nv12_frame().expect("an NV12 frame on screen");
    frame.mark_refused();
    if !cfg!(not(target_os = "macos")) {
        player.live_replace_refused(t0);
    }
    draw(vcx);
    assert!(player.live_nv12_frame().is_none(), "the refused frame went");
    let picture = player.current_picture().expect("the tile is never empty");
    assert!(picture.image().is_some(), "an image");
    assert_eq!(picture.size(), (160, 90));
    drop(picture);
    // A new frame during the pause is an image, converted off the UI
    // thread (one more turn).
    push(&source, 2, t0);
    draw(vcx);
    draw(vcx);
    assert!(player.live_nv12_frame().is_none());
    assert_eq!(player.live_conversions(), 2);
    assert!(
        player
            .current_picture()
            .is_some_and(|p| p.image().is_some())
    );
    // After the pause: NV12 again, during the draw.
    let later = t0 + NV12_RETRY;
    player.set_live_clock(later);
    push(&source, 3, later);
    draw(vcx);
    assert!(player.live_nv12_frame().is_some(), "NV12 again");
    assert!(player.live_nv12_pooled() > 0);
    // The feed ends: its spare buffers go, its last frame stays up.
    source.end(LiveEnd::Over);
    draw(vcx);
    assert_eq!(player.live_nv12_pooled(), 0, "the pool went with the feed");
    assert!(player.live_nv12_frame().is_some(), "the last frame stays");
    // Released with the player: its images and NV12 frames go.
    let made = take_made_images();
    assert!(!made.is_empty());
    vcx.update(|_, cx| player.release_live(cx));
    assert!(player.live_nv12_frame().is_none());
    let alive = made.iter().filter(|image| image.strong_count() > 0).count();
    // The picture the player draws next (an image on macOS) at most.
    assert!(alive <= 1, "released: {alive} images alive");
}
