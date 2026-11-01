//! Settings > Calls' microphone test while the device opens off the UI
//! thread (needs the `demo-capture` feature for gpui-kit's test support).
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use gpui_kit::component::Root;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::calls::level_tap::LevelSource;

use super::MicTestRun;
use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;

/// A microphone that counts how many times it is open.
struct Microphone(Arc<AtomicUsize>);

impl LevelSource for Microphone {
    fn take_level(&self) -> Option<f32> {
        Some(0.5)
    }

    fn error(&self) -> Option<String> {
        None
    }
}

impl Drop for Microphone {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

fn settings_with_a_microphone(
    cx: &mut TestAppContext,
) -> (Entity<QuillApp>, VisualTestContext, Arc<AtomicUsize>) {
    cx.update(gpui_kit::init);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(800.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(window, cx, None, Some(ScreenshotDemo::named("ready-chats")))
        });
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some(view);
        Root::new(shell, window, cx)
    });
    let app: Entity<QuillApp> = slot.borrow().clone().expect("demo app");
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    let open = Arc::new(AtomicUsize::new(0));
    let counted = open.clone();
    app.update(&mut vcx, |app, _| {
        app.calls.open_mic_test = Arc::new(move || {
            counted.fetch_add(1, Ordering::SeqCst);
            Ok(Box::new(Microphone(counted.clone())) as Box<dyn LevelSource + Send>)
        });
    });
    (app, vcx, open)
}

fn testing(app: &Entity<QuillApp>, vcx: &VisualTestContext) -> bool {
    app.read_with(vcx, |app, _| !matches!(app.calls.mic_test, MicTestRun::Off))
}

#[gpui_kit::test]
fn the_test_shows_as_running_from_the_click(cx: &mut TestAppContext) {
    let (app, mut vcx, open) = settings_with_a_microphone(cx);
    app.update(&mut vcx, |app, cx| app.toggle_mic_test(cx));
    assert!(testing(&app, &vcx), "the button reads Stop test at once");
    vcx.run_until_parked();
    assert!(testing(&app, &vcx));
    assert_eq!(open.load(Ordering::SeqCst), 1, "the microphone is on");
}

#[gpui_kit::test]
fn stopping_the_test_while_the_microphone_opens_leaves_it_off(cx: &mut TestAppContext) {
    let (app, mut vcx, open) = settings_with_a_microphone(cx);
    app.update(&mut vcx, |app, cx| app.toggle_mic_test(cx));
    app.update(&mut vcx, |app, cx| app.toggle_mic_test(cx));
    assert!(
        !testing(&app, &vcx),
        "the button reads Test microphone again"
    );
    vcx.run_until_parked();
    assert!(
        !testing(&app, &vcx),
        "the late device did not start the test"
    );
    assert_eq!(open.load(Ordering::SeqCst), 0, "the microphone was closed");
}

#[gpui_kit::test]
fn closing_settings_while_the_microphone_opens_leaves_it_off(cx: &mut TestAppContext) {
    let (app, mut vcx, open) = settings_with_a_microphone(cx);
    app.update(&mut vcx, |app, cx| {
        app.open_settings(Some(quill::settings_catalog::Page::Calls));
        app.toggle_mic_test(cx);
        app.close_settings();
    });
    vcx.run_until_parked();
    assert!(!testing(&app, &vcx));
    assert_eq!(open.load(Ordering::SeqCst), 0, "the microphone was closed");
}
