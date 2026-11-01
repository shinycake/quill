//! An HDR picture file in the viewer switches the window to extended
//! range while it shows upright, and back when it is rotated or closed
//! (needs the `demo-capture` feature for gpui-kit's test support).
use super::{HdrBadge, Headroom, set_test_headroom, set_test_renderer_left_edr};
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::ids::{ChatId, MessageId};

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
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
                Some(ScreenshotDemo::named("ready-hdr-files")),
            )
        });
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some(view);
        Root::new(shell, window, cx)
    });
    let Some(app) = slot.borrow().clone() else {
        panic!("the window builds the app");
    };
    let vcx = VisualTestContext::from_window(handle.into(), cx);
    (app, vcx)
}

/// The demo's sunset is an ISO gain-map HEIC, which ImageIO reads from
/// macOS 15 on.
fn reads_iso_gain_maps() -> bool {
    objc2_foundation::NSProcessInfo::processInfo().isOperatingSystemAtLeastVersion(
        objc2_foundation::NSOperatingSystemVersion {
            majorVersion: 15,
            minorVersion: 0,
            patchVersion: 0,
        },
    )
}

fn open_sunset(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) {
    app.update_in(vcx, |app, _, cx| {
        assert!(app.open_file_viewer(ChatId(11), MessageId(904), cx));
    });
    // Render, decode in the background, render the frame.
    vcx.run_until_parked();
    app.update_in(vcx, |_, _, cx| cx.notify());
    vcx.run_until_parked();
}

#[gpui_kit::test]
fn an_hdr_file_shows_in_extended_range_while_upright(cx: &mut TestAppContext) {
    if !reads_iso_gain_maps() {
        return;
    }
    set_test_headroom(Some(Headroom {
        current: 4.0,
        potential: 8.0,
    }));
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    open_sunset(&app, vcx);
    app.read_with(vcx, |app, _| {
        assert_eq!(app.viewer_hdr_badge(), Some(HdrBadge::Shown));
        assert!(app.viewer.hdr.edr_on());
    });
    // A rotated picture draws the SDR image.
    app.update_in(vcx, |app, _, cx| app.rotate_viewer_photo(cx));
    vcx.run_until_parked();
    app.read_with(vcx, |app, _| {
        assert_eq!(app.viewer_hdr_badge(), Some(HdrBadge::StandardRange));
        assert!(!app.viewer.hdr.edr_on());
    });
    // Closing releases extended range.
    app.update_in(vcx, |app, _, cx| {
        app.viewer.orientation = Default::default();
        cx.notify();
    });
    vcx.run_until_parked();
    app.read_with(vcx, |app, _| assert!(app.viewer.hdr.edr_on()));
    app.update_in(vcx, |app, _, cx| app.close_media_viewer(cx));
    vcx.run_until_parked();
    app.read_with(vcx, |app, _| {
        assert!(!app.viewer.state.is_open());
        assert!(!app.viewer.hdr.edr_on());
    });
    set_test_headroom(None);
}

#[gpui_kit::test]
fn an_sdr_display_shows_the_standard_range_picture(cx: &mut TestAppContext) {
    if !reads_iso_gain_maps() {
        return;
    }
    set_test_headroom(Some(Headroom::SDR));
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    open_sunset(&app, vcx);
    app.read_with(vcx, |app, _| {
        assert_eq!(app.viewer_hdr_badge(), Some(HdrBadge::StandardRange));
        assert!(!app.viewer.hdr.edr_on());
    });
    set_test_headroom(None);
}

#[gpui_kit::test]
fn the_tag_says_standard_range_once_the_renderer_left_extended_range(cx: &mut TestAppContext) {
    if !reads_iso_gain_maps() {
        return;
    }
    set_test_headroom(Some(Headroom {
        current: 4.0,
        potential: 8.0,
    }));
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    open_sunset(&app, vcx);
    app.read_with(vcx, |app, _| {
        assert_eq!(app.viewer_hdr_badge(), Some(HdrBadge::Shown));
    });
    set_test_renderer_left_edr(true);
    app.update_in(vcx, |_, _, cx| cx.notify());
    vcx.run_until_parked();
    app.read_with(vcx, |app, _| {
        assert_eq!(app.viewer_hdr_badge(), Some(HdrBadge::StandardRange));
    });
    set_test_renderer_left_edr(false);
    set_test_headroom(None);
}
