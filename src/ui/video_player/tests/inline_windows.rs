//! A chat with a round video (message 611) and a video tile (204) open in
//! its own window while the main window shows another chat and keeps
//! drawing: each window's history passes leave alone the clips only the
//! other window shows, so the window's players stay open and play; they
//! go when the window closes.

use super::{Script, queue_backend};
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use crate::ui::video_player::VideoPlayer;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::ids::ChatId;
use std::cell::RefCell;
use std::rc::Rc;

const CHAT: ChatId = ChatId(11);
/// Another chat of the demo, with no clips: the main window shows it.
const OTHER: ChatId = ChatId(12);
const TILE: (i64, i64) = (11, 204);
const ROUND: (i64, i64) = (11, 611);

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = Rc::new(RefCell::new(None));
    let slot_in = slot.clone();
    // Tall enough that the chat window (3/4 of it) shows both clips.
    let handle = cx.open_window(size(px(1100.), px(1400.)), move |window, cx| {
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

/// One backend for every player the test may open, in any order (enough
/// to count the reopens of players dropped every frame).
fn scripts() -> Vec<Rc<RefCell<Script>>> {
    (0..200)
        .map(|_| {
            let script = Rc::new(RefCell::new(Script {
                ready: true,
                picture: true,
                ..Script::default()
            }));
            queue_backend(&script);
            script
        })
        .collect()
}

/// Backends handed to a player so far (each open takes the next one).
fn opened(scripts: &[Rc<RefCell<Script>>]) -> usize {
    scripts
        .iter()
        .filter(|script| !script.borrow().calls.is_empty() || script.borrow().dropped)
        .count()
}

fn player(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, key: (i64, i64)) -> VideoPlayer {
    app.read_with(vcx, |app, _| app.playback.inline_videos.player(key))
        .unwrap_or_else(|| panic!("{key:?} has a player"))
}

/// The chat window draws a frame, then the main window runs a history
/// pass of its own chat, as both do while each window animates.
fn draw_both(app: &Entity<QuillApp>, chat_window: AnyWindowHandle, vcx: &mut VisualTestContext) {
    vcx.update_window(chat_window, |_, window, cx| window.render_frame(cx))
        .unwrap();
    vcx.run_until_parked();
    app.update_in(vcx, |app, _, cx| app.notify_conversation(cx));
    vcx.run_until_parked();
}

#[gpui_kit::test]
fn a_chat_windows_clips_keep_playing_while_the_main_window_draws(cx: &mut TestAppContext) {
    let scripts = scripts();
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    vcx.update(|window, _| window.activate_window());
    settle(vcx);
    // Read: its window shows the messages loaded (an unread run would
    // load around its bar, which the demo cannot).
    app.update(vcx, |app, _| {
        if let Some(chat) = app
            .demo_session
            .as_mut()
            .and_then(|session| session.chats.get_mut(&CHAT.0))
        {
            chat.unread_count = 0;
        }
    });
    app.update_in(vcx, |app, window, cx| {
        app.open_chat_in_new_window(CHAT, window, cx)
    });
    vcx.run_until_parked();
    let main = vcx.update(|window, _| window.window_handle().window_id());
    let chat_window = vcx
        .windows()
        .into_iter()
        .find(|handle| handle.window_id() != main)
        .expect("the chat window opened");
    app.update_in(vcx, |app, window, cx| {
        app.select_listed_chat(OTHER, window, cx)
    });
    settle(vcx);
    for _ in 0..4 {
        draw_both(&app, chat_window, vcx);
    }
    let before = opened(&scripts);
    let marks: Vec<usize> = scripts.iter().map(|s| s.borrow().calls.len()).collect();
    let round = player(&app, vcx, ROUND);
    let tile = player(&app, vcx, TILE);

    for _ in 0..8 {
        draw_both(&app, chat_window, vcx);
    }
    // No backend opened (or dropped) since, and the same players.
    assert_eq!(
        opened(&scripts) - before,
        0,
        "players opened again over 8 frames of each window"
    );
    for (key, kept) in [(ROUND, &round), (TILE, &tile)] {
        assert!(player(&app, vcx, key).same_as(kept), "{key:?} reopened");
    }
    // Neither window's sweep pauses what the other one plays in view.
    let churn: Vec<String> = scripts
        .iter()
        .zip(&marks)
        .flat_map(|(script, mark)| {
            script
                .borrow()
                .calls
                .get(*mark..)
                .unwrap_or_default()
                .to_vec()
        })
        .collect();
    assert!(churn.is_empty(), "play / pause churn: {churn:?}");
    // Playing in view, with a picture.
    for (key, player) in [(ROUND, &round), (TILE, &tile)] {
        assert!(player.wants_play(), "{key:?} plays");
        assert!(
            !app.read_with(vcx, |app, _| app.playback.inline_videos.is_hidden(key)),
            "{key:?} is held as if off screen"
        );
        assert!(
            player.current_picture().is_some(),
            "{key:?} shows a picture"
        );
    }

    // The window closes: its clips go with it.
    drop((round, tile));
    vcx.update_window(chat_window, |_, window, cx| {
        window.dispatch_action(Box::new(crate::ui::actions::CloseWindow), cx);
    })
    .unwrap();
    vcx.run_until_parked();
    settle(vcx);
    app.read_with(vcx, |app, _| {
        let videos = &app.playback.inline_videos;
        assert!(videos.player(ROUND).is_none() && videos.player(TILE).is_none());
    });
    let live = scripts.iter().filter(|script| {
        let script = script.borrow();
        !script.dropped && !script.calls.is_empty()
    });
    assert_eq!(live.count(), 0, "every backend is released");
}
