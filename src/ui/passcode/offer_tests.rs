//! The Touch ID offer on the lock screen waits until Quill is the app in
//! front: it would otherwise open the system prompt over whatever else is
//! active.
use crate::ui::app::QuillApp;
use crate::ui::passcode::lock_screen::LockScreen;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};

fn open_locked(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let demo = ScreenshotDemo::named("ready-chats");
    let handle = cx.open_window(size(px(1100.), px(700.)), move |window, cx| {
        let view = cx.new(|cx| QuillApp::new_with_demo(window, cx, None, Some(demo)));
        *slot_in.borrow_mut() = Some(view.clone());
        let shell = cx.new(|_| QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let app: Entity<QuillApp> = slot.borrow().clone().expect("demo app");
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    app.update(&mut vcx, |app, cx| {
        app.account.passcode.enabled = true;
        app.lock_by_passcode(cx);
        // Locked with the system unlock on: the offer is due.
        app.account.passcode.screen = LockScreen::OfferSystemUnlock;
    });
    (app, vcx)
}

fn screen(app: &Entity<QuillApp>, vcx: &VisualTestContext) -> LockScreen {
    app.read_with(vcx, |app, _| app.account.passcode.screen)
}

#[gpui_kit::test]
fn the_system_offer_waits_for_the_window_to_be_active(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_locked(cx);
    let vcx = &mut vcx;
    vcx.deactivate_window();
    assert!(!vcx.update(|window, _| window.is_window_active()));
    app.update_in(vcx, |app, window, cx| app.passcode_frame(window, cx));
    assert_eq!(
        screen(&app, vcx),
        LockScreen::OfferSystemUnlock,
        "an inactive window keeps the offer"
    );
    vcx.update(|window, _| window.activate_window());
    vcx.run_until_parked();
    assert!(vcx.update(|window, _| window.is_window_active()));
    app.update_in(vcx, |app, window, cx| app.passcode_frame(window, cx));
    assert_ne!(
        screen(&app, vcx),
        LockScreen::OfferSystemUnlock,
        "the first active frame takes it"
    );
}
