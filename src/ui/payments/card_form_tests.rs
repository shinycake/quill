//! The "New Card" form in a real window: formatting as it is typed, focus
//! moving between fields, Done's checks, the provider's answers (from a
//! recorded transport, never the network), and the card becoming the
//! checkout's payment method.

use super::card_form_demo::stripe_checkout;
use crate::ui::app::QuillApp;
use crate::ui::dialogs::PaymentCredentialChoice;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, Focusable, TestAppContext, VisualTestContext, px, size};
use quill::payment_card::CardField;
use quill::payment_card::transport::{HttpRequest, HttpResponse, Transport, TransportError};
use quill::telegram::envelope::PasswordState;
use std::sync::{Arc, Mutex};

const TOKEN: &str =
    r#"{"id":"tok_visa","card":{"brand":"Visa","last4":"4242"},"created":1,"livemode":false}"#;
const DECLINED: &str =
    r#"{"error":{"code":"card_declined","message":"Declined.","type":"card_error"}}"#;
const BAD_CVC: &str =
    r#"{"error":{"code":"incorrect_cvc","message":"Bad CVC.","type":"card_error"}}"#;

/// Answers every tokenize call with one recorded response and keeps the
/// request bodies it saw.
struct Recorded {
    status: u16,
    body: &'static str,
    bodies: Mutex<Vec<String>>,
}

impl Recorded {
    fn new(status: u16, body: &'static str) -> Arc<Self> {
        Arc::new(Self {
            status,
            body,
            bodies: Mutex::new(Vec::new()),
        })
    }

    fn bodies(&self) -> Vec<String> {
        self.bodies.lock().map(|b| b.clone()).unwrap_or_default()
    }
}

impl Transport for Recorded {
    fn post(&self, request: &HttpRequest) -> Result<HttpResponse, TransportError> {
        if let Ok(mut bodies) = self.bodies.lock() {
            bodies.push(String::from_utf8_lossy(&request.body).into_owned());
        }
        Ok(HttpResponse {
            status: self.status,
            body: self.body.as_bytes().to_vec(),
        })
    }
}

fn open_checkout(
    cx: &mut TestAppContext,
    can_save: bool,
    need_password: bool,
    transport: Arc<Recorded>,
) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(800.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(window, cx, None, Some(ScreenshotDemo::named("ready-chats")))
        });
        *slot_in.borrow_mut() = Some(view.clone());
        let focus = view.focus_handle(cx);
        window.focus(&focus, cx);
        let shell = cx.new(|_| QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let app: Entity<QuillApp> = slot.borrow().clone().expect("the window fills the slot");
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    app.update_in(&mut vcx, |app, window, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            session.payments.form = Some(stripe_checkout(can_save, need_password));
        }
        app.payments.card_transport = Some(transport);
        app.open_payment_dialog(window, cx);
        cx.notify();
    });
    draw(&mut vcx);
    (app, vcx)
}

fn draw(vcx: &mut VisualTestContext) {
    vcx.run_until_parked();
    for _ in 0..3 {
        vcx.update(|window, cx| window.render_frame(cx));
    }
    vcx.run_until_parked();
}

fn open_card_form(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) {
    app.update_in(vcx, |app, window, cx| app.open_card_form(window, cx));
    draw(vcx);
}

fn value(app: &Entity<QuillApp>, field: CardField, vcx: &mut VisualTestContext) -> String {
    vcx.update(|_, cx| {
        app.read(cx)
            .card_form()
            .and_then(|form| form.input(field))
            .map(|input| input.state.read(cx).value().to_string())
            .unwrap_or_default()
    })
}

fn invalid(app: &Entity<QuillApp>, field: CardField, vcx: &mut VisualTestContext) -> bool {
    vcx.update(|_, cx| {
        app.read(cx)
            .card_form()
            .and_then(|form| form.input(field))
            .is_some_and(|input| input.invalid)
    })
}

fn focused(app: &Entity<QuillApp>, field: CardField, vcx: &mut VisualTestContext) -> bool {
    vcx.update(|window, cx| {
        let Some(form) = app.read(cx).card_form() else {
            return false;
        };
        if field == CardField::Country {
            return form.country_focus.is_focused(window);
        }
        form.input(field)
            .is_some_and(|input| input.state.focus_handle(cx).is_focused(window))
    })
}

fn type_into(app: &Entity<QuillApp>, field: CardField, text: &str, vcx: &mut VisualTestContext) {
    app.update_in(vcx, |app, window, cx| {
        app.focus_card_field(field, window, cx)
    });
    draw(vcx);
    vcx.simulate_input(text);
    draw(vcx);
}

/// Every field filled with Stripe's Visa test card.
fn fill_visa(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) {
    type_into(app, CardField::Number, "4242424242424242", vcx);
    type_into(app, CardField::Expiry, "1230", vcx);
    type_into(app, CardField::Cvc, "123", vcx);
    type_into(app, CardField::Name, "jane doe", vcx);
    app.update_in(vcx, |app, window, cx| {
        app.card_country_picked("US".into(), window, cx);
    });
    type_into(app, CardField::Zip, "94107", vcx);
}

fn press_done(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) {
    app.update_in(vcx, |app, window, cx| app.submit_card_form(window, cx));
    draw(vcx);
}

#[gpui_kit::test]
fn the_card_number_and_expiry_format_as_they_are_typed(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_checkout(cx, true, false, Recorded::new(200, TOKEN));
    open_card_form(&app, &mut vcx);
    assert!(
        focused(&app, CardField::Number, &mut vcx),
        "the form opens on the number"
    );

    vcx.simulate_input("4242424242424242");
    draw(&mut vcx);
    assert_eq!(
        value(&app, CardField::Number, &mut vcx),
        "4242 4242 4242 4242"
    );
    assert!(!invalid(&app, CardField::Number, &mut vcx));

    // Enter on a valid field moves on; a complete expiry moves on by itself.
    vcx.simulate_keystrokes("enter");
    draw(&mut vcx);
    assert!(focused(&app, CardField::Expiry, &mut vcx));
    vcx.simulate_input("1230");
    draw(&mut vcx);
    assert_eq!(value(&app, CardField::Expiry, &mut vcx), "12/30");
    assert!(focused(&app, CardField::Cvc, &mut vcx));

    // Letters never reach the CVC, and the holder name is upper case.
    vcx.simulate_input("1a2");
    draw(&mut vcx);
    assert_eq!(value(&app, CardField::Cvc, &mut vcx), "12");
    type_into(&app, CardField::Name, "jane doe", &mut vcx);
    assert_eq!(value(&app, CardField::Name, &mut vcx), "JANE DOE");
}

#[gpui_kit::test]
fn backspace_after_a_space_removes_the_digit_before_it(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_checkout(cx, true, false, Recorded::new(200, TOKEN));
    open_card_form(&app, &mut vcx);
    vcx.simulate_input("42424");
    draw(&mut vcx);
    assert_eq!(value(&app, CardField::Number, &mut vcx), "4242 4");
    app.update_in(&mut vcx, |app, window, cx| {
        if let Some(input) = app
            .card_form()
            .and_then(|form| form.input(CardField::Number))
        {
            input.state.update(cx, |state, cx| {
                state.set_selected_range(5..5, cx);
                state.focus(window, cx);
            });
        }
    });
    draw(&mut vcx);
    vcx.simulate_keystrokes("backspace");
    draw(&mut vcx);
    assert_eq!(value(&app, CardField::Number, &mut vcx), "4244");

    // Backspace at the front of a field goes back to the previous one.
    type_into(&app, CardField::Expiry, "", &mut vcx);
    vcx.simulate_keystrokes("backspace");
    draw(&mut vcx);
    assert!(focused(&app, CardField::Number, &mut vcx));
}

#[gpui_kit::test]
fn done_marks_the_first_field_that_is_wrong(cx: &mut TestAppContext) {
    let transport = Recorded::new(200, TOKEN);
    let (app, mut vcx) = open_checkout(cx, true, false, transport.clone());
    open_card_form(&app, &mut vcx);
    type_into(&app, CardField::Number, "4242424242424241", &mut vcx);
    press_done(&app, &mut vcx);
    assert!(
        invalid(&app, CardField::Number, &mut vcx),
        "a failed Luhn check"
    );
    assert!(focused(&app, CardField::Number, &mut vcx));

    // A good number, but no country yet: the country is marked.
    app.update_in(&mut vcx, |app, window, cx| {
        if let Some(input) = app
            .card_form()
            .and_then(|form| form.input(CardField::Number))
        {
            input
                .state
                .update(cx, |state, cx| state.set_value("", window, cx));
        }
    });
    type_into(&app, CardField::Number, "4242424242424242", &mut vcx);
    type_into(&app, CardField::Expiry, "1230", &mut vcx);
    type_into(&app, CardField::Cvc, "123", &mut vcx);
    type_into(&app, CardField::Name, "Jane", &mut vcx);
    press_done(&app, &mut vcx);
    let country_invalid = vcx.update(|_, cx| {
        app.read(cx)
            .card_form()
            .is_some_and(|form| form.country_invalid)
    });
    assert!(country_invalid);
    assert!(focused(&app, CardField::Country, &mut vcx));
    assert!(
        transport.bodies().is_empty(),
        "nothing reaches the provider"
    );

    // Enter on the focused country opens the box; picking moves to the ZIP.
    vcx.simulate_keystrokes("enter");
    draw(&mut vcx);
    let box_open = vcx.update(|_, cx| {
        app.read(cx)
            .card_form()
            .is_some_and(|form| form.country_box.is_some())
    });
    assert!(box_open);
    app.update_in(&mut vcx, |app, window, cx| {
        app.close_country_box(crate::ui::country_box::CountryHost::PaymentCard, cx);
        app.card_country_picked("US".into(), window, cx);
    });
    draw(&mut vcx);
    assert!(focused(&app, CardField::Zip, &mut vcx));
}

#[gpui_kit::test]
fn the_provider_token_becomes_the_payment_method(cx: &mut TestAppContext) {
    let transport = Recorded::new(200, TOKEN);
    let (app, mut vcx) = open_checkout(cx, true, false, transport.clone());
    open_card_form(&app, &mut vcx);
    fill_visa(&app, &mut vcx);
    app.update_in(&mut vcx, |app, _, cx| app.toggle_card_save(true, cx));
    // Enter in the last field is Done.
    vcx.simulate_keystrokes("enter");
    draw(&mut vcx);

    let bodies = transport.bodies();
    assert_eq!(bodies.len(), 1);
    assert!(
        bodies
            .iter()
            .all(|body| body.contains("card%5Bnumber%5D=4242424242424242"))
    );
    vcx.update(|_, cx| {
        let dialog = app
            .read(cx)
            .payments
            .dialog
            .as_ref()
            .expect("checkout open");
        assert!(dialog.card_form.is_none(), "back on the checkout");
        assert_eq!(dialog.credential_choice, PaymentCredentialChoice::NewCard);
        let card = dialog.new_card.as_ref().expect("a new card");
        assert_eq!(card.credentials.title(), "visa *4242");
        assert_eq!(
            card.credentials.data(),
            r#"{"id":"tok_visa","type":"card"}"#
        );
        assert!(card.save);
        assert!(!dialog.password_prompt);
    });
}

#[gpui_kit::test]
fn provider_errors_keep_the_form_open(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_checkout(cx, true, false, Recorded::new(402, DECLINED));
    open_card_form(&app, &mut vcx);
    fill_visa(&app, &mut vcx);
    press_done(&app, &mut vcx);
    vcx.update(|_, cx| {
        let dialog = app
            .read(cx)
            .payments
            .dialog
            .as_ref()
            .expect("checkout open");
        let form = dialog.card_form.as_ref().expect("still on the card form");
        assert!(!form.is_tokenizing(), "Done works again");
        assert!(dialog.new_card.is_none());
        assert_eq!(
            form.notice.as_ref().map(|notice| notice.as_ref()),
            Some(quill::lang::tr(quill::lang::keys::PAYMENTS_CARD_DECLINED))
        );
    });
    assert!(
        vcx.update(|window, _| window.try_find("payment-card-notice").is_some()),
        "the refusal shows inside the checkout"
    );
    // Done again drops the old refusal while the provider answers.
    let cleared = app.update_in(&mut vcx, |app, window, cx| {
        app.submit_card_form(window, cx);
        app.card_form().is_some_and(|form| form.notice.is_none())
    });
    assert!(cleared);
    draw(&mut vcx);

    let (app, mut vcx) = open_checkout(cx, true, false, Recorded::new(402, BAD_CVC));
    open_card_form(&app, &mut vcx);
    fill_visa(&app, &mut vcx);
    press_done(&app, &mut vcx);
    assert!(invalid(&app, CardField::Cvc, &mut vcx));
    assert!(focused(&app, CardField::Cvc, &mut vcx));
}

fn prompt_shown(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> bool {
    vcx.update(|_, cx| {
        app.read(cx)
            .payments
            .dialog
            .as_ref()
            .is_some_and(|dialog| dialog.password_prompt)
    })
}

fn pay_enabled(vcx: &mut VisualTestContext) -> Option<bool> {
    vcx.update(|window, _| {
        window
            .try_find("p1-payment-pay")
            .map(|pay| pay.disabled() != Some(true))
    })
}

#[gpui_kit::test]
fn saving_without_a_password_asks_to_turn_it_on(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_checkout(cx, false, true, Recorded::new(200, TOKEN));
    open_card_form(&app, &mut vcx);
    let offered = vcx.update(|_, cx| app.read(cx).card_form().and_then(|form| form.save));
    assert_eq!(offered, Some(false), "the save box shows, unticked");
    fill_visa(&app, &mut vcx);
    app.update_in(&mut vcx, |app, _, cx| app.toggle_card_save(true, cx));
    press_done(&app, &mut vcx);
    assert!(prompt_shown(&app, &mut vcx));
    assert_eq!(
        pay_enabled(&mut vcx),
        Some(false),
        "answer the prompt first"
    );

    // Another method drops the prompt; the new card again asks again.
    vcx.update(|window, cx| window.within("payment-method").click(1usize, cx));
    draw(&mut vcx);
    assert!(!prompt_shown(&app, &mut vcx));
    assert_eq!(pay_enabled(&mut vcx), Some(true));
    vcx.update(|window, cx| window.within("payment-method").click(2usize, cx));
    draw(&mut vcx);
    assert!(prompt_shown(&app, &mut vcx));

    // Continue: Two-Step Verification over the checkout, which keeps the card.
    vcx.update(|window, cx| window.click("payment-card-password-continue", cx));
    draw(&mut vcx);
    let (twofa_open, card_kept) = vcx.update(|_, cx| {
        let app = app.read(cx);
        let dialog = app.payments.dialog.as_ref();
        (
            app.twofa.open,
            dialog.is_some_and(|dialog| {
                dialog.new_card.is_some()
                    && dialog.credential_choice == PaymentCredentialChoice::NewCard
            }),
        )
    });
    assert!(twofa_open);
    assert!(card_kept, "the checkout and its new card stay");
    assert_eq!(pay_enabled(&mut vcx), None, "Two-Step Verification shows");
    assert!(!vcx.update(|_, cx| app.read(cx).checkout_can_save_credentials()));

    // A password set there lets the card be saved; closing comes back.
    app.update_in(&mut vcx, |app, _, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            session.auth_state.password_state = Some(PasswordState {
                has_password: true,
                password_hint: String::new(),
                has_recovery_email_address: false,
                has_passport_data: false,
                pending_email_pattern: None,
                pending_email_code_length: 0,
                login_email_address_pattern: String::new(),
                pending_reset_date: 0,
            });
        }
        app.twofa.open = false;
        cx.notify();
    });
    draw(&mut vcx);
    assert_eq!(pay_enabled(&mut vcx), Some(true), "back on the checkout");
    assert!(vcx.update(|_, cx| app.read(cx).checkout_can_save_credentials()));
    vcx.update(|window, cx| window.within("payment-method").click(2usize, cx));
    draw(&mut vcx);
    assert!(!prompt_shown(&app, &mut vcx), "nothing left to ask");
}

#[gpui_kit::test]
fn a_delete_that_changes_nothing_leaves_no_snapshot(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_checkout(cx, true, false, Recorded::new(200, TOKEN));
    open_card_form(&app, &mut vcx);
    vcx.simulate_input("4242");
    draw(&mut vcx);
    // Delete at the end of the text has nothing to remove.
    vcx.simulate_keystrokes("delete");
    draw(&mut vcx);
    let pending = vcx.update(|_, cx| {
        app.read(cx)
            .card_form()
            .and_then(|form| form.input(CardField::Number))
            .is_some_and(|input| input.has_delete_snapshot())
    });
    assert!(!pending);
    assert_eq!(value(&app, CardField::Number, &mut vcx), "4242");
    // A Backspace that does edit still steps over the separator.
    vcx.simulate_input("4");
    draw(&mut vcx);
    app.update_in(&mut vcx, |app, window, cx| {
        if let Some(input) = app
            .card_form()
            .and_then(|form| form.input(CardField::Number))
        {
            input.state.update(cx, |state, cx| {
                state.set_selected_range(5..5, cx);
                state.focus(window, cx);
            });
        }
    });
    draw(&mut vcx);
    vcx.simulate_keystrokes("backspace");
    draw(&mut vcx);
    assert_eq!(value(&app, CardField::Number, &mut vcx), "4244");
}

#[gpui_kit::test]
fn new_card_opens_the_form_over_the_checkout(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_checkout(cx, true, false, Recorded::new(200, TOKEN));
    let shown = |vcx: &mut VisualTestContext, id: &'static str| {
        vcx.update(|window, _| window.try_find(id).is_some())
    };
    let saved_listed = |vcx: &mut VisualTestContext| {
        vcx.update(|window, _| window.within("payment-method").try_find(1usize).is_some())
    };
    assert!(saved_listed(&mut vcx), "the saved card is listed");
    assert!(shown(&mut vcx, "p1-payment-pay"));
    let choice = vcx.update(|_, cx| {
        app.read(cx)
            .payments
            .dialog
            .as_ref()
            .map(|dialog| dialog.credential_choice.clone())
    });
    assert_eq!(
        choice,
        Some(PaymentCredentialChoice::Saved("saved-1".into()))
    );

    vcx.update(|window, cx| window.within("payment-method").click(0usize, cx));
    draw(&mut vcx);
    assert!(shown(&mut vcx, "payment-card-done"));
    assert!(
        !shown(&mut vcx, "p1-payment-pay"),
        "the form replaces the checkout"
    );
}

#[gpui_kit::test]
fn escape_leaves_the_card_form_for_the_checkout(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_checkout(cx, true, false, Recorded::new(200, TOKEN));
    open_card_form(&app, &mut vcx);
    vcx.simulate_keystrokes("escape");
    draw(&mut vcx);
    vcx.update(|_, cx| {
        let dialog = app
            .read(cx)
            .payments
            .dialog
            .as_ref()
            .expect("checkout open");
        assert!(dialog.card_form.is_none());
    });
}
