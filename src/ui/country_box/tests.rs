//! The country box on its two hosts: the checkout's shipping country (one,
//! picked from the list) and the poll's allowed countries (several, with
//! the TDLib limit and Save).

use super::CountryHost;
use super::demo::shipping_form;
use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::ScreenshotDemo;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let demo = ScreenshotDemo::named("ready-chats");
    let handle = cx.open_window(size(px(1000.), px(800.)), move |window, cx| {
        let view = cx.new(|cx| QuillApp::new_with_demo(window, cx, None, Some(demo)));
        *slot_in.borrow_mut() = Some(view.clone());
        Root::new(view, window, cx)
    });
    let app = slot.borrow().clone().unwrap();
    (app, VisualTestContext::from_window(handle.into(), cx))
}

fn search(app: &QuillApp, host: CountryHost, cx: &gpui_kit::App) -> String {
    let country_box = match host {
        CountryHost::Payment => app.payments.dialog.as_ref().unwrap().country_box.as_ref(),
        CountryHost::PaymentCard => app.card_form().unwrap().country_box.as_ref(),
        CountryHost::Poll => app
            .composer_ui
            .poll_dialog
            .as_ref()
            .unwrap()
            .country_box
            .as_ref(),
    };
    country_box.unwrap().search.read(cx).value().to_string()
}

#[gpui_kit::test]
fn the_shipping_country_is_picked_from_the_list(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    app.update_in(&mut vcx, |app, window, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            session.payments.form = Some(shipping_form());
        }
        app.open_payment_dialog(window, cx);
        let dialog = app.payments.dialog.as_ref().unwrap();
        assert_eq!(dialog.country_iso, "");
        assert!(dialog.country_box.is_none());

        app.open_country_box(CountryHost::Payment, window, cx);
        assert!(app.payments.dialog.as_ref().unwrap().country_box.is_some());
        assert_eq!(search(app, CountryHost::Payment, cx), "");

        // Picking a row stores the code, ends the box, and the order
        // carries it.
        app.country_box_click(CountryHost::Payment, "FR", window, cx);
        let dialog = app.payments.dialog.as_ref().unwrap();
        assert!(dialog.country_box.is_none());
        assert_eq!(dialog.country_iso, "FR");
        assert_eq!(dialog.order(cx).shipping_address.country_code, "FR");
        assert_eq!(app.country_display_name("FR"), "France");
    });
}

#[gpui_kit::test]
fn cancelling_keeps_the_shipping_country(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    app.update_in(&mut vcx, |app, window, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            session.payments.form = Some(shipping_form());
        }
        app.open_payment_dialog(window, cx);
        app.payments.dialog.as_mut().unwrap().country_iso = "DE".into();
        app.open_country_box(CountryHost::Payment, window, cx);
        app.close_country_box(CountryHost::Payment, cx);
        let dialog = app.payments.dialog.as_ref().unwrap();
        assert!(dialog.country_box.is_none());
        assert_eq!(dialog.country_iso, "DE");
    });
}

#[gpui_kit::test]
fn the_poll_takes_several_countries_up_to_the_limit(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    app.update_in(&mut vcx, |app, window, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            session.payments.poll_countries_max = Some(2);
        }
        app.open_poll_dialog(window, cx);
        app.composer_ui
            .poll_dialog
            .as_mut()
            .unwrap()
            .limit_by_country = true;
        app.open_country_box(CountryHost::Poll, window, cx);

        app.country_box_click(CountryHost::Poll, "US", window, cx);
        app.country_box_click(CountryHost::Poll, "GB", window, cx);
        // At the limit: the third click is refused with the TDLib number.
        app.country_box_click(CountryHost::Poll, "FR", window, cx);
        assert!(app.connection.status_note().contains("up to 2 countries"));
        // Nothing is kept until Save.
        let dialog = app.composer_ui.poll_dialog.as_ref().unwrap();
        assert!(dialog.country_codes.is_empty());

        app.country_box_remove(CountryHost::Poll, "US", cx);
        app.country_box_click(CountryHost::Poll, "FR", window, cx);
        app.country_box_save(CountryHost::Poll, cx);

        let dialog = app.composer_ui.poll_dialog.as_ref().unwrap();
        assert!(dialog.country_box.is_none());
        assert_eq!(dialog.country_codes, ["GB", "FR"]);
        assert!(dialog.is_dirty(cx));
        assert_eq!(dialog.draft(cx).country_codes, ["GB", "FR"]);
        assert_eq!(app.country_summary(&dialog_codes(app)), "2 countries");

        // Reopening starts from what was saved; Cancel leaves it alone.
        app.open_country_box(CountryHost::Poll, window, cx);
        app.country_box_click(CountryHost::Poll, "GB", window, cx);
        app.close_country_box(CountryHost::Poll, cx);
        let dialog = app.composer_ui.poll_dialog.as_ref().unwrap();
        assert_eq!(dialog.country_codes, ["GB", "FR"]);
    });
}

fn dialog_codes(app: &QuillApp) -> Vec<String> {
    app.composer_ui
        .poll_dialog
        .as_ref()
        .map(|dialog| dialog.country_codes.clone())
        .unwrap_or_default()
}

#[gpui_kit::test]
fn the_poll_limit_falls_back_to_tdesktops_until_tdlib_reports_it(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    app.update_in(&mut vcx, |app, _, _| {
        assert_eq!(
            app.poll_countries_max(),
            quill::country_select::POLL_COUNTRIES_FALLBACK
        );
        if let Some(session) = app.demo_session.as_mut() {
            session.payments.poll_countries_max = Some(7);
        }
        assert_eq!(app.poll_countries_max(), 7);
    });
}

#[gpui_kit::test]
fn both_boxes_render_their_rows_and_buttons(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    app.update_in(&mut vcx, |app, window, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            session.payments.form = Some(shipping_form());
        }
        app.open_payment_dialog(window, cx);
        app.open_poll_dialog(window, cx);
        app.open_country_box(CountryHost::Payment, window, cx);
        app.open_country_box(CountryHost::Poll, window, cx);
        assert!(
            app.country_box_view(CountryHost::Payment, "Country", cx)
                .is_some()
        );
        assert!(
            app.country_box_view(CountryHost::Poll, "Allowed", cx)
                .is_some()
        );
        assert!(app.poll_country_panel(cx).is_some());
    });
}

#[gpui_kit::test]
fn the_limit_switch_is_for_channels_and_off_forgets_the_countries(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let shown = |vcx: &mut VisualTestContext| {
        vcx.run_until_parked();
        for _ in 0..3 {
            vcx.update(|window, cx| window.render_frame(cx));
        }
        vcx.update(|window, _| window.try_find("poll-toggle-country").is_some())
    };
    // The demo chat is a group: no switch.
    app.update_in(&mut vcx, |app, window, cx| app.open_poll_dialog(window, cx));
    assert!(!shown(&mut vcx));
    // A channel shows it (the dialog opens again after the chat change).
    app.update_in(&mut vcx, |app, window, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            super::demo::open_demo_channel(session);
        }
        app.open_poll_dialog(window, cx);
    });
    assert!(shown(&mut vcx));
    app.update_in(&mut vcx, |app, window, cx| {
        let dialog = app.composer_ui.poll_dialog.as_mut().unwrap();
        dialog.limit_by_country = true;
        dialog.country_codes = vec!["US".into()];
        assert_eq!(dialog.draft(cx).country_codes, ["US"]);
        // On with no country is refused with tdesktop's text.
        dialog.country_codes.clear();
        app.submit_poll_dialog(window, cx);
        assert!(
            app.connection
                .status_note()
                .contains("at least one country")
        );
        // Off: no codes reach the draft.
        let dialog = app.composer_ui.poll_dialog.as_mut().unwrap();
        dialog.country_codes = vec!["US".into()];
        dialog.limit_by_country = false;
        assert!(dialog.draft(cx).country_codes.is_empty());
    });
}
