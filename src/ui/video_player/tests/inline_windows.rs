//! A chat with a round video (message 611) and a video tile (204) open in
//! its own window while the main window shows another chat and keeps
//! drawing: each window's history passes leave alone the clips only the
//! other window shows, so the window's players stay open and play; they
//! go when the window closes. A window the platform stops presenting
//! (minimized, covered) pauses its clips and lets them sleep; one window's
//! viewer or closed GIF panel leaves the other window's clips alone.

use super::{Script, queue_backend};
use crate::ui::app::QuillApp;
use crate::ui::inline_players::{Panel, RELEASE_AFTER};
use crate::ui::keybindings::bind_keys;
use crate::ui::media_panel::PanelTab;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use crate::ui::video_player::VideoPlayer;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, AppContext, Entity, TestAppContext, VisualTestContext, WindowVisibility, px,
    size,
};
use quill::ids::{ChatId, MessageId};
use quill::telegram::envelope::MessageContent;
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

/// Backends handed to a player and not dropped: decoders held.
fn live(scripts: &[Rc<RefCell<Script>>]) -> usize {
    scripts
        .iter()
        .filter(|script| {
            let script = script.borrow();
            !script.dropped && !script.calls.is_empty()
        })
        .count()
}

/// Open `chat` in a window of its own and return the window. Read first,
/// so its window shows the messages loaded (an unread run would load
/// around its bar, which the demo cannot).
fn open_window(
    app: &Entity<QuillApp>,
    vcx: &mut VisualTestContext,
    chat: ChatId,
) -> AnyWindowHandle {
    app.update(vcx, |app, _| {
        if let Some(chat) = app
            .demo_session
            .as_mut()
            .and_then(|session| session.chats.get_mut(&chat.0))
        {
            chat.unread_count = 0;
        }
    });
    app.update_in(vcx, |app, window, cx| {
        app.open_chat_in_new_window(chat, window, cx)
    });
    vcx.run_until_parked();
    let main = vcx.update(|window, _| window.window_handle().window_id());
    vcx.windows()
        .into_iter()
        .find(|handle| handle.window_id() != main)
        .expect("the chat window opened")
}

fn draw_chat_window(chat_window: AnyWindowHandle, vcx: &mut VisualTestContext) {
    vcx.update_window(chat_window, |_, window, cx| window.render_frame(cx))
        .unwrap();
    vcx.run_until_parked();
}

/// A history pass of the main window's chat.
fn draw_main(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) {
    app.update_in(vcx, |app, _, cx| app.notify_conversation(cx));
    vcx.run_until_parked();
}

/// Each clip has a player that plays, in view, with a picture.
fn assert_plays(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, keys: &[(i64, i64)]) {
    for key in keys {
        let player = player(app, vcx, *key);
        let (hidden, asleep) = app.read_with(vcx, |app, _| {
            let videos = &app.playback.inline_videos;
            (videos.is_hidden(*key), videos.is_asleep(*key))
        });
        assert!(!hidden && !asleep, "{key:?} is held as if off screen");
        assert!(player.wants_play(), "{key:?} plays");
        assert!(
            player.current_picture().is_some(),
            "{key:?} shows a picture"
        );
    }
}

fn player(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, key: (i64, i64)) -> VideoPlayer {
    app.read_with(vcx, |app, _| app.playback.inline_videos.player(key))
        .unwrap_or_else(|| panic!("{key:?} has a player"))
}

/// The chat window draws a frame, then the main window runs a history
/// pass of its own chat, as both do while each window animates.
fn draw_both(app: &Entity<QuillApp>, chat_window: AnyWindowHandle, vcx: &mut VisualTestContext) {
    draw_chat_window(chat_window, vcx);
    draw_main(app, vcx);
}

#[gpui_kit::test]
fn a_chat_windows_clips_keep_playing_while_the_main_window_draws(cx: &mut TestAppContext) {
    let scripts = scripts();
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    vcx.update(|window, _| window.activate_window());
    settle(vcx);
    let chat_window = open_window(&app, vcx, CHAT);
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
    assert_plays(&app, vcx, &[ROUND, TILE]);

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
    assert_eq!(live(&scripts), 0, "every backend is released");
}

/// Off screen in a window the platform stops presenting: paused at once,
/// asleep after `RELEASE_AFTER`, and only the window's own pass wakes
/// them when it is presented again.
fn assert_sleeps_out_of_sight(
    app: &Entity<QuillApp>,
    vcx: &mut VisualTestContext,
    scripts: &[Rc<RefCell<Script>>],
) {
    for key in [ROUND, TILE] {
        let hidden = app.read_with(vcx, |app, _| app.playback.inline_videos.is_hidden(key));
        assert!(hidden, "{key:?} pauses out of sight");
        assert!(!player(app, vcx, key).wants_play(), "{key:?} is paused");
    }
    vcx.executor().advance_clock(RELEASE_AFTER);
    vcx.run_until_parked();
    for key in [ROUND, TILE] {
        let asleep = app.read_with(vcx, |app, _| app.playback.inline_videos.is_asleep(key));
        assert!(asleep, "{key:?} sleeps after RELEASE_AFTER");
    }
    assert_eq!(live(scripts), 0, "every decoder is released");
}

#[gpui_kit::test]
fn a_minimized_chat_windows_clips_pause_then_sleep_while_the_main_window_draws(
    cx: &mut TestAppContext,
) {
    let scripts = scripts();
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    vcx.update(|window, _| window.activate_window());
    settle(vcx);
    let chat_window = open_window(&app, vcx, CHAT);
    app.update_in(vcx, |app, window, cx| {
        app.select_listed_chat(OTHER, window, cx)
    });
    settle(vcx);
    for _ in 0..4 {
        draw_both(&app, chat_window, vcx);
    }
    assert_plays(&app, vcx, &[ROUND, TILE]);

    // Minimized: the main window draws on, the chat window no more.
    vcx.simulate_window_visibility_change(chat_window, WindowVisibility::Hidden);
    vcx.run_until_parked();
    for _ in 0..2 {
        draw_main(&app, vcx);
    }
    assert_sleeps_out_of_sight(&app, vcx, &scripts);

    // Restored: its next frame plays them again.
    vcx.simulate_window_visibility_change(chat_window, WindowVisibility::Visible);
    vcx.run_until_parked();
    for _ in 0..2 {
        draw_both(&app, chat_window, vcx);
    }
    assert_plays(&app, vcx, &[ROUND, TILE]);
}

#[gpui_kit::test]
fn a_covered_main_windows_clips_sleep_and_play_again_once_it_shows(cx: &mut TestAppContext) {
    let scripts = scripts();
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    vcx.update(|window, _| window.activate_window());
    settle(vcx);
    // The main window shows the clips; a chat window another chat.
    let chat_window = open_window(&app, vcx, OTHER);
    for _ in 0..4 {
        draw_both(&app, chat_window, vcx);
    }
    assert_plays(&app, vcx, &[ROUND, TILE]);

    // Covered: the chat window draws on, the main window no more.
    vcx.simulate_visibility_change(WindowVisibility::Hidden);
    vcx.run_until_parked();
    for _ in 0..2 {
        draw_chat_window(chat_window, vcx);
    }
    assert_sleeps_out_of_sight(&app, vcx, &scripts);

    // Shown again, with nothing else asking for a redraw: they play again
    // (a sleeping one on a new player, which redraws the history).
    vcx.simulate_visibility_change(WindowVisibility::Visible);
    for _ in 0..3 {
        vcx.run_until_parked();
    }
    assert_plays(&app, vcx, &[ROUND, TILE]);
}

#[gpui_kit::test]
fn a_viewer_in_the_main_window_leaves_a_click_in_a_chat_window_alone(cx: &mut TestAppContext) {
    let _scripts = scripts();
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    vcx.update(|window, _| window.activate_window());
    app.update(vcx, |app, _| {
        let session = app.demo_session.as_mut().expect("the demo session");
        session.settings.media_prefs.autoplay_videos = false;
        session.settings.media_prefs.autoplay_gifs = false;
    });
    settle(vcx);
    let chat_window = open_window(&app, vcx, CHAT);
    // The main window shows the same chat, under the viewer.
    app.update_in(vcx, |app, _, cx| {
        app.open_media_viewer(CHAT, MessageId(TILE.1), cx);
    });
    settle(vcx);
    draw_both(&app, chat_window, vcx);
    let note = app.read_with(vcx, |app, _| {
        app.session()
            .and_then(|session| session.histories.get(&ROUND.0))
            .and_then(|history| history.messages.get(&ROUND.1))
            .map(|message| message.content.clone())
    });
    let Some(MessageContent::VideoNote(note)) = note else {
        panic!("message 611 is a round video");
    };
    assert!(
        app.read_with(vcx, |app, _| app.playback.inline_videos.player(ROUND))
            .is_none(),
        "with autoplay off the round video rests on its still"
    );

    // A click on the round video in the chat window; both windows draw,
    // the main one first.
    app.update_in(vcx, |app, _, cx| {
        app.in_window_scope(Some(CHAT), cx, |app, cx| {
            app.play_video_note(CHAT, MessageId(ROUND.1), &note, cx);
        })
        .expect("the chat window's scope");
    });
    vcx.run_until_parked();
    for _ in 0..2 {
        draw_main(&app, vcx);
        draw_chat_window(chat_window, vcx);
    }
    app.read_with(vcx, |app, _| {
        let videos = &app.playback.inline_videos;
        assert!(videos.player(ROUND).is_some(), "the click plays it");
        assert!(videos.is_sounding(ROUND), "with sound");
    });
}

#[gpui_kit::test]
fn a_gif_panel_closed_in_the_main_window_leaves_the_chat_windows_open(cx: &mut TestAppContext) {
    let scripts = scripts();
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    vcx.update(|window, _| window.activate_window());
    app.update(vcx, |app, _| {
        let session = app.demo_session.as_mut().expect("the demo session");
        crate::ui::demos::video_fixtures::apply_ready_gifs(
            session,
            &app.demo_ui.sink,
            &app.demo_ui.seq,
        );
        session.stickers.gifs.open = false;
    });
    settle(vcx);
    let chat_window = open_window(&app, vcx, CHAT);
    app.update_in(vcx, |app, window, cx| {
        app.select_listed_chat(OTHER, window, cx)
    });
    settle(vcx);
    // The GIF tab opens in both windows, then closes in the main one.
    app.update_in(vcx, |app, _, cx| {
        app.in_window_scope(Some(CHAT), cx, |app, cx| {
            app.open_media_panel(PanelTab::Gifs, cx);
        })
        .expect("the chat window's scope");
        app.open_media_panel(PanelTab::Gifs, cx);
    });
    for _ in 0..2 {
        draw_both(&app, chat_window, vcx);
    }
    app.update_in(vcx, |app, _, cx| {
        app.close_media_panel(cx);
    });
    for _ in 0..2 {
        draw_both(&app, chat_window, vcx);
    }
    // A saved GIF's preview loop plays in the chat window's panel.
    let gif = (Panel::Gifs.chat(), 64);
    let kept = player(&app, vcx, gif);
    let before = opened(&scripts);

    for _ in 0..8 {
        draw_both(&app, chat_window, vcx);
    }
    assert_eq!(
        opened(&scripts) - before,
        0,
        "GIF panel players opened again over 8 frames of each window"
    );
    assert!(player(&app, vcx, gif).same_as(&kept), "the GIF reopened");
}
