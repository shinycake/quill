//! The call clock's 1 s tick redraws the call window's panel and the call
//! bar over the open chat, not the chat list, over the video call demo
//! (nothing reaches Telegram).
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};

fn frames(vcx: &mut VisualTestContext) {
    vcx.run_until_parked();
    for _ in 0..2 {
        vcx.update(|window, cx| window.render_frame(cx));
    }
}

fn open_call(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(800.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(
                window,
                cx,
                None,
                Some(ScreenshotDemo::named("ready-video-call")),
            )
        });
        *slot_in.borrow_mut() = Some(view.clone());
        let shell = cx.new(|_| QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let app: Entity<QuillApp> = slot.borrow().clone().expect("the app");
    let mut main = VisualTestContext::from_window(handle.into(), cx);
    frames(&mut main);
    app.update(&mut main, |app, cx| app.sync_call_window(cx));
    frames(&mut main);
    let window = app
        .read_with(&main, |app, _| app.calls.window)
        .expect("the call window opens");
    let mut call = VisualTestContext::from_window(window, cx);
    frames(&mut call);
    (app, main, call)
}

fn draw(vcx: &mut VisualTestContext) {
    vcx.run_until_parked();
    vcx.update(|window, cx| window.draw(cx).clear(cx));
}

#[gpui_kit::test]
fn the_clock_tick_redraws_the_panel_and_the_call_bar_only(cx: &mut TestAppContext) {
    let (app, mut main, mut call) = open_call(cx);
    let _ = crate::ui::frame_clock::take_slice_renders();
    app.update(&mut main, |app, cx| app.notify_call_clock(cx));
    draw(&mut main);
    draw(&mut call);
    let rendered = crate::ui::frame_clock::take_slice_renders();
    assert!(rendered.contains_key("call-panel"), "{rendered:?}");
    assert!(rendered.contains_key("conversation"), "{rendered:?}");
    for other in ["sidebar", "(app notified)"] {
        assert!(
            !rendered.contains_key(other),
            "{other} rendered: {rendered:?}"
        );
    }
}

#[gpui_kit::test]
fn closing_the_call_window_forgets_its_panel_view(cx: &mut TestAppContext) {
    let (app, mut main, _call) = open_call(cx);
    assert!(
        app.read_with(&main, |app, _| app.calls.panel_view.get().is_some()),
        "the open window registers its panel"
    );
    // The user closed this call's window: the sync closes it.
    app.update(&mut main, |app, cx| {
        let id = app
            .session()
            .and_then(|s| s.calls.active_call.as_ref().map(|c| c.id));
        app.calls.window_closed_by_user = id;
        app.sync_call_window(cx);
    });
    frames(&mut main);
    app.read_with(&main, |app, _| {
        assert!(app.calls.window.is_none());
        assert!(app.calls.panel_view.get().is_none(), "no dead panel id");
    });
    // The clock tick has nothing left to notify, and does not fail.
    app.update(&mut main, |app, cx| app.notify_call_clock(cx));
}

#[gpui_kit::test]
fn an_app_notify_still_redraws_the_call_panel(cx: &mut TestAppContext) {
    let (app, mut main, mut call) = open_call(cx);
    let _ = crate::ui::frame_clock::take_slice_renders();
    app.update(&mut main, |_, cx| cx.notify());
    draw(&mut main);
    draw(&mut call);
    let rendered = crate::ui::frame_clock::take_slice_renders();
    assert!(rendered.contains_key("call-panel"), "{rendered:?}");
    assert!(rendered.contains_key("(app notified)"), "{rendered:?}");
}
