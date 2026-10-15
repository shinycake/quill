//! Settings → My Grams in a real window (needs the `demo-capture` feature
//! for gpui-kit's test support): the screen, the tabs, the receipt and the
//! Settings row, from the recorded fixture.
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::premium_hub::TxFilter;

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(2400.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(window, cx, None, Some(ScreenshotDemo::named("ready-grams")))
        });
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some(view);
        Root::new(shell, window, cx)
    });
    let app = slot.borrow().clone().expect("app view");
    let vcx = VisualTestContext::from_window(handle.into(), cx);
    (app, vcx)
}

fn grams_open(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> bool {
    app.read_with(vcx, |app, _| {
        app.session().is_some_and(|s| s.payments.hub.grams.open)
    })
}

#[gpui_kit::test]
fn the_screen_shows_balance_withdrawal_and_every_row(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    app.update_in(vcx, |app, _, cx| app.open_grams(cx));
    vcx.run_until_parked();
    assert!(grams_open(&app, vcx));
    assert!(vcx.debug_bounds("grams-balance").is_some());
    assert!(vcx.debug_bounds("grams-withdrawal").is_some());
    for id in [
        "grams-tx-tx-topup",
        "grams-tx-tx-withdrawal",
        "grams-tx-tx-suggested",
        "grams-tx-tx-sale",
        "grams-tx-tx-unsupported",
    ] {
        assert!(vcx.debug_bounds(id).is_some(), "{id}");
    }

    // A tab switch keeps the dialog and changes the filter.
    app.update_in(vcx, |app, _, cx| {
        app.set_grams_filter(TxFilter::Outgoing, cx)
    });
    vcx.run_until_parked();
    let filter = app.read_with(vcx, |app, _| {
        app.session().map(|s| s.payments.hub.grams.filter)
    });
    assert_eq!(filter, Some(TxFilter::Outgoing));

    // A row opens its receipt, Back returns to the list.
    app.update_in(vcx, |app, _, cx| {
        app.select_grams_tx(Some("tx-sale".into()), cx)
    });
    vcx.run_until_parked();
    assert!(vcx.debug_bounds("grams-receipt").is_some());
    assert!(vcx.debug_bounds("grams-balance").is_none());
    app.update_in(vcx, |app, _, cx| app.select_grams_tx(None, cx));
    vcx.run_until_parked();
    assert!(vcx.debug_bounds("grams-receipt").is_none());

    app.update_in(vcx, |app, _, cx| app.close_grams(cx));
    vcx.run_until_parked();
    assert!(!grams_open(&app, vcx));
}

#[gpui_kit::test]
fn settings_lists_my_grams_only_with_a_balance(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    let row = |app: &Entity<QuillApp>, vcx: &mut VisualTestContext| {
        app.update_in(vcx, |app, _, cx| {
            app.grams_settings_row(|_, _| {}, cx).is_some()
        })
    };
    assert!(row(&app, vcx));
    // The Settings list renders with the row.
    app.update_in(vcx, |app, _, cx| {
        app.settings.open = true;
        cx.notify();
    });
    vcx.run_until_parked();
    app.update_in(vcx, |app, _, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            session.payments.hub.grams.balance_nano = Some(0);
        }
        cx.notify();
    });
    vcx.run_until_parked();
    assert!(!row(&app, vcx));
}
