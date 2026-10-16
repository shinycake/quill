//! The "Glass effects" row: it sits in the Appearance page where the
//! platform has glass, follows the saved choice, and is locked off with a
//! note while the system's Reduce transparency is on.

use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::ScreenshotDemo;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AppContext, Context, Entity, IntoElement, Modifiers, ParentElement, Render, Styled,
    TestAppContext, VisualTestContext, Window, div, px, size,
};
use quill::glass::OsTransparency;

/// Renders the row of a demo app, on or off the Mac as `supported` says.
struct Host {
    app: Entity<QuillApp>,
    supported: bool,
}

impl Render for Host {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let supported = self.supported;
        let row = self
            .app
            .update(cx, |app, cx| app.glass_section(supported, cx));
        div().size_full().p(px(20.)).children(row)
    }
}

fn open_row(cx: &mut TestAppContext, supported: bool) -> (Entity<QuillApp>, VisualTestContext) {
    let root = std::env::temp_dir().join(format!("quill-glass-ui-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&root);
    quill::settings::use_isolated_app_root(root);
    cx.update(gpui_kit::init);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(600.), px(200.)), move |window, cx| {
        let app = cx.new(|cx| {
            QuillApp::new_with_demo(window, cx, None, Some(ScreenshotDemo::named("ready-chats")))
        });
        *slot_in.borrow_mut() = Some(app.clone());
        let host = cx.new(|_| Host { app, supported });
        Root::new(host, window, cx)
    });
    let app = slot.borrow().clone().unwrap();
    let vcx = VisualTestContext::from_window(handle.into(), cx);
    vcx.run_until_parked();
    (app, vcx)
}

fn draw(vcx: &mut VisualTestContext) {
    vcx.run_until_parked();
    vcx.update(|window, cx| {
        window.refresh();
        window.render_frame(cx);
    });
}

fn click_switch(vcx: &mut VisualTestContext) {
    let switch = vcx
        .debug_bounds("glass-effects-switch")
        .expect("the switch is drawn");
    vcx.simulate_click(switch.center(), Modifiers::default());
    vcx.run_until_parked();
}

fn set_glass_pref(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, on: bool) {
    app.update(vcx, |app, _| app.appearance.glass_effects = on);
}

fn glass_pref(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> bool {
    app.update(vcx, |app, _| app.appearance.glass_effects)
}

#[gpui_kit::test]
fn the_row_does_not_exist_where_the_platform_has_no_glass(cx: &mut TestAppContext) {
    let (_app, mut vcx) = open_row(cx, false);
    draw(&mut vcx);
    assert!(vcx.debug_bounds("glass-effects-switch").is_none());
    assert!(vcx.debug_bounds("glass-effects-hint").is_none());
    assert!(vcx.debug_bounds("glass-effects-note").is_none());
}

#[gpui_kit::test]
fn the_switch_changes_the_saved_choice(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_row(cx, true);
    quill::glass::isolate_for_tests();
    quill::glass::set_os(OsTransparency::default());
    set_glass_pref(&app, &mut vcx, true);
    draw(&mut vcx);
    assert!(glass_pref(&app, &mut vcx));
    click_switch(&mut vcx);
    assert!(!glass_pref(&app, &mut vcx));
    click_switch(&mut vcx);
    assert!(glass_pref(&app, &mut vcx));
}

#[gpui_kit::test]
fn reduce_transparency_locks_the_switch_off_and_says_why(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_row(cx, true);
    quill::glass::isolate_for_tests();
    quill::glass::set_os(OsTransparency::default());
    set_glass_pref(&app, &mut vcx, true);
    draw(&mut vcx);
    assert!(vcx.debug_bounds("glass-effects-hint").is_some());
    assert!(vcx.debug_bounds("glass-effects-note").is_none());

    quill::glass::set_os(OsTransparency {
        reduce_transparency: true,
        increase_contrast: false,
    });
    draw(&mut vcx);
    assert!(
        vcx.debug_bounds("glass-effects-note").is_some(),
        "the note explains the system setting"
    );
    assert!(vcx.debug_bounds("glass-effects-hint").is_none());
    assert!(glass_pref(&app, &mut vcx), "the saved choice is untouched");
    click_switch(&mut vcx);
    assert!(
        glass_pref(&app, &mut vcx),
        "a locked switch ignores the click"
    );

    quill::glass::set_os(OsTransparency::default());
    draw(&mut vcx);
    click_switch(&mut vcx);
    assert!(!glass_pref(&app, &mut vcx), "unlocked again, it works");
}
