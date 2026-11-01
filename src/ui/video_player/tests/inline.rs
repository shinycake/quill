//! A chat with an inline video tile (message 204) and a round video
//! (message 611) on scripted players: both play muted and looped, their
//! frames are asked for by the conversation's animation layer (not the
//! slice), they wait while the window is in the background, leaving the
//! chat stops them, and a tile scrolled out of view pauses, is released
//! and comes back. Also: a story paused while its clip opens stays paused.

use super::{Script, Scripted, calls, queue_backend};
use crate::ui::app::QuillApp;
use crate::ui::frame_clock::take_video_targets;
use crate::ui::inline_players::RELEASE_AFTER;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use crate::ui::video_player::{PlayState, VideoPlayer};
use gpui_kit::component::Root;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use gpui_kit::{Bounds, Pixels, Point};
use gpui_kit::{Modifiers, ScrollDelta, ScrollEvent, point};
use quill::ids::ChatId;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;

const TILE: (i64, i64) = (11, 204);
const ROUND: (i64, i64) = (11, 611);

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    new_app_sized(cx, 900.)
}

fn new_app_sized(cx: &mut TestAppContext, height: f32) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = Rc::new(RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(height)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(
                window,
                cx,
                None,
                Some(ScreenshotDemo::named("ready-video-inline")),
            )
        });
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some(view);
        Root::new(shell, window, cx)
    });
    let app = slot.borrow().clone().unwrap();
    let vcx = VisualTestContext::from_window(handle.into(), cx);
    (app, vcx)
}

fn settle(vcx: &mut VisualTestContext) {
    for _ in 0..4 {
        vcx.update(|window, _| window.refresh());
        vcx.run_until_parked();
    }
}

fn ready_script() -> Rc<RefCell<Script>> {
    Rc::new(RefCell::new(Script {
        ready: true,
        picture: true,
        ..Script::default()
    }))
}

#[gpui_kit::test]
fn inline_tiles_play_muted_on_the_layer_and_stop_with_the_chat(cx: &mut TestAppContext) {
    // One backend for each clip the chat opens, in whatever order.
    let scripts = [ready_script(), ready_script()];
    for script in &scripts {
        queue_backend(script);
    }
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    settle(vcx);
    // Behind another app the muted loops open but wait.
    app.read_with(vcx, |app, _| {
        let videos = &app.playback.inline_videos;
        for key in [TILE, ROUND] {
            let player = videos.player(key).expect("a player for each clip");
            assert!(!player.state().animates(), "{key:?}: {:?}", player.state());
        }
    });
    for script in &scripts {
        let calls = calls(script);
        assert!(!calls.iter().any(|c| c.starts_with("play")), "{calls:?}");
        script.borrow_mut().calls = calls;
    }
    vcx.update(|window, _| window.activate_window());
    settle(vcx);
    let (tile, round) = app.read_with(vcx, |app, _| {
        let videos = &app.playback.inline_videos;
        (videos.player(TILE), videos.player(ROUND))
    });
    let tile = tile.expect("the video tile has a player");
    let round = round.expect("the round video has a player");
    assert!(tile.state().animates(), "{:?}", tile.state());
    assert!(round.state().animates(), "{:?}", round.state());
    for script in &scripts {
        let calls = calls(script);
        assert!(calls.iter().any(|c| c == "volume 0"), "{calls:?}");
        assert!(calls.iter().any(|c| c == "play 1"), "{calls:?}");
        assert!(!calls.iter().any(|c| c == "pause"), "{calls:?}");
    }

    // Pictures keep coming: only the animation layer is asked to redraw.
    let (slice, layer) = app
        .read_with(vcx, |app, _| app.slices.conversation_ids())
        .expect("the conversation slice");
    take_video_targets();
    for script in &scripts {
        script.borrow_mut().picture = true;
    }
    settle(vcx);
    let targets = take_video_targets();
    assert!(targets.contains(&layer), "{targets:?} (layer {layer:?})");
    assert!(!targets.contains(&slice), "{targets:?} (slice {slice:?})");
    app.read_with(vcx, |app, _| {
        let videos = &app.playback.inline_videos;
        assert!(!videos.is_hidden(TILE) && !videos.is_hidden(ROUND));
    });

    // Another chat: the rows stop rendering and the players go.
    drop((tile, round));
    let other = app
        .read_with(vcx, |app, _| {
            app.session()
                .and_then(|session| session.chats.keys().copied().find(|id| *id != 11))
        })
        .expect("a second demo chat");
    app.update_in(vcx, |app, window, cx| {
        app.select_listed_chat(ChatId(other), window, cx)
    });
    settle(vcx);
    app.read_with(vcx, |app, _| {
        let videos = &app.playback.inline_videos;
        assert!(videos.player(TILE).is_none() && videos.player(ROUND).is_none());
    });
    for script in &scripts {
        assert!(script.borrow().dropped, "the backend is released");
    }
}

/// What a clip's layer draws over it (its picture, the time pill above
/// it) builds no hitboxes: GPUI would draw a layer holding any in whole
/// frames, and the whole window would render at the clip's rate.
#[gpui_kit::test]
fn a_clips_layer_builds_no_hitboxes(cx: &mut TestAppContext) {
    let scripts = [ready_script(), ready_script()];
    for script in &scripts {
        queue_backend(script);
    }
    let (app, mut vcx) = new_app(cx);
    vcx.update(|window, _| window.activate_window());
    settle(&mut vcx);
    let tile = app.read_with(&vcx, |app, _| app.playback.inline_videos.player(TILE));
    assert!(tile.is_some_and(|tile| tile.state().animates()));
    let interactive = vcx.update(|window, _| window.interactive_live_views());
    assert_eq!(interactive, 0);
}

/// Re-render the conversation (a history pass) `times` times.
fn history_passes(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, times: usize) {
    for _ in 0..times {
        app.update_in(vcx, |app, _, cx| app.notify_conversation(cx));
        vcx.run_until_parked();
    }
}

fn scroll(vcx: &mut VisualTestContext, dy: f32) {
    scroll_at(vcx, point(px(800.), px(300.)), dy);
}

fn scroll_at(vcx: &mut VisualTestContext, at: Point<Pixels>, dy: f32) {
    vcx.simulate_event(ScrollEvent {
        position: at,
        delta: ScrollDelta::Pixels(point(px(0.), px(dy))),
        modifiers: Modifiers::default(),
        ..Default::default()
    });
    settle(vcx);
}

/// Scroll by `dy` until the round video plays in view.
fn scroll_until_playing(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, dy: f32) {
    let mut steps = 0;
    while round_state(app, vcx) != Some((false, true)) {
        steps += 1;
        assert!(
            steps < 150,
            "never played in view: {:?}",
            round_state(app, vcx)
        );
        scroll(vcx, dy);
    }
}

/// The round video's player: whether there is one, whether it is held
/// off screen, and whether play is asked of it.
fn round_state(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> Option<(bool, bool)> {
    app.read_with(vcx, |app, _| {
        let videos = &app.playback.inline_videos;
        videos
            .player(ROUND)
            .map(|player| (videos.is_hidden(ROUND), player.wants_play()))
    })
}

#[gpui_kit::test]
fn a_clip_scrolled_out_of_view_pauses_is_released_and_comes_back(cx: &mut TestAppContext) {
    // Rows rendered in the list's overdraw open and drop players while it
    // scrolls, each taking the next backend.
    let scripts: Vec<_> = (0..16).map(|_| ready_script()).collect();
    for script in &scripts {
        queue_backend(script);
    }
    let (app, mut vcx) = new_app_sized(cx, 460.);
    let vcx = &mut vcx;
    vcx.update(|window, _| window.activate_window());
    settle(vcx);
    // From the top of the history down to the round video: where it lands
    // depends on the platform's text metrics, so step until it plays.
    scroll(vcx, 3000.);
    scroll_until_playing(&app, vcx, -40.);
    // Scroll in small steps until the round video's row still renders (the
    // list's first row) but its picture is above the visible area.
    let mut steps = 0;
    while round_state(&app, vcx) != Some((true, false)) {
        steps += 1;
        assert!(
            steps < 60,
            "never held off screen: {:?}",
            round_state(&app, vcx)
        );
        scroll(vcx, -10.);
    }
    let held = app
        .read_with(vcx, |app, _| app.playback.inline_videos.player(ROUND))
        .expect("the held player");
    let script = scripts
        .iter()
        .find(|script| {
            let calls = &script.borrow().calls;
            !script.borrow().dropped && calls.last().is_some_and(|c| c == "pause")
        })
        .expect("the round video's backend, paused last")
        .clone();
    script.borrow_mut().calls.clear();
    // More history passes while it sits there: no play / pause churn.
    history_passes(&app, vcx, 4);
    assert_eq!(round_state(&app, vcx), Some((true, false)));
    let calls = calls(&script);
    assert!(calls.is_empty(), "{calls:?}");
    // Off screen for RELEASE_AFTER: the slot sleeps, its backend freed.
    drop(held);
    vcx.executor().advance_clock(RELEASE_AFTER);
    vcx.run_until_parked();
    assert!(script.borrow().dropped, "the backend is released");
    assert!(app.read_with(vcx, |app, _| app.playback.inline_videos.is_asleep(ROUND)));
    // Later passes that still render the row (in the overdraw) open no
    // new backend: the sleeping slot waits to be painted in view.
    let used = |scripts: &[Rc<RefCell<Script>>]| {
        scripts
            .iter()
            .filter(|script| !script.borrow().calls.is_empty() || script.borrow().dropped)
            .count()
    };
    let before = used(&scripts);
    history_passes(&app, vcx, 4);
    vcx.executor().advance_clock(RELEASE_AFTER);
    vcx.run_until_parked();
    assert_eq!(round_state(&app, vcx), Some((true, false)));
    assert_eq!(
        used(&scripts),
        before,
        "a sleeping slot reopened its decoder"
    );
    // Back in view: the sweep gives it a new player, which plays. Its
    // decoder has no picture yet: the tile keeps the last one meanwhile.
    for script in &scripts {
        let mut script = script.borrow_mut();
        if script.calls.is_empty() && !script.dropped {
            script.picture = false;
        }
    }
    scroll_until_playing(&app, vcx, 40.);
    assert!(!app.read_with(vcx, |app, _| app.playback.inline_videos.is_asleep(ROUND)));
    assert_eq!(used(&scripts), before + 1);
    let player = app
        .read_with(vcx, |app, _| app.playback.inline_videos.player(ROUND))
        .expect("the new player");
    assert!(
        player.current_picture().is_some(),
        "the reopened round video paints no picture before its first"
    );
}

#[gpui_kit::test]
fn a_round_video_playing_with_sound_keeps_going_off_screen(cx: &mut TestAppContext) {
    let scripts: Vec<_> = (0..16).map(|_| ready_script()).collect();
    for script in &scripts {
        queue_backend(script);
    }
    let (app, mut vcx) = new_app_sized(cx, 460.);
    let vcx = &mut vcx;
    vcx.update(|window, _| window.activate_window());
    settle(vcx);
    scroll(vcx, 3000.);
    scroll_until_playing(&app, vcx, -40.);
    // A click plays it from the start with sound.
    assert!(app.read_with(vcx, |app, _| {
        app.playback.inline_videos.toggle_sound(ROUND.0, ROUND.1)
    }));
    settle(vcx);
    let player = app
        .read_with(vcx, |app, _| app.playback.inline_videos.player(ROUND))
        .expect("the sounding player");
    // Scrolled until its picture leaves the view: still heard, never
    // paused or released.
    let mut steps = 0;
    while !app.read_with(vcx, |app, _| app.playback.inline_videos.is_hidden(ROUND)) {
        steps += 1;
        assert!(steps < 60, "never left the view");
        scroll(vcx, -10.);
    }
    assert!(player.wants_play(), "kept playing off screen");
    vcx.executor().advance_clock(RELEASE_AFTER * 2);
    vcx.run_until_parked();
    let kept = app.read_with(vcx, |app, _| app.playback.inline_videos.player(ROUND));
    assert!(
        kept.is_some_and(|kept| kept.wants_play()),
        "not released while heard"
    );
}

#[gpui_kit::test]
fn a_story_paused_while_its_clip_opens_does_not_start_once_open(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    let script = Rc::new(RefCell::new(Script::default()));
    let player = VideoPlayer::from_backend(Box::new(Scripted(script.clone())), Some(10.0));
    player.play();
    assert_eq!(player.state(), PlayState::Opening);
    let story_player = player.clone();
    app.update_in(vcx, |app, _, _| {
        app.stories.player = Some(story_player);
        // Press-and-hold (or Space) right after the story opened.
        app.apply_story_native_pause(true);
    });
    assert!(!player.wants_play());
    script.borrow_mut().ready = true;
    player.pull(Instant::now(), None);
    assert_eq!(player.state(), PlayState::Ready);
    let calls = calls(&script);
    assert!(!calls.iter().any(|c| c.starts_with("play")), "{calls:?}");
    app.update_in(vcx, |app, _, _| app.stories.player = None);
}

/// Scrolled up a little, the jump button stays hidden and nothing covers
/// the clips (they stay on the layer); once it shows, only its own box
/// does.
#[gpui_kit::test]
fn only_a_shown_jump_button_covers_the_clips(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    vcx.update(|window, _| window.activate_window());
    settle(vcx);
    let occluders = |app: &Entity<QuillApp>, vcx: &mut VisualTestContext| {
        app.read_with(vcx, |app, _| app.slices.conversation_occluders())
    };
    scroll(vcx, 20.);
    let scrolled_up = app.read_with(vcx, |app, cx| {
        app.history.scroller.read(cx).is_scrolled_up()
    });
    assert!(scrolled_up);
    assert_eq!(occluders(&app, vcx), []);

    // Unread messages below show the button wherever the list is scrolled.
    app.update_in(vcx, |app, _, cx| {
        let session = app.demo_session.as_mut().expect("demo session");
        session
            .chats
            .get_mut(&TILE.0)
            .expect("the chat")
            .unread_count = 2;
        app.notify_conversation(cx);
    });
    vcx.executor()
        .advance_clock(std::time::Duration::from_secs(1));
    settle(vcx);
    let covered = occluders(&app, vcx);
    let [button] = covered.as_slice() else {
        panic!("one occluder: {covered:?}");
    };
    assert!(
        button.size.width > px(0.)
            && button.size.height > px(0.)
            && button.size.width < px(100.)
            && button.size.height < px(60.),
        "{button:?}"
    );
}

fn viewer_open(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> bool {
    app.read_with(vcx, |app, _| app.viewer.state.is_open())
}

fn scrolled_up(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> bool {
    app.read_with(vcx, |app, cx| {
        app.history.scroller.read(cx).is_scrolled_up()
    })
}

/// Scroll the history, with the wheel at `at`, until the clip covers `at`;
/// its bounds then.
fn scroll_clip_under(vcx: &mut VisualTestContext, at: Point<Pixels>) -> Bounds<Pixels> {
    let mut steps = 0;
    loop {
        let tile = vcx.debug_bounds("video-204");
        if let Some(tile) = tile.filter(|tile| tile.contains(&at)) {
            return tile;
        }
        steps += 1;
        assert!(steps < 100, "the clip never reached {at:?}: {tile:?}");
        let below = tile.is_some_and(|tile| tile.top() > at.y);
        scroll_at(vcx, at, if below { -20. } else { 20. });
    }
}

fn jump_button(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> Bounds<Pixels> {
    let covered = app.read_with(vcx, |app, _| app.slices.conversation_occluders());
    let [button] = covered.as_slice() else {
        panic!("the jump button shows: {covered:?}");
    };
    *button
}

/// One press on the jump button floating over a clip jumps to the latest
/// message and leaves the clip alone: the button takes the click, the
/// media viewer under it stays closed. A press on the clip beside the
/// button still opens the viewer.
#[gpui_kit::test]
fn the_jump_button_over_a_clip_takes_the_click(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    vcx.update(|window, _| window.activate_window());
    settle(vcx);
    // Unread messages below show the button wherever the list is scrolled.
    app.update_in(vcx, |app, _, cx| {
        let session = app.demo_session.as_mut().expect("demo session");
        session
            .chats
            .get_mut(&TILE.0)
            .expect("the chat")
            .unread_count = 2;
        app.notify_conversation(cx);
    });
    // At the top of the chat the clip sits highest; the button stays at
    // the bottom. Shorten the window until the button is over the clip's
    // lower part.
    let mut top = None;
    for steps in 0.. {
        assert!(steps < 50, "never reached the top: {top:?}");
        scroll(vcx, 200.);
        let tile = vcx.debug_bounds("video-204");
        if tile.is_some() && tile == top {
            break;
        }
        top = tile;
    }
    let tile = top.expect("the clip");
    vcx.executor()
        .advance_clock(std::time::Duration::from_secs(1));
    settle(vcx);
    let raise = jump_button(&app, vcx).center().y - (tile.bottom() - px(40.));
    vcx.simulate_resize(size(px(1100.), px(900.) - raise));
    settle(vcx);
    vcx.executor()
        .advance_clock(std::time::Duration::from_secs(1));
    settle(vcx);
    let button = jump_button(&app, vcx);
    let press = button.center();
    scroll_clip_under(vcx, press);
    assert!(scrolled_up(&app, vcx));
    assert!(!viewer_open(&app, vcx));

    vcx.simulate_click(press, Modifiers::none());
    settle(vcx);
    assert!(!scrolled_up(&app, vcx), "jumped to the latest message");
    assert!(!viewer_open(&app, vcx), "the clip under the button opened");

    // Beside the button, the clip takes the click.
    let tile = scroll_clip_under(vcx, press);
    let beside = point(tile.left() + px(20.), press.y);
    assert!(!button.contains(&beside), "{beside:?} is on {button:?}");
    vcx.simulate_click(beside, Modifiers::none());
    settle(vcx);
    assert!(viewer_open(&app, vcx), "a press on the clip opens it");
}
