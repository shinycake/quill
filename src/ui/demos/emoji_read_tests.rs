//! UI tests on the `ready-emoji-read-effect` demo: an unread incoming lone
//! emoji plays its interaction effect by itself once the history reports
//! it read, once.

use super::UNREAD_MESSAGE;
use crate::ui::app::QuillApp;
use crate::ui::emoji_effect::emoji_effects_on;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::ids::{ChatId, FileId, MessageId};
use quill::sticker_playback::player::DecodedFrames;
use std::sync::Arc;

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
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let demo = ScreenshotDemo::named("ready-emoji-read-effect");
    let handle = cx.open_window(size(px(1000.), px(900.)), move |window, cx| {
        let view = cx.new(|cx| QuillApp::new_with_demo(window, cx, None, Some(demo)));
        for file in [HAND_FILE, EFFECT_FILE] {
            view.read(cx).playback.stickers.set_fixture(
                file,
                Arc::new(|edge| DecodedFrames {
                    frames: (0..30u8)
                        .map(|i| vec![i; (edge as usize).pow(2) * 4])
                        .collect(),
                    fps: 30.0,
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
