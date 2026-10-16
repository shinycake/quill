//! Picture-in-Picture on a scripted player: the viewer hands its player
//! to the window (no reopen, no seek) and closes; only the window is
//! asked to redraw for pictures; a click and Space pause and resume; a
//! move snaps to the screen edge; Escape closes and drops the player;
//! "back to the viewer" reopens the viewer on the same message with the
//! same player.

use super::{Script, calls, queue_backend};
use crate::ui::app::QuillApp;
use crate::ui::frame_clock::take_video_targets;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use crate::ui::video_player::VideoPlayer;
use crate::ui::video_player::controls::pip::{VideoPip, take_frames};
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AppContext, Entity, Focusable as _, Modifiers, MouseButton, TestAppContext, VisualTestContext,
    WindowHandle, point, px, size,
};
use quill::ids::{ChatId, MessageId};
use quill::media_viewer::pip::panel::PipRect;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::Ordering;
use std::time::Duration;

pub(super) fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = Rc::new(RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(800.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(
                window,
                cx,
                None,
                Some(ScreenshotDemo::named("ready-privacy")),
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

pub(super) fn settle(vcx: &mut VisualTestContext) {
    for _ in 0..4 {
        vcx.update(|window, _| window.refresh());
        vcx.run_until_parked();
    }
}

/// The viewer open on the demo clip (message 204), played by `script`,
/// then handed to Picture-in-Picture. Returns the viewer's player.
fn open_pip(
    cx: &mut TestAppContext,
    script: &Rc<RefCell<Script>>,
) -> (Entity<QuillApp>, VisualTestContext, VideoPlayer) {
    open_pip_on(cx, script, |_| {}, MessageId(204), 0)
}

/// The demo chat with `setup` applied to its session, the viewer opened
/// on `message`'s `offset`-th item played by `script`, then handed to
/// Picture-in-Picture.
pub(super) fn open_pip_on(
    cx: &mut TestAppContext,
    script: &Rc<RefCell<Script>>,
    setup: impl FnOnce(&mut QuillApp),
    message: MessageId,
    offset: usize,
) -> (Entity<QuillApp>, VisualTestContext, VideoPlayer) {
    let (app, mut vcx) = new_app(cx);
    let vcx_ref = &mut vcx;
    vcx_ref.update(|window, _| window.activate_window());
    app.update_in(vcx_ref, |app, window, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            app.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            crate::ui::demos::video_fixtures::apply_ready_video_viewer(
                session,
                &app.demo_ui.sink,
                &app.demo_ui.seq,
            );
        }
        setup(app);
        let focus = app.focus_handle(cx);
        window.focus(&focus, cx);
        cx.notify();
    });
    queue_backend(script);
    app.update_in(vcx_ref, |app, _, cx| {
        app.open_media_viewer_at(ChatId(11), message, offset, cx);
    });
    settle(vcx_ref);
    let player = app
        .read_with(vcx_ref, |app, _| app.viewer.player.clone())
        .expect("the viewer plays the clip");
    calls(script);
    app.update_in(vcx_ref, |app, _, cx| app.open_video_pip(cx));
    settle(vcx_ref);
    (app, vcx, player)
}

/// The window and its view.
pub(super) fn pip_of(
    app: &Entity<QuillApp>,
    vcx: &mut VisualTestContext,
) -> (WindowHandle<Root>, Entity<VideoPip>) {
    app.read_with(vcx, |app, _| {
        let link = app
            .viewer
            .pip
            .as_ref()
            .expect("a Picture-in-Picture window");
        (
            link.window.window,
            link.window.view.upgrade().expect("its view is alive"),
        )
    })
}

pub(super) fn pip_player(
    view: &Entity<VideoPip>,
    vcx: &mut VisualTestContext,
) -> Option<VideoPlayer> {
    view.read_with(vcx, |pip, _| pip.player().cloned())
}

#[gpui_kit::test]
fn the_viewer_hands_its_player_over_and_the_window_plays_pauses_and_closes(
    cx: &mut TestAppContext,
) {
    let script = Rc::new(RefCell::new(Script {
        ready: true,
        picture: true,
        ..Script::default()
    }));
    let (app, mut vcx, viewer_player) = open_pip(cx, &script);
    let vcx = &mut vcx;
    // The viewer closed and let go of its player; the window has it.
    app.read_with(vcx, |app, _| {
        assert!(!app.viewer.state.is_open());
        assert!(app.viewer.player.is_none());
    });
    let (window, view) = pip_of(&app, vcx);
    let player = pip_player(&view, vcx).expect("the window plays");
    assert!(
        player.same_as(&viewer_player),
        "the same player, not a new one"
    );
    drop(viewer_player);
    let handed = calls(&script);
    assert!(
        !handed.iter().any(|c| c.starts_with("seek") || c == "pause"),
        "no reopen, no seek, no pause: {handed:?}"
    );
    assert!(player.wants_play(), "still playing");

    let mut pip = VisualTestContext::from_window(window.into(), cx);
    let pip = &mut pip;
    pip.run_until_parked();
    // Pictures are asked for by the window's view alone.
    take_video_targets();
    script.borrow_mut().picture = true;
    pip.update(|window, _| window.refresh());
    pip.run_until_parked();
    let targets = take_video_targets();
    assert!(!targets.is_empty(), "the window pulls pictures");
    assert!(
        targets.iter().all(|target| *target == view.entity_id()),
        "{targets:?} (window view {:?})",
        view.entity_id()
    );

    // A click on the picture pauses; Space resumes.
    let viewport = pip.update(|window, _| window.viewport_size());
    let middle = point(viewport.width / 2., viewport.height / 2.);
    pip.simulate_mouse_move(middle, None, Modifiers::none());
    pip.simulate_click(middle, Modifiers::none());
    pip.run_until_parked();
    assert!(!player.wants_play(), "the click paused");
    assert_eq!(calls(&script), ["pause"]);
    pip.simulate_keystrokes("space");
    pip.run_until_parked();
    assert!(player.wants_play(), "Space resumed");
    assert_eq!(calls(&script), ["play 1"]);

    // Escape closes the window and drops the player.
    drop(player);
    drop(view);
    pip.simulate_keystrokes("escape");
    pip.run_until_parked();
    settle(vcx);
    assert!(script.borrow().dropped, "closing dropped the player");
    app.read_with(vcx, |app, _| {
        assert!(app.viewer.pip.is_none());
        assert!(!app.viewer.state.is_open());
    });
}

#[gpui_kit::test]
fn back_to_the_viewer_reopens_it_with_the_same_player_where_it_was(cx: &mut TestAppContext) {
    let script = Rc::new(RefCell::new(Script {
        ready: true,
        picture: true,
        ..Script::default()
    }));
    let (app, mut vcx, viewer_player) = open_pip(cx, &script);
    let vcx = &mut vcx;
    let (window, view) = pip_of(&app, vcx);
    drop(view);
    script.borrow_mut().position = 4.0;
    calls(&script);
    let mut pip = VisualTestContext::from_window(window.into(), cx);
    let pip = &mut pip;
    pip.run_until_parked();
    pip.update(|window, cx| {
        window.hover("pip-enlarge", cx);
        window.click("pip-enlarge", cx);
    });
    pip.run_until_parked();
    settle(vcx);
    app.read_with(vcx, |app, _| {
        assert!(app.viewer.pip.is_none(), "the window closed");
        assert!(app.viewer.state.is_open(), "the viewer is back");
        let item = app.viewer.state.current().expect("an item");
        assert_eq!(
            (item.chat_id, item.message_id),
            (ChatId(11), MessageId(204))
        );
        let player = app.viewer.player.as_ref().expect("the viewer plays");
        assert!(player.same_as(&viewer_player), "the same player came back");
        assert!(player.wants_play());
        // Where it was (the transport runs on while playing).
        let position = player.transport().position;
        assert!((4.0..5.0).contains(&position), "{position}");
    });
    assert!(!script.borrow().dropped);
    let back = calls(&script);
    assert!(
        !back.iter().any(|c| c.starts_with("seek") || c == "pause"),
        "nothing restarted: {back:?}"
    );
}

#[gpui_kit::test]
fn the_window_opens_snapped_and_a_move_snaps_to_the_edge(cx: &mut TestAppContext) {
    let script = Rc::new(RefCell::new(Script {
        ready: true,
        picture: true,
        ..Script::default()
    }));
    let (app, mut vcx, viewer_player) = open_pip(cx, &script);
    drop(viewer_player);
    let vcx = &mut vcx;
    let (window, view) = pip_of(&app, vcx);
    drop(view);
    let mut pip = VisualTestContext::from_window(window.into(), cx);
    let pip = &mut pip;
    pip.run_until_parked();
    // The scripted picture is square: 320 × 320, 20 px from the top-left
    // of the test screen.
    let bounds = pip.update(|window, _| window.bounds());
    assert_eq!(
        (
            bounds.origin.x,
            bounds.origin.y,
            bounds.size.width,
            bounds.size.height
        ),
        (px(20.), px(20.), px(320.), px(320.))
    );
    take_frames();
    let start = point(px(160.), px(160.));
    pip.simulate_mouse_move(start, None, Modifiers::none());
    pip.simulate_mouse_down(start, MouseButton::Left, Modifiers::none());
    // Far from every edge: the window follows.
    pip.simulate_mouse_move(
        point(px(200.), px(200.)),
        MouseButton::Left,
        Modifiers::none(),
    );
    pip.run_until_parked();
    assert_eq!(take_frames(), [PipRect::new(60, 60, 320, 320)]);
    // 12 px right of where it started: within 16 px of the left snap line,
    // so it slides onto it.
    pip.simulate_mouse_move(
        point(px(172.), px(200.)),
        MouseButton::Left,
        Modifiers::none(),
    );
    for _ in 0..4 {
        pip.executor().advance_clock(Duration::from_millis(50));
        pip.update(|window, _| window.refresh());
        pip.run_until_parked();
    }
    let frames = take_frames();
    assert_eq!(
        frames.last(),
        Some(&PipRect::new(20, 60, 320, 320)),
        "{frames:?}"
    );
    pip.simulate_mouse_up(
        point(px(172.), px(200.)),
        MouseButton::Left,
        Modifiers::none(),
    );
    pip.run_until_parked();
    // The release after a move neither pauses nor moves the window again.
    assert!(take_frames().is_empty());
    assert!(!calls(&script).iter().any(|c| c == "pause"));
    let player = view_player(&app, vcx);
    assert!(player.is_some_and(|player| player.wants_play()));
}

fn view_player(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> Option<VideoPlayer> {
    let (_, view) = pip_of(app, vcx);
    pip_player(&view, vcx)
}
