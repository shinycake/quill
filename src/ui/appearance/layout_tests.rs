//! The layout choice in the UI: the Appearance picker saves it, every
//! window reads its chrome from its own width, and a narrow window draws
//! Classic whatever was chosen.

use super::tile_selector;
use crate::ui::app::QuillApp;
use crate::ui::appearance::own_appearance_prefs;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::layout_style::{Chrome, FLOATING, LayoutStyle};
use quill::settings_catalog::Page;

/// A tile is a radio's content, found by its debug selector.
fn click_tile(vcx: &mut VisualTestContext, style: LayoutStyle) {
    let bounds = vcx.debug_bounds(tile_selector(style)).expect("tile");
    vcx.simulate_click(bounds.center(), gpui_kit::Modifiers::default());
}

fn new_app(cx: &mut TestAppContext, width: f32) -> (Entity<QuillApp>, VisualTestContext) {
    quill::settings::use_isolated_app_root(quill::scratch::process_dir("quill-layout-ui"));
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    crate::ui::interface_zoom::set_initial_zoom(1.);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let demo = ScreenshotDemo::named("ready-chats");
    // Tall enough for the Appearance page down to the layout row.
    let handle = cx.open_window(size(px(width), px(1900.)), move |window, cx| {
        let view = cx.new(|cx| QuillApp::new_with_demo(window, cx, None, Some(demo)));
        *slot_in.borrow_mut() = Some(view.clone());
        let shell = cx.new(|_| QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let app: Entity<QuillApp> = slot.borrow().clone().expect("app");
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    frames(&mut vcx);
    (app, vcx)
}

fn frames(vcx: &mut VisualTestContext) {
    vcx.run_until_parked();
    for _ in 0..3 {
        vcx.update(|window, cx| window.render_frame(cx));
    }
}

fn chrome(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> Chrome {
    app.read_with(vcx, |app, cx| app.chrome(cx))
}

#[gpui_kit::test]
fn the_picker_saves_the_layout_and_the_window_follows_it(cx: &mut TestAppContext) {
    let _prefs = own_appearance_prefs("quill-layout-picker");
    let (app, mut vcx) = new_app(cx, 1200.);
    let vcx = &mut vcx;
    assert_eq!(chrome(&app, vcx), Chrome::CLASSIC);

    app.update(vcx, |app, cx| app.open_settings(Some(Page::Appearance), cx));
    frames(vcx);
    click_tile(vcx, LayoutStyle::Floating);
    frames(vcx);
    assert_eq!(QuillApp::load_appearance().layout, LayoutStyle::Floating);
    assert_eq!(chrome(&app, vcx).metrics, &FLOATING);

    click_tile(vcx, LayoutStyle::Classic);
    frames(vcx);
    assert_eq!(QuillApp::load_appearance().layout, LayoutStyle::Classic);
    assert_eq!(chrome(&app, vcx), Chrome::CLASSIC);
}

#[gpui_kit::test]
fn a_window_of_600_or_under_draws_classic_with_floating_chosen(cx: &mut TestAppContext) {
    let _prefs = own_appearance_prefs("quill-layout-narrow");
    let (app, mut vcx) = new_app(cx, 600.);
    let vcx = &mut vcx;
    app.update(vcx, |app, cx| {
        app.set_appearance(cx, |appearance| appearance.layout = LayoutStyle::Floating);
    });
    frames(vcx);
    assert_eq!(chrome(&app, vcx), Chrome::CLASSIC);

    // Each window answers from its own width: the cell the render wrote.
    app.update(vcx, |app, _| app.slices.window.width.set(px(1200.)));
    assert_eq!(chrome(&app, vcx).metrics, &FLOATING);
    app.update(vcx, |app, _| app.slices.window.width.set(px(601.)));
    assert_eq!(chrome(&app, vcx).metrics, &FLOATING);
    app.update(vcx, |app, _| app.slices.window.width.set(px(600.)));
    assert_eq!(chrome(&app, vcx), Chrome::CLASSIC);
}

/// A layout change goes through the step that lays the open lists out
/// again (`appearance_layout_changed`): Floating's rows have other sizes.
#[test]
fn the_layout_is_a_row_layout_setting() {
    use crate::ui::appearance::density::RowLayout;
    let classic = quill::settings::AppearancePrefs::default();
    let floating = quill::settings::AppearancePrefs {
        layout: LayoutStyle::Floating,
        ..classic.clone()
    };
    assert!(RowLayout::of(&classic) != RowLayout::of(&floating));
}

/// The wallpaper offer: only with Floating chosen and no wallpaper at all;
/// pressing it sets the account's wallpaper and the row goes; Classic has
/// none. No state of its own.
#[gpui_kit::test]
fn floating_without_a_wallpaper_offers_one(cx: &mut TestAppContext) {
    let _prefs = own_appearance_prefs("quill-layout-offer");
    let (app, mut vcx) = new_app(cx, 1200.);
    let vcx = &mut vcx;
    app.update(vcx, |app, cx| app.open_settings(Some(Page::Appearance), cx));
    frames(vcx);
    assert!(
        vcx.debug_bounds("layout-wallpaper-offer").is_none(),
        "Classic"
    );

    click_tile(vcx, LayoutStyle::Floating);
    frames(vcx);
    let offer = vcx.debug_bounds("layout-wallpaper-offer").expect("offered");
    let picker = vcx
        .debug_bounds(tile_selector(LayoutStyle::Floating))
        .expect("picker");
    assert!(offer.top() > picker.top(), "under the picker");

    vcx.update(|window, cx| window.click("layout-wallpaper-use", cx));
    frames(vcx);
    assert_eq!(
        QuillApp::load_appearance().wallpaper,
        quill::settings::WallpaperSource::Telegram
    );
    assert!(
        vcx.debug_bounds("layout-wallpaper-offer").is_none(),
        "taken"
    );

    // A wallpaper chosen another way hides it too; back to the theme's
    // and Classic, it stays hidden.
    app.update(vcx, |app, cx| {
        app.set_appearance(cx, |a| {
            a.wallpaper = quill::settings::WallpaperSource::Theme
        });
    });
    frames(vcx);
    assert!(vcx.debug_bounds("layout-wallpaper-offer").is_some());
    click_tile(vcx, LayoutStyle::Classic);
    frames(vcx);
    assert!(vcx.debug_bounds("layout-wallpaper-offer").is_none());
}
