//! Video profile photos: the info cover plays its clip muted in a loop
//! from the main frame, pauses under the viewer and drops its decoder
//! when the panel closes; the viewer loops the clip with no transport.

use super::{Script, queue_backend};
use crate::ui::app::QuillApp;
use crate::ui::demos::video_avatar::{CLIP_FILE, VIDEO_GROUP};
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::ids::{ChatId, FileId};
use quill::state::PhotoOwner;
use std::cell::RefCell;
use std::rc::Rc;

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = Rc::new(RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1200.), px(800.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(
                window,
                cx,
                None,
                Some(ScreenshotDemo::named("ready-info-video-avatar")),
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
    for _ in 0..3 {
        vcx.update(|window, _| window.refresh());
        vcx.run_until_parked();
    }
}

fn ready_script() -> Rc<RefCell<Script>> {
    Rc::new(RefCell::new(Script {
        ready: true,
        ..Script::default()
    }))
}

fn cover_playing(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> Option<bool> {
    app.read_with(vcx, |app, _| {
        app.video_avatar.player().map(|player| player.wants_play())
    })
}

#[gpui_kit::test]
fn the_cover_plays_pauses_under_the_viewer_and_stops_with_the_panel(cx: &mut TestAppContext) {
    let cover = ready_script();
    queue_backend(&cover);
    let viewer = ready_script();
    queue_backend(&viewer);
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    vcx.update(|window, _| window.activate_window());
    settle(vcx);

    assert_eq!(cover_playing(&app, vcx), Some(true), "the cover clip plays");
    let calls = super::calls(&cover);
    assert!(calls.contains(&"volume 0".to_string()), "muted: {calls:?}");
    assert!(
        calls.contains(&"seek 1".to_string()),
        "from the main frame: {calls:?}"
    );
    assert!(calls.iter().any(|c| c.starts_with("play")), "{calls:?}");

    // In the background it pauses, like tdesktop's GIFs.
    vcx.deactivate_window();
    settle(vcx);
    assert_eq!(
        cover_playing(&app, vcx),
        Some(false),
        "paused in the background"
    );
    vcx.update(|window, _| window.activate_window());
    settle(vcx);
    assert_eq!(cover_playing(&app, vcx), Some(true));

    // The viewer covers the panel: the cover pauses, the viewer loops the
    // clip with no transport.
    app.update_in(vcx, |app, _, cx| {
        app.open_peer_photos(PhotoOwner::Chat(ChatId(VIDEO_GROUP)), cx)
    });
    settle(vcx);
    assert_eq!(
        cover_playing(&app, vcx),
        Some(false),
        "paused under the viewer"
    );
    app.read_with(vcx, |app, _| {
        let item = app.viewer.state.current().expect("the current photo");
        assert_eq!(item.play_file_id, Some(FileId(CLIP_FILE)));
        assert!(app.viewer_loops(), "a video photo loops");
        assert!(app.viewer_video_playing());
        assert!(app.viewer.controls.seek.is_none(), "no seek bar");
    });
    assert!(super::calls(&viewer).contains(&"seek 1".to_string()));

    // Back to the panel: the cover plays on.
    app.update_in(vcx, |app, _, cx| app.close_media_viewer(cx));
    settle(vcx);
    assert_eq!(cover_playing(&app, vcx), Some(true));
    assert!(viewer.borrow().dropped);

    // Closing the panel drops the player and its decoder.
    app.update_in(vcx, |app, _, cx| app.close_info_panel(cx));
    settle(vcx);
    assert_eq!(cover_playing(&app, vcx), None);
    assert!(
        cover.borrow().dropped,
        "closing the panel released the decoder"
    );
}

#[gpui_kit::test]
fn with_autoplay_off_the_cover_stays_still(cx: &mut TestAppContext) {
    let cover = ready_script();
    queue_backend(&cover);
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    vcx.update(|window, _| window.activate_window());
    settle(vcx);
    assert_eq!(cover_playing(&app, vcx), Some(true));
    app.update_in(vcx, |app, _, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            session.settings.media_prefs.autoplay_gifs = false;
        }
        cx.notify();
    });
    settle(vcx);
    assert_eq!(cover_playing(&app, vcx), None);
    assert!(cover.borrow().dropped);
}
