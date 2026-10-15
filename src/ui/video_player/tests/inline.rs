//! A chat with an inline video tile (message 204) and a round video
//! (message 611) on scripted players: both play muted and looped, their
//! frames are asked for by the conversation's animation layer (not the
//! slice), they wait while the window is in the background, and leaving
//! the chat stops them.

use super::{Script, calls, queue_backend};
use crate::ui::app::QuillApp;
use crate::ui::frame_clock::take_video_targets;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::ids::ChatId;
use std::cell::RefCell;
use std::rc::Rc;

const TILE: (i64, i64) = (11, 204);
const ROUND: (i64, i64) = (11, 611);

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = Rc::new(RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(900.)), move |window, cx| {
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
