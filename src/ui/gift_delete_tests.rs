//! Delete Gift over the demo's gifts dialog: the entry is there only for a
//! gift TDLib lets the owner delete, it asks first, and confirming removes
//! the gift (needs the `demo-capture` feature for gpui-kit's test support).
//! The demo has no connection, so the recorded-request check lives in the
//! driver tests (`connect/tests/gift_manage.rs`).
use super::{DELETABLE, KEPT};
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::{AppContext, Entity, Modifiers, TestAppContext, VisualTestContext, px, size};

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    quill::gift_market::set_gift_market_persistence(false);
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
                Some(ScreenshotDemo::named("ready-gift-delete")),
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

fn select(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, id: &str) {
    app.update_in(vcx, |app, _, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            session.payments.hub.gift_selected = Some(id.into());
        }
        cx.notify();
    });
    vcx.run_until_parked();
}

fn ids(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> Vec<String> {
    app.read_with(vcx, |app, _| {
        app.session()
            .map(|s| s.payments.hub.gifts.iter().map(|g| g.id.clone()).collect())
            .unwrap_or_default()
    })
}

#[gpui_kit::test]
fn the_entry_appears_only_for_a_deletable_gift(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    vcx.run_until_parked();
    assert!(vcx.debug_bounds("gift-delete").is_some());
    select(&app, vcx, KEPT);
    assert!(vcx.debug_bounds("gift-delete").is_none());
}

#[gpui_kit::test]
fn confirming_removes_the_gift_from_the_grid_and_the_box(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    vcx.run_until_parked();
    let button = vcx.debug_bounds("gift-delete").expect("the entry");
    vcx.simulate_click(button.center(), Modifiers::default());
    vcx.run_until_parked();
    // The confirmation is up and nothing is gone yet.
    assert_eq!(ids(&app, vcx), [DELETABLE, KEPT]);
    app.update_in(vcx, |app, _, cx| app.confirm_delete_gift(DELETABLE, cx));
    vcx.run_until_parked();
    assert_eq!(ids(&app, vcx), [KEPT]);
    app.read_with(vcx, |app, _| {
        let hub = app.session().map(|s| &s.payments.hub);
        assert_eq!(hub.and_then(|h| h.gift_selected.clone()), None);
        assert_eq!(hub.map(|h| h.gifts_total), Some(1));
    });
}
