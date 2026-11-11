//! Starting a checkout that cannot be asked (no live connection): the
//! checkout that was on screen is gone with its dialog, and the user is told.

use super::card_form_demo::stripe_checkout;
use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::payment_invoice::InputInvoice;
use quill::state::{CheckoutForm, PaymentRequest};

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(800.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(window, cx, None, Some(ScreenshotDemo::named("ready-chats")))
        });
        *slot_in.borrow_mut() = Some(view.clone());
        let shell = cx.new(|_| QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let app: Entity<QuillApp> = slot.borrow().clone().expect("the window fills the slot");
    let vcx = VisualTestContext::from_window(handle.into(), cx);
    vcx.run_until_parked();
    (app, vcx)
}

/// The form request is refused (nothing to send it with): the open
/// checkout's dialog closes and a note says why, instead of the old dialog
/// staying over a checkout that was wiped.
#[gpui_kit::test]
fn a_checkout_that_cannot_be_asked_drops_the_old_dialog_and_says_so(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    app.update_in(&mut vcx, |app, window, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            session.payments.checkout = CheckoutForm::ready(
                stripe_checkout(true, false),
                PaymentRequest {
                    invoice: InputInvoice::Name("old".into()),
                },
            );
        }
        app.open_payment_dialog(window, cx);
        assert!(app.payments.dialog.is_some(), "the old checkout is open");
        app.start_invoice_checkout(InputInvoice::Name("new".into()), window, cx);
        assert!(app.payments.dialog.is_none(), "its dialog goes");
        assert!(
            !app.screen.connection.status_note().is_empty(),
            "the user is told"
        );
    });
}
