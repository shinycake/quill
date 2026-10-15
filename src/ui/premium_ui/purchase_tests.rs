//! The subscribe button follows the app config (needs the `demo-capture`
//! feature for gpui-kit's test support).
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::premium_hub::promo::PromoPage;
use quill::premium_hub::purchase::PremiumPurchase;

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
                Some(ScreenshotDemo::named("ready-privacy")),
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

fn open_premium(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, purchase: PremiumPurchase) {
    app.update_in(vcx, |app, _, cx| {
        let session = app.demo_session.as_mut().expect("demo session");
        session.payments.hub.premium_open = true;
        session.payments.hub.purchase = purchase;
        cx.notify();
    });
    vcx.run_until_parked();
}

fn hub_state(
    app: &Entity<QuillApp>,
    vcx: &mut VisualTestContext,
) -> (bool, Option<(String, String)>) {
    app.read_with(vcx, |app, _| {
        let hub = &app.session().expect("session").payments.hub;
        (hub.premium_open, hub.auto_start.clone())
    })
}

#[gpui_kit::test]
fn the_bot_from_the_app_config_gets_the_ref_and_the_dialog_closes(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    open_premium(
        &app,
        vcx,
        PremiumPurchase {
            bot_username: "PremiumBot".into(),
            invoice_slug: "slug".into(),
        },
    );
    app.update_in(vcx, |app, window, cx| {
        let session = app.demo_session.as_mut().expect("demo session");
        session.payments.hub.promo_page = PromoPage::Limits;
        app.start_premium_payment(window, cx);
    });
    vcx.run_until_parked();
    assert_eq!(
        hub_state(&app, vcx),
        (false, Some(("premiumbot".into(), "double_limits".into())))
    );
}

#[gpui_kit::test]
fn without_a_bot_or_slug_nothing_opens_and_the_dialog_stays(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    open_premium(&app, vcx, PremiumPurchase::default());
    app.update_in(vcx, |app, window, cx| app.start_premium_payment(window, cx));
    vcx.run_until_parked();
    assert_eq!(hub_state(&app, vcx), (true, None));
}

#[gpui_kit::test]
fn the_invoice_slug_is_used_when_no_bot_is_named(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    open_premium(
        &app,
        vcx,
        PremiumPurchase {
            bot_username: String::new(),
            invoice_slug: "slug".into(),
        },
    );
    app.update_in(vcx, |app, window, cx| app.start_premium_payment(window, cx));
    vcx.run_until_parked();
    // No bot start is armed; the payment form opens instead.
    assert_eq!(hub_state(&app, vcx), (false, None));
}
