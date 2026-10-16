//! The stream volume slider over the screenshot demo's live story: a
//! move writes the media viewer's volume, and the slider follows the
//! volume when something else changes it. Nothing reaches Telegram.
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::component::slider::{SliderEvent, SliderState, SliderValue};
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};

fn frames(vcx: &mut VisualTestContext) {
    vcx.run_until_parked();
    for _ in 0..2 {
        vcx.update(|window, cx| window.render_frame(cx));
    }
}

fn open_live_story(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(900.)), move |window, cx| {
        let demo = ScreenshotDemo::named("ready-live-story");
        let view = cx.new(|cx| QuillApp::new_with_demo(window, cx, None, Some(demo)));
        *slot_in.borrow_mut() = Some(view.clone());
        let shell = cx.new(|_| QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let app: Entity<QuillApp> = slot.borrow().clone().expect("the app");
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    frames(&mut vcx);
    (app, vcx)
}

fn slider(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> Entity<SliderState> {
    app.read_with(vcx, |app, _| app.stories.live.volume.clone())
        .expect("the header's volume slider")
}

fn volume(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> f32 {
    app.read_with(vcx, |app, _| app.playback.volume)
}

#[gpui_kit::test]
fn moving_the_slider_writes_the_media_viewer_volume(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_live_story(cx);
    let vcx = &mut vcx;
    let slider = slider(&app, vcx);
    assert_eq!(
        slider.read_with(vcx, |s, _| s.value()),
        SliderValue::Single(100.)
    );
    slider.update(vcx, |_, cx| {
        cx.emit(SliderEvent::Change(SliderValue::Single(40.)))
    });
    frames(vcx);
    assert!(
        (volume(&app, vcx) - 0.4).abs() < 1e-6,
        "applies while dragging"
    );
    slider.update(vcx, |_, cx| {
        cx.emit(SliderEvent::Release(SliderValue::Single(35.)))
    });
    frames(vcx);
    assert!((volume(&app, vcx) - 0.35).abs() < 1e-6);
}

#[gpui_kit::test]
fn the_slider_follows_the_volume_and_steps_by_five(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_live_story(cx);
    let vcx = &mut vcx;
    app.update(vcx, |app, cx| app.set_playback_volume(0.62, cx));
    frames(vcx);
    let shown = slider(&app, vcx).read_with(vcx, |s, _| s.value());
    assert_eq!(shown, SliderValue::Single(62.), "rebuilt from the setting");
    app.update(vcx, |app, cx| {
        let call = app
            .session()
            .and_then(|s| s.calls.story_stream.as_ref())
            .map(|c| c.id);
        app.step_stream_volume(true, call, cx);
    });
    frames(vcx);
    assert!(
        (volume(&app, vcx) - 0.65).abs() < 1e-6,
        "Up lands on the 5 % grid"
    );
    app.update(vcx, |app, cx| {
        app.set_playback_volume(1.0, cx);
        app.step_stream_volume(true, None, cx);
    });
    assert!((volume(&app, vcx) - 1.0).abs() < 1e-6, "100 % is the top");
}
