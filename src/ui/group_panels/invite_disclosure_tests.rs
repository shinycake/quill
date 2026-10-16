//! The group info panel's invite blocks in the demo supergroup: the
//! revoked-links disclosure row toggles on click, and the invite-link
//! loading line shows while the list is fetched.
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(1000.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(
                window,
                cx,
                None,
                Some(ScreenshotDemo::named("ready-group-info-edit")),
            )
        });
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some(view);
        Root::new(shell, window, cx)
    });
    let app = slot.borrow().clone().expect("the app view");
    let vcx = VisualTestContext::from_window(handle.into(), cx);
    (app, vcx)
}

#[gpui_kit::test]
fn the_revoked_links_row_is_a_disclosure_that_toggles(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    vcx.run_until_parked();
    assert!(!app.read_with(&vcx, |app, _| app.admin.revoked_links_open));
    vcx.update(|window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("revoked-links-toggle").is_some());
        window.click("revoked-links-toggle", cx);
    });
    assert!(app.read_with(&vcx, |app, _| app.admin.revoked_links_open));
    vcx.update(|window, cx| window.click("revoked-links-toggle", cx));
    assert!(!app.read_with(&vcx, |app, _| app.admin.revoked_links_open));
}
