//! UI tests on the `ready-emoji-read-effect` demo: an unread incoming lone
//! emoji plays its interaction effect by itself once the history reports
//! it read, once.

use super::UNREAD_MESSAGE;
use crate::ui::app::QuillApp;
use crate::ui::emoji_effect::{emoji_effects_on, move_anchor};
use crate::ui::frame_clock::take_slice_renders;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::ids::{ChatId, FileId, MessageId};
use quill::sticker_playback::player::DecodedFrames;
use std::sync::Arc;
use std::time::Duration;

const CHAT: i64 = 11;
/// The demo hand (`demos/animated_emoji.rs`) and the effect (`emoji_tap.rs`).
const HAND_FILE: i32 = 7_301;
const EFFECT_FILE: i32 = 7_201;

fn frames(vcx: &mut VisualTestContext) {
    vcx.run_until_parked();
    for _ in 0..3 {
        vcx.update(|window, cx| {
            window.activate_window();
            window.render_frame(cx);
        });
    }
}

fn settle(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) {
    for _ in 0..4 {
        frames(vcx);
        app.read_with(vcx, |app, _| app.playback.stickers.settle());
        app.update(vcx, |_, cx| cx.notify());
    }
    frames(vcx);
}

fn open_demo(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    open_demo_at(cx, 30.0)
}

/// The demo with its stickers at `fps`, their heads on the test clock.
fn open_demo_at(cx: &mut TestAppContext, fps: f64) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let demo = ScreenshotDemo::named("ready-emoji-read-effect");
    let handle = cx.open_window(size(px(1000.), px(900.)), move |window, cx| {
        let view = cx.new(|cx| QuillApp::new_with_demo(window, cx, None, Some(demo)));
        view.read(cx)
            .playback
            .stickers
            .follow(cx.background_executor().clone());
        for file in [HAND_FILE, EFFECT_FILE] {
            view.read(cx).playback.stickers.set_fixture(
                file,
                Arc::new(move |edge| DecodedFrames {
                    frames: (0..30u8)
                        .map(|i| vec![i; (edge as usize).pow(2) * 4])
                        .collect(),
                    fps,
                }),
            );
        }
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some(view);
        Root::new(shell, window, cx)
    });
    let app = slot.borrow().clone().expect("app");
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    settle(&app, &mut vcx);
    (app, vcx)
}

#[gpui_kit::test]
fn an_unread_lone_emoji_plays_its_effect_when_read(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_demo(cx);
    let key = (CHAT, UNREAD_MESSAGE);
    let effects = emoji_effects_on(key);
    assert_eq!(effects.len(), 1, "one effect over the emoji");
    let (effect, started) = &effects[0];
    assert_eq!(effect.sticker.file_id, FileId(EFFECT_FILE));
    assert!(!effect.incoming, "the partner is not told");
    assert!(started, "the effect started playing");
    // Shown while read again: once a session.
    app.update(&mut vcx, |app, _| {
        let session = app.demo_session.as_mut().expect("demo session");
        assert!(session.stickers.animations.is_ready());
        session.autoplay_emoji_effects(ChatId(CHAT), &[MessageId(UNREAD_MESSAGE)], false, 1);
        assert!(session.stickers.interactions.take_pending().is_empty());
    });
}

fn app_draws(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, steps: u32) -> u32 {
    let beside = vcx.update(|_, cx| cx.new(|_| ()).entity_id());
    let _ = take_slice_renders();
    for _ in 0..steps {
        app.update(vcx, |app, cx| {
            app.request_animation_tick_for(Some(beside), 60, cx);
        });
        vcx.executor().advance_clock(Duration::from_millis(16));
        vcx.run_until_parked();
    }
    take_slice_renders().get("app").copied().unwrap_or(0)
}

/// Runs the effect just past its second frame (the first draws with the
/// clock's start): nothing is due for a while.
fn past_a_frame(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) {
    for _ in 0..2 {
        let drew = (0..40).any(|_| app_draws(app, vcx, 1) > 0);
        assert!(drew, "it plays");
    }
    let _ = app_draws(app, vcx, 1);
}

/// Steps in which no frame comes after [`past_a_frame`], with a tick in them.
const HELD_STEPS: u32 = 3;
/// Half a second: four of the effect's frames.
const STEPS_OF_FOUR_FRAMES: u32 = 31;

#[gpui_kit::test]
fn a_tick_beside_a_tap_effect_draws_it_only_around_its_next_frame(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_demo_at(cx, 8.0);
    let draws = app_draws(&app, &mut vcx, STEPS_OF_FOUR_FRAMES);
    assert!(
        (3..=STEPS_OF_FOUR_FRAMES / 2).contains(&draws),
        "{draws} app draws"
    );
}

#[gpui_kit::test]
fn a_tap_effect_holding_a_frame_draws_nothing(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_demo_at(cx, 8.0);
    past_a_frame(&app, &mut vcx);
    assert_eq!(app_draws(&app, &mut vcx, HELD_STEPS), 0);
}

#[gpui_kit::test]
fn a_tap_effect_follows_the_chat_under_it_between_its_frames(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_demo_at(cx, 8.0);
    past_a_frame(&app, &mut vcx);
    move_anchor((CHAT, UNREAD_MESSAGE), px(40.));
    assert!(app_draws(&app, &mut vcx, HELD_STEPS) > 0, "it moved");
}

#[gpui_kit::test]
fn another_ask_for_a_tap_effects_tick_draws_it(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_demo_at(cx, 8.0);
    past_a_frame(&app, &mut vcx);
    app.update(&mut vcx, |app, cx| app.request_app_tick(60, cx));
    assert!(app_draws(&app, &mut vcx, HELD_STEPS) > 0);
}
