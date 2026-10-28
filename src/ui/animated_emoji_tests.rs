//! UI tests on the `ready-animated-emoji` demo: a lone animated emoji is a
//! sticker in its skin tone that plays once, rests, and plays again on a
//! tap; leaving the chat forgets what played; custom emoji get their sizes.

use super::{message_emoji_media, play_head};
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::animated_emoji::EmojiMedia;
use quill::ids::{ChatId, MessageId};
use quill::state::HistoryMessage;
use quill::sticker_playback::FitzType;
use quill::sticker_playback::player::DecodedFrames;
use quill::telegram::envelope::MessageContent;
use std::sync::Arc;

const CHAT: i64 = 11;
/// The demo's first message (`demos/animated_emoji.rs`).
const FIRST_MESSAGE: i64 = 9_301;
const HAND_FILE: i32 = 7_301;
const CUSTOM_FILE: i32 = 7_302;

fn frames(vcx: &mut VisualTestContext) {
    vcx.run_until_parked();
    for _ in 0..3 {
        vcx.update(|window, cx| window.render_frame(cx));
    }
}

fn open_demo(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let demo = ScreenshotDemo::named("ready-animated-emoji");
    let handle = cx.open_window(size(px(1000.), px(900.)), move |window, cx| {
        let view = cx.new(|cx| QuillApp::new_with_demo(window, cx, None, Some(demo)));
        // A one-second clip: long enough to tap during a replay.
        for file in [HAND_FILE, CUSTOM_FILE] {
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

/// Let the player draw what it was asked for, then draw the window again.
fn settle(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) {
    for _ in 0..3 {
        frames(vcx);
        app.read_with(vcx, |app, _| app.playback.stickers.settle());
        app.update(vcx, |_, cx| cx.notify());
    }
    frames(vcx);
}

fn message(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, id: i64) -> HistoryMessage {
    app.read_with(vcx, |app, _| {
        app.demo_session
            .as_ref()
            .and_then(|s| s.histories.get(&CHAT))
            .and_then(|history| history.messages.get(&id))
            .cloned()
            .expect("the demo message")
    })
}

#[gpui_kit::test]
fn a_lone_emoji_is_a_sticker_in_its_skin_tone(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_demo(cx);
    for (id, fitz) in [
        (FIRST_MESSAGE, FitzType::None),
        (FIRST_MESSAGE + 1, FitzType::Type4),
    ] {
        let message = message(&app, &mut vcx, id);
        let MessageContent::Text(text) = &message.content else {
            panic!("a text message");
        };
        assert_eq!(text.animated.as_ref().map(|a| a.fitz), Some(fitz));
        let media = app.read_with(&vcx, |app, _| message_emoji_media(&message, app.session()));
        assert_eq!(media, Some(EmojiMedia::Sticker));
        let view = app.update(&mut vcx, |app, cx| app.lone_emoji_view(&message, cx));
        assert!(view.is_some(), "the sticker plays");
    }
}

#[gpui_kit::test]
fn a_lone_emoji_plays_once_and_a_tap_plays_it_again(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_demo(cx);
    let key = (CHAT, FIRST_MESSAGE);
    let message = message(&app, &mut vcx, FIRST_MESSAGE);
    // Shown: its first play started, and the next build notes it.
    let _ = app.update(&mut vcx, |app, cx| app.lone_emoji_view(&message, cx));
    app.read_with(&vcx, |app, _| {
        assert_eq!(
            app.playback
                .stickers
                .head_known(play_head(key, 0), HAND_FILE),
            Some(true),
            "the play head started when the row painted"
        );
        assert!(app.playback.emoji_plays.borrow().has_played(key));
    });
    // Drawn for a second (30 frames at 30 fps), it played through.
    let done = |app: &QuillApp| {
        app.playback
            .stickers
            .head_progress(play_head(key, 0), HAND_FILE)
            .is_some_and(|(pos, total)| pos >= total)
    };
    for _ in 0..60 {
        if app.read_with(&vcx, |app, _| done(app)) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
        app.update(&mut vcx, |_, cx| cx.notify());
        frames(&mut vcx);
    }
    assert!(app.read_with(&vcx, |app, _| done(app)), "it played through");
    // A tap plays it again, on a new head.
    app.update(&mut vcx, |app, _| {
        app.tap_animated_emoji(ChatId(CHAT), MessageId(FIRST_MESSAGE));
    });
    app.read_with(&vcx, |app, _| {
        assert_eq!(app.playback.emoji_plays.borrow().generation(key), 1);
    });
    settle(&app, &mut vcx);
    app.read_with(&vcx, |app, _| {
        assert_eq!(
            app.playback
                .stickers
                .head_known(play_head(key, 1), HAND_FILE),
            Some(true),
            "the replay started"
        );
    });
    // A second tap while it plays does not start it over.
    app.update(&mut vcx, |app, _| {
        app.tap_animated_emoji(ChatId(CHAT), MessageId(FIRST_MESSAGE));
    });
    app.read_with(&vcx, |app, _| {
        assert_eq!(app.playback.emoji_plays.borrow().generation(key), 1);
    });
    // Leaving the chat forgets it: it plays again next time.
    app.update(&mut vcx, |app, _| app.stop_sticker_playback());
    app.read_with(&vcx, |app, _| {
        let plays = app.playback.emoji_plays.borrow();
        assert!(!plays.has_played(key));
        assert_eq!(plays.generation(key), 0);
    });
}

#[gpui_kit::test]
fn custom_emoji_messages_take_their_sizes(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_demo(cx);
    for (id, side, count) in [(FIRST_MESSAGE + 2, 78, 2), (FIRST_MESSAGE + 3, 58, 3)] {
        let message = message(&app, &mut vcx, id);
        let media = app
            .read_with(&vcx, |app, _| message_emoji_media(&message, app.session()))
            .expect("drawn big");
        assert_eq!(media.side(), side);
        let views = app
            .update(&mut vcx, |app, cx| app.big_custom_emoji_views(&message, cx))
            .expect("big custom emoji");
        assert_eq!(views.len(), count);
    }
}
