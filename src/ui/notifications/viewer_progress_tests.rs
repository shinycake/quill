//! Download progress of the open chat's media while the media viewer shows
//! it: the viewer's "Downloading N%" follows each update.
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::state::RedrawNeed;

fn open_app(cx: &mut TestAppContext, demo: &str) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let demo = ScreenshotDemo::named(demo);
    let handle = cx.open_window(size(px(1100.), px(800.)), move |window, cx| {
        let view = cx.new(|cx| QuillApp::new_with_demo(window, cx, None, Some(demo)));
        *slot_in.borrow_mut() = Some(view.clone());
        let shell = cx.new(|_| QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let app = slot.borrow().clone();
    let Some(app): Option<Entity<QuillApp>> = app else {
        panic!("the window builder did not run");
    };
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    vcx.update(|window, cx| window.render_frame(cx));
    vcx.run_until_parked();
    (app, vcx)
}

fn renders_after_progress(
    app: &Entity<QuillApp>,
    vcx: &mut VisualTestContext,
) -> std::collections::BTreeMap<&'static str, u32> {
    let _ = crate::ui::frame_clock::take_slice_renders();
    app.update(vcx, |app, cx| {
        app.frame.window_active.set(true);
        app.redraw_polled(RedrawNeed::Conversation, false, cx);
    });
    vcx.run_until_parked();
    vcx.update(|window, cx| window.draw(cx).clear(cx));
    crate::ui::frame_clock::take_slice_renders()
}

#[gpui_kit::test]
fn the_viewer_follows_the_progress_of_the_download_it_shows(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_app(cx, "ready-status-toast-viewer");
    assert!(app.read_with(&vcx, |app, _| app.viewer.state.is_open()));
    let rendered = renders_after_progress(&app, &mut vcx);
    assert!(rendered.contains_key("(app notified)"), "{rendered:?}");
}

#[gpui_kit::test]
fn without_the_viewer_progress_redraws_only_the_conversation(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_app(cx, "ready-voice-playing");
    let rendered = renders_after_progress(&app, &mut vcx);
    assert!(rendered.contains_key("conversation"), "{rendered:?}");
    assert!(!rendered.contains_key("(app notified)"), "{rendered:?}");
}
