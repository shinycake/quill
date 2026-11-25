#![cfg(test)]
#![cfg(feature = "demo-capture")]
//! The first-run onboarding in the UI (needs the `demo-capture` feature
//! for gpui-kit's test support): the notice leads to the highlights, the
//! highlights to the sign-in screen, and finishing is remembered; the
//! replay from Settings › Advanced plays it over the signed-in app.

use crate::ui::app::QuillApp;
use crate::ui::first_run_state::FirstRun;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, Modifiers, TestAppContext, VisualTestContext, px, size};
use quill::layout_style::LayoutStyle;

fn new_app(cx: &mut TestAppContext, demo: &'static str) -> (Entity<QuillApp>, VisualTestContext) {
    let root = quill::scratch::process_dir("quill-first-run-ui");
    quill::settings::use_isolated_app_root(root);
    cx.update(gpui_kit::init);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(800.)), move |window, cx| {
        let view = cx
            .new(|cx| QuillApp::new_with_demo(window, cx, None, Some(ScreenshotDemo::named(demo))));
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
    let (app, mut vcx) = new_app(cx, "first-run-notice");
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
    assert!(vcx.debug_bounds("first-run-start").is_none());

    click(&mut vcx, "first-run-continue");
    vcx.read(|cx| assert!(app.read(cx).first_run.in_style()));
    click(&mut vcx, "first-run-start");
    vcx.read(|cx| assert!(matches!(app.read(cx).first_run, FirstRun::Done)));
    assert!(vcx.debug_bounds("first-run-start").is_none());
    let root = quill::settings::safe_app_root().expect("isolated root");
    assert!(
        !quill::settings::first_run_pending(&root),
        "finishing is remembered"
    );
}

#[gpui_kit::test]
fn the_replay_from_settings_plays_over_the_signed_in_app(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-settings-advanced");
    frames(&mut vcx);
    assert!(vcx.debug_bounds("first-run-understand").is_none());
    vcx.update(|window, cx| app.update(cx, |app, cx| app.replay_first_run(window, cx)));
    frames(&mut vcx);
    vcx.read(|cx| {
        let app = app.read(cx);
        assert!(app.first_run.in_welcome());
        assert!(!app.settings.open(), "the settings dialog closes");
    });
    // Without its clip the notice shows at once; then the highlights.
    vcx.update(|_, cx| {
        app.update(cx, |app, _| {
            app.first_run = FirstRun::welcome(None, std::time::Instant::now());
        });
    });
    frames(&mut vcx);
    click(&mut vcx, "first-run-understand");
    click(&mut vcx, "first-run-continue");
    click(&mut vcx, "first-run-start");
    vcx.read(|cx| assert!(matches!(app.read(cx).first_run, FirstRun::Done)));
    assert!(vcx.debug_bounds("first-run-start").is_none());
}

/// The layout card writes the pref the Settings card writes and stays up;
/// Escape finishes onboarding with the pref as it was.
#[gpui_kit::test]
fn the_style_card_picks_the_layout_and_escape_finishes(cx: &mut TestAppContext) {
    let _prefs = crate::ui::appearance::own_appearance_prefs("quill-first-run-style");
    let (app, mut vcx) = new_app(cx, "first-run-style");
    frames(&mut vcx);
    vcx.read(|cx| assert!(app.read(cx).first_run.in_style()));
    click(&mut vcx, "layout-option-floating");
    vcx.read(|cx| {
        let app = app.read(cx);
        assert_eq!(app.appearance.layout, LayoutStyle::Floating);
        assert!(app.first_run.in_style(), "picking does not leave the card");
    });
    assert_eq!(QuillApp::load_appearance().layout, LayoutStyle::Floating);
    // No wallpaper is set: the card offers one.
    assert!(vcx.debug_bounds("layout-wallpaper-offer").is_some());

    // Escape: the topmost layer is the card.
    let dismissed = app.update_in(&mut vcx, |app, window, cx| {
        app.dismiss_topmost_layer(window, cx)
    });
    assert!(dismissed);
    frames(&mut vcx);
    vcx.read(|cx| {
        let app = app.read(cx);
        assert!(matches!(app.first_run, FirstRun::Done));
        assert_eq!(app.appearance.layout, LayoutStyle::Floating);
    });
}
