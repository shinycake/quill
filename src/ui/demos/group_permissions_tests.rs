//! The permissions box and the "Add exception" picker over the offline
//! demo groups (needs the `demo-capture` feature for gpui-kit's test
//! support). The demo has no connection: Save writes into the demo
//! session, and the recorded requests are checked in the driver tests
//! (`connect/tests/permissions_box.rs`).
use super::GROUP;
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::scheduled::picked_unix;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use crate::ui::wall_clock::WallClock;
use gpui_kit::component::Root;
use gpui_kit::{AppContext, Entity, Modifiers, TestAppContext, VisualTestContext, px, size};
use quill::add_special::SpecialConfirm;
use quill::telegram::envelope::ChatPermissions;

fn new_app(cx: &mut TestAppContext, demo: &str) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let demo = demo.to_string();
    let handle = cx.open_window(size(px(1200.), px(1000.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(window, cx, None, Some(ScreenshotDemo::named(&demo)))
        });
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some(view);
        Root::new(shell, window, cx)
    });
    let app = slot.borrow().clone().expect("the app view");
    let vcx = VisualTestContext::from_window(handle.into(), cx);
    vcx.run_until_parked();
    (app, vcx)
}

fn click(vcx: &mut VisualTestContext, id: &'static str) {
    let bounds = vcx
        .debug_bounds(id)
        .unwrap_or_else(|| panic!("{id} is on screen"));
    vcx.simulate_click(bounds.center(), Modifiers::default());
    vcx.run_until_parked();
}

/// Click the left end of a row (a checkbox's box sits there).
fn click_start(vcx: &mut VisualTestContext, id: &'static str) {
    let bounds = vcx
        .debug_bounds(id)
        .unwrap_or_else(|| panic!("{id} is on screen"));
    let point = gpui_kit::point(bounds.origin.x + px(8.), bounds.center().y);
    vcx.simulate_click(point, Modifiers::default());
    vcx.run_until_parked();
}

fn settle_animation(vcx: &mut VisualTestContext) {
    for _ in 0..40 {
        vcx.executor()
            .advance_clock(std::time::Duration::from_millis(16));
        vcx.update(|window, _| window.refresh());
        vcx.run_until_parked();
    }
}

fn saved(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> ChatPermissions {
    app.read_with(vcx, |app, _| {
        app.session()
            .and_then(|session| session.chats.get(&GROUP))
            .and_then(|chat| chat.permissions)
            .unwrap_or_default()
    })
}

#[gpui_kit::test]
fn a_media_permission_is_toggled_and_saved(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "group-permissions");
    let vcx = &mut vcx;
    assert!(!saved(&app, vcx).can_send_photos);
    click(vcx, "permission-media-expand");
    app.read_with(vcx, |app, _| {
        assert!(
            app.admin
                .permissions_dialog
                .as_ref()
                .is_some_and(|d| d.media_open)
        );
    });
    settle_animation(vcx);
    click_start(vcx, "permission-photos");
    app.read_with(vcx, |app, _| {
        let staged = app.admin.permissions_dialog.as_ref().map(|d| d.permissions);
        assert!(staged.is_some_and(|p| p.can_send_photos), "staged");
    });
    assert!(!saved(&app, vcx).can_send_photos);
    click(vcx, "permissions-save");
    let permissions = saved(&app, vcx);
    assert!(permissions.can_send_photos);
    assert!(!permissions.can_send_polls, "the other rows are kept");
    app.read_with(vcx, |app, _| {
        assert!(app.admin.permissions_dialog.is_none());
        let session = app.session().expect("demo session");
        assert_eq!(session.chat_slow_mode_delay(quill::ids::ChatId(GROUP)), 30);
    });
}

#[gpui_kit::test]
fn adding_an_exception_opens_the_restrict_box_after_the_admin_question(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "group-add-exception");
    let vcx = &mut vcx;
    click(vcx, "add-special-1");
    app.read_with(vcx, |app, _| {
        let picker = app.admin.add_special.as_ref().expect("the picker");
        assert_eq!(picker.confirm, Some((1, SpecialConfirm::SureBanAdmin)));
        assert!(app.admin.restrict_dialog.is_none());
    });
    click(vcx, "add-special-back");
    click(vcx, "add-special-5");
    app.read_with(vcx, |app, _| {
        let restrict = app
            .admin
            .restrict_dialog
            .as_ref()
            .expect("the restrict box");
        assert_eq!(restrict.user_id, 5);
        assert!(!restrict.ban);
        assert!(app.admin.add_special.is_some());
    });
}

#[gpui_kit::test]
fn the_custom_end_opens_a_day_after_the_installed_clock(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "group-add-exception");
    let vcx = &mut vcx;
    let clock = WallClock::chicago_repeated_hour();
    let tomorrow = clock.now() + 86_400;
    vcx.update(|_, cx| cx.set_global(clock));
    click(vcx, "add-special-5");
    app.read_with(vcx, |app, cx| {
        let restrict = app
            .admin
            .restrict_dialog
            .as_ref()
            .expect("the restrict box");
        assert_eq!(picked_unix(&restrict.custom, cx), Some(tomorrow));
    });
}
