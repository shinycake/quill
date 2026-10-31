//! The Picture-in-Picture window's volume is the app's. Over the
//! `ready-audio` demo (a song playing in chat 11); nothing reaches Telegram.

use crate::ui::app::QuillApp;
use crate::ui::frame_clock::frames_over as frames;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::lang::{keys, tr};

fn open(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(800.)), move |window, cx| {
        let demo = ScreenshotDemo::named("ready-audio");
        let view = cx.new(|cx| QuillApp::new_with_demo(window, cx, None, Some(demo)));
        *slot_in.borrow_mut() = Some(view.clone());
        let shell = cx.new(|_| QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let app: Entity<QuillApp> = slot.borrow().clone().expect("the app");
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    vcx.update(|window, _| window.activate_window());
    vcx.run_until_parked();
    (app, vcx)
}

/// Whether the window's accessibility tree has a node labelled `label`.
fn labelled(vcx: &mut VisualTestContext, label: &str) -> bool {
    let quoted = format!("{label:?}");
    vcx.update(|window, _| {
        let tree = window.a11y_tree_snapshot().expect("a tree");
        format!("{:?}", tree.nodes).contains(&quoted)
    })
}

/// Muting the Picture-in-Picture window shows on the player bar's speaker
/// with the song paused: the bar redraws for the change itself. It used to
/// wait for the progress tick, which a paused track no longer redraws on.
#[gpui_kit::test]
fn muting_the_pip_window_shows_on_a_paused_player_bar(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx);
    let vcx = &mut vcx;
    app.update(vcx, |app, cx| app.toggle_active_playback(cx));
    assert!(app.read_with(vcx, |app, _| !app.playback.is_playing()));
    vcx.update(|window, _| {
        window.set_a11y_forced(true);
        window.refresh();
    });
    let _ = frames(vcx, 5);
    let (mute, unmute) = (tr(keys::GROUP_CALL_CONTEXT_MUTE), tr(keys::Q_AV_UNMUTE));
    assert!(labelled(vcx, mute), "the speaker offers to mute");
    assert!(!labelled(vcx, unmute));

    let hooks = super::pip_hooks(app.downgrade());
    vcx.update(|_, cx| (hooks.volume)(0.0, cx));
    let _ = frames(vcx, 1);
    assert!(labelled(vcx, unmute), "the speaker shows the sound muted");
}
