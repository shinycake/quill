#![cfg(test)]
#![cfg(feature = "demo-capture")]
//! The first-run onboarding in the UI (needs the `demo-capture` feature
//! for gpui-kit's test support): the notice leads to the highlights, the
//! highlights to the sign-in screen, and finishing is remembered.

use crate::ui::app::QuillApp;
use crate::ui::first_run_state::FirstRun;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, Modifiers, TestAppContext, VisualTestContext, px, size};

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    let root = std::env::temp_dir().join(format!("quill-first-run-ui-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&root);
    quill::settings::use_isolated_app_root(root);
    cx.update(gpui_kit::init);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(800.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(
                window,
                cx,
                None,
                Some(ScreenshotDemo::named("first-run-notice")),
            )
        });
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some(view);
        Root::new(shell, window, cx)
    });
    let app = slot.borrow().clone().expect("app view");
    let vcx = VisualTestContext::from_window(handle.into(), cx);
    (app, vcx)
}

fn frames(vcx: &mut VisualTestContext) {
    vcx.run_until_parked();
    for _ in 0..2 {
        vcx.update(|window, cx| window.render_frame(cx));
    }
}

fn click(vcx: &mut VisualTestContext, selector: &'static str) {
    let bounds = vcx.debug_bounds(selector).expect(selector);
    vcx.simulate_click(bounds.center(), Modifiers::default());
    frames(vcx);
}

#[gpui_kit::test]
fn the_notice_then_the_highlights_lead_to_sign_in(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    frames(&mut vcx);
    assert!(vcx.debug_bounds("first-run-start").is_none());
    click(&mut vcx, "first-run-understand");
    vcx.read(|cx| {
        assert!(matches!(
            app.read(cx).first_run,
            FirstRun::Highlights { .. }
        ));
    });
    assert!(vcx.debug_bounds("first-run-understand").is_none());

    click(&mut vcx, "first-run-start");
    vcx.read(|cx| assert!(matches!(app.read(cx).first_run, FirstRun::Done)));
    assert!(vcx.debug_bounds("first-run-start").is_none());
    let root = quill::settings::safe_app_root().expect("isolated root");
    assert!(
        !quill::settings::first_run_pending(&root),
        "finishing is remembered"
    );
}
