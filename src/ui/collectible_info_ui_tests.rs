//! The collectible box opens when a lookup has its answer (needs the
//! `demo-capture` feature for gpui-kit's test support).
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::collectible_info::{CollectibleInfo, CollectibleLookup};

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

fn lookup_open(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> bool {
    app.read_with(vcx, |app, _| {
        app.session()
            .is_some_and(|s| s.payments.hub.collectible.is_some())
    })
}

#[gpui_kit::test]
fn the_box_opens_with_the_answer_and_closes_with_the_lookup(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    app.update_in(vcx, |app, _, cx| {
        let session = app.demo_session.as_mut().expect("demo session");
        let mut lookup = CollectibleLookup::new("durov", "Pavel", None, String::new());
        lookup.info = Some(CollectibleInfo {
            purchase_date: 1_700_000_000,
            currency: "USD".into(),
            amount: 1250,
            crypto_currency: "TON".into(),
            crypto_amount: 5_500_000_000,
            url: "https://fragment.com/username/durov".into(),
        });
        session.payments.hub.collectible = Some(lookup);
        cx.notify();
    });
    vcx.run_until_parked();
    assert!(lookup_open(&app, vcx));
    app.update_in(vcx, |app, _, cx| {
        app.demo_session
            .as_mut()
            .expect("demo session")
            .payments
            .hub
            .collectible = None;
        cx.notify();
    });
    vcx.run_until_parked();
    assert!(!lookup_open(&app, vcx));
}
